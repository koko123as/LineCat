//! The tools page and the settings pages under it, on `App`'s side: what a tap or a switch
//! changes, saves and applies (lib/views/tools.dart, lib/views/config/general.dart,
//! developer.dart, about.dart, theme.dart).

use crate::settings_view::{self, choices};
use crate::{
    overrides, profile, store, ActionItem, Anchor, App, AppWindow, PageMeta, SetSection, CALL,
};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The settings pages built so far; the tools list leaves the others out.
pub const PAGES: &[&str] = &["access", "backup", "advanced"];

/// The list or map the list and map pages edit, each with the page it was opened from.
#[derive(Default)]
struct Targets {
    list: (String, String),
    map: (String, String),
}

fn targets() -> &'static std::sync::Mutex<Targets> {
    static T: std::sync::OnceLock<std::sync::Mutex<Targets>> = std::sync::OnceLock::new();
    T.get_or_init(Default::default)
}

/// The list a key names (a network list, or `<spec>:<path>` of an override), with its
/// title and the description its page shows.
pub(crate) fn list_get(
    s: &store::Settings,
    key: &str,
) -> Option<(Vec<String>, String, &'static str)> {
    let n = &s.network;
    match key {
        "bypass-domain" => Some((n.bypass_domain.clone(), "排除域名".into(), "")),
        "route-address" => Some((n.route_address.clone(), "路由地址".into(), "")),
        "route-exclude" => Some((n.route_exclude_address.clone(), "排除路由地址".into(), "")),
        _ => {
            let (spec, state, path) = override_of(s, key)?;
            let k = spec.key(path)?;
            let overrides::Kind::List(_, desc) = k.kind else {
                return None;
            };
            Some((
                overrides::strings(&spec.value(state, path)),
                k.label.into(),
                desc,
            ))
        }
    }
}

fn list_put(s: &mut store::Settings, key: &str, items: Vec<String>) {
    let n = &mut s.network;
    match key {
        "bypass-domain" => n.bypass_domain = items,
        "route-address" => n.route_address = items,
        "route-exclude" => n.route_exclude_address = items,
        _ => {
            if let Some((spec, state, path)) = override_mut(s, key) {
                spec.set(state, path, serde_json::json!(items));
            }
        }
    }
}

/// The map a key names (`hosts`, or `<spec>:<path>` of an override), with its title and
/// the description its page shows.
fn map_get(
    s: &store::Settings,
    key: &str,
) -> Option<(
    std::collections::BTreeMap<String, String>,
    String,
    &'static str,
)> {
    if key == "hosts" {
        return Some((s.general.hosts.clone(), "Hosts".into(), ""));
    }
    let (spec, state, path) = override_of(s, key)?;
    let k = spec.key(path)?;
    let overrides::Kind::Map(desc) = k.kind else {
        return None;
    };
    Some((
        overrides::pairs(&spec.value(state, path)),
        k.label.into(),
        desc,
    ))
}

fn map_put(s: &mut store::Settings, key: &str, map: std::collections::BTreeMap<String, String>) {
    if key == "hosts" {
        s.general.hosts = map;
    } else if let Some((spec, state, path)) = override_mut(s, key) {
        spec.set(state, path, serde_json::json!(map));
    }
}

/// The override state a spec id keeps in the settings.
fn override_state<'a>(s: &'a store::Settings, spec: &str) -> Option<&'a overrides::OverrideState> {
    match spec {
        "dns" => Some(&s.dns_override),
        "general" => Some(&s.general_override),
        _ => None,
    }
}

fn override_state_mut<'a>(
    s: &'a mut store::Settings,
    spec: &str,
) -> Option<&'a mut overrides::OverrideState> {
    match spec {
        "dns" => Some(&mut s.dns_override),
        "general" => Some(&mut s.general_override),
        _ => None,
    }
}

fn override_of<'a, 'k>(
    s: &'a store::Settings,
    key: &'k str,
) -> Option<(
    &'static overrides::Spec,
    &'a overrides::OverrideState,
    &'k str,
)> {
    let (id, path) = key.split_once(':')?;
    Some((overrides::spec(id)?, override_state(s, id)?, path))
}

fn override_mut<'a, 'k>(
    s: &'a mut store::Settings,
    key: &'k str,
) -> Option<(
    &'static overrides::Spec,
    &'a mut overrides::OverrideState,
    &'k str,
)> {
    let (id, path) = key.split_once(':')?;
    Some((overrides::spec(id)?, override_state_mut(s, id)?, path))
}

/// Points the list and map pages at a list and a map, each with its parent page.
pub fn set_targets(list: (&str, &str), map: (&str, &str)) {
    let mut t = targets().lock().unwrap();
    t.list = (list.0.into(), list.1.into());
    t.map = (map.0.into(), map.1.into());
}

/// What the page shell shows for a settings page; `known` false for the pages with their
/// own layout.
pub fn page_meta(settings: &store::Settings, page: &str) -> PageMeta {
    let meta = |title: &str, parent: &str| PageMeta {
        known: true,
        title: title.into(),
        parent: parent.into(),
        ..Default::default()
    };
    if let Some(meta) = crate::rules_app::page_meta(settings, page) {
        return meta;
    }
    if let Some(meta) = crate::items_app::page_meta(page) {
        return meta;
    }
    if let Some(meta) = crate::on_demand_app::page_meta(page) {
        return meta;
    }
    let t = targets().lock().unwrap();
    match page {
        "general" => meta("常规", ""),
        "developer" => meta("开发者模式", ""),
        "backup" => meta("备份与恢复", ""),
        "advanced" => meta("进阶配置", ""),
        "network" => meta("网络", "advanced"),
        "user-agents" => PageMeta {
            action: "add-circle".into(),
            action2: if settings.general.user_agents.len() > 1 {
                "sort".into()
            } else {
                "".into()
            },
            long_press: true,
            ..meta("用户代理", "general")
        },
        "list" | "map" => {
            let (key, parent) = if page == "list" { &t.list } else { &t.map };
            let s = settings;
            let title = if page == "list" {
                list_get(s, key).map(|l| l.1)
            } else {
                map_get(s, key).map(|m| m.1)
            };
            PageMeta {
                fab: true,
                ..meta(&title.unwrap_or_default(), parent)
            }
        }
        id => match overrides::spec(id) {
            Some(spec) => PageMeta {
                action: "add-circle".into(),
                empty: format!("暂无{}", spec.entries).into(),
                empty_desc: spec.description.into(),
                empty_action: spec.add.into(),
                ..meta(spec.title, "advanced")
            },
            None => PageMeta::default(),
        },
    }
}

/// What a change to a general setting needs beyond saving it.
#[derive(PartialEq)]
pub(crate) enum Effect {
    None,
    /// The Core takes it with the next `config.apply`.
    Reapply,
}

fn models(sections: Vec<SetSection>) -> ModelRc<SetSection> {
    ModelRc::new(VecModel::from(sections))
}

impl App {
    pub(crate) fn bind_settings(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_tools_clicked(move |id| a.tools_clicked(&id));
        let a = self.clone();
        let w = ui.as_weak();
        ui.on_tools_page_changed(move || {
            if let Some(ui) = w.upgrade() {
                a.rules_page_shown(&ui.get_tools_page());
            }
            a.publish_settings();
        });
        let a = self.clone();
        ui.on_set_clicked(move |page, id| a.set_clicked(&page, &id));
        let a = self.clone();
        ui.on_set_toggled(move |page, id, on| a.set_toggled(&page, &id, on));
        let a = self.clone();
        ui.on_choice_picked(move |id| a.choice_picked(&id));
        let a = self.clone();
        ui.on_page_meta(move |page, _| {
            let settings = a.store.lock().unwrap().state.settings.clone();
            page_meta(&settings, &page)
        });
        let a = self.clone();
        ui.on_input_check(move |key, v1, v2| {
            if let Some(e) = a.items_check(&key, &v1, &v2) {
                return e.into();
            }
            if let Some(e) = a.on_demand_check(&key, &v1) {
                return e.into();
            }
            if key.starts_with("map:") {
                return if v1.trim().is_empty() {
                    "键不能为空".into()
                } else if v2.trim().is_empty() {
                    "值不能为空".into()
                } else {
                    SharedString::new()
                };
            }
            settings_view::check_input(&key, &v1)
                .err()
                .unwrap_or_default()
                .into()
        });
        ui.on_ports_check(|m, h, s| {
            settings_view::check_ports([&m, &h, &s])
                .err()
                .unwrap_or_default()
                .into()
        });
        let a = self.clone();
        ui.on_ports_submit(move |m, h, s| {
            if let Ok([m, h, s]) = settings_view::check_ports([&m, &h, &s]) {
                a.update_general(Effect::Reapply, |g| {
                    g.mixed_port = m;
                    g.http_port = h;
                    g.socks_port = s;
                });
            }
        });
        let a = self.clone();
        ui.on_input_submit(move |key, v1, v2| a.input_submit(&key, &v1, &v2));
        let a = self.clone();
        ui.on_input_remove(move |key| {
            if a.items_removed(&key) {
                return;
            }
            if key.starts_with("list:") {
                a.list_set(&key, None);
            }
            if key.starts_with("map:") {
                a.map_set(&key, None);
            }
            if let Some(i) = key
                .strip_prefix("ua-edit:")
                .and_then(|i| i.parse::<usize>().ok())
            {
                a.update_general(Effect::Reapply, |g| {
                    if i < g.user_agents.len() {
                        let old = g.user_agents.remove(i);
                        if g.global_ua.as_deref() == Some(old.as_str()) {
                            g.global_ua = None;
                        }
                    }
                });
            }
        });
        let a = self.clone();
        ui.on_ctx_picked(move |kind, arg, _, id| {
            let Ok(i) = arg.parse::<usize>() else { return };
            match (kind.as_str(), id.as_str()) {
                ("ua", "edit") => a.set_clicked("user-agents", &format!("edit:ua:{i}")),
                ("ua", "delete") => a.confirm(
                    "提示",
                    "确定删除当前用户代理吗？".into(),
                    "ua-delete",
                    i.to_string(),
                ),
                _ => {}
            }
        });
        let a = self.clone();
        ui.on_check_update(move || a.check_update(true));
        let a = self.clone();
        ui.on_od_clicked(move |id| a.on_demand_clicked(&id));
        let a = self.clone();
        ui.on_about_logo_tapped(move || a.logo_tapped());
        self.publish_settings();
    }

    /// The tools list and the open settings page, from the settings as they are.
    pub(crate) fn publish_settings(self: &Arc<Self>) {
        let settings = self.store.lock().unwrap().state.settings.clone();
        let app = self.clone();
        self.ui(move |ui| {
            let window = ui.window();
            let width = window.size().width as f32 / window.scale_factor().max(0.1);
            let mobile = width <= 600.0;
            ui.set_tools_sections(models(settings_view::tools(&settings, mobile, PAGES)));
            ui.set_pure_black(settings.pure_black);
            let page = ui.get_tools_page();
            let sections = match page.as_str() {
                "general" => settings_view::general(&settings),
                "developer" => settings_view::developer(&settings),
                "about" => settings_view::about(&settings),
                "backup" => app.backup_sections(&settings),
                "advanced" => settings_view::advanced(&settings),
                "network" => settings_view::network(&settings),
                "on-demand" => Vec::new(),
                "user-agents" => settings_view::user_agents(&settings),
                "list" => {
                    let key = targets().lock().unwrap().list.0.clone();
                    list_get(&settings, &key)
                        .map(|(items, _, desc)| settings_view::list(&items, desc))
                        .unwrap_or_default()
                }
                "map" => {
                    let key = targets().lock().unwrap().map.0.clone();
                    map_get(&settings, &key)
                        .map(|(map, _, desc)| settings_view::map(&map, desc))
                        .unwrap_or_default()
                }
                id => match (overrides::spec(id), override_state(&settings, id)) {
                    (Some(spec), Some(state)) => spec.sections(state),
                    _ => match crate::rules_app::sections(&settings, id)
                        .or_else(|| crate::items_app::sections(&settings, id))
                    {
                        Some(s) => s,
                        None => return,
                    },
                },
            };
            ui.set_page_sections(models(sections));
        });
    }

    fn open_page(self: &Arc<Self>, page: &str) {
        let page: SharedString = page.into();
        self.ui(move |ui| {
            if page == "about" && ui.get_app_icon().size().width == 0 {
                if let Some(icon) = settings_view::app_icon() {
                    ui.set_app_icon(icon);
                }
            }
            ui.set_tools_page(page);
        });
    }

    fn tools_clicked(self: &Arc<Self>, id: &str) {
        match id {
            "language" => self.open_choice("language", "语言"),
            "access" => {
                self.open_access();
                self.open_page(id);
            }
            "backup" => {
                self.open_page(id);
                self.ping_dav();
            }
            _ => self.open_page(id),
        }
    }

    fn open_choice(self: &Arc<Self>, key: &str, title: &str) {
        let settings = self.store.lock().unwrap().state.settings.clone();
        let items: Vec<ActionItem> = choices(&settings, key)
            .into_iter()
            .map(|(id, label, checked)| ActionItem {
                id: format!("{key}:{id}").into(),
                label: label.into(),
                check: true,
                checked,
                enabled: true,
                ..Default::default()
            })
            .collect();
        let title: SharedString = title.into();
        self.ui(move |ui| {
            ui.set_choice_title(title);
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    pub(crate) fn open_input(self: &Arc<Self>, key: &str, title: &str, label: &str, value: &str) {
        self.open_input_with(key, title, label, value, "", "", false);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open_input_with(
        self: &Arc<Self>,
        key: &str,
        title: &str,
        label: &str,
        value: &str,
        label2: &str,
        value2: &str,
        removable: bool,
    ) {
        let (key, title, label, value, label2, value2): (
            SharedString,
            SharedString,
            SharedString,
            SharedString,
            SharedString,
            SharedString,
        ) = (
            key.into(),
            title.into(),
            label.into(),
            value.into(),
            label2.into(),
            value2.into(),
        );
        self.ui(move |ui| {
            ui.set_input_number(key == "mtu");
            ui.set_input_key(key);
            ui.set_input_title(title);
            ui.set_input_label(label);
            ui.set_input_value(value);
            ui.set_input_label2(label2);
            ui.set_input_value2(value2);
            ui.set_input_removable(removable);
            ui.set_input_open(true);
        });
    }

    pub(crate) fn update_settings(
        self: &Arc<Self>,
        effect: Effect,
        f: impl FnOnce(&mut store::Settings),
    ) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings);
            store.save();
        }
        self.publish_settings();
        if effect == Effect::Reapply {
            self.reapply();
        }
    }

    pub(crate) fn update_general(
        self: &Arc<Self>,
        effect: Effect,
        f: impl FnOnce(&mut store::General),
    ) {
        self.update_settings(effect, |s| f(&mut s.general));
    }

    /// `_handleDelete`: the user agent goes, and the global one with it when it was that.
    pub(crate) fn delete_user_agent(self: &Arc<Self>, index: &str) {
        let Ok(i) = index.parse::<usize>() else {
            return;
        };
        self.update_general(Effect::Reapply, |g| {
            if i < g.user_agents.len() {
                let old = g.user_agents.remove(i);
                if g.global_ua.as_deref() == Some(old.as_str()) {
                    g.global_ua = None;
                }
            }
        });
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
    }

    fn set_clicked(self: &Arc<Self>, page: &str, id: &str) {
        if self.rules_clicked(page, id) || self.items_clicked(page, id) {
            return;
        }
        let g = self.store.lock().unwrap().state.settings.general.clone();
        match (page, id) {
            ("general", "user-agent") => self.open_page("user-agents"),
            ("general", "on-demand") => {
                self.open_on_demand();
                self.open_page("on-demand");
            }
            ("user-agents", "action") => self.open_input("user-agent-add", "添加", "用户代理", ""),
            ("user-agents", "action2") => {
                let titles: Vec<SharedString> =
                    g.user_agents.iter().map(|u| u.as_str().into()).collect();
                self.ui(move |ui| {
                    ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles)));
                    ui.set_set_sorting(true);
                });
            }
            ("user-agents", id) if id.starts_with("sort-move:") => {
                let mut it = id["sort-move:".len()..]
                    .split(':')
                    .filter_map(|n| n.parse::<usize>().ok());
                if let (Some(from), Some(to)) = (it.next(), it.next()) {
                    self.update_general(Effect::None, |g| {
                        crate::reorder::move_item(&mut g.user_agents, from, to)
                    });
                    let titles: Vec<SharedString> = self
                        .store
                        .lock()
                        .unwrap()
                        .state
                        .settings
                        .general
                        .user_agents
                        .iter()
                        .map(|u| u.as_str().into())
                        .collect();
                    self.ui(move |ui| ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles))));
                }
            }
            ("user-agents", id) if id.starts_with("long:ua:") => {
                if let Ok(i) = id["long:ua:".len()..].parse::<usize>() {
                    self.ui(move |ui| {
                        let anchor = ui.global::<Anchor>();
                        ui.set_ctx_names(ModelRc::default());
                        ui.set_ctx_items(ModelRc::new(VecModel::from(vec![
                            ActionItem {
                                id: "edit".into(),
                                glyph: "edit".into(),
                                label: "编辑".into(),
                                enabled: true,
                                ..Default::default()
                            },
                            ActionItem {
                                id: "delete".into(),
                                glyph: "delete".into(),
                                label: "删除".into(),
                                danger: true,
                                enabled: true,
                                ..Default::default()
                            },
                        ])));
                        ui.set_ctx_a(i.to_string().into());
                        ui.set_ctx_x(anchor.get_x());
                        ui.set_ctx_y(anchor.get_y());
                        ui.set_ctx_w(anchor.get_w());
                        ui.set_ctx_h(anchor.get_h());
                        ui.set_ctx_kind("ua".into());
                    });
                }
            }
            ("user-agents", id) => {
                if let Some(i) = id
                    .strip_prefix("edit:ua:")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    let value = g.user_agents.get(i).cloned().unwrap_or_default();
                    self.open_input_with(
                        &format!("ua-edit:{i}"),
                        "编辑",
                        "用户代理",
                        &value,
                        "",
                        "",
                        false,
                    );
                } else if let Some(i) = id.strip_prefix("ua:") {
                    let ua = i
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| g.user_agents.get(i).cloned());
                    self.update_general(Effect::Reapply, |g| g.global_ua = ua);
                }
            }
            ("general", "delay-timeout") => self.open_choice("delay-timeout", "延迟测试超时"),
            ("general", "port") => {
                let ports = [g.mixed_port, g.http_port, g.socks_port]
                    .map(|p| SharedString::from(p.to_string()));
                self.ui(move |ui| {
                    let [m, h, s] = ports;
                    ui.set_port_mixed(m);
                    ui.set_port_http(h);
                    ui.set_port_socks(s);
                    ui.set_ports_open(true);
                });
            }
            ("general", "secret") => self.open_input("secret", "外部控制器密钥", "", &g.secret),
            ("general", "auth-user") => {
                self.open_input("auth-user", "账号", "", &g.authentication.username)
            }
            ("general", "auth-pass") => {
                self.open_input("auth-pass", "密码", "", &g.authentication.password)
            }
            ("backup", id) => self.backup_clicked(id),
            ("advanced", "network") => self.open_page("network"),
            ("network", "mtu") => {
                let mtu = self.store.lock().unwrap().state.settings.network.mtu;
                self.open_input("mtu", "MTU", "", &mtu.to_string());
            }
            ("network", "congestion") => self.open_choice("congestion", "TCP 拥塞控制"),
            ("network", key @ ("bypass-domain" | "route-address" | "route-exclude")) => {
                self.open_list(key, "network")
            }
            ("list", "add") => {
                let key = targets().lock().unwrap().list.0.clone();
                self.open_input(&format!("list:{key}:new"), "添加", "", "");
            }
            ("list", id) => {
                if let Some(i) = id
                    .strip_prefix("item:")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    let key = targets().lock().unwrap().list.0.clone();
                    let s = self.store.lock().unwrap().state.settings.clone();
                    let value = list_get(&s, &key)
                        .and_then(|(v, _, _)| v.get(i).cloned())
                        .unwrap_or_default();
                    self.open_input_with(
                        &format!("list:{key}:{i}"),
                        "编辑",
                        "",
                        &value,
                        "",
                        "",
                        true,
                    );
                }
            }
            ("general", "hosts") => self.open_map("hosts", "general"),
            ("map", "add") => {
                let key = targets().lock().unwrap().map.0.clone();
                self.open_input_with(&format!("map:{key}\t"), "添加", "键", "", "值", "", false)
            }
            ("map", id) => {
                if let Some(name) = id.strip_prefix("entry:") {
                    let key = targets().lock().unwrap().map.0.clone();
                    let s = self.store.lock().unwrap().state.settings.clone();
                    let value = map_get(&s, &key)
                        .and_then(|(m, _, _)| m.get(name).cloned())
                        .unwrap_or_default();
                    self.open_input_with(
                        &format!("map:{key}\t{name}"),
                        "编辑",
                        "键",
                        name,
                        "值",
                        &value,
                        true,
                    );
                }
            }
            ("advanced", id)
                if overrides::spec(id).is_some()
                    || matches!(id, "rules" | "scripts" | "filters") =>
            {
                self.open_page(id)
            }
            (page, id) if overrides::spec(page).is_some() => self.override_clicked(page, id),
            ("developer", "message-test") => self.message_test(),
            ("developer", "logs-test") => self.logs_test(),
            ("developer", "live-status-test") => self.live_preview(),
            ("developer", "clear-data") => self.confirm(
                "提示",
                "确定要清除所有数据？".into(),
                "clear-data",
                String::new(),
            ),
            ("developer", "prune-cache") => self.prune_cache(),
            ("about", id) => {
                if let Some(url) = id.strip_prefix("link:") {
                    let url = url.to_string();
                    self.spawn(move |app| {
                        if let Err(e) = app
                            .bridge()
                            .and_then(|b| b.shell("url.open", json!({"url": url}), CALL))
                        {
                            app.toast(e);
                        }
                    });
                }
            }
            _ => {}
        }
    }

    fn set_toggled(self: &Arc<Self>, page: &str, id: &str, on: bool) {
        if self.rules_toggled(page, id, on) {
            return;
        }
        match (page, id) {
            ("developer", "developer-mode") => {
                self.update_settings(Effect::None, |s| s.developer_mode = on)
            }
            ("about", "auto-check-update") => {
                self.update_settings(Effect::None, |s| s.auto_check_update = on)
            }
            ("general", "auto-run") => self.update_general(Effect::None, |g| g.auto_run = on),
            ("general", "minimize-on-exit") => {
                self.update_general(Effect::None, |g| g.minimize_on_exit = on)
            }
            ("general", "hidden") => {
                self.update_general(Effect::None, |g| g.hidden = on);
                self.apply_hidden(on);
            }
            ("general", "stop-action") => {
                self.update_general(Effect::None, |g| g.show_notification_stop_action = on)
            }
            ("general", "check-certificate") => {
                self.update_general(Effect::Reapply, |g| g.check_certificate = on)
            }
            ("general", "allow-lan") => self.update_general(Effect::Reapply, |g| g.allow_lan = on),
            ("general", "external-controller") => self.update_general(Effect::Reapply, |g| {
                g.external_controller = on;
                if on && g.secret.is_empty() {
                    g.secret = settings_view::random_secret(16);
                }
            }),
            ("general", "controller-cors") => {
                self.update_general(Effect::Reapply, |g| g.external_controller_cors = on)
            }
            ("general", "authentication") => self.update_general(Effect::Reapply, |g| {
                g.authentication.enable = on;
                if on && g.authentication.username.is_empty() {
                    g.authentication.username = settings_view::random_secret(8);
                    g.authentication.password = settings_view::random_secret(16);
                }
            }),
            ("general", "find-process") => {
                self.update_general(Effect::Reapply, |g| g.find_process_always = on)
            }
            ("general", "close-connections") => {
                self.update_general(Effect::None, |g| g.close_connections = on)
            }
            ("general", "only-proxy") => {
                self.update_general(Effect::None, |g| g.only_statistics_proxy = on)
            }
            ("general", "append-system-dns") => {
                self.update_general(Effect::Reapply, |g| g.append_system_dns = on)
            }
            ("network", id) => self.update_settings(Effect::Reapply, |s| {
                let n = &mut s.network;
                match id {
                    "vpn" => n.vpn = on,
                    "system-proxy" => n.system_proxy = on,
                    "allow-bypass" => n.allow_bypass = on,
                    "ipv6" => n.ipv6 = on,
                    "dns-hijacking" => n.dns_hijacking = on,
                    "bypass-private" => n.bypass_private_route = on,
                    _ => {}
                }
            }),
            ("general", "open-logs") => self.update_general(Effect::None, |g| g.open_logs = on),
            (page, id) if overrides::spec(page).is_some() => self.override_set(page, id, on.into()),
            _ => {}
        }
    }

    fn choice_picked(self: &Arc<Self>, id: &str) {
        let (key, value) = id.split_once(':').unwrap_or((id, ""));
        let value = value.to_string();
        if self.rules_picked(key, &value)
            || self.items_picked(key, &value)
            || self.theme_picked(key, &value)
            || self.logs_picked(key, &value)
        {
            return;
        }
        match key {
            "language" => self.update_settings(Effect::None, |s| {
                s.locale = (!value.is_empty()).then_some(value)
            }),
            "delay-timeout" => {
                if let Ok(ms) = value.parse::<i64>() {
                    self.update_general(Effect::Reapply, |g| g.delay_test_timeout = ms);
                }
            }
            "acc" => self.access_menu_picked(&value),
            "congestion" => {
                self.update_settings(Effect::None, |s| s.network.congestion_controller = value)
            }
            "restore-option" => self.run_restore(if value == "all" {
                crate::backup::RestoreOption::All
            } else {
                crate::backup::RestoreOption::OnlyProfiles
            }),
            "restore-strategy" => self.set_restore_strategy(value == "override"),
            "ov" => {
                if let Some((target, option)) = value.rsplit_once(':') {
                    if let Some((id, path)) = target.split_once(':') {
                        self.override_set(id, path, option.into());
                    }
                }
            }
            "ovadd" => {
                if let Some((id, path)) = value.split_once(':') {
                    let (id, path) = (id.to_string(), path.to_string());
                    self.update_settings(Effect::Reapply, move |s| {
                        if let (Some(spec), Some(state)) =
                            (overrides::spec(&id), override_state_mut(s, &id))
                        {
                            spec.add(state, &path);
                        }
                    });
                }
            }
            _ => {}
        }
    }

    fn input_submit(self: &Arc<Self>, key: &str, v1: &str, v2: &str) {
        if self.items_input(key, v1, v2) || self.on_demand_input(key, v1) {
            return;
        }
        let Ok(value) = settings_view::check_input(key, v1) else {
            return;
        };
        if self.rules_input(key, &value) {
            return;
        }
        match key {
            "secret" => self.update_general(Effect::Reapply, |g| g.secret = value),
            "dav-file" => self.set_dav_file(&value),
            "mtu" => {
                if let Ok(mtu) = value.parse::<i64>() {
                    self.update_settings(Effect::None, |s| s.network.mtu = mtu);
                }
            }
            "auth-user" => {
                self.update_general(Effect::Reapply, |g| g.authentication.username = value)
            }
            "auth-pass" => {
                self.update_general(Effect::Reapply, |g| g.authentication.password = value)
            }
            "user-agent-add" => {
                self.update_general(Effect::None, |g| {
                    if !g.user_agents.contains(&value) {
                        g.user_agents.push(value);
                    }
                });
                self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
            }
            k if k.starts_with("list:") => self.list_set(k, Some(value)),
            k if k.starts_with("ua-edit:") => {
                if let Ok(i) = k["ua-edit:".len()..].parse::<usize>() {
                    self.update_general(Effect::Reapply, |g| {
                        if i < g.user_agents.len() && !g.user_agents.contains(&value) {
                            let old = std::mem::replace(&mut g.user_agents[i], value.clone());
                            if g.global_ua.as_deref() == Some(old.as_str()) {
                                g.global_ua = Some(value);
                            }
                        }
                    });
                }
            }
            k if k.starts_with("map:") => {
                self.map_set(k, Some((value, v2.trim().to_string())));
            }
            k if k.starts_with("ov:") => {
                let (id, path) = k[3..].split_once(':').unwrap_or_default();
                if let Some(spec) = overrides::spec(id) {
                    if let Ok(v) = spec.check(path, &value) {
                        self.override_set(id, path, v);
                    }
                }
            }
            _ => {}
        }
    }

    /// A list page's add (`list:<key>:new`), edit (`list:<key>:<i>`) or removal (None).
    fn list_set(self: &Arc<Self>, key: &str, value: Option<String>) {
        let Some((list, at)) = key.strip_prefix("list:").and_then(|k| k.rsplit_once(':')) else {
            return;
        };
        let (list, at) = (list.to_string(), at.to_string());
        self.update_settings(Effect::Reapply, move |s| {
            let Some((mut items, _, _)) = list_get(s, &list) else {
                return;
            };
            match (at.parse::<usize>().ok(), value) {
                (None, Some(v)) => {
                    if !items.contains(&v) {
                        items.push(v);
                    }
                }
                (Some(i), Some(v)) if i < items.len() => items[i] = v,
                (Some(i), None) if i < items.len() => {
                    items.remove(i);
                }
                _ => {}
            }
            list_put(s, &list, items);
        });
    }

    /// A map page's add (`map:<key>\t`), edit (`map:<key>\t<name>`) or removal (None).
    fn map_set(self: &Arc<Self>, key: &str, entry: Option<(String, String)>) {
        let Some((map, old)) = key.strip_prefix("map:").and_then(|k| k.split_once('\t')) else {
            return;
        };
        let (map, old) = (map.to_string(), old.to_string());
        self.update_settings(Effect::Reapply, move |s| {
            let Some((mut entries, _, _)) = map_get(s, &map) else {
                return;
            };
            if !old.is_empty() {
                entries.remove(&old);
            }
            if let Some((name, value)) = entry {
                entries.insert(name, value);
            }
            map_put(s, &map, entries);
        });
    }

    fn open_list(self: &Arc<Self>, key: &str, parent: &str) {
        targets().lock().unwrap().list = (key.to_string(), parent.to_string());
        self.open_page("list");
    }

    fn open_map(self: &Arc<Self>, key: &str, parent: &str) {
        targets().lock().unwrap().map = (key.to_string(), parent.to_string());
        self.open_page("map");
    }

    fn override_set(self: &Arc<Self>, id: &str, path: &str, value: Value) {
        let (id, path) = (id.to_string(), path.to_string());
        self.update_settings(Effect::Reapply, move |s| {
            if let (Some(spec), Some(state)) = (overrides::spec(&id), override_state_mut(s, &id)) {
                spec.set(state, &path, value);
            }
        });
    }

    /// A tap on an override page: the bar's add, a row's remove button, or the row itself.
    fn override_clicked(self: &Arc<Self>, page: &str, id: &str) {
        let Some(spec) = overrides::spec(page) else {
            return;
        };
        let settings = self.store.lock().unwrap().state.settings.clone();
        let Some(state) = override_state(&settings, page) else {
            return;
        };
        if id == "action" {
            if !spec.can_add(state) {
                return;
            }
            let mut last = "";
            let items: Vec<ActionItem> = spec
                .addable(state)
                .into_iter()
                .map(|(section, k)| {
                    let heading = if section != last { section } else { "" };
                    last = section;
                    ActionItem {
                        id: format!("ovadd:{page}:{}", k.path).into(),
                        heading: heading.into(),
                        label: k.label.into(),
                        subtitle: k.desc.into(),
                        enabled: true,
                        ..Default::default()
                    }
                })
                .collect();
            let title: SharedString = spec.add.into();
            self.ui(move |ui| {
                ui.set_choice_title(title);
                ui.set_choice_items(ModelRc::new(VecModel::from(items)));
                ui.set_choice_open(true);
            });
            return;
        }
        if let Some(path) = id.strip_prefix("remove:") {
            let (page, path) = (page.to_string(), path.to_string());
            self.update_settings(Effect::Reapply, move |s| {
                if let Some(state) = override_state_mut(s, &page) {
                    spec.remove(state, &path);
                }
            });
            return;
        }
        let Some(k) = spec.key(id) else {
            return;
        };
        let key = format!("{page}:{id}");
        match k.kind {
            overrides::Kind::List(..) => self.open_list(&key, page),
            overrides::Kind::Map(_) => self.open_map(&key, page),
            overrides::Kind::Options(options) => {
                let current = spec.text(state, id);
                let items: Vec<ActionItem> = options
                    .iter()
                    .map(|o| ActionItem {
                        id: format!("ov:{key}:{o}").into(),
                        label: (*o).into(),
                        check: true,
                        checked: *o == current,
                        enabled: true,
                        ..Default::default()
                    })
                    .collect();
                let title: SharedString = k.label.into();
                self.ui(move |ui| {
                    ui.set_choice_title(title);
                    ui.set_choice_items(ModelRc::new(VecModel::from(items)));
                    ui.set_choice_open(true);
                });
            }
            overrides::Kind::Number { .. } | overrides::Kind::Text(_) => {
                let number = matches!(k.kind, overrides::Kind::Number { .. });
                self.open_input(&format!("ov:{key}"), k.label, "", &spec.text(state, id));
                if number {
                    self.ui(|ui| ui.set_input_number(true));
                }
            }
            overrides::Kind::Toggle => {}
        }
    }

    /// `updateExcludeFromRecents`.
    pub(crate) fn apply_hidden(self: &Arc<Self>, hidden: bool) {
        self.spawn(move |app| {
            if let Ok(b) = app.bridge() {
                let _ = b.shell("app.hidden", json!({"value": hidden}), CALL);
            }
        });
    }

    /// `messageTest`: one banner for each level, in turn.
    fn message_test(self: &Arc<Self>) {
        let app = self.clone();
        std::thread::spawn(move || {
            for level in ["info", "success", "warning", "error"] {
                app.toast(format!("{level}: 这是一条消息。"));
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
        });
    }

    fn logs_test(self: &Arc<Self>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        for i in 0..1000 {
            let text = format!("[{i}]{}", settings_view::random_secret(20 + i % 180));
            self.push_log(&json!({"level": "info", "time": now, "text": text}));
        }
    }

    /// `shakingStore`: profile files no profile names any more go.
    fn prune_cache(self: &Arc<Self>) {
        let store = self.store.lock().unwrap();
        let keep: std::collections::HashSet<String> = store
            .state
            .profiles
            .iter()
            .map(|p| format!("{}.yaml", p.id))
            .collect();
        if let Ok(dir) = std::fs::read_dir(store.dir.join("profiles")) {
            for entry in dir.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !keep.contains(&name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    /// `handleClear`: every setting and profile goes, then the app exits.
    pub(crate) fn clear_data(self: &Arc<Self>) {
        self.spawn(|app| {
            if let Ok(b) = app.bridge() {
                let _ = b.shell("vpn.stop", json!({}), Duration::from_secs(20));
            }
            let dir = app.store.lock().unwrap().dir.clone();
            let _ = std::fs::remove_dir_all(&dir);
            std::process::exit(0);
        });
    }

    /// `_DeveloperModeDetector`: five taps, each within a second of the last.
    fn logo_tapped(self: &Arc<Self>) {
        let enough = {
            let mut taps = self.logo_taps.lock().unwrap();
            let now = Instant::now();
            if taps
                .1
                .is_some_and(|t| now.duration_since(t) > Duration::from_secs(1))
            {
                taps.0 = 0;
            }
            taps.0 += 1;
            taps.1 = Some(now);
            let enough = taps.0 >= 5;
            if enough {
                *taps = (0, None);
            }
            enough
        };
        if enough {
            self.update_settings(Effect::None, |s| s.developer_mode = true);
            self.toast("开发者模式已启用。");
        }
    }

    /// `checkForUpdate` on the stable channel: the latest release of the repository.
    pub(crate) fn check_update(self: &Arc<Self>, by_user: bool) {
        self.ui(|ui| ui.set_update_checking(true));
        self.spawn(move |app| {
            let version = app.app_version.lock().unwrap().clone();
            let latest = profile::latest_release("koko123as/LineCat");
            app.ui(|ui| ui.set_update_checking(false));
            match latest {
                Ok(Some(release)) if settings_view::is_newer(&release.tag, &version) => {
                    app.alert(
                        "发现新版本",
                        format!("{}\n\n{}\n\n{}", release.tag, release.notes, release.url),
                    );
                }
                Ok(_) if by_user => app.toast("当前应用已经是最新版了"),
                Err(e) if by_user => app.toast(e),
                _ => {}
            }
        });
    }

    /// What the shell's hello says about the app, and what runs once it is connected.
    pub(crate) fn on_connected(self: &Arc<Self>, hello: &Value) {
        if let Some(v) = hello.get("versionName").and_then(Value::as_str) {
            *self.app_version.lock().unwrap() = v.to_string();
            let label: SharedString = format!("v{}", v.trim_start_matches('v')).into();
            self.ui(move |ui| ui.set_app_version(label));
        }
        let (general, auto_check) = {
            let s = &self.store.lock().unwrap().state.settings;
            (s.general.clone(), s.auto_check_update)
        };
        if general.hidden {
            self.apply_hidden(true);
        }
        if !self
            .started_once
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            if auto_check {
                self.check_update(false);
            }
            let running = self.run_state.lock().unwrap().as_str() == "STARTED";
            if general.auto_run && !running && self.store.lock().unwrap().active().is_some() {
                self.toggle_vpn();
            }
        }
    }
}
