//! SeasonCat's Slint UI. It makes no JNI or Android calls of its own: everything Android
//! (VPN consent, the VPN service, the Core session) goes through the Kotlin shell's local
//! socket, see `bridge`.

pub mod bridge;
pub mod dashboard;
pub mod profile;
pub mod proxies;
pub mod store;
pub mod theme;

slint::include_modules!();

use bridge::Bridge;
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use store::Store;

const CALL: Duration = Duration::from_secs(30);
const APPLY: Duration = Duration::from_secs(120);
const MAX_LOGS: usize = 500;
const DELAY_WORKERS: usize = 8;
// The proxies list's measures, as app.slint lays it out.
const PROXY_PAD: f32 = 8.0;
const PROXY_GAP: f32 = 8.0;
const PROXY_HEADER: f32 = 64.0;
const PROXY_GROUP_GAP: f32 = 10.0;
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    let files = app
        .internal_data_path()
        .unwrap_or_else(|| PathBuf::from("/data/data/season.us.ci.cat/files"));
    let crash = files.join("crash.txt");
    let hook_path = crash.clone();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = std::fs::write(&hook_path, format!("native panic: {info}\n{bt}"));
    }));
    let fail = |what: String| {
        let _ = std::fs::write(&crash, what);
    };
    if let Err(e) = slint::android::init(app) {
        fail(format!("slint init failed: {e:?}"));
        return;
    }
    if let Err(e) = run(files) {
        fail(format!("run failed: {e}"));
    }
}

struct App {
    ui: slint::Weak<AppWindow>,
    home: PathBuf,
    store: Mutex<Store>,
    bridge: Mutex<Option<Bridge>>,
    alive: AtomicBool,
    configured: AtomicBool,
    last_apply: Mutex<Option<Value>>,
    run_state: Mutex<String>,
    groups: Mutex<Value>,
    delays: Mutex<HashMap<String, i32>>,
    sorted_delays: Mutex<HashMap<String, i32>>,
    testing_groups: Mutex<HashSet<String>>,
    proxy_query: Mutex<String>,
    traffic_up: Mutex<VecDeque<i64>>,
    traffic_down: Mutex<VecDeque<i64>>,
    peaks: Mutex<(i64, i64)>,
    started_at: Mutex<Option<std::time::Instant>>,
    ip_seq: AtomicU64,
    busy: Mutex<HashSet<String>>,
    pending_logs: Mutex<Vec<LogItem>>,
    logs_scheduled: AtomicBool,
    toast_seq: AtomicU64,
    last_id: Mutex<u64>,
    pending_key: Mutex<Option<PendingKey>>,
    sort_order: Mutex<Vec<String>>,
}

/// Why a profile file could not be taken in.
enum Ingest {
    /// age-encrypted and no key given (`AgeSecretKeyRequiredException`).
    NeedKey,
    Failed(String),
}

impl Ingest {
    fn text(self) -> String {
        match self {
            Ingest::NeedKey => "该配置已用 age 加密，需要 age 私钥才能解密".into(),
            Ingest::Failed(e) => e,
        }
    }
}

/// What to retry once the age key dialog is answered.
enum PendingKey {
    Link { label: String, url: String },
    File { id: String, name: String },
}

/// The edit page's fields.
#[derive(Clone)]
struct EditForm {
    id: String,
    is_url: bool,
    name: String,
    url: String,
    age: String,
    auto: bool,
    interval: String,
    file_changed: bool,
}

fn edit_form(ui: &AppWindow) -> EditForm {
    EditForm {
        id: ui.get_edit_id().into(),
        is_url: ui.get_edit_is_url(),
        name: ui.get_edit_name().into(),
        url: ui.get_edit_url().into(),
        age: ui.get_edit_age().into(),
        auto: ui.get_edit_auto(),
        interval: ui.get_edit_interval().into(),
        file_changed: ui.get_edit_file_changed(),
    }
}

/// `ProfilesCard`: the name, the days left or the source, when it was updated and the
/// usage meter.
fn dash_profile(p: &store::Profile, busy: bool) -> DashProfile {
    let sub = if p.is_url() { p.subscription } else { None };
    let expire = sub.filter(|s| s.expire > 0).map(|s| s.expire);
    let (end, warn) = match expire {
        Some(e) => {
            let days = ((e - now_secs() as i64).max(0) + 86_399) / 86_400;
            (format!("剩余 {days} 天"), days <= 7)
        }
        None => ((if p.is_url() { "远程" } else { "本地" }).to_string(), false),
    };
    let usage = sub.and_then(|s| dashboard::usage(s.upload + s.download, s.total));
    let (percent, used, ratio) = usage.clone().unwrap_or_default();
    DashProfile {
        id: p.id.as_str().into(),
        name: p.label().into(),
        end: end.into(),
        warn,
        updated: if p.updated == 0 { SharedString::new() } else { format!("更新于 {}", ago_short(p.updated)).into() },
        usage: usage.is_some(),
        percent: percent.into(),
        used: used.into(),
        ratio,
        syncable: p.is_url(),
        busy,
    }
}

/// `_ProfileFileItem`'s subtitle and size: when the file changed, and how big it is.
fn file_info(path: &std::path::Path) -> String {
    match std::fs::metadata(path) {
        Ok(m) => {
            let when = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| ago_short(d.as_secs()))
                .unwrap_or_else(|| "未知".into());
            format!("{} · {when}", size(m.len() as i64))
        }
        Err(_) => "未知".into(),
    }
}

fn rgb(v: u32) -> slint::Color {
    slint::Color::from_argb_encoded(0xff00_0000 | v)
}

fn scheme(p: &theme::Palette) -> Scheme {
    Scheme {
        primary: rgb(p.primary),
        on_primary: rgb(p.on_primary),
        primary_container: rgb(p.primary_container),
        on_primary_container: rgb(p.on_primary_container),
        page: rgb(p.page),
        card: rgb(p.card),
        block: rgb(p.block),
        inset: rgb(p.inset),
        on_surface: rgb(p.on_surface),
        on_surface_variant: rgb(p.on_surface_variant),
        outline_variant: rgb(p.outline_variant),
        error: rgb(p.error),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Local wall-clock time of a unix millisecond stamp, through libc so the device's zone applies.
fn clock(ms: i64) -> String {
    let secs = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return String::new();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

/// `LastUpdateTimeText` (compact): empty when never updated.
fn ago(secs: u64) -> String {
    if secs == 0 {
        return String::new();
    }
    ago_short(secs)
}

/// `getLastUpdateTimeDesc`: 刚刚, N 分钟前, N 小时前, N 天前, N 个月前, N 年前.
fn ago_short(secs: u64) -> String {
    let d = now_secs().saturating_sub(secs);
    match d {
        0..=59 => "刚刚".into(),
        60..=3599 => format!("{} 分钟前", d / 60),
        3600..=86_399 => format!("{} 小时前", d / 3600),
        86_400..=2_591_999 => format!("{} 天前", d / 86_400),
        2_592_000..=31_535_999 => format!("{} 个月前", d / 2_592_000),
        _ => format!("{} 年前", d / 31_536_000),
    }
}

/// `DateTime.show`: the local date, yyyy-MM-dd.
fn date(secs: i64) -> String {
    let t = secs as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return String::new();
    }
    format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    )
}

pub fn speed(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    match b {
        b if b < 1024.0 => format!("{} B/s", b as i64),
        b if b < 1024.0 * 1024.0 => format!("{:.1} KB/s", b / 1024.0),
        b if b < 1024.0 * 1024.0 * 1024.0 => format!("{:.1} MB/s", b / 1048576.0),
        b => format!("{:.2} GB/s", b / 1073741824.0),
    }
}

pub fn size(bytes: i64) -> String {
    let s = speed(bytes);
    s.trim_end_matches("/s").to_string()
}

fn profile_ref(path: &std::path::Path) -> Value {
    json!({"path": path.to_string_lossy()})
}

impl App {
    fn ui(&self, f: impl FnOnce(AppWindow) + Send + 'static) {
        let _ = self.ui.upgrade_in_event_loop(f);
    }

    fn bridge(&self) -> Result<Bridge, String> {
        self.bridge
            .lock()
            .unwrap()
            .clone()
            .filter(|b| !b.is_closed())
            .ok_or_else(|| "未连接到内核".to_string())
    }

    fn toast(self: &Arc<Self>, text: impl Into<String>) {
        let text: SharedString = text.into().into();
        let seq = self.toast_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.clone();
        self.ui(move |ui| {
            ui.set_toast(text);
            let weak = ui.as_weak();
            slint::Timer::single_shot(Duration::from_millis(2600), move || {
                if app.toast_seq.load(Ordering::SeqCst) == seq {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_toast("".into());
                    }
                }
            });
        });
    }

    fn spawn(self: &Arc<Self>, f: impl FnOnce(Arc<App>) + Send + 'static) {
        let app = self.clone();
        let _ = std::thread::Builder::new()
            .name("ui-task".into())
            .spawn(move || f(app));
    }

    // ------------------------------------------------------------ connection

    fn connect_loop(self: Arc<Self>) {
        let name = bridge::socket_name(&self.home.to_string_lossy());
        while self.alive.load(Ordering::SeqCst) {
            match Bridge::connect(&name) {
                Ok(bridge) => {
                    if let Err(e) = self.setup(&bridge) {
                        self.toast(format!("内核连接失败: {e}"));
                    }
                    while self.alive.load(Ordering::SeqCst) && !bridge.is_closed() {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    bridge.close();
                    *self.bridge.lock().unwrap() = None;
                    self.configured.store(false, Ordering::SeqCst);
                    self.set_run_state("OFFLINE");
                }
                Err(_) => std::thread::sleep(Duration::from_millis(300)),
            }
        }
    }

    fn set_run_state(self: &Arc<Self>, state: &str) {
        let previous = std::mem::replace(&mut *self.run_state.lock().unwrap(), state.to_string());
        let s: SharedString = state.into();
        self.ui(move |ui| ui.set_run_state(s));
        if previous == state {
            return;
        }
        match state {
            "STARTED" => {
                *self.started_at.lock().unwrap() = Some(std::time::Instant::now());
                self.probe_ip();
            }
            "STOPPED" => {
                *self.started_at.lock().unwrap() = None;
                self.ui(|ui| ui.set_dash_run_time("00:00:00".into()));
                self.probe_ip();
            }
            _ => {}
        }
    }

    fn setup(self: &Arc<Self>, bridge: &Bridge) -> Result<(), String> {
        let app = Arc::downgrade(self);
        bridge.set_shell_listener(Box::new(move |event| {
            let Some(app) = app.upgrade() else { return };
            if event.get("ev").and_then(Value::as_str) == Some("state") {
                if let Some(s) = event.get("state").and_then(Value::as_str) {
                    app.set_run_state(s);
                }
            }
        }));
        let hello = bridge.shell("hello", json!({}), Duration::from_secs(90))?;
        let home = hello
            .get("home")
            .and_then(Value::as_str)
            .map(String::from)
            .unwrap_or_else(|| self.home.to_string_lossy().into_owned());
        let sdk = hello.get("sdkInt").and_then(Value::as_i64);
        let greeting = bridge.call(
            "core.hello",
            json!({"protocol": {"min": 1, "max": 1}, "client": {"name": "seasoncat-ui", "version": VERSION}}),
            CALL,
        )?;
        *self.bridge.lock().unwrap() = Some(bridge.clone());
        let core_version = greeting
            .pointer("/server/version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.ui(move |ui| ui.set_version(format!("UI {VERSION} · Concord {core_version}").into()));
        let engine = greeting
            .pointer("/engine/state")
            .and_then(Value::as_str)
            .unwrap_or("created")
            .to_string();
        if engine == "created" || engine == "failed" {
            bridge.call("core.init", json!({"home": home, "sdkInt": sdk}), CALL)?;
        }
        if let Some(s) = hello.get("state").and_then(Value::as_str) {
            self.set_run_state(s);
        }
        self.probe_ip();
        let traffic = Arc::downgrade(self);
        bridge.watch(
            "traffic.watch",
            json!({"intervalMs": 1000, "onlyProxy": false}),
            Box::new(move |t| {
                let Some(app) = traffic.upgrade() else { return };
                let get = |k: &str| t.get(k).and_then(Value::as_i64).unwrap_or(0);
                app.on_traffic(get("up"), get("down"), get("totalUp"), get("totalDown"));
            }),
        )?;
        let logs = Arc::downgrade(self);
        bridge.watch(
            "log.watch",
            json!({"level": "info"}),
            Box::new(move |entry| {
                if let Some(app) = logs.upgrade() {
                    app.push_log(&entry);
                }
            }),
        )?;
        if engine == "configured" || engine == "running" {
            self.configured.store(true, Ordering::SeqCst);
            self.refresh_groups();
        } else if self.store.lock().unwrap().active().is_some() {
            self.apply_active()?;
        }
        Ok(())
    }

    fn push_log(self: &Arc<Self>, entry: &Value) {
        let item = LogItem {
            level: entry
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            time: clock(entry.get("time").and_then(Value::as_i64).unwrap_or(0)).into(),
            text: entry
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
        };
        {
            let mut pending = self.pending_logs.lock().unwrap();
            pending.push(item);
            if pending.len() > MAX_LOGS {
                let extra = pending.len() - MAX_LOGS;
                pending.drain(..extra);
            }
        }
        if self.logs_scheduled.swap(true, Ordering::SeqCst) {
            return;
        }
        let app = self.clone();
        // One UI update per burst: the Core may log hundreds of lines a second.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            app.logs_scheduled.store(false, Ordering::SeqCst);
            let batch: Vec<LogItem> = std::mem::take(&mut *app.pending_logs.lock().unwrap());
            app.ui(move |ui| {
                let model = ui.get_logs();
                if let Some(vec) = model.as_any().downcast_ref::<VecModel<LogItem>>() {
                    for item in batch {
                        vec.push(item);
                    }
                    let extra = vec.row_count().saturating_sub(MAX_LOGS);
                    for _ in 0..extra {
                        vec.remove(0);
                    }
                }
            });
        });
    }

    // ------------------------------------------------------------ config

    fn apply_active(self: &Arc<Self>) -> Result<(), String> {
        let bridge = self.bridge()?;
        let (path, selection, mode) = {
            let store = self.store.lock().unwrap();
            let Some(p) = store.active() else {
                return Err("添加一个配置文件后即可开始使用".into());
            };
            (
                store.profile_path(&p.id),
                p.selection.clone(),
                store.state.settings.mode,
            )
        };
        let inspect = bridge.call(
            "profile.inspect",
            json!({"profile": profile_ref(&path)}),
            CALL,
        )?;
        let overlay = profile::overlay(
            inspect.get("config").unwrap_or(&Value::Null),
            store::mode_name(mode),
        );
        let params = json!({
            "profile": profile_ref(&path),
            "overlay": overlay,
            "selection": selection,
            "options": profile::options(),
        });
        bridge.call("config.apply", params.clone(), APPLY)?;
        *self.last_apply.lock().unwrap() = Some(params);
        self.configured.store(true, Ordering::SeqCst);
        self.delays.lock().unwrap().clear();
        self.sorted_delays.lock().unwrap().clear();
        self.refresh_groups();
        self.probe_ip();
        Ok(())
    }

    // ------------------------------------------------------------ dashboard
    // lib/views/dashboard.

    fn on_traffic(self: &Arc<Self>, up: i64, down: i64, total_up: i64, total_down: i64) {
        let push = |q: &Mutex<VecDeque<i64>>, v: i64| {
            let mut q = q.lock().unwrap();
            q.push_back(v);
            while q.len() > dashboard::HISTORY {
                q.pop_front();
            }
            dashboard::sparkline(&q)
        };
        let (up_line, up_fill) = push(&self.traffic_up, up);
        let (down_line, down_fill) = push(&self.traffic_down, down);
        let (up_peak, down_peak) = {
            let mut p = self.peaks.lock().unwrap();
            p.0 = p.0.max(up);
            p.1 = p.1.max(down);
            *p
        };
        let (uv, uu) = dashboard::split_unit(&speed(up));
        let (dv, du) = dashboard::split_unit(&speed(down));
        let traffic = DashTraffic {
            up_value: uv.into(),
            up_unit: uu.into(),
            up_total: format!("累计 {}", size(total_up)).into(),
            up_peak: format!("峰值 {}", speed(up_peak)).into(),
            up_line: up_line.into(),
            up_fill: up_fill.into(),
            down_value: dv.into(),
            down_unit: du.into(),
            down_total: format!("累计 {}", size(total_down)).into(),
            down_peak: format!("峰值 {}", speed(down_peak)).into(),
            down_line: down_line.into(),
            down_fill: down_fill.into(),
        };
        let run = self.started_at.lock().unwrap().map(|t| dashboard::run_time(t.elapsed().as_secs()));
        let memory = dashboard::resident_memory().map(size).unwrap_or_default();
        let (us, ds) = (speed(up), speed(down));
        self.ui(move |ui| {
            ui.set_up_speed(us.into());
            ui.set_down_speed(ds.into());
            ui.set_dash_traffic(traffic);
            if let Some(run) = run {
                ui.set_dash_run_time(run.into());
            }
            ui.set_dash_memory(memory.into());
        });
    }

    /// `OutboundIpProbe`: where traffic leaves right now, through the rules while running
    /// and directly otherwise, then what kind of address it is.
    fn probe_ip(self: &Arc<Self>) {
        let seq = self.ip_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let proxy = if self.run_state.lock().unwrap().as_str() == "STARTED" { "" } else { "DIRECT" };
        self.ui(|ui| {
            let mut s = ui.get_dash_status();
            s.probing = true;
            ui.set_dash_status(s);
        });
        self.spawn(move |app| {
            let result = app.bridge().and_then(|b| {
                b.call(
                    "probe.outboundIp",
                    json!({"proxy": proxy, "urls": dashboard::IP_SOURCES, "timeout": 6000}),
                    Duration::from_secs(12),
                )
            });
            let found = result.ok().and_then(|r| {
                let url = r.get("url").and_then(Value::as_str).unwrap_or("");
                let body = r.get("body").and_then(Value::as_str).unwrap_or("");
                dashboard::parse_ip(url, body)
            });
            if app.ip_seq.load(Ordering::SeqCst) != seq {
                return;
            }
            let Some((ip, cc)) = found else {
                app.ui(|ui| {
                    ui.set_dash_status(DashStatus { ip: "获取失败".into(), ..Default::default() });
                });
                return;
            };
            let (ip2, cc2) = (ip.clone(), cc.clone());
            app.ui(move |ui| {
                ui.set_dash_status(DashStatus { ip: ip2.into(), country: cc2.into(), probing: true, ..Default::default() });
            });
            let quality = dashboard::lookup_quality(&ip);
            if app.ip_seq.load(Ordering::SeqCst) != seq {
                return;
            }
            app.ui(move |ui| {
                let (org, kind, level) = quality.unwrap_or_else(|| (String::new(), "类型未知".into(), 0));
                ui.set_dash_status(DashStatus {
                    ip: ip.into(),
                    country: cc.into(),
                    kind: kind.into(),
                    quality: level,
                    org: org.into(),
                    probing: false,
                });
            });
        });
    }

    fn publish_dashboard_widgets(self: &Arc<Self>) {
        let list = self.store.lock().unwrap().state.settings.dashboard.clone().unwrap_or_else(dashboard::default_widgets);
        let hidden: Vec<SharedString> = dashboard::WIDGETS.iter().filter(|w| !list.iter().any(|l| l == *w)).map(|w| (*w).into()).collect();
        let list: Vec<SharedString> = list.into_iter().map(Into::into).collect();
        let ip = dashboard::local_ip().unwrap_or_else(|| "未知".into());
        let has_start = list.iter().any(|w| w == "status" || w == "vpn");
        self.ui(move |ui| {
            ui.set_dash_has_start(has_start);
            ui.set_dash_widgets(ModelRc::new(VecModel::from(list)));
            ui.set_dash_hidden(ModelRc::new(VecModel::from(hidden)));
            ui.set_dash_local_ip(ip.into());
        });
    }

    /// The dashboard editor: add, remove or move a card.
    fn edit_dashboard(self: &Arc<Self>, action: &str, widget: &str) {
        {
            let mut store = self.store.lock().unwrap();
            let mut list = store.state.settings.dashboard.clone().unwrap_or_else(dashboard::default_widgets);
            let at = list.iter().position(|w| w == widget);
            match (action, at) {
                ("add", None) if dashboard::WIDGETS.contains(&widget) => list.push(widget.to_string()),
                ("remove", Some(i)) => {
                    list.remove(i);
                }
                ("up", Some(i)) if i > 0 => list.swap(i, i - 1),
                ("down", Some(i)) if i + 1 < list.len() => list.swap(i, i + 1),
                ("reset", _) => list = dashboard::default_widgets(),
                _ => return,
            }
            store.state.settings.dashboard = Some(list);
            store.save();
        }
        self.publish_dashboard_widgets();
    }

    fn toggle_vpn(self: &Arc<Self>) {
        let state = self.run_state.lock().unwrap().clone();
        self.spawn(move |app| {
            let result = (|| -> Result<(), String> {
                let bridge = app.bridge()?;
                if state == "STARTED" {
                    bridge.shell("vpn.stop", json!({}), APPLY)?;
                    return Ok(());
                }
                let name = match app.store.lock().unwrap().active() {
                    Some(p) => p.name.clone(),
                    None => return Err("添加一个配置文件后即可开始使用".into()),
                };
                if !app.configured.load(Ordering::SeqCst) || app.last_apply.lock().unwrap().is_none() {
                    app.apply_active()?;
                }
                let setup = app.last_apply.lock().unwrap().clone().unwrap_or(Value::Null);
                bridge.shell(
                    "vpn.start",
                    json!({"profileName": name, "vpnOptions": profile::vpn_options(), "setupParams": setup}),
                    Duration::from_secs(300),
                )?;
                Ok(())
            })();
            if let Err(e) = result {
                app.toast(e);
            }
        });
    }

    fn set_mode(self: &Arc<Self>, mode: i32) {
        {
            let mut store = self.store.lock().unwrap();
            store.state.settings.mode = mode;
            store.save();
        }
        self.ui(move |ui| ui.set_mode(mode));
        if !self.configured.load(Ordering::SeqCst) {
            return;
        }
        self.spawn(move |app| {
            let r = app.bridge().and_then(|b| {
                b.call(
                    "config.patch",
                    json!({"mode": store::mode_name(mode)}),
                    CALL,
                )
            });
            if let Some(params) = app.last_apply.lock().unwrap().as_mut() {
                params["overlay"]["mode"] = store::mode_name(mode).into();
            }
            match r {
                Ok(_) => {
                    app.refresh_groups();
                    app.probe_ip();
                }
                Err(e) => app.toast(e),
            }
        });
    }

    // ------------------------------------------------------------ profiles
    // lib/views/profiles and lib/providers/actions/profiles.dart.

    fn profile_item(
        &self,
        store: &Store,
        p: &store::Profile,
        busy: &HashSet<String>,
    ) -> ProfileItem {
        let sub = if p.is_url() { p.subscription } else { None };
        let subtitle = match sub {
            Some(s) if s.total > 0 => {
                format!("{} / {}", size(s.upload + s.download), size(s.total))
            }
            _ if p.is_url() => "远程".into(),
            _ => "本地".into(),
        };
        let expire = sub.filter(|s| s.expire > 0).map(|s| s.expire);
        ProfileItem {
            id: p.id.as_str().into(),
            name: p.label().into(),
            subtitle: subtitle.into(),
            trailing: ago(p.updated).into(),
            expire: expire.map(date).unwrap_or_default().into(),
            expired: expire.is_some_and(|e| (e as u64) < now_secs()),
            active: store.state.active.as_deref() == Some(p.id.as_str()),
            busy: busy.contains(&p.id),
            is_url: p.is_url(),
        }
    }

    fn refresh_profiles(&self) {
        let (items, name, dash) = {
            let store = self.store.lock().unwrap();
            let busy = self.busy.lock().unwrap();
            let items: Vec<ProfileItem> = store
                .state
                .profiles
                .iter()
                .map(|p| self.profile_item(&store, p, &busy))
                .collect();
            let dash = store.active().map(|p| dash_profile(p, busy.contains(&p.id))).unwrap_or_default();
            (items, store.active().map(|p| p.label()).unwrap_or_default(), dash)
        };
        let count = items.len() as i32;
        self.ui(move |ui| {
            ui.set_profiles(ModelRc::new(VecModel::from(items)));
            ui.set_profile_name(name.into());
            ui.set_dash_profile(dash);
            ui.set_profile_count(count);
        });
    }

    fn alert(self: &Arc<Self>, title: impl Into<String>, text: impl Into<String>) {
        let (title, text): (SharedString, SharedString) = (title.into().into(), text.into().into());
        self.ui(move |ui| {
            ui.set_alert_title(title);
            ui.set_alert_text(text);
            ui.set_alert_open(true);
        });
    }

    fn confirm(self: &Arc<Self>, title: &str, text: String, action: &str, arg: String) {
        let (title, text, action): (SharedString, SharedString, SharedString) =
            (title.into(), text.into(), action.into());
        let arg: SharedString = arg.into();
        self.ui(move |ui| {
            ui.set_confirm_title(title);
            ui.set_confirm_text(text);
            ui.set_confirm_action(action);
            ui.set_confirm_arg(arg);
            ui.set_confirm_open(true);
        });
    }

    /// `showFailures`: one line per profile that failed.
    fn show_failures(self: &Arc<Self>, failures: Vec<(String, String)>) {
        if failures.is_empty() {
            return;
        }
        let text = failures
            .iter()
            .map(|(label, why)| format!("{label}: {why}"))
            .collect::<Vec<_>>()
            .join("\n");
        self.alert("提示", text);
    }

    fn temp_for(&self, key: &str) -> PathBuf {
        self.store
            .lock()
            .unwrap()
            .dir
            .join(format!("{key}.part.yaml"))
    }

    fn first_issue(bridge: &Bridge, path: &std::path::Path) -> Result<Option<String>, String> {
        let r = bridge.call(
            "config.validate",
            json!({"profile": profile_ref(path)}),
            CALL,
        )?;
        Ok(r.pointer("/issues/0/message")
            .and_then(Value::as_str)
            .map(String::from))
    }

    /// `validateProfile` in lib/core/controller.dart, on the file at `path`: an age-encrypted
    /// file is decrypted in place, and a file the Core cannot load as a config is tried as a
    /// share-link list and rewritten into a config of its proxies.
    fn ingest(&self, path: &std::path::Path, age_key: &str) -> Result<(), Ingest> {
        let bridge = self.bridge().map_err(Ingest::Failed)?;
        let encrypted = std::fs::File::open(path)
            .and_then(|mut f| {
                let mut head = vec![0u8; profile::AGE_ARMOR_HEADER.len()];
                std::io::Read::read_exact(&mut f, &mut head).map(|_| head)
            })
            .is_ok_and(|head| head == profile::AGE_ARMOR_HEADER);
        if encrypted {
            if age_key.trim().is_empty() {
                return Err(Ingest::NeedKey);
            }
            let plain = bridge
                .call(
                    "config.decrypt",
                    json!({"profile": profile_ref(path), "key": age_key.trim()}),
                    CALL,
                )
                .map_err(Ingest::Failed)?;
            let text = plain.get("text").and_then(Value::as_str).unwrap_or("");
            std::fs::write(path, text).map_err(|e| Ingest::Failed(e.to_string()))?;
        }
        let Some(issue) = Self::first_issue(&bridge, path).map_err(Ingest::Failed)? else {
            return Ok(());
        };
        let converted = bridge
            .call(
                "convert.subscription",
                json!({"profile": profile_ref(path)}),
                CALL,
            )
            .ok()
            .and_then(|r| r.get("proxies").cloned())
            .filter(|p| p.as_array().is_some_and(|a| !a.is_empty()));
        let Some(proxies) = converted else {
            return Err(Ingest::Failed(issue));
        };
        // JSON is YAML, so the Core reads this as a config.
        std::fs::write(
            path,
            serde_json::to_vec(&json!({"proxies": proxies})).unwrap(),
        )
        .map_err(|e| Ingest::Failed(e.to_string()))?;
        match Self::first_issue(&bridge, path).map_err(Ingest::Failed)? {
            Some(issue) => Err(Ingest::Failed(issue)),
            None => Ok(()),
        }
    }

    /// `Profile.saveFile`: the bytes at `tmp` replace the profile's file once they pass.
    fn save_file(&self, tmp: &std::path::Path, id: &str, age_key: &str) -> Result<(), Ingest> {
        let result = self.ingest(tmp, age_key);
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
            return result;
        }
        let dest = self.store.lock().unwrap().profile_path(id);
        std::fs::rename(tmp, &dest).map_err(|e| Ingest::Failed(e.to_string()))
    }

    /// `Profile.update`: downloads the url into the profile's file. Returns the
    /// content-disposition name and the subscription info.
    fn download_into(
        &self,
        id: &str,
        url: &str,
        age_key: &str,
    ) -> Result<(Option<String>, store::Subscription), Ingest> {
        let download = profile::download(url).map_err(Ingest::Failed)?;
        let tmp = self.temp_for(id);
        std::fs::write(&tmp, &download.bytes).map_err(|e| Ingest::Failed(e.to_string()))?;
        self.save_file(&tmp, id, age_key)?;
        Ok((
            download.name,
            store::Subscription::parse(download.userinfo.as_deref().unwrap_or("")),
        ))
    }

    fn new_id(&self) -> String {
        let mut last = self.last_id.lock().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        *last = (*last + 1).max(now);
        last.to_string()
    }

    /// `putProfile`: added at the end; the first profile becomes the current one.
    fn put_new(self: &Arc<Self>, p: store::Profile) {
        let first = {
            let mut store = self.store.lock().unwrap();
            let first = store.state.active.is_none();
            if first {
                store.state.active = Some(p.id.clone());
            }
            store.state.profiles.push(p);
            store.save();
            first
        };
        self.refresh_profiles();
        if first {
            if let Err(e) = self.apply_active() {
                self.toast(e);
            }
        }
    }

    /// `_fromShareLinks`: the links' proxies as one local profile.
    fn from_share_links(self: &Arc<Self>, label: String, links: Vec<String>) -> Result<(), String> {
        let bridge = self.bridge()?;
        let decoded = bridge.call("convert.decodeLinks", json!({"lines": links}), CALL)?;
        let results = decoded
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (proxies, default_label) = profile::share_link_proxies(&links, &results)?;
        let id = self.new_id();
        let tmp = self.temp_for(&id);
        std::fs::write(
            &tmp,
            serde_json::to_vec(&json!({"proxies": proxies})).unwrap(),
        )
        .map_err(|e| e.to_string())?;
        self.save_file(&tmp, &id, "").map_err(Ingest::text)?;
        self.put_new(store::Profile {
            id,
            name: if label.trim().is_empty() {
                default_label
            } else {
                label
            },
            kind: "file".into(),
            updated: now_secs(),
            auto_update: false,
            ..Default::default()
        });
        Ok(())
    }

    /// `addProfileFromLink`, with `_withAgeSecretKey`: an encrypted download without a key
    /// asks for one and tries again.
    fn add_from_link(
        self: &Arc<Self>,
        label: String,
        url: String,
        age_key: String,
    ) -> Result<(), String> {
        if profile::is_share_link(&url) {
            return self.from_share_links(label, vec![url]);
        }
        let id = self.new_id();
        match self.download_into(&id, &url, &age_key) {
            Ok((name, sub)) => {
                self.put_new(store::Profile {
                    id,
                    name: if label.trim().is_empty() {
                        name.unwrap_or_default()
                    } else {
                        label
                    },
                    url,
                    kind: "url".into(),
                    updated: now_secs(),
                    age_key,
                    subscription: Some(sub),
                    ..Default::default()
                });
                Ok(())
            }
            Err(Ingest::NeedKey) => {
                *self.pending_key.lock().unwrap() = Some(PendingKey::Link { label, url });
                self.ui(|ui| ui.set_age_open(true));
                Ok(())
            }
            Err(Ingest::Failed(e)) => Err(e),
        }
    }

    fn import_links(self: &Arc<Self>, label: String, text: String, age_key: String, batch: bool) {
        let lines: Vec<String> = text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if lines.is_empty() {
            self.toast("链接不能为空");
            return;
        }
        if let Some(bad) = lines
            .iter()
            .find(|l| !profile::is_url(l) && !profile::is_share_link(l))
        {
            self.toast(if batch {
                format!("{bad}: 不是有效的订阅链接或代理分享链接")
            } else {
                "不是有效的订阅链接或代理分享链接".into()
            });
            return;
        }
        self.ui(|ui| {
            ui.set_adding(true);
            ui.invoke_links_done();
        });
        self.spawn(move |app| {
            if !batch {
                let url = lines.into_iter().next().unwrap_or_default();
                if let Err(e) = app.add_from_link(label, url, age_key) {
                    app.alert("添加配置", e);
                }
                app.ui(|ui| ui.set_adding(false));
                return;
            }
            // `addProfilesFromLinks`: the urls already listed are skipped, the share links
            // make up one profile in the place of the first.
            let existing: HashSet<String> = app
                .store
                .lock()
                .unwrap()
                .state
                .profiles
                .iter()
                .map(|p| p.url.clone())
                .collect();
            let mut seen = HashSet::new();
            let lines: Vec<String> = lines
                .into_iter()
                .filter(|l| !existing.contains(l) && seen.insert(l.clone()))
                .collect();
            let shares: Vec<String> = lines
                .iter()
                .filter(|l| profile::is_share_link(l))
                .cloned()
                .collect();
            let mut failures = Vec::new();
            for line in &lines {
                let result = if profile::is_share_link(line) {
                    if Some(line) != shares.first() {
                        continue;
                    }
                    app.from_share_links(String::new(), shares.clone())
                } else {
                    let id = app.new_id();
                    app.download_into(&id, line, "")
                        .map_err(Ingest::text)
                        .map(|(name, sub)| {
                            app.put_new(store::Profile {
                                id,
                                name: name.unwrap_or_default(),
                                url: line.clone(),
                                kind: "url".into(),
                                updated: now_secs(),
                                subscription: Some(sub),
                                ..Default::default()
                            })
                        })
                };
                if let Err(e) = result {
                    failures.push((line.clone(), e));
                }
            }
            app.ui(|ui| ui.set_adding(false));
            app.show_failures(failures);
        });
    }

    /// `addProfileFormFile`.
    fn add_file(self: &Arc<Self>) {
        self.spawn(|app| {
            let id = app.new_id();
            let tmp = app.temp_for(&id);
            let picked = app.bridge().and_then(|b| {
                b.shell(
                    "file.open",
                    json!({"dest": tmp.to_string_lossy()}),
                    Duration::from_secs(3600),
                )
            });
            let name = match picked {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => return,
                Ok(r) => r
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                Err(e) => return app.toast(e),
            };
            app.ui(|ui| ui.set_adding(true));
            app.finish_file(id, name, String::new());
            app.ui(|ui| ui.set_adding(false));
        });
    }

    fn finish_file(self: &Arc<Self>, id: String, name: String, age_key: String) {
        let tmp = self.temp_for(&id);
        // A failed attempt deletes the temp file; keep a copy for the age-key retry.
        let keep = self.temp_for(&format!("{id}.src"));
        if age_key.is_empty() {
            let _ = std::fs::copy(&tmp, &keep);
        } else {
            let _ = std::fs::copy(&keep, &tmp);
        }
        match self.save_file(&tmp, &id, &age_key) {
            Ok(()) => {
                let _ = std::fs::remove_file(&keep);
                self.put_new(store::Profile {
                    id,
                    name,
                    kind: "file".into(),
                    updated: now_secs(),
                    auto_update: false,
                    age_key,
                    ..Default::default()
                });
            }
            Err(Ingest::NeedKey) => {
                *self.pending_key.lock().unwrap() = Some(PendingKey::File { id, name });
                self.ui(|ui| ui.set_age_open(true));
            }
            Err(Ingest::Failed(e)) => {
                let _ = std::fs::remove_file(&keep);
                self.alert("添加配置", e);
            }
        }
    }

    fn age_key_given(self: &Arc<Self>, key: String) {
        let Some(pending) = self.pending_key.lock().unwrap().take() else {
            return;
        };
        let key = key.trim().to_string();
        if key.is_empty() {
            if let PendingKey::File { id, .. } = &pending {
                let _ = std::fs::remove_file(self.temp_for(&format!("{id}.src")));
            }
            self.toast("该配置已用 age 加密，需要 age 私钥才能解密");
            return;
        }
        self.ui(|ui| ui.set_adding(true));
        self.spawn(move |app| {
            match pending {
                PendingKey::Link { label, url } => {
                    if let Err(e) = app.add_from_link(label, url, key) {
                        app.alert("添加配置", e);
                    }
                }
                PendingKey::File { id, name } => app.finish_file(id, name, key),
            }
            app.ui(|ui| ui.set_adding(false));
        });
    }

    /// `updateProfile`: edits and deletes made while it downloads win.
    fn update_one(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let Some((url, age_key)) = ({
            let store = self.store.lock().unwrap();
            store
                .state
                .profiles
                .iter()
                .find(|p| p.id == id && p.is_url())
                .map(|p| (p.url.clone(), p.age_key.clone()))
        }) else {
            return Ok(());
        };
        if !self.busy.lock().unwrap().insert(id.to_string()) {
            return Ok(());
        }
        self.refresh_profiles();
        let result = self.download_into(id, &url, &age_key);
        self.busy.lock().unwrap().remove(id);
        let outcome = match result {
            Ok((name, sub)) => {
                let active = {
                    let mut store = self.store.lock().unwrap();
                    if let Some(p) = store.profile_mut(id) {
                        p.updated = now_secs();
                        p.subscription = Some(sub);
                        if p.name.trim().is_empty() {
                            p.name = name.unwrap_or_default();
                        }
                    }
                    store.save();
                    store.state.active.as_deref() == Some(id)
                };
                if active {
                    if let Err(e) = self.apply_active() {
                        self.toast(e);
                    }
                }
                Ok(())
            }
            Err(e) => Err(e.text()),
        };
        self.refresh_profiles();
        outcome
    }

    fn update_profile(self: &Arc<Self>, id: String) {
        self.spawn(move |app| {
            let label = app.label_of(&id);
            if let Err(e) = app.update_one(&id) {
                app.show_failures(vec![(label, e)]);
            }
        });
    }

    fn label_of(&self, id: &str) -> String {
        let store = self.store.lock().unwrap();
        store
            .state
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.label())
            .unwrap_or_default()
    }

    /// The app bar's update: every remote profile at once, failures shown together.
    fn update_all(self: &Arc<Self>) {
        let ids: Vec<String> = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .filter(|p| p.is_url())
            .map(|p| p.id.clone())
            .collect();
        let failures = Arc::new(Mutex::new(Vec::new()));
        let left = Arc::new(AtomicU64::new(ids.len() as u64));
        for id in ids {
            let (failures, left) = (failures.clone(), left.clone());
            self.spawn(move |app| {
                if let Err(e) = app.update_one(&id) {
                    failures.lock().unwrap().push((app.label_of(&id), e));
                }
                if left.fetch_sub(1, Ordering::SeqCst) == 1 {
                    let list = std::mem::take(&mut *failures.lock().unwrap());
                    app.show_failures(list);
                }
            });
        }
    }

    /// `autoUpdateProfiles`, checked every minute.
    fn auto_update_loop(self: Arc<Self>) {
        // A failed profile waits its interval before the next try, not a minute.
        let mut failed: HashMap<String, u64> = HashMap::new();
        while self.alive.load(Ordering::SeqCst) {
            if self.bridge().is_ok() {
                let now = now_secs();
                let due: Vec<(String, u64)> = self
                    .store
                    .lock()
                    .unwrap()
                    .state
                    .profiles
                    .iter()
                    .filter(|p| p.due(now))
                    .map(|p| (p.id.clone(), p.interval_minutes.max(1) * 60))
                    .collect();
                for (id, interval) in due {
                    if failed.get(&id).is_some_and(|at| at + interval > now) {
                        continue;
                    }
                    match self.update_one(&id) {
                        Ok(()) => {
                            failed.remove(&id);
                        }
                        Err(e) => {
                            failed.insert(id.clone(), now);
                            self.push_log(&json!({"level": "warning", "time": now as i64 * 1000, "text": format!("{}: {e}", self.label_of(&id))}));
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    }

    fn select_profile(self: &Arc<Self>, id: String) {
        {
            let mut store = self.store.lock().unwrap();
            if store.state.active.as_deref() == Some(id.as_str()) {
                return;
            }
            store.state.active = Some(id);
            store.save();
        }
        self.refresh_profiles();
        self.spawn(|app| {
            if let Err(e) = app.apply_active() {
                app.toast(e);
            }
        });
    }

    fn ask_delete(self: &Arc<Self>, id: String) {
        self.confirm("提示", "确定删除当前配置吗？".into(), "delete", id);
    }

    fn delete_profile(self: &Arc<Self>, id: String) {
        let was_active = {
            let mut store = self.store.lock().unwrap();
            let was = store.state.active.as_deref() == Some(id.as_str());
            store.remove(&id);
            was && store.active().is_some()
        };
        self.refresh_profiles();
        if was_active {
            self.spawn(|app| {
                if let Err(e) = app.apply_active() {
                    app.toast(e);
                }
            });
        }
    }

    // Edit (lib/views/profiles/edit.dart)

    fn open_edit(self: &Arc<Self>, id: String) {
        let Some(p) = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        let path = self.store.lock().unwrap().profile_path(&id);
        let _ = std::fs::remove_file(self.temp_for("edit"));
        let info = file_info(&path);
        self.ui(move |ui| {
            ui.set_edit_id(p.id.as_str().into());
            ui.set_edit_is_url(p.is_url());
            ui.set_edit_name(p.name.as_str().into());
            ui.set_edit_url(p.url.as_str().into());
            ui.set_edit_age(p.age_key.as_str().into());
            ui.set_edit_auto(p.auto_update);
            ui.set_edit_interval(p.interval_minutes.to_string().into());
            ui.set_edit_file_info(info.into());
            ui.set_edit_file_changed(false);
            ui.set_profiles_page("edit".into());
        });
    }

    /// `_handleConfirm`'s checks, then the prompt for a changed file of an auto-updating
    /// profile.
    fn request_edit_save(self: &Arc<Self>, form: EditForm) {
        if form.name.trim().is_empty() {
            return self.toast("请输入配置名称");
        }
        if form.is_url {
            if form.url.trim().is_empty() {
                return self.toast("请输入配置URL");
            }
            if !profile::is_url(form.url.trim()) {
                return self.toast("请输入有效配置URL");
            }
            if form.auto {
                if form.interval.trim().is_empty() {
                    return self.toast("请输入自动更新间隔时间");
                }
                if form.interval.trim().parse::<u64>().map_or(true, |m| m == 0) {
                    return self.toast("请输入有效间隔时间格式");
                }
            }
        }
        if form.file_changed && form.is_url && form.auto {
            self.confirm(
                "提示",
                "配置文件已经修改，是否关闭自动更新？".into(),
                "edit-save",
                String::new(),
            );
        } else {
            self.save_edit(form, false);
        }
    }

    fn save_edit(self: &Arc<Self>, form: EditForm, auto_off: bool) {
        let id = form.id.clone();
        let Some(current) = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        let mut next = current.clone();
        next.name = form.name.clone();
        next.age_key = form.age.trim().to_string();
        if form.is_url {
            next.url = form.url.trim().to_string();
            next.auto_update = form.auto && !auto_off;
            if let Ok(m) = form.interval.trim().parse::<u64>() {
                if m > 0 {
                    next.interval_minutes = m;
                }
            }
        }
        let has_update = current.url != next.url || current.age_key != next.age_key;
        self.ui(|ui| ui.set_adding(true));
        self.spawn(move |app| {
            if form.file_changed {
                let draft = app.temp_for("edit");
                let tmp = app.temp_for(&id);
                if let Err(e) = std::fs::copy(&draft, &tmp) {
                    app.ui(|ui| ui.set_adding(false));
                    return app.alert("提示", e.to_string());
                }
                match app.save_file(&tmp, &id, &next.age_key) {
                    Ok(()) => next.updated = now_secs(),
                    Err(e) => {
                        app.ui(|ui| ui.set_adding(false));
                        return app.alert(
                            "提示",
                            match e {
                                Ingest::NeedKey => {
                                    "该配置已用 age 加密，需要 age 私钥才能解密".into()
                                }
                                Ingest::Failed(e) => e,
                            },
                        );
                    }
                }
                let _ = std::fs::remove_file(&draft);
            }
            let active = {
                let mut store = app.store.lock().unwrap();
                if let Some(p) = store.profile_mut(&id) {
                    *p = next;
                }
                store.save();
                store.state.active.as_deref() == Some(id.as_str())
            };
            app.refresh_profiles();
            app.ui(|ui| {
                ui.set_adding(false);
                ui.set_profiles_page("".into());
            });
            if !form.file_changed && has_update {
                if let Err(e) = app.update_one(&id) {
                    app.show_failures(vec![(app.label_of(&id), e)]);
                }
            } else if active {
                // EditProfileView.dispose: autoApplyProfile.
                if let Err(e) = app.apply_active() {
                    app.toast(e);
                }
            }
        });
    }

    /// The profile file row's edit: the draft when there is one, else the file.
    fn edit_file(self: &Arc<Self>, id: String, changed: bool) {
        let path = if changed {
            self.temp_for("edit")
        } else {
            self.store.lock().unwrap().profile_path(&id)
        };
        let text = std::fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let title = self.label_of(&id);
        self.ui(move |ui| {
            ui.set_editor_title(title.into());
            ui.set_editor_text(text.into());
            ui.set_editor_readonly(false);
            ui.set_profiles_page("editor".into());
        });
    }

    /// `_handleSaveEdit`: the text must load as a config before it becomes the draft.
    fn save_editor(self: &Arc<Self>, text: String) {
        self.spawn(move |app| {
            let tmp = app.temp_for("editor");
            let result = std::fs::write(&tmp, &text)
                .map_err(|e| e.to_string())
                .and_then(|_| app.bridge())
                .and_then(|b| Self::first_issue(&b, &tmp));
            match result {
                Ok(None) => {
                    let _ = std::fs::rename(&tmp, app.temp_for("edit"));
                    let info = format!("{} · 刚刚", size(text.len() as i64));
                    app.ui(move |ui| {
                        ui.set_edit_file_changed(true);
                        ui.set_edit_file_info(info.into());
                        ui.set_profiles_page("edit".into());
                    });
                }
                Ok(Some(issue)) => app.alert("提示", issue),
                Err(e) => app.alert("提示", e),
            }
        });
    }

    /// The profile file row's upload.
    fn upload_file(self: &Arc<Self>) {
        self.spawn(|app| {
            let draft = app.temp_for("edit");
            let picked = app.bridge().and_then(|b| {
                b.shell(
                    "file.open",
                    json!({"dest": draft.to_string_lossy()}),
                    Duration::from_secs(3600),
                )
            });
            match picked {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => {}
                Ok(_) => {
                    let info = format!(
                        "{} · 刚刚",
                        size(
                            std::fs::metadata(&draft)
                                .map(|m| m.len() as i64)
                                .unwrap_or(0)
                        )
                    );
                    app.ui(move |ui| {
                        ui.set_edit_file_changed(true);
                        ui.set_edit_file_info(info.into());
                    });
                }
                Err(e) => app.toast(e),
            }
        });
    }

    /// `PreviewProfileView`: the config as it would be applied, read only.
    fn preview(self: &Arc<Self>, id: String) {
        self.spawn(move |app| {
            let path = app.store.lock().unwrap().profile_path(&id);
            let mode = app.store.lock().unwrap().state.settings.mode;
            let result = app.bridge().and_then(|b| {
                let inspect = b.call(
                    "profile.inspect",
                    json!({"profile": profile_ref(&path)}),
                    CALL,
                )?;
                let overlay = profile::overlay(
                    inspect.get("config").unwrap_or(&Value::Null),
                    store::mode_name(mode),
                );
                b.call(
                    "profile.build",
                    json!({"profile": profile_ref(&path), "overlay": overlay}),
                    CALL,
                )
            });
            match result {
                Ok(r) => {
                    let text = r
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let title = app.label_of(&id);
                    app.ui(move |ui| {
                        ui.set_editor_title(title.into());
                        ui.set_editor_text(text.into());
                        ui.set_editor_readonly(true);
                        ui.set_profiles_page("preview".into());
                    });
                }
                Err(e) => app.toast(e),
            }
        });
    }

    fn copy_link(self: &Arc<Self>, id: String) {
        let url = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.url.clone())
            .unwrap_or_default();
        self.spawn(move |app| {
            match app
                .bridge()
                .and_then(|b| b.shell("clipboard.set", json!({"text": url}), CALL))
            {
                Ok(_) => app.toast("复制成功"),
                Err(e) => app.toast(e),
            }
        });
    }

    fn show_qr(self: &Arc<Self>, id: String) {
        let Some(p) = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        let Ok(code) = qrcode::QrCode::new(p.url.as_bytes()) else {
            return self.alert("二维码", "链接过长，无法生成二维码，请改用“复制链接”。");
        };
        let modules = code.width();
        let quiet = 2;
        let scale = 8;
        let side = (modules + quiet * 2) * scale;
        let mut buffer =
            slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(side as u32, side as u32);
        let colors = code.to_colors();
        for (i, px) in buffer.make_mut_slice().iter_mut().enumerate() {
            let (x, y) = (i % side / scale, i / side / scale);
            let dark = x >= quiet
                && y >= quiet
                && x < modules + quiet
                && y < modules + quiet
                && colors[(y - quiet) * modules + (x - quiet)] == qrcode::Color::Dark;
            let v = if dark { 0 } else { 255 };
            *px = slint::Rgba8Pixel {
                r: v,
                g: v,
                b: v,
                a: 255,
            };
        }
        let title = p.label();
        self.ui(move |ui| {
            ui.set_qr_title(title.into());
            ui.set_qr_image(slint::Image::from_rgba8(buffer));
            ui.set_qr_open(true);
        });
    }

    fn export_file(self: &Arc<Self>, id: String) {
        let Some((path, name)) = ({
            let store = self.store.lock().unwrap();
            store
                .state
                .profiles
                .iter()
                .find(|p| p.id == id)
                .map(|p| (store.profile_path(&id), p.export_name()))
        }) else {
            return;
        };
        self.spawn(move |app| {
            match app.bridge().and_then(|b| {
                b.shell(
                    "file.save",
                    json!({"src": path.to_string_lossy(), "name": name}),
                    Duration::from_secs(3600),
                )
            }) {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => {}
                Ok(_) => app.toast("导出成功"),
                Err(e) => app.alert("提示", e),
            }
        });
    }

    // Sort (ReorderableProfilesSheet)

    fn open_sort(self: &Arc<Self>) {
        let ids: Vec<String> = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .map(|p| p.id.clone())
            .collect();
        *self.sort_order.lock().unwrap() = ids;
        self.publish_sort();
        self.ui(|ui| ui.set_profiles_page("sort".into()));
    }

    fn publish_sort(self: &Arc<Self>) {
        let items: Vec<ProfileItem> = {
            let store = self.store.lock().unwrap();
            let busy = self.busy.lock().unwrap();
            self.sort_order
                .lock()
                .unwrap()
                .iter()
                .filter_map(|id| store.state.profiles.iter().find(|p| &p.id == id))
                .map(|p| self.profile_item(&store, p, &busy))
                .collect()
        };
        self.ui(move |ui| ui.set_sort_items(ModelRc::new(VecModel::from(items))));
    }

    fn sort_move(self: &Arc<Self>, index: i32, delta: i32) {
        {
            let mut order = self.sort_order.lock().unwrap();
            let (i, j) = (index, index + delta);
            if i < 0 || j < 0 || i as usize >= order.len() || j as usize >= order.len() {
                return;
            }
            order.swap(i as usize, j as usize);
        }
        self.publish_sort();
    }

    /// `_handleSave`: profiles added meanwhile keep their place at the end.
    fn sort_save(self: &Arc<Self>) {
        {
            let order = std::mem::take(&mut *self.sort_order.lock().unwrap());
            let mut store = self.store.lock().unwrap();
            let mut rest = std::mem::take(&mut store.state.profiles);
            let mut sorted = Vec::new();
            for id in order {
                if let Some(i) = rest.iter().position(|p| p.id == id) {
                    sorted.push(rest.remove(i));
                }
            }
            sorted.extend(rest);
            store.state.profiles = sorted;
            store.save();
        }
        self.refresh_profiles();
        self.ui(|ui| ui.set_profiles_page("".into()));
    }

    fn confirmed(self: &Arc<Self>, action: String, arg: String, yes: bool, form: EditForm) {
        match (action.as_str(), yes) {
            ("delete", true) => self.delete_profile(arg),
            ("edit-save", _) => self.save_edit(form, yes),
            _ => {}
        }
    }

    // ------------------------------------------------------------ proxies
    // lib/views/proxies and lib/providers/actions/proxies.dart.

    fn refresh_groups(self: &Arc<Self>) {
        let Ok(bridge) = self.bridge() else { return };
        match bridge.call("proxy.groups", json!({}), CALL) {
            Ok(groups) => {
                *self.groups.lock().unwrap() = groups;
                self.publish_groups();
            }
            Err(e) => self.toast(e),
        }
    }

    fn publish_groups(self: &Arc<Self>) {
        let (mode, style) = {
            let store = self.store.lock().unwrap();
            (store.state.settings.mode, store.state.settings.proxies.clone())
        };
        let groups = self.groups.lock().unwrap().clone();
        let delays = self.delays.lock().unwrap().clone();
        let sorted_by = self.sorted_delays.lock().unwrap().clone();
        let testing = self.testing_groups.lock().unwrap().clone();
        let query = self.proxy_query.lock().unwrap().clone();
        let hint = if mode == 2 { "直连模式下不经过代理" } else { "暂无代理" };
        self.ui(move |ui| {
            let groups = proxies::Groups::parse(&groups);
            let list = proxies::View {
                groups: &groups,
                mode,
                delays: &delays,
                sorted_by: &sorted_by,
                style: &style,
                query: &query,
                testing: &testing,
            }
            .build();
            // The tab style shows one group at a time: keep the open group while it is
            // listed, else open the first.
            let mut current = ui.get_current_group().to_string();
            if !list.iter().any(|g| g.name.as_str() == current) {
                current = list.first().map(|g| g.name.to_string()).unwrap_or_default();
                ui.set_current_group(current.clone().into());
            }
            ui.set_testing(testing.contains(&current));
            let dash: Vec<ProxyGroup> = groups
                .visible(mode)
                .iter()
                .map(|g| ProxyGroup {
                    name: g.name.as_str().into(),
                    kind: g.kind.as_str().into(),
                    now: g.now.as_str().into(),
                    count: g.all.len() as i32,
                    monogram: g.name.chars().next().map(String::from).unwrap_or_default().into(),
                    expanded: false,
                    testing: false,
                    nodes: ModelRc::default(),
                })
                .collect();
            ui.set_dash_groups(ModelRc::new(VecModel::from(dash)));
            ui.set_proxies_hint(hint.into());
            ui.set_proxy_groups(ModelRc::new(VecModel::from(list)));
            ui.set_proxy_style(ProxyStyle {
                kind: style.kind,
                sort: style.sort,
                layout: style.layout,
                card: style.card,
                icon: style.icon,
                hide_timeout: style.hide_timeout,
            });
        });
    }

    /// `selectGroupProxy`: a selector takes the tapped proxy; a group that picks by itself
    /// takes it as a pin, and a tap on the pin lifts it.
    fn select_node(self: &Arc<Self>, group: String, node: String) {
        let groups = proxies::Groups::parse(&self.groups.lock().unwrap());
        let Some(g) = groups.groups.get(&group) else { return };
        if !g.selectable() {
            return self.toast("当前代理组无法选中");
        }
        let next = if g.computed() && g.fixed == node { String::new() } else { node };
        if !g.computed() && g.now == next {
            return;
        }
        self.spawn(move |app| {
            let r = app
                .bridge()
                .and_then(|b| b.call("proxy.select", json!({"group": group, "proxy": next}), CALL));
            match r {
                Ok(r) => {
                    {
                        let mut store = app.store.lock().unwrap();
                        if let Some(id) = store.state.active.clone() {
                            if let Some(p) = store.profile_mut(&id) {
                                p.selection.insert(group.clone(), next.clone());
                            }
                            store.save();
                        }
                    }
                    if let Some(params) = app.last_apply.lock().unwrap().as_mut() {
                        params["selection"][group.as_str()] = next.into();
                    }
                    // `closeConnections` is on by default: the old picks' connections go.
                    if r.get("changed").and_then(Value::as_bool).unwrap_or(true) {
                        if let Ok(b) = app.bridge() {
                            let _ = b.call("connection.close", json!({"all": true}), CALL);
                        }
                    }
                    app.refresh_groups();
                    app.probe_ip();
                }
                Err(_) => app.toast("切换代理失败，已恢复上一次的选择"),
            }
        });
    }

    /// `delayTestPageGroup`: the group's nodes the search shows, each measured once as
    /// the proxy it stands for.
    fn test_group(self: &Arc<Self>, group: String) {
        if !self.testing_groups.lock().unwrap().insert(group.clone()) {
            return;
        }
        let (targets, url) = {
            let groups = proxies::Groups::parse(&self.groups.lock().unwrap());
            let query = self.proxy_query.lock().unwrap().clone();
            (proxies::targets(&groups, &group, &query), groups.test_url(&group))
        };
        self.run_delays(targets, url, Some(group));
    }

    /// `proxyDelayTest`: the one card's delay.
    fn test_node(self: &Arc<Self>, group: String, node: String) {
        let (target, url) = {
            let groups = proxies::Groups::parse(&self.groups.lock().unwrap());
            (groups.real(&node), groups.test_url(&group))
        };
        if target.is_empty() {
            return;
        }
        self.run_delays(vec![target], url, None);
    }

    fn run_delays(self: &Arc<Self>, names: Vec<String>, url: String, group: Option<String>) {
        {
            let mut delays = self.delays.lock().unwrap();
            for n in &names {
                delays.insert(n.clone(), proxies::DELAY_QUEUED);
            }
        }
        self.publish_groups();
        let workers = DELAY_WORKERS.min(names.len()).max(1);
        let queue = Arc::new(Mutex::new(VecDeque::from(names)));
        let left = Arc::new(AtomicU64::new(workers as u64));
        let group = Arc::new(group);
        for _ in 0..workers {
            let (queue, left, url, group) = (queue.clone(), left.clone(), url.clone(), group.clone());
            self.spawn(move |app| {
                loop {
                    let Some(name) = queue.lock().unwrap().pop_front() else { break };
                    app.delays.lock().unwrap().insert(name.clone(), proxies::DELAY_RUNNING);
                    app.publish_groups();
                    let value = app
                        .bridge()
                        .and_then(|b| {
                            b.call(
                                "delay.test",
                                json!({"proxy": name, "url": url, "timeout": profile::DELAY_TEST_TIMEOUT_MS}),
                                Duration::from_millis(profile::DELAY_TEST_TIMEOUT_MS + 7000),
                            )
                        })
                        .ok()
                        .and_then(|r| r.get("value").and_then(Value::as_i64))
                        .unwrap_or(-2) as i32;
                    app.delays.lock().unwrap().insert(name, if value == 0 { -2 } else { value });
                    app.publish_groups();
                }
                if left.fetch_sub(1, Ordering::SeqCst) == 1 {
                    // `sortNum`: sorting by delay moves the cards once the test is done.
                    let snapshot = app.delays.lock().unwrap().clone();
                    *app.sorted_delays.lock().unwrap() = snapshot;
                    if let Some(g) = group.as_ref() {
                        app.testing_groups.lock().unwrap().remove(g);
                    }
                    app.publish_groups();
                }
            });
        }
    }

    fn set_proxy_style(self: &Arc<Self>, key: &str, value: i32) {
        {
            let mut store = self.store.lock().unwrap();
            let s = &mut store.state.settings.proxies;
            match key {
                "kind" => s.kind = value,
                "sort" => s.sort = value,
                "layout" => s.layout = value,
                "card" => s.card = value,
                "icon" => s.icon = value,
                "hide" => s.hide_timeout = value != 0,
                _ => return,
            }
            store.save();
        }
        self.publish_groups();
    }

    /// The list style's unfold toggle (`unfoldSet`).
    fn toggle_group(self: &Arc<Self>, group: String) {
        {
            let mut store = self.store.lock().unwrap();
            let unfold = &mut store.state.settings.proxies.unfold;
            if let Some(i) = unfold.iter().position(|g| *g == group) {
                unfold.remove(i);
            } else {
                unfold.push(group);
            }
            store.save();
        }
        self.publish_groups();
    }

    fn search_proxies(self: &Arc<Self>, query: String) {
        *self.proxy_query.lock().unwrap() = query;
        self.publish_groups();
    }
}

/// Where the selected card of a group sits in the proxies list, from the top of its
/// content, by the same measures the page lays the list out with.
fn locate_offset(ui: &AppWindow, group: &str) -> f32 {
    let cols = ui.get_proxy_columns().max(1) as usize;
    let card = ui.get_proxy_card_height() + PROXY_GAP;
    let groups = ui.get_proxy_groups();
    let selected = |g: &ProxyGroup| g.nodes.iter().position(|n| n.selected).unwrap_or(0);
    if ui.get_proxy_style().kind == 0 {
        return groups
            .iter()
            .find(|g| g.name == group)
            .map(|g| PROXY_PAD + (selected(&g) / cols) as f32 * card)
            .unwrap_or(0.0);
    }
    let mut y = PROXY_PAD;
    for g in groups.iter() {
        if g.name == group {
            return y + PROXY_HEADER + PROXY_GAP + (selected(&g) / cols) as f32 * card;
        }
        y += PROXY_HEADER + PROXY_GROUP_GAP;
        if g.expanded {
            let rows = g.nodes.row_count().div_ceil(cols);
            y += PROXY_GAP + rows as f32 * card - PROXY_GAP;
        }
    }
    0.0
}

/// Builds the window, starts the shell connection and runs the event loop until the
/// activity goes away.
pub fn run(home: PathBuf) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let store = Store::open(&home);
    let settings = store.state.settings.clone();
    let app = Arc::new(App {
        ui: ui.as_weak(),
        home,
        store: Mutex::new(store),
        bridge: Mutex::new(None),
        alive: AtomicBool::new(true),
        configured: AtomicBool::new(false),
        last_apply: Mutex::new(None),
        run_state: Mutex::new("OFFLINE".into()),
        groups: Mutex::new(Value::Null),
        delays: Mutex::new(HashMap::new()),
        sorted_delays: Mutex::new(HashMap::new()),
        testing_groups: Mutex::new(HashSet::new()),
        proxy_query: Mutex::new(String::new()),
        traffic_up: Mutex::new(VecDeque::new()),
        traffic_down: Mutex::new(VecDeque::new()),
        peaks: Mutex::new((0, 0)),
        started_at: Mutex::new(None),
        ip_seq: AtomicU64::new(0),
        busy: Mutex::new(HashSet::new()),
        pending_logs: Mutex::new(Vec::new()),
        logs_scheduled: AtomicBool::new(false),
        toast_seq: AtomicU64::new(0),
        last_id: Mutex::new(0),
        pending_key: Mutex::new(None),
        sort_order: Mutex::new(Vec::new()),
    });

    let apply_theme = |ui: &AppWindow, accent: usize| {
        let [light, dark] = theme::SCHEMES[accent.min(theme::SCHEMES.len() - 1)];
        ui.global::<Theme>().set_light(scheme(&light));
        ui.global::<Theme>().set_dark(scheme(&dark));
    };
    apply_theme(&ui, settings.accent);
    ui.global::<Theme>().set_mode(settings.theme_mode);
    ui.set_accent(settings.accent as i32);
    ui.set_theme_mode(settings.theme_mode);
    ui.set_mode(settings.mode);
    ui.set_version(format!("UI {VERSION}").into());
    ui.set_accents(ModelRc::new(VecModel::from(
        theme::ACCENTS.iter().map(|c| rgb(*c)).collect::<Vec<_>>(),
    )));
    ui.set_logs(ModelRc::new(VecModel::<LogItem>::default()));
    app.refresh_profiles();

    let a = app.clone();
    ui.on_toggle_vpn(move || a.toggle_vpn());
    let a = app.clone();
    ui.on_set_mode(move |m| a.set_mode(m));
    let a = app.clone();
    ui.on_import_links(move |label, text, key, batch| {
        a.import_links(label.into(), text.into(), key.into(), batch)
    });
    let a = app.clone();
    ui.on_add_file(move || a.add_file());
    let a = app.clone();
    ui.on_age_submit(move |key| a.age_key_given(key.into()));
    let a = app.clone();
    ui.on_update_profile(move |id| a.update_profile(id.into()));
    let a = app.clone();
    ui.on_update_all(move || a.update_all());
    let a = app.clone();
    ui.on_select_profile(move |id| a.select_profile(id.into()));
    let a = app.clone();
    ui.on_delete_profile(move |id| a.ask_delete(id.into()));
    let a = app.clone();
    ui.on_edit_profile(move |id| a.open_edit(id.into()));
    let a = app.clone();
    let w = ui.as_weak();
    ui.on_edit_save(move || {
        if let Some(ui) = w.upgrade() {
            a.request_edit_save(edit_form(&ui));
        }
    });
    let a = app.clone();
    let w = ui.as_weak();
    ui.on_edit_file(move || {
        if let Some(ui) = w.upgrade() {
            a.edit_file(ui.get_edit_id().into(), ui.get_edit_file_changed());
        }
    });
    let a = app.clone();
    ui.on_upload_file(move || a.upload_file());
    let a = app.clone();
    ui.on_editor_save(move |text| a.save_editor(text.into()));
    let a = app.clone();
    ui.on_preview_profile(move |id| a.preview(id.into()));
    let a = app.clone();
    ui.on_copy_link(move |id| a.copy_link(id.into()));
    let a = app.clone();
    ui.on_show_qr(move |id| a.show_qr(id.into()));
    let a = app.clone();
    ui.on_export_profile(move |id| a.export_file(id.into()));
    let a = app.clone();
    ui.on_open_sort(move || a.open_sort());
    let a = app.clone();
    ui.on_sort_move(move |i, d| a.sort_move(i, d));
    let a = app.clone();
    ui.on_sort_save(move || a.sort_save());
    let a = app.clone();
    let w = ui.as_weak();
    ui.on_confirm_answer(move |action, arg, yes| {
        if let Some(ui) = w.upgrade() {
            a.confirmed(action.into(), arg.into(), yes, edit_form(&ui));
        }
    });
    let a = app.clone();
    ui.on_edit_dashboard(move |action, widget| a.edit_dashboard(&action, &widget));
    let a = app.clone();
    ui.on_refresh_ip(move || a.probe_ip());
    let a = app.clone();
    ui.on_copy_text(move |text| {
        let text = text.to_string();
        a.spawn(move |app| match app.bridge().and_then(|b| b.shell("clipboard.set", json!({"text": text}), CALL)) {
            Ok(_) => app.toast("复制成功"),
            Err(e) => app.toast(e),
        });
    });
    app.publish_dashboard_widgets();
    let a = app.clone();
    ui.on_open_group(move |_| a.publish_groups());
    let a = app.clone();
    ui.on_select_node(move |g, n| a.select_node(g.into(), n.into()));
    let a = app.clone();
    ui.on_test_group(move |g| a.test_group(g.into()));
    let a = app.clone();
    ui.on_test_node(move |g, n| a.test_node(g.into(), n.into()));
    let a = app.clone();
    ui.on_toggle_group(move |g| a.toggle_group(g.into()));
    let a = app.clone();
    ui.on_proxy_style_changed(move |k, v| a.set_proxy_style(&k, v));
    let a = app.clone();
    ui.on_search_proxies(move |q| a.search_proxies(q.into()));
    let w = ui.as_weak();
    ui.on_locate(move |group| {
        let Some(ui) = w.upgrade() else { return 0.0 };
        locate_offset(&ui, &group)
    });
    let a = app.clone();
    ui.on_tab_changed(move |tab| {
        // Tabs follow FlClash's mobile bar: 0 dashboard, 1 proxies, 2 profiles, 3 tools.
        if tab == 1 {
            a.spawn(|app| app.refresh_groups());
        }
    });
    let w = ui.as_weak();
    ui.on_clear_logs(move || {
        if let Some(ui) = w.upgrade() {
            ui.set_logs(ModelRc::new(VecModel::<LogItem>::default()));
        }
    });
    let a = app.clone();
    let w = ui.as_weak();
    ui.on_set_accent(move |i| {
        let i = i.max(0) as usize;
        if let Some(ui) = w.upgrade() {
            apply_theme(&ui, i);
            ui.set_accent(i as i32);
        }
        let mut store = a.store.lock().unwrap();
        store.state.settings.accent = i;
        store.save();
    });
    let a = app.clone();
    let w = ui.as_weak();
    ui.on_set_theme_mode(move |m| {
        if let Some(ui) = w.upgrade() {
            ui.global::<Theme>().set_mode(m);
            ui.set_theme_mode(m);
        }
        let mut store = a.store.lock().unwrap();
        store.state.settings.theme_mode = m;
        store.save();
    });

    let a = app.clone();
    std::thread::Builder::new()
        .name("ui-connect".into())
        .spawn(move || a.connect_loop())
        .map_err(|e| slint::PlatformError::Other(e.to_string()))?;

    let a = app.clone();
    std::thread::Builder::new()
        .name("ui-auto-update".into())
        .spawn(move || a.auto_update_loop())
        .map_err(|e| slint::PlatformError::Other(e.to_string()))?;

    let result = ui.run();
    app.alive.store(false, Ordering::SeqCst);
    if let Some(b) = app.bridge.lock().unwrap().take() {
        b.close();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_read_like_flclash() {
        assert_eq!(speed(512), "512 B/s");
        assert_eq!(speed(1536), "1.5 KB/s");
        assert_eq!(size(3 * 1048576), "3.0 MB");
    }
}
