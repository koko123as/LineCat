//! The local socket to the Kotlin shell (`UiBridge.kt`).
//!
//! Every frame is `u32` big-endian payload length, one channel byte, then the payload.
//! Channel 0 carries the Core's JSON-RPC 2.0 messages verbatim; the shell forwards them to a
//! Core session it opened for this connection. Channel 1 carries small JSON shell commands
//! (`{"id":1,"op":"vpn.start",...}`), their answers (`{"id":1,"ok":true,...}`) and pushed
//! events (`{"ev":"state","state":"STARTED"}`).

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const CH_CORE: u8 = 0;
pub const CH_SHELL: u8 = 1;
const MAX_FRAME: usize = 64 << 20;
const INITIAL_CREDIT: u64 = 4;
/// FlClash: lib/core/api/core_client.dart (`_resyncPause`).
const RESYNC_PAUSE: Duration = Duration::from_millis(50);

#[cfg(any(target_os = "android", target_os = "linux"))]
type Stream = std::os::unix::net::UnixStream;

pub type Handler = Box<dyn Fn(Value) + Send + Sync>;
pub type Resynced = Box<dyn Fn() + Send + Sync>;

/// A stream method: the handler gets the snapshot and then every event of the stream. The
/// Core ends a subscription that fell behind with a resync, after which it is opened again
/// and `resynced` hears that events were lost.
#[derive(Clone)]
struct Watch {
    method: String,
    params: Value,
    handler: Arc<Handler>,
    resynced: Option<Arc<Resynced>>,
}

enum Pending {
    Call(Sender<Result<Value, String>>),
    Watch(Watch),
}

struct Inner {
    writer: Mutex<Stream>,
    shutdown: Stream,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
    subs: Mutex<HashMap<u64, Watch>>,
    shell_pending: Mutex<HashMap<u64, Sender<Value>>>,
    on_shell_event: Mutex<Option<Handler>>,
    closed: AtomicBool,
}

#[derive(Clone)]
pub struct Bridge(Arc<Inner>);

pub fn write_frame(out: &mut impl Write, channel: u8, payload: &[u8]) -> std::io::Result<()> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.push(channel);
    frame.extend_from_slice(payload);
    out.write_all(&frame)?;
    out.flush()
}

pub fn read_frame(input: &mut impl Read) -> std::io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    input.read_exact(&mut head)?;
    let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
    if len > MAX_FRAME {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0u8; len];
    input.read_exact(&mut payload)?;
    Ok((head[4], payload))
}

/// The shell's abstract socket name: `<applicationId>.ui`. The application id is read from
/// the data directory (`/data/user/0/<applicationId>/files`), so a `.dev` build talks to its own shell.
pub fn socket_name(files_dir: &str) -> String {
    let package = std::path::Path::new(files_dir)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .filter(|n| n.contains('.'))
        .unwrap_or("season.us.ci.cat");
    format!("{package}.ui")
}

impl Bridge {
    #[cfg(target_os = "android")]
    pub fn connect(name: &str) -> std::io::Result<Self> {
        use std::os::android::net::SocketAddrExt;
        use std::os::unix::net::SocketAddr;
        let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        Self::start(Stream::connect_addr(&addr)?)
    }

    #[cfg(all(not(target_os = "android"), target_os = "linux"))]
    pub fn connect(name: &str) -> std::io::Result<Self> {
        use std::os::linux::net::SocketAddrExt;
        use std::os::unix::net::SocketAddr;
        let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
        Self::start(Stream::connect_addr(&addr)?)
    }

    fn start(stream: Stream) -> std::io::Result<Self> {
        let reader = stream.try_clone()?;
        let shutdown = stream.try_clone()?;
        let inner = Arc::new(Inner {
            writer: Mutex::new(stream),
            shutdown,
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            subs: Mutex::new(HashMap::new()),
            shell_pending: Mutex::new(HashMap::new()),
            on_shell_event: Mutex::new(None),
            closed: AtomicBool::new(false),
        });
        let bridge = Bridge(inner);
        let weak = Arc::downgrade(&bridge.0);
        std::thread::Builder::new()
            .name("ui-bridge".into())
            .spawn(move || {
                let mut reader = reader;
                while let Ok((channel, payload)) = read_frame(&mut reader) {
                    let Some(inner) = weak.upgrade() else { break };
                    Bridge(inner).dispatch(channel, &payload);
                }
                if let Some(inner) = weak.upgrade() {
                    Bridge(inner).fail_all("the shell closed the connection");
                }
            })?;
        Ok(bridge)
    }

    pub fn is_closed(&self) -> bool {
        self.0.closed.load(Ordering::SeqCst)
    }

    pub fn close(&self) {
        let _ = self.0.shutdown.shutdown(std::net::Shutdown::Both);
        self.fail_all("closed");
    }

    pub fn set_shell_listener(&self, handler: Handler) {
        *self.0.on_shell_event.lock().unwrap() = Some(handler);
    }

    fn send(&self, channel: u8, value: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        let mut w = self.0.writer.lock().unwrap();
        write_frame(&mut *w, channel, &bytes).map_err(|e| {
            self.0.closed.store(true, Ordering::SeqCst);
            e.to_string()
        })
    }

    fn fail_all(&self, why: &str) {
        self.0.closed.store(true, Ordering::SeqCst);
        let pending: Vec<_> = self.0.pending.lock().unwrap().drain().collect();
        for (_, p) in pending {
            if let Pending::Call(tx) = p {
                let _ = tx.send(Err(why.to_string()));
            }
        }
        self.0.shell_pending.lock().unwrap().clear();
        self.0.subs.lock().unwrap().clear();
        if let Some(h) = self.0.on_shell_event.lock().unwrap().as_ref() {
            h(json!({"ev": "closed"}));
        }
    }

    fn dispatch(&self, channel: u8, payload: &[u8]) {
        let Ok(message) = serde_json::from_slice::<Value>(payload) else {
            return;
        };
        match channel {
            CH_CORE => self.dispatch_core(message),
            CH_SHELL => {
                if let Some(id) = message.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = self.0.shell_pending.lock().unwrap().remove(&id) {
                        let _ = tx.send(message);
                    }
                } else if let Some(h) = self.0.on_shell_event.lock().unwrap().as_ref() {
                    h(message);
                }
            }
            _ => {}
        }
    }

    fn dispatch_core(&self, message: Value) {
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            let Some(entry) = self.0.pending.lock().unwrap().remove(&id) else {
                return;
            };
            let result = match message.get("error") {
                Some(error) => Err(error_text(error)),
                None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
            };
            match entry {
                Pending::Call(tx) => {
                    let _ = tx.send(result);
                }
                Pending::Watch(watch) => {
                    if let Ok(opened) = result {
                        let handler = watch.handler.clone();
                        if let Some(sub) = opened.get("sub").and_then(Value::as_u64) {
                            self.0.subs.lock().unwrap().insert(sub, watch);
                        }
                        if let Some(snapshot) = opened.get("snapshot") {
                            if !snapshot.is_null() {
                                handler(snapshot.clone());
                            }
                        }
                    }
                }
            }
            return;
        }
        if message.get("method").and_then(Value::as_str) != Some("$/event") {
            return;
        }
        let Some(params) = message.get("params") else {
            return;
        };
        let Some(sub) = params.get("sub").and_then(Value::as_u64) else {
            return;
        };
        if params.get("resync").and_then(Value::as_bool) == Some(true) {
            let Some(watch) = self.0.subs.lock().unwrap().remove(&sub) else {
                return;
            };
            if let Some(resynced) = &watch.resynced {
                resynced();
            }
            let bridge = self.clone();
            std::thread::spawn(move || {
                std::thread::sleep(RESYNC_PAUSE);
                if !bridge.is_closed() {
                    let _ = bridge.open(watch);
                }
            });
            return;
        }
        let handler = self
            .0
            .subs
            .lock()
            .unwrap()
            .get(&sub)
            .map(|w| w.handler.clone());
        let (Some(handler), Some(data)) = (handler, params.get("data")) else {
            return;
        };
        match data {
            Value::Array(items) => items.iter().for_each(|item| handler(item.clone())),
            other => handler(other.clone()),
        }
        let _ = self.notify("$/credit", json!({"sub": sub, "n": 1}));
    }

    fn envelope(id: u64, method: &str, params: Value) -> Value {
        let mut m = Map::new();
        m.insert("jsonrpc".into(), "2.0".into());
        m.insert("id".into(), id.into());
        m.insert("method".into(), method.into());
        if !params.is_null() {
            m.insert("params".into(), params);
        }
        Value::Object(m)
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.send(
            CH_CORE,
            &json!({"jsonrpc": "2.0", "method": method, "params": params}),
        )
    }

    /// A Core call; blocks the calling thread, so never call it on the UI thread.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, String> {
        if self.is_closed() {
            return Err("未连接到内核".into());
        }
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = channel();
        self.0.pending.lock().unwrap().insert(id, Pending::Call(tx));
        if let Err(e) = self.send(CH_CORE, &Self::envelope(id, method, params)) {
            self.0.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(_) => {
                self.0.pending.lock().unwrap().remove(&id);
                Err(format!("{method} 超时"))
            }
        }
    }

    /// Subscribes to a stream method with the default credit; the handler runs on the bridge thread.
    pub fn watch(&self, method: &str, params: Value, handler: Handler) -> Result<(), String> {
        self.open(Watch {
            method: method.into(),
            params,
            handler: Arc::new(handler),
            resynced: None,
        })
    }

    /// `watch`, with `resynced` run whenever events were lost before the stream reopens.
    pub fn watch_resyncing(
        &self,
        method: &str,
        params: Value,
        handler: Handler,
        resynced: Resynced,
    ) -> Result<(), String> {
        self.open(Watch {
            method: method.into(),
            params,
            handler: Arc::new(handler),
            resynced: Some(Arc::new(resynced)),
        })
    }

    fn open(&self, watch: Watch) -> Result<(), String> {
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst);
        let mut params = match watch.params.clone() {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        params.insert("credit".into(), INITIAL_CREDIT.into());
        let envelope = Self::envelope(id, &watch.method, Value::Object(params));
        self.0
            .pending
            .lock()
            .unwrap()
            .insert(id, Pending::Watch(watch));
        self.send(CH_CORE, &envelope)
    }

    /// A shell command; the answer is the shell's JSON object.
    pub fn shell(&self, op: &str, mut body: Value, timeout: Duration) -> Result<Value, String> {
        if self.is_closed() {
            return Err("未连接到应用外壳".into());
        }
        let id = self.0.next_id.fetch_add(1, Ordering::SeqCst);
        if !body.is_object() {
            body = json!({});
        }
        body["id"] = id.into();
        body["op"] = op.into();
        let (tx, rx) = channel();
        self.0.shell_pending.lock().unwrap().insert(id, tx);
        self.send(CH_SHELL, &body)?;
        let answer = rx.recv_timeout(timeout).map_err(|_| {
            self.0.shell_pending.lock().unwrap().remove(&id);
            format!("{op} 超时")
        })?;
        if answer.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(answer)
        } else {
            Err(answer
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("失败")
                .to_string())
        }
    }
}

/// The Core's error object as one line: the first issue's message, else the message.
pub fn error_text(error: &Value) -> String {
    error
        .pointer("/data/issues/0/message")
        .and_then(Value::as_str)
        .or_else(|| error.get("message").and_then(Value::as_str))
        .unwrap_or("unknown error")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, 1, br#"{"op":"vpn.state"}"#).unwrap();
        assert_eq!(&buf[..5], &[0, 0, 0, 18, 1]);
        let (ch, payload) = read_frame(&mut buf.as_slice()).unwrap();
        assert_eq!(ch, 1);
        assert_eq!(payload, br#"{"op":"vpn.state"}"#);
    }

    #[test]
    fn socket_name_follows_the_package() {
        assert_eq!(
            socket_name("/data/user/0/season.us.ci.cat/files"),
            "season.us.ci.cat.ui"
        );
        assert_eq!(
            socket_name("/data/user/0/season.us.ci.cat.dev/files"),
            "season.us.ci.cat.dev.ui"
        );
        assert_eq!(socket_name("/tmp/x"), "season.us.ci.cat.ui");
    }

    #[test]
    fn core_error_prefers_the_issue() {
        let e = json!({"code": 1, "message": "invalid", "data": {"kind": "invalid", "issues": [{"message": "bad proxy"}]}});
        assert_eq!(error_text(&e), "bad proxy");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_resync_opens_the_stream_again() {
        let (ours, mut theirs) = Stream::pair().unwrap();
        theirs
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let bridge = Bridge::start(ours).unwrap();
        let resyncs = Arc::new(AtomicU64::new(0));
        let r = resyncs.clone();
        let params = json!({"delays": false});
        let counted = Box::new(move || {
            r.fetch_add(1, Ordering::SeqCst);
        });
        bridge
            .watch_resyncing("events.watch", params, Box::new(|_| {}), counted)
            .unwrap();
        let read = |s: &mut Stream| -> Value {
            let (channel, payload) = read_frame(s).unwrap();
            assert_eq!(channel, CH_CORE);
            serde_json::from_slice(&payload).unwrap()
        };
        let send = |s: &mut Stream, v: Value| {
            write_frame(s, CH_CORE, &serde_json::to_vec(&v).unwrap()).unwrap();
        };
        let first = read(&mut theirs);
        assert_eq!(first["method"], "events.watch");
        send(
            &mut theirs,
            json!({"jsonrpc": "2.0", "id": first["id"], "result": {"sub": 7}}),
        );
        send(
            &mut theirs,
            json!({"jsonrpc": "2.0", "method": "$/event", "params": {"sub": 7, "resync": true}}),
        );
        let again = read(&mut theirs);
        assert_eq!(again["method"], "events.watch");
        assert_eq!(again["params"]["delays"], false);
        assert_ne!(again["id"], first["id"]);
        assert_eq!(resyncs.load(Ordering::SeqCst), 1);
    }
}
