//! SeasonCat's Slint UI. It makes no JNI or Android calls of its own: everything Android
//! (VPN consent, the VPN service, the Core session) goes through the Kotlin shell's local
//! socket, see `bridge`.

pub mod bridge;
pub mod profile;
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
    busy: Mutex<HashSet<String>>,
    pending_logs: Mutex<Vec<LogItem>>,
    logs_scheduled: AtomicBool,
    toast_seq: AtomicU64,
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

fn ago(secs: u64) -> String {
    if secs == 0 {
        return "未更新".into();
    }
    let d = now_secs().saturating_sub(secs);
    match d {
        0..=59 => "刚刚更新".into(),
        60..=3599 => format!("{} 分钟前更新", d / 60),
        3600..=86_399 => format!("{} 小时前更新", d / 3600),
        _ => format!("{} 天前更新", d / 86_400),
    }
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

/// What the proxies page lists in a mode, after `proxy.groups`: Global shows only GLOBAL,
/// Rule every visible group but GLOBAL, Direct none (as FlClash does).
pub fn visible_groups(groups: &Value, mode: i32) -> Vec<GroupItem> {
    if mode == 2 {
        return vec![];
    }
    let list = groups
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let by_name: HashMap<&str, &Value> = list
        .iter()
        .filter_map(|g| Some((g.get("name")?.as_str()?, g)))
        .collect();
    let order: Vec<String> = groups
        .get("order")
        .and_then(Value::as_array)
        .map(|o| {
            o.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    order
        .iter()
        .filter_map(|name| by_name.get(name.as_str()).map(|g| (name, *g)))
        .filter(|(name, g)| {
            let hidden = g.get("hidden").and_then(Value::as_bool).unwrap_or(false);
            if mode == 1 {
                name.as_str() == "GLOBAL"
            } else {
                name.as_str() != "GLOBAL" && !hidden
            }
        })
        .map(|(name, g)| GroupItem {
            name: name.as_str().into(),
            kind: g.get("type").and_then(Value::as_str).unwrap_or("").into(),
            now: g.get("now").and_then(Value::as_str).unwrap_or("").into(),
            count: g
                .get("all")
                .and_then(Value::as_array)
                .map(|a| a.len())
                .unwrap_or(0) as i32,
        })
        .collect()
}

pub fn group_nodes(groups: &Value, group: &str, delays: &HashMap<String, i32>) -> Vec<NodeItem> {
    let list = groups
        .get("groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(g) = list
        .iter()
        .find(|g| g.get("name").and_then(Value::as_str) == Some(group))
    else {
        return vec![];
    };
    let mut kinds: HashMap<String, String> = HashMap::new();
    for item in groups
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(list.iter())
    {
        if let (Some(n), Some(t)) = (
            item.get("name").and_then(Value::as_str),
            item.get("type").and_then(Value::as_str),
        ) {
            kinds.insert(n.into(), t.into());
        }
    }
    let now = g.get("now").and_then(Value::as_str).unwrap_or("");
    g.get("all")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|n| NodeItem {
            name: n.into(),
            kind: kinds.get(n).cloned().unwrap_or_default().into(),
            delay: delays.get(n).copied().unwrap_or(0),
            selected: n == now,
        })
        .collect()
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

    fn set_run_state(&self, state: &str) {
        *self.run_state.lock().unwrap() = state.to_string();
        let s: SharedString = state.into();
        self.ui(move |ui| ui.set_run_state(s));
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
        let traffic = Arc::downgrade(self);
        bridge.watch(
            "traffic.watch",
            json!({"intervalMs": 1000, "onlyProxy": false}),
            Box::new(move |t| {
                let Some(app) = traffic.upgrade() else { return };
                let get = |k: &str| t.get(k).and_then(Value::as_i64).unwrap_or(0);
                let (up, down, tu, td) = (
                    speed(get("up")),
                    speed(get("down")),
                    size(get("totalUp")),
                    size(get("totalDown")),
                );
                app.ui(move |ui| {
                    ui.set_up_speed(up.into());
                    ui.set_down_speed(down.into());
                    ui.set_up_total(format!("共 {tu}").into());
                    ui.set_down_total(format!("共 {td}").into());
                });
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
        self.refresh_groups();
        Ok(())
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
                Ok(_) => app.refresh_groups(),
                Err(e) => app.toast(e),
            }
        });
    }

    // ------------------------------------------------------------ profiles

    fn refresh_profiles(&self) {
        let (items, name) = {
            let store = self.store.lock().unwrap();
            let busy = self.busy.lock().unwrap();
            let items: Vec<ProfileItem> = store
                .state
                .profiles
                .iter()
                .map(|p| ProfileItem {
                    id: p.id.as_str().into(),
                    name: p.name.as_str().into(),
                    detail: format!("{} · {}", ago(p.updated), p.url).into(),
                    active: store.state.active.as_deref() == Some(p.id.as_str()),
                    busy: busy.contains(&p.id),
                })
                .collect();
            (
                items,
                store.active().map(|p| p.name.clone()).unwrap_or_default(),
            )
        };
        self.ui(move |ui| {
            ui.set_profiles(ModelRc::new(VecModel::from(items)));
            ui.set_profile_name(name.into());
        });
    }

    /// `validateProfile` in lib/core/controller.dart: a file the Core cannot load as a config
    /// is tried as a share-link list and rewritten into a config of its proxies.
    fn fetch_into(&self, url: &str, dest: &std::path::Path) -> Result<Option<String>, String> {
        let bridge = self.bridge()?;
        let download = profile::download(url)?;
        let tmp = self.store.lock().unwrap().temp_path();
        std::fs::write(&tmp, &download.bytes).map_err(|e| e.to_string())?;
        let first_issue = |b: &Bridge| -> Result<Option<String>, String> {
            let r = b.call(
                "config.validate",
                json!({"profile": profile_ref(&tmp)}),
                CALL,
            )?;
            Ok(r.pointer("/issues/0/message")
                .and_then(Value::as_str)
                .map(String::from))
        };
        if let Some(issue) = first_issue(&bridge)? {
            let converted = bridge
                .call(
                    "convert.subscription",
                    json!({"profile": profile_ref(&tmp)}),
                    CALL,
                )
                .ok()
                .and_then(|r| r.get("proxies").cloned())
                .filter(|p| p.as_array().is_some_and(|a| !a.is_empty()));
            let Some(proxies) = converted else {
                let _ = std::fs::remove_file(&tmp);
                return Err(issue);
            };
            // JSON is YAML, so the Core reads this as a config.
            std::fs::write(
                &tmp,
                serde_json::to_vec(&json!({"proxies": proxies})).unwrap(),
            )
            .map_err(|e| e.to_string())?;
            if let Some(issue) = first_issue(&bridge)? {
                let _ = std::fs::remove_file(&tmp);
                return Err(issue);
            }
        }
        std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
        Ok(download.name)
    }

    fn add_profile(self: &Arc<Self>, url: String) {
        let url = url.trim().to_string();
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            self.toast("请输入有效配置URL");
            return;
        }
        self.ui(|ui| ui.set_adding(true));
        self.spawn(move |app| {
            let id = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
                .to_string();
            let dest = app.store.lock().unwrap().profile_path(&id);
            let result = app.fetch_into(&url, &dest);
            app.ui(|ui| ui.set_adding(false));
            match result {
                Ok(name) => {
                    let first = {
                        let mut store = app.store.lock().unwrap();
                        store.state.profiles.push(store::Profile {
                            id: id.clone(),
                            name: name.unwrap_or_else(|| profile::url_label(&url)),
                            url,
                            updated: now_secs(),
                            selection: Default::default(),
                        });
                        let first = store.state.active.is_none();
                        if first {
                            store.state.active = Some(id);
                        }
                        store.save();
                        first
                    };
                    app.refresh_profiles();
                    app.ui(|ui| ui.invoke_profile_added());
                    app.toast("已添加");
                    if first {
                        if let Err(e) = app.apply_active() {
                            app.toast(e);
                        }
                    }
                }
                Err(e) => app.toast(e),
            }
        });
    }

    fn update_profile(self: &Arc<Self>, id: String) {
        let Some((url, dest, active)) = ({
            let store = self.store.lock().unwrap();
            store.state.profiles.iter().find(|p| p.id == id).map(|p| {
                (
                    p.url.clone(),
                    store.profile_path(&id),
                    store.state.active.as_deref() == Some(id.as_str()),
                )
            })
        }) else {
            return;
        };
        self.busy.lock().unwrap().insert(id.clone());
        self.refresh_profiles();
        self.spawn(move |app| {
            let result = app.fetch_into(&url, &dest);
            app.busy.lock().unwrap().remove(&id);
            match result {
                Ok(name) => {
                    {
                        let mut store = app.store.lock().unwrap();
                        if let Some(p) = store.profile_mut(&id) {
                            p.updated = now_secs();
                            if let Some(name) = name {
                                p.name = name;
                            }
                        }
                        store.save();
                    }
                    app.toast("更新成功");
                    if active {
                        if let Err(e) = app.apply_active() {
                            app.toast(e);
                        }
                    }
                }
                Err(e) => app.toast(e),
            }
            app.refresh_profiles();
        });
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

    // ------------------------------------------------------------ proxies

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
        let mode = self.store.lock().unwrap().state.settings.mode;
        let groups = self.groups.lock().unwrap().clone();
        let delays = self.delays.lock().unwrap().clone();
        let list = visible_groups(&groups, mode);
        let hint = if mode == 2 {
            "直连模式下不经过代理"
        } else {
            "暂无数据"
        };
        self.ui(move |ui| {
            ui.set_groups(ModelRc::new(VecModel::from(list)));
            ui.set_proxies_hint(hint.into());
            let current = ui.get_current_group();
            if !current.is_empty() {
                ui.set_nodes(ModelRc::new(VecModel::from(group_nodes(
                    &groups, &current, &delays,
                ))));
            }
        });
    }

    fn select_node(self: &Arc<Self>, group: String, node: String) {
        self.spawn(move |app| {
            let r = app
                .bridge()
                .and_then(|b| b.call("proxy.select", json!({"group": group, "proxy": node}), CALL));
            match r {
                Ok(_) => {
                    {
                        let mut store = app.store.lock().unwrap();
                        if let Some(id) = store.state.active.clone() {
                            if let Some(p) = store.profile_mut(&id) {
                                p.selection.insert(group.clone(), node.clone());
                            }
                            store.save();
                        }
                    }
                    if let Some(params) = app.last_apply.lock().unwrap().as_mut() {
                        params["selection"][group.as_str()] = node.into();
                    }
                    app.refresh_groups();
                }
                Err(e) => app.toast(e),
            }
        });
    }

    fn test_group(self: &Arc<Self>, group: String) {
        let groups = self.groups.lock().unwrap().clone();
        let names: Vec<String> = group_nodes(&groups, &group, &HashMap::new())
            .into_iter()
            .map(|n| n.name.into())
            .collect();
        let url = groups
            .get("groups")
            .and_then(Value::as_array)
            .and_then(|l| {
                l.iter()
                    .find(|g| g.get("name").and_then(Value::as_str) == Some(group.as_str()))
            })
            .and_then(|g| g.get("testUrl").and_then(Value::as_str))
            .filter(|u| !u.is_empty())
            .unwrap_or(profile::DEFAULT_TEST_URL)
            .to_string();
        {
            let mut delays = self.delays.lock().unwrap();
            for n in &names {
                delays.insert(n.clone(), -3);
            }
        }
        self.ui(|ui| ui.set_testing(true));
        self.publish_groups();
        let queue = Arc::new(Mutex::new(VecDeque::from(names)));
        let left = Arc::new(AtomicU64::new(DELAY_WORKERS as u64));
        for _ in 0..DELAY_WORKERS {
            let (queue, left, url) = (queue.clone(), left.clone(), url.clone());
            self.spawn(move |app| {
                while let Some(name) = queue.lock().unwrap().pop_front() {
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
                    app.ui(|ui| ui.set_testing(false));
                }
            });
        }
    }
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
        busy: Mutex::new(HashSet::new()),
        pending_logs: Mutex::new(Vec::new()),
        logs_scheduled: AtomicBool::new(false),
        toast_seq: AtomicU64::new(0),
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
    ui.on_add_profile(move |url| a.add_profile(url.into()));
    let a = app.clone();
    ui.on_update_profile(move |id| a.update_profile(id.into()));
    let a = app.clone();
    ui.on_select_profile(move |id| a.select_profile(id.into()));
    let a = app.clone();
    ui.on_delete_profile(move |id| a.delete_profile(id.into()));
    let a = app.clone();
    ui.on_open_group(move |_| a.publish_groups());
    let a = app.clone();
    ui.on_select_node(move |g, n| a.select_node(g.into(), n.into()));
    let a = app.clone();
    ui.on_test_group(move |g| a.test_group(g.into()));
    let a = app.clone();
    ui.on_tab_changed(move |tab| {
        if tab == 2 {
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

    fn sample() -> Value {
        json!({
            "order": ["GLOBAL", "节点选择", "隐藏"],
            "groups": [
                {"name": "GLOBAL", "type": "Selector", "now": "节点选择", "all": ["节点选择", "DIRECT"], "testUrl": "", "hidden": false, "icon": "", "emptyFallback": ""},
                {"name": "节点选择", "type": "Selector", "now": "香港", "all": ["香港", "日本"], "testUrl": "", "hidden": false, "icon": "", "emptyFallback": ""},
                {"name": "隐藏", "type": "URLTest", "now": "日本", "all": ["日本"], "testUrl": "", "hidden": true, "icon": "", "emptyFallback": ""}
            ],
            "nodes": [{"name": "香港", "type": "Shadowsocks"}, {"name": "日本", "type": "Trojan"}, {"name": "DIRECT", "type": "Direct"}]
        })
    }

    #[test]
    fn groups_follow_the_mode() {
        let g = sample();
        let rule: Vec<String> = visible_groups(&g, 0)
            .iter()
            .map(|g| g.name.to_string())
            .collect();
        assert_eq!(rule, ["节点选择"]);
        let global: Vec<String> = visible_groups(&g, 1)
            .iter()
            .map(|g| g.name.to_string())
            .collect();
        assert_eq!(global, ["GLOBAL"]);
        assert!(visible_groups(&g, 2).is_empty());
    }

    #[test]
    fn nodes_carry_type_delay_and_pick() {
        let mut delays = HashMap::new();
        delays.insert("日本".to_string(), 120);
        let nodes = group_nodes(&sample(), "节点选择", &delays);
        assert_eq!(nodes.len(), 2);
        assert!(nodes[0].selected);
        assert_eq!(nodes[0].kind.as_str(), "Shadowsocks");
        assert_eq!(nodes[1].delay, 120);
        let global = group_nodes(&sample(), "GLOBAL", &delays);
        assert_eq!(global[0].kind.as_str(), "Selector");
    }

    #[test]
    fn speeds_read_like_flclash() {
        assert_eq!(speed(512), "512 B/s");
        assert_eq!(speed(1536), "1.5 KB/s");
        assert_eq!(size(3 * 1048576), "3.0 MB");
    }
}
