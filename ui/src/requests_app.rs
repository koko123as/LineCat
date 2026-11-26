//! The requests and resources pages' side of `App`: the Core's streams they read, the calls
//! they make (lib/core/api/rpc_core_api.dart) and what they publish to the window.

use crate::requests_view::{self, Ctx, CONNECTIONS, DNS, HTTP};
use crate::{
    http_capture, profile, proxies, resources, store, ActionItem, App, AppWindow, ResourceItem,
    CALL,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// The device zone's offset from UTC, in seconds, now.
pub(crate) fn utc_offset(ms: i64) -> i64 {
    let t = (ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return 0;
    }
    tm.tm_gmtoff as i64
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

/// Swaps a list model's rows in place, so a list keeps its scroll as rows change.
fn set_rows<T: Clone + 'static>(current: ModelRc<T>, rows: Vec<T>) -> Option<ModelRc<T>> {
    match current.as_any().downcast_ref::<VecModel<T>>() {
        Some(vec) => {
            if vec.row_count() == rows.len() {
                for (i, r) in rows.into_iter().enumerate() {
                    vec.set_row_data(i, r);
                }
            } else {
                vec.set_vec(rows);
            }
            None
        }
        None => Some(model(rows)),
    }
}

impl App {
    /// Subscribes the streams the requests and resources pages read.
    pub(crate) fn watch_requests(
        self: &Arc<Self>,
        bridge: &crate::bridge::Bridge,
    ) -> Result<(), String> {
        let conns = Arc::downgrade(self);
        bridge.watch(
            "connection.watch",
            json!({}),
            Box::new(move |event| {
                if let Some(app) = conns.upgrade() {
                    let mut req = app.req.lock().unwrap();
                    req.conns.take(&event, now_ms());
                    req.drop_gone();
                }
            }),
        )?;
        let dns = Arc::downgrade(self);
        bridge.watch(
            "dns.watch",
            json!({}),
            Box::new(move |event| {
                if let Some(app) = dns.upgrade() {
                    app.req.lock().unwrap().dns.push(&event);
                }
            }),
        )?;
        let events = Arc::downgrade(self);
        bridge.watch(
            "events.watch",
            json!({"delays": false, "state": false}),
            Box::new(move |event| {
                if let Some(app) = events.upgrade() {
                    app.core_event(&event);
                }
            }),
        )
    }

    /// `GeoResourceAction.handleCoreUpdate`.
    fn core_event(self: &Arc<Self>, event: &Value) {
        let text = |k: &str| {
            event
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        if text("type") != "resourceUpdate" || text("kind") != "geo" {
            return;
        }
        let name = text("name").to_uppercase();
        let phase = text("phase");
        if phase == "updating" {
            self.geo_busy.lock().unwrap().insert(name);
            return;
        }
        let was = self.geo_busy.lock().unwrap().remove(&name);
        let manual = self.geo_manual.lock().unwrap().remove(&name);
        match phase.as_str() {
            "failed" if was && text("error") != "superseded" => self.toast(text("error")),
            "updated" if manual => self.toast(format!("{name} 已更新")),
            "unchanged" if manual => self.toast(format!("{name} 已是最新版本")),
            _ => {}
        }
    }

    fn mitm(&self) -> store::MitmSettings {
        self.store.lock().unwrap().state.settings.mitm.clone()
    }

    /// One tick of the requests page while it shows: the HTTP pass, then the page.
    pub(crate) fn tick_requests(self: &Arc<Self>, ui: &AppWindow) {
        if ui.get_req_visible() {
            let reading_http =
                self.req.lock().unwrap().segment == HTTP && http_capture::capturing(&self.mitm());
            if reading_http && !self.http_reading.swap(true, Ordering::SeqCst) {
                self.spawn(|app| {
                    app.read_http();
                    app.http_reading.store(false, Ordering::SeqCst);
                });
            }
            self.publish_requests(ui);
        }
        if ui.get_tab() == 3 && ui.get_tools_page() == "resources" {
            self.publish_resources(ui);
        }
    }

    /// `HttpRequestFeed.read`: a pass through `mitm.query`, read again from the start when
    /// the Core started over.
    fn read_http(self: &Arc<Self>) {
        let Ok(bridge) = self.bridge() else { return };
        for _ in 0..2 {
            let since = self.req.lock().unwrap().http.since();
            let mut pages = Vec::new();
            let mut before: Option<i64> = None;
            loop {
                let mut params = json!({"filter": {}, "limit": http_capture::PAGE_LIMIT});
                if let Some(b) = before {
                    params["before"] = b.into();
                }
                if let Some(s) = since {
                    params["since"] = s.into();
                }
                let Ok(answer) = bridge.call("mitm.query", params, CALL) else {
                    return;
                };
                let page = http_capture::Page::parse(&answer);
                before = page.next;
                pages.push(page);
                if before.is_none() {
                    break;
                }
            }
            let mut req = self.req.lock().unwrap();
            if req.http.take(pages) {
                req.drop_gone();
                return;
            }
        }
    }

    pub(crate) fn publish_requests(self: &Arc<Self>, ui: &AppWindow) {
        let mitm = self.mitm();
        let shows_http = http_capture::capturing(&mitm);
        let groups = proxies::Groups::parse(&self.groups.lock().unwrap());
        let delays = self.delays.lock().unwrap().clone();
        let now = now_ms();
        let cx = Ctx {
            now,
            offset: utc_offset(now),
            capturing: shows_http,
            groups: &groups,
            delays: &delays,
            mitm_hosts: mitm.enable.then_some(mitm.hostname.as_slice()),
        };
        let mut req = self.req.lock().unwrap();
        if req.segment == HTTP && !shows_http {
            req.segment = CONNECTIONS;
        }
        let rows = req.rows(&cx);
        if let Some(m) = set_rows(ui.get_req_rows(), rows) {
            ui.set_req_rows(m);
        }
        ui.set_req_segment(req.segment);
        ui.set_req_shows_http(shows_http);
        ui.set_req_statuses(model(req.status_labels()));
        ui.set_req_status(req.filters().status);
        ui.set_req_conditions(model(req.condition_lines()));
        let (empty, desc) = req.empty(http_capture::has_no_host(&mitm));
        ui.set_req_empty(empty.into());
        ui.set_req_empty_desc(desc.into());
        ui.set_req_has_menu(!req.page_actions(&mitm).is_empty());
        ui.set_req_detail(req.detail(&cx));
    }

    pub(crate) fn republish(self: &Arc<Self>) {
        let app = self.clone();
        self.ui(move |ui| app.publish_requests(&ui));
    }

    fn show_actions(self: &Arc<Self>, title: String, items: Vec<ActionItem>) {
        if items.is_empty() {
            return;
        }
        self.ui(move |ui| {
            ui.set_req_actions_title(title.into());
            ui.set_req_actions(model(items));
            ui.set_req_actions_open(true);
        });
    }

    fn copy(self: &Arc<Self>, text: String) {
        self.spawn(move |app| {
            match app
                .bridge()
                .and_then(|b| b.shell("clipboard.set", json!({"text": text}), CALL))
            {
                Ok(_) => app.toast("复制成功"),
                Err(e) => app.toast(e),
            }
        });
    }

    pub(crate) fn close_active(self: &Arc<Self>) {
        self.close_connections(None);
    }

    fn close_connections(self: &Arc<Self>, ids: Option<Vec<i64>>) {
        self.spawn(move |app| {
            let params = match ids {
                Some(ids) => json!({"ids": ids}),
                None => json!({"all": true}),
            };
            if let Err(e) = app
                .bridge()
                .and_then(|b| b.call("connection.close", params, CALL))
            {
                app.toast(e);
            }
            app.republish();
        });
    }

    /// Reads what the selection's detail needs from the Core: a request and its bodies, or
    /// a connection's requests.
    fn load_selection(self: &Arc<Self>) {
        let (segment, id) = {
            let req = self.req.lock().unwrap();
            (req.segment, req.selected[req.segment.clamp(0, 2) as usize])
        };
        let Some(id) = id else { return };
        let capturing = http_capture::capturing(&self.mitm());
        self.spawn(move |app| {
            let Ok(bridge) = app.bridge() else { return };
            if segment == HTTP {
                let exchange = bridge
                    .call("mitm.exchange", json!({"id": id}), CALL)
                    .ok()
                    .filter(Value::is_object);
                let mut bodies = [None, None];
                if let Some(e) = &exchange {
                    let pending = e.get("pending").and_then(Value::as_bool).unwrap_or(false);
                    for (i, key) in ["request-size", "response-size"].iter().enumerate() {
                        let size = e.get(*key).and_then(Value::as_i64).unwrap_or(0);
                        if size > 0 && !pending {
                            bodies[i] = bridge
                                .call("mitm.body", json!({"id": id, "response": i == 1}), CALL)
                                .ok()
                                .filter(Value::is_object)
                                .map(|b| http_capture::body_text(&b, crate::size));
                        }
                    }
                }
                let mut req = app.req.lock().unwrap();
                if req.selected[HTTP as usize] == Some(id) {
                    req.exchange = exchange;
                    req.bodies = bodies;
                }
            } else if segment == CONNECTIONS && capturing {
                let mitm = app
                    .req
                    .lock()
                    .unwrap()
                    .conns
                    .conns
                    .get(&id)
                    .is_some_and(|c| c.mitm);
                if !mitm {
                    return;
                }
                let list = bridge
                    .call("mitm.exchanges", json!({"connection": id}), CALL)
                    .ok()
                    .map(|v| {
                        v.get("exchanges")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(http_capture::Summary::parse)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                app.req.lock().unwrap().conn_http = Some((id, list));
            }
            app.republish();
        });
    }

    fn select(self: &Arc<Self>, id: i64, split: bool) {
        {
            let mut req = self.req.lock().unwrap();
            let seg = req.segment.clamp(0, 2) as usize;
            if split && req.selected[seg] == Some(id) {
                req.selected[seg] = None;
            } else {
                if req.selected[seg] != Some(id) {
                    req.tab[seg] = 0;
                    if seg == HTTP as usize {
                        req.exchange = None;
                        req.bodies = [None, None];
                    }
                }
                req.selected[seg] = Some(id);
            }
        }
        self.load_selection();
        self.republish();
    }

    /// Opens a record a detail links to: a connection, or one of its requests.
    fn open_link(self: &Arc<Self>, arg: &str) {
        if let Some(text) = arg.strip_prefix("copy:") {
            self.copy(text.to_string());
            return;
        }
        if let Some(change) = arg.strip_prefix("mitm-host:") {
            return self.ask_mitm_host(change);
        }
        let (segment, id) = match arg.split_once(':') {
            Some(("conn", id)) => (CONNECTIONS, id.parse().ok()),
            Some(("http", id)) => (HTTP, id.parse().ok()),
            _ => return,
        };
        let Some(id) = id else { return };
        if segment == HTTP && !http_capture::capturing(&self.mitm()) {
            return;
        }
        {
            let mut req = self.req.lock().unwrap();
            req.segment = segment;
            req.selected[segment as usize] = Some(id);
            req.tab[segment as usize] = 0;
            req.exchange = None;
            req.bodies = [None, None];
        }
        self.load_selection();
        self.republish();
    }

    fn action(self: &Arc<Self>, id: &str) {
        let (name, arg) = id.split_once(':').unwrap_or((id, ""));
        let num: i64 = arg.parse().unwrap_or(0);
        match name {
            "capture-on" | "capture-off" => {
                let on = name == "capture-on";
                self.store.lock().unwrap().state.settings.mitm.capture = on;
                self.store.lock().unwrap().save();
                let settings = self.store.lock().unwrap().state.settings.clone();
                self.patch(
                    json!({"options": profile::options(&settings, on && self.mitm().enable)}),
                );
            }
            "capture-limit" => {
                let items = {
                    let mut req = self.req.lock().unwrap();
                    req.limit_open = !req.limit_open;
                    req.page_actions(&self.mitm())
                };
                self.ui(move |ui| ui.set_req_actions(model(items)));
                return;
            }
            "capture-entries" => {
                self.store
                    .lock()
                    .unwrap()
                    .state
                    .settings
                    .mitm
                    .capture_entries = Some(num);
                self.store.lock().unwrap().save();
                self.reapply();
            }
            "close-active" => {
                let count = self.req.lock().unwrap().conns.active_count();
                if count > 0 {
                    self.confirm(
                        "关闭全部活动连接",
                        format!("关闭全部 {count} 条活动连接？正在使用它们的应用会自行重连，进行中的下载或通话可能中断。"),
                        "close-active",
                        String::new(),
                    );
                }
            }
            "clear-dns" => self.spawn(|app| {
                match app
                    .bridge()
                    .and_then(|b| b.call("dns.clearCache", json!({}), CALL))
                {
                    Ok(_) => app.toast("DNS缓存已清除"),
                    Err(_) => app.toast("清除DNS缓存失败"),
                }
            }),
            "close" => self.close_connections(Some(vec![num])),
            // `showTrackerInfoDetail(opensRequests: true)`.
            "view-http" => {
                let capturing = http_capture::capturing(&self.mitm());
                {
                    let mut req = self.req.lock().unwrap();
                    req.selected[CONNECTIONS as usize] = Some(num);
                    req.tab[CONNECTIONS as usize] = if capturing { 1 } else { 0 };
                }
                self.load_selection();
                self.republish();
                self.ui(|ui| {
                    if !ui.get_req_split() {
                        ui.set_req_sheet_open(true);
                    }
                });
            }
            "copy-summary" => {
                let text = {
                    let req = self.req.lock().unwrap();
                    req.conns.conns.get(&num).map(|c| c.summary(now_ms()))
                };
                if let Some(text) = text {
                    self.copy(text);
                }
            }
            "copy-host" | "copy-dest" | "copy-domain" | "copy-answers" => {
                let text = self.req.lock().unwrap().copy_text(name, num);
                if let Some(text) = text {
                    self.copy(text);
                }
            }
            "copy-url" => {
                let url = self
                    .req
                    .lock()
                    .unwrap()
                    .exchange
                    .as_ref()
                    .and_then(|e| e.get("url").and_then(Value::as_str).map(String::from));
                if let Some(url) = url {
                    self.copy(url);
                }
            }
            "only-app" | "hide-host" | "only-domain" => {
                self.req.lock().unwrap().filter_by(name, num);
            }
            "res-sync" => self.sync_resource(arg),
            "res-edit" => self.open_geo_url(arg),
            "res-copy" => {
                let settings = self.store.lock().unwrap().state.settings.clone();
                let url = match arg.split_once(':') {
                    Some(_) => resources::remote_scripts(&settings.scripts)
                        .into_iter()
                        .chain(resources::remote_modules(&settings.mitm_modules))
                        .find(|r| r.key == arg)
                        .map(|r| r.url),
                    None => settings.geox_url.get(arg).cloned(),
                };
                if let Some(url) = url {
                    self.copy(url);
                }
            }
            _ => {}
        }
        self.republish();
    }

    /// `config.patch` while the Core runs a config; a later apply carries it otherwise.
    pub(crate) fn patch(self: &Arc<Self>, params: Value) {
        if !self.configured.load(Ordering::SeqCst) {
            return;
        }
        self.spawn(move |app| {
            if let Err(e) = app
                .bridge()
                .and_then(|b| b.call("config.patch", params, CALL))
            {
                app.toast(e);
            }
        });
    }

    pub(crate) fn reapply(self: &Arc<Self>) {
        if !self.configured.load(Ordering::SeqCst) {
            return;
        }
        self.spawn(|app| {
            if let Err(e) = app.apply_active() {
                app.toast(e);
            }
        });
    }

    fn patch_geo(self: &Arc<Self>) {
        let s = self.store.lock().unwrap().state.settings.clone();
        self.patch(json!({
            "geoAutoUpdate": s.geo_auto_update,
            "geoUpdateInterval": s.geo_update_interval,
            "geoxUrl": s.geox_url,
        }));
    }

    // ------------------------------------------------------------ resources

    pub(crate) fn publish_resources(self: &Arc<Self>, ui: &AppWindow) {
        let settings = self.store.lock().unwrap().state.settings.clone();
        let busy = self.geo_busy.lock().unwrap().clone();
        let home = self.core_home.lock().unwrap().clone();
        let items: Vec<ResourceItem> = resources::GEO
            .iter()
            .map(|(name, key, file)| {
                let meta = std::fs::metadata(home.join(file)).ok();
                let modified = meta
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                ResourceItem {
                    key: (*key).into(),
                    title: (*name).into(),
                    subtitle: modified
                        .map(crate::ago_short)
                        .unwrap_or_else(|| "未知".into())
                        .into(),
                    chip: meta
                        .map(|m| crate::size(m.len() as i64))
                        .unwrap_or_default()
                        .into(),
                    url: settings
                        .geox_url
                        .get(*key)
                        .cloned()
                        .unwrap_or_default()
                        .into(),
                    busy: busy.contains(*name),
                }
            })
            .collect();
        let remote = |r: resources::Remote, chip: String, busy: bool| ResourceItem {
            key: r.key.into(),
            title: r.title.into(),
            subtitle: r
                .updated
                .map(crate::ago_short)
                .unwrap_or_else(|| "未知".into())
                .into(),
            chip: chip.into(),
            url: r.url.into(),
            busy,
        };
        let scripts: Vec<ResourceItem> = resources::remote_scripts(&settings.scripts)
            .into_iter()
            .map(|r| {
                let id = r.key["script:".len()..].to_string();
                let chip = std::fs::metadata(self.script_path(&id))
                    .map(|m| crate::size(m.len() as i64))
                    .unwrap_or_default();
                let busy = self.script_syncing(&id);
                remote(r, chip, busy)
            })
            .collect();
        let modules: Vec<ResourceItem> = resources::remote_modules(&settings.mitm_modules)
            .into_iter()
            .map(|r| {
                let busy = self.module_updating(&r.key["module:".len()..]);
                remote(r, String::new(), busy)
            })
            .collect();
        ui.set_res_updating(items.iter().chain(&scripts).chain(&modules).any(|i| i.busy));
        if let Some(m) = set_rows(ui.get_res_items(), items) {
            ui.set_res_items(m);
        }
        if let Some(m) = set_rows(ui.get_res_scripts(), scripts) {
            ui.set_res_scripts(m);
        }
        if let Some(m) = set_rows(ui.get_res_modules(), modules) {
            ui.set_res_modules(m);
        }
        ui.set_res_auto(settings.geo_auto_update);
        ui.set_res_interval(format!("{} 小时", settings.geo_update_interval).into());
    }

    pub(crate) fn republish_resources(self: &Arc<Self>) {
        let app = self.clone();
        self.ui(move |ui| app.publish_resources(&ui));
    }

    /// `updateGeoResource`: done once the Core accepts it; `events.watch` tells the end.
    fn update_geo(self: &Arc<Self>, key: String, manual: bool) {
        let Some((name, _, _)) = resources::GEO.iter().find(|g| g.1 == key || g.0 == key) else {
            return;
        };
        let name = name.to_string();
        if !self.geo_busy.lock().unwrap().insert(name.clone()) {
            return;
        }
        if manual {
            self.geo_manual.lock().unwrap().insert(name.clone());
        }
        self.republish_resources();
        self.spawn(move |app| {
            if let Err(e) = app
                .bridge()
                .and_then(|b| b.call("geo.update", json!({"type": name}), CALL))
            {
                app.geo_busy.lock().unwrap().remove(&name);
                app.geo_manual.lock().unwrap().remove(&name);
                app.show_failures(vec![(name, e)]);
            }
            app.republish_resources();
        });
    }

    /// `updateAll`: the geo files, then every remote script and module.
    fn update_all_geo(self: &Arc<Self>) {
        for (_, key, _) in resources::GEO {
            self.update_geo(key.to_string(), false);
        }
        let settings = self.store.lock().unwrap().state.settings.clone();
        for r in resources::remote_scripts(&settings.scripts) {
            self.sync_script(&r.key["script:".len()..]);
        }
        for r in resources::remote_modules(&settings.mitm_modules) {
            self.sync_module(&r.key["module:".len()..]);
        }
    }

    /// 同步 on a row: a geo file, a script or a module by its key.
    fn sync_resource(self: &Arc<Self>, key: &str) {
        match key.split_once(':') {
            Some(("script", id)) => self.sync_script(id),
            Some(("module", id)) => self.sync_module(id),
            _ => self.update_geo(key.to_string(), true),
        }
    }

    fn open_geo_url(self: &Arc<Self>, key: &str) {
        let Some((name, key, _)) = resources::GEO.iter().find(|g| g.1 == key) else {
            return;
        };
        let url = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .geox_url
            .get(*key)
            .cloned()
            .unwrap_or_default();
        let (name, key) = (name.to_string(), key.to_string());
        let default = resources::default_url(&key).to_string();
        self.ui(move |ui| {
            ui.set_res_dialog_title(name.into());
            ui.set_res_dialog_key(key.into());
            ui.set_res_dialog_default(default.into());
            ui.set_res_dialog_value(url.into());
            ui.set_res_touched(false);
            ui.set_res_dialog("url".into());
        });
    }

    fn dialog_submitted(self: &Arc<Self>, kind: &str, key: &str, value: &str) {
        match kind {
            "interval" => {
                let Ok(hours) = resources::check_interval(value) else {
                    return;
                };
                self.store
                    .lock()
                    .unwrap()
                    .state
                    .settings
                    .geo_update_interval = hours;
            }
            "url" => {
                let Ok(url) = resources::check_url(value) else {
                    return;
                };
                let mut store = self.store.lock().unwrap();
                if store.state.settings.geox_url.get(key) == Some(&url) {
                    return;
                }
                store.state.settings.geox_url.insert(key.to_string(), url);
            }
            _ => return,
        }
        self.store.lock().unwrap().save();
        self.patch_geo();
        self.republish_resources();
    }

    /// The condition dialog's fields, choice and outcome.
    fn publish_draft(self: &Arc<Self>, open: bool) {
        let app = self.clone();
        self.ui(move |ui| {
            let req = app.req.lock().unwrap();
            let Some(d) = req.draft.clone() else { return };
            let fields = requests_view::fields(req.segment);
            let label = fields.get(d.field).map(|f| f.1).unwrap_or("");
            let (suggestions, shows, ready) = req.draft_outcome();
            if open {
                ui.set_cond_fields(model(fields.iter().map(|f| SharedString::from(f.1)).collect()));
                ui.set_cond_editing(d.editing.is_some());
                ui.set_cond_field(d.field as i32);
                ui.set_cond_op(d.operator as i32);
                ui.set_cond_tip(
                    if req.segment == CONNECTIONS {
                        "同一字段的条件满足其一即可，不同字段的都要满足。搜索框里也能这样写：host:github.com -app:slack"
                    } else {
                        "同一字段的条件满足其一即可，不同字段的都要满足。"
                    }
                    .into(),
                );
            }
            let noun = match req.segment {
                HTTP => "请求",
                DNS => "查询",
                _ => "连接",
            };
            ui.set_cond_values_in(format!("当前{noun}里的{label}").into());
            ui.set_cond_suggestions(model(suggestions));
            ui.set_cond_shows(shows.into());
            ui.set_cond_ready(ready);
            drop(req);
            if open {
                ui.set_cond_value(d.value.into());
                ui.set_cond_open(true);
            }
        });
    }

    pub(crate) fn bind_requests(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_req_search(move |q| {
            a.req.lock().unwrap().query = q.into();
            a.republish();
        });
        let a = self.clone();
        ui.on_req_open_menu(move || {
            let items = {
                let mut req = a.req.lock().unwrap();
                req.limit_open = false;
                req.page_actions(&a.mitm())
            };
            a.show_actions(String::new(), items);
        });
        let a = self.clone();
        ui.on_req_set_segment(move |s| {
            a.req.lock().unwrap().segment = s.clamp(0, 2);
            a.republish();
        });
        let a = self.clone();
        ui.on_req_set_status(move |s| {
            a.req.lock().unwrap().filters_mut().status = s.clamp(0, 2);
            a.republish();
        });
        let a = self.clone();
        ui.on_req_add_condition(move || {
            a.req.lock().unwrap().open_draft(None);
            a.publish_draft(true);
        });
        let a = self.clone();
        ui.on_req_edit_condition(move |i| {
            a.req.lock().unwrap().open_draft(Some(i.max(0) as usize));
            a.publish_draft(true);
        });
        let a = self.clone();
        ui.on_req_remove_condition(move |i| {
            a.req
                .lock()
                .unwrap()
                .filters_mut()
                .remove(i.max(0) as usize);
            a.republish();
        });
        let a = self.clone();
        ui.on_req_clear_conditions(move || {
            a.req.lock().unwrap().filters_mut().conditions.clear();
            a.republish();
        });
        let a = self.clone();
        ui.on_req_select(move |id, split| a.select(id as i64, split));
        let a = self.clone();
        ui.on_req_cond_width(move |w| {
            let mut req = a.req.lock().unwrap();
            if (req.cond_width - w).abs() < 1.0 {
                return;
            }
            req.cond_width = w;
            drop(req);
            a.republish();
        });
        let a = self.clone();
        ui.on_req_row_menu(move |id| {
            let items = {
                let req = a.req.lock().unwrap();
                req.row_actions(req.segment, id as i64)
            };
            a.show_actions(String::new(), items);
        });
        let a = self.clone();
        ui.on_req_detail_tab(move |i| {
            let mut req = a.req.lock().unwrap();
            let seg = req.segment.clamp(0, 2) as usize;
            req.tab[seg] = i;
            drop(req);
            a.republish();
        });
        let a = self.clone();
        ui.on_req_detail_close(move || {
            let id = a.req.lock().unwrap().selected[CONNECTIONS as usize];
            if let Some(id) = id {
                a.close_connections(Some(vec![id]));
            }
        });
        let a = self.clone();
        ui.on_req_detail_menu(move || {
            let items = {
                let req = a.req.lock().unwrap();
                let seg = req.segment.clamp(0, 2);
                match (seg, req.selected[seg as usize]) {
                    (HTTP, Some(_)) => vec![ActionItem {
                        id: "copy-url".into(),
                        glyph: "copy".into(),
                        label: "复制链接".into(),
                        enabled: true,
                        ..Default::default()
                    }],
                    (CONNECTIONS, Some(id)) => req
                        .detail_actions(id)
                        .into_iter()
                        .map(|(id, glyph, label)| ActionItem {
                            id: id.into(),
                            glyph: glyph.into(),
                            label: label.into(),
                            enabled: true,
                            ..Default::default()
                        })
                        .collect(),
                    (_, Some(id)) => req
                        .row_actions(seg, id)
                        .into_iter()
                        .filter(|a| {
                            !a.id.starts_with("close:") && !a.id.starts_with("only-domain:")
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            };
            a.show_actions(String::new(), items);
        });
        let a = self.clone();
        ui.on_req_detail_row(move |arg| a.open_link(&arg));
        let a = self.clone();
        ui.on_req_sheet_closed(move || {
            a.req.lock().unwrap().selected = [None; 3];
            a.republish();
        });
        let a = self.clone();
        ui.on_req_action(move |id| a.action(&id));
        let a = self.clone();
        let w = ui.as_weak();
        ui.on_cond_changed(move || {
            let Some(ui) = w.upgrade() else { return };
            {
                let mut req = a.req.lock().unwrap();
                let Some(d) = req.draft.as_mut() else { return };
                d.field = ui.get_cond_field().max(0) as usize;
                d.operator = ui.get_cond_op().clamp(0, 3) as usize;
                d.value = ui.get_cond_value().into();
            }
            a.publish_draft(false);
        });
        let a = self.clone();
        ui.on_cond_submit(move || {
            if a.req.lock().unwrap().submit_draft() {
                a.ui(|ui| ui.set_cond_open(false));
                a.republish();
            }
        });
        // Resources
        let a = self.clone();
        ui.on_res_update_all(move || a.update_all_geo());
        let a = self.clone();
        ui.on_res_set_auto(move |on| {
            a.store.lock().unwrap().state.settings.geo_auto_update = on;
            a.store.lock().unwrap().save();
            a.patch_geo();
            a.republish_resources();
        });
        let a = self.clone();
        ui.on_res_open_interval(move || {
            let hours = a.store.lock().unwrap().state.settings.geo_update_interval;
            a.ui(move |ui| {
                ui.set_res_dialog_title("自动更新间隔".into());
                ui.set_res_dialog_key("".into());
                ui.set_res_dialog_default("".into());
                ui.set_res_dialog_value(hours.to_string().into());
                ui.set_res_touched(false);
                ui.set_res_dialog("interval".into());
            });
        });
        let a = self.clone();
        // A script or module row has no edit (`_ScriptItem`, `_ModuleItem`).
        ui.on_res_tapped(move |key| {
            if !key.contains(':') {
                a.open_geo_url(&key);
            }
        });
        let a = self.clone();
        ui.on_res_menu(move |key| {
            let busy = match key.split_once(':') {
                Some(("script", id)) => a.script_syncing(id),
                Some((_, id)) => a.module_updating(id),
                None => resources::GEO
                    .iter()
                    .find(|g| g.1 == key.as_str())
                    .is_some_and(|g| a.geo_busy.lock().unwrap().contains(g.0)),
            };
            let item = |id: String, glyph: &str, label: &str| ActionItem {
                id: id.into(),
                glyph: glyph.into(),
                label: label.into(),
                enabled: true,
                ..Default::default()
            };
            let mut items = Vec::new();
            if !busy {
                items.push(item(format!("res-sync:{key}"), "sync", "同步"));
            }
            if !key.contains(':') {
                items.push(item(format!("res-edit:{key}"), "edit", "编辑"));
            }
            items.push(item(format!("res-copy:{key}"), "copy", "复制链接"));
            a.show_actions(String::new(), items);
        });
        let a = self.clone();
        ui.on_res_dialog_submit(move |kind, key, value| a.dialog_submitted(&kind, &key, &value));
        ui.on_res_check(|kind, value| {
            let error = match kind.as_str() {
                "interval" => resources::check_interval(&value).err(),
                _ => resources::check_url(&value).err(),
            };
            error.unwrap_or_default().into()
        });
        ui.on_res_digits(|value| resources::digits_only(&value).into());
    }
}
