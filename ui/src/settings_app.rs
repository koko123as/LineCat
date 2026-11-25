//! The tools page and the settings pages under it, on `App`'s side: what a tap or a switch
//! changes, saves and applies (lib/views/tools.dart, lib/views/config/general.dart,
//! developer.dart, about.dart, theme.dart).

use crate::settings_view::{self, choices};
use crate::{profile, store, ActionItem, App, AppWindow, SetSection, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The settings pages built so far; the tools list leaves the others out.
pub const PAGES: &[&str] = &["access", "backup"];

/// What a change to a general setting needs beyond saving it.
#[derive(PartialEq)]
enum Effect {
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
        ui.on_tools_page_changed(move || a.publish_settings());
        let a = self.clone();
        ui.on_set_clicked(move |page, id| a.set_clicked(&page, &id));
        let a = self.clone();
        ui.on_set_toggled(move |page, id, on| a.set_toggled(&page, &id, on));
        let a = self.clone();
        ui.on_choice_picked(move |id| a.choice_picked(&id));
        ui.on_input_check(|key, v1, v2| {
            if key.starts_with("host:") {
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
        let a = self.clone();
        ui.on_input_submit(move |key, v1, v2| a.input_submit(&key, &v1, &v2));
        let a = self.clone();
        ui.on_input_remove(move |key| {
            if let Some(host) = key.strip_prefix("host:") {
                a.update_general(Effect::Reapply, |g| {
                    g.hosts.remove(host);
                });
            }
        });
        let a = self.clone();
        ui.on_check_update(move || a.check_update(true));
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
                "hosts" => settings_view::hosts(&settings),
                "backup" => app.backup_sections(&settings),
                _ => return,
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
        let mut items: Vec<ActionItem> = choices(&settings, key)
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
        if key == "user-agent" {
            items.push(ActionItem {
                id: "user-agent-add:".into(),
                glyph: "edit".into(),
                label: "添加".into(),
                enabled: true,
                section: true,
                ..Default::default()
            });
        }
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
    fn open_input_with(
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
            ui.set_input_number(key == "port");
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

    fn update_settings(self: &Arc<Self>, effect: Effect, f: impl FnOnce(&mut store::Settings)) {
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

    fn update_general(self: &Arc<Self>, effect: Effect, f: impl FnOnce(&mut store::General)) {
        self.update_settings(effect, |s| f(&mut s.general));
    }

    fn set_clicked(self: &Arc<Self>, page: &str, id: &str) {
        let g = self.store.lock().unwrap().state.settings.general.clone();
        match (page, id) {
            ("general", "user-agent") => self.open_choice("user-agent", "用户代理"),
            ("general", "delay-timeout") => self.open_choice("delay-timeout", "延迟测试超时"),
            ("general", "port") => {
                self.open_input("port", "端口", "混合端口", &g.mixed_port.to_string())
            }
            ("general", "secret") => self.open_input("secret", "外部控制器密钥", "", &g.secret),
            ("general", "auth-user") => {
                self.open_input("auth-user", "账号", "", &g.authentication.username)
            }
            ("general", "auth-pass") => {
                self.open_input("auth-pass", "密码", "", &g.authentication.password)
            }
            ("general", "hosts") => self.open_page("hosts"),
            ("backup", id) => self.backup_clicked(id),
            ("hosts", "add") => self.open_input_with("host:", "添加", "键", "", "值", "", false),
            ("hosts", id) => {
                if let Some(host) = id.strip_prefix("host:") {
                    let value = g.hosts.get(host).cloned().unwrap_or_default();
                    self.open_input_with(id, "编辑", "键", host, "值", &value, true);
                }
            }
            ("developer", "message-test") => self.toast("info: 这是一条消息。"),
            ("developer", "logs-test") => self.logs_test(),
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
            ("general", "open-logs") => self.update_general(Effect::None, |g| g.open_logs = on),
            _ => {}
        }
    }

    fn choice_picked(self: &Arc<Self>, id: &str) {
        let (key, value) = id.split_once(':').unwrap_or((id, ""));
        let value = value.to_string();
        match key {
            "language" => self.update_settings(Effect::None, |s| {
                s.locale = (!value.is_empty()).then_some(value)
            }),
            "delay-timeout" => {
                if let Ok(ms) = value.parse::<i64>() {
                    self.update_general(Effect::Reapply, |g| g.delay_test_timeout = ms);
                }
            }
            "user-agent" => self.update_general(Effect::Reapply, |g| {
                g.global_ua = (!value.is_empty()).then_some(value)
            }),
            "user-agent-add" => self.open_input("user-agent-add", "添加", "用户代理", ""),
            "acc" => self.access_menu_picked(&value),
            "restore-option" => self.run_restore(if value == "all" {
                crate::backup::RestoreOption::All
            } else {
                crate::backup::RestoreOption::OnlyProfiles
            }),
            "restore-strategy" => self.set_restore_strategy(value == "override"),
            _ => {}
        }
    }

    fn input_submit(self: &Arc<Self>, key: &str, v1: &str, v2: &str) {
        let Ok(value) = settings_view::check_input(key, v1) else {
            return;
        };
        match key {
            "port" => {
                if let Ok(port) = value.parse::<i64>() {
                    self.update_general(Effect::Reapply, |g| g.mixed_port = port);
                }
            }
            "secret" => self.update_general(Effect::Reapply, |g| g.secret = value),
            "dav-file" => self.set_dav_file(&value),
            "auth-user" => {
                self.update_general(Effect::Reapply, |g| g.authentication.username = value)
            }
            "auth-pass" => {
                self.update_general(Effect::Reapply, |g| g.authentication.password = value)
            }
            "user-agent-add" => self.update_general(Effect::Reapply, |g| {
                if !g.user_agents.contains(&value) {
                    g.user_agents.push(value.clone());
                }
                g.global_ua = Some(value);
            }),
            _ => {
                if let Some(old) = key.strip_prefix("host:") {
                    let (host, address) = (value.trim().to_string(), v2.trim().to_string());
                    let old = old.to_string();
                    self.update_general(Effect::Reapply, |g| {
                        if !old.is_empty() {
                            g.hosts.remove(&old);
                        }
                        g.hosts.insert(host, address);
                    });
                }
            }
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
