//! The access control page on `App`'s side: the installed apps from the shell's `packages.*`
//! ops, the draft selection, and saving it into `vpnOptions`. FlClash: lib/views/access.dart.

use crate::access::{self, AccessControl, Mode, Package, Sort};
use crate::{AccessRow, ActionItem, App, AppWindow, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The page while it is open.
#[derive(Default)]
struct Page {
    packages: Vec<Package>,
    loaded: bool,
    granted: bool,
    /// `_pinedList`: the selection as it was when the page opened or the mode changed.
    pinned: Vec<String>,
    draft: AccessControl,
    query: String,
}

fn page() -> &'static Mutex<Page> {
    static PAGE: OnceLock<Mutex<Page>> = OnceLock::new();
    PAGE.get_or_init(|| {
        Mutex::new(Page {
            granted: true,
            ..Default::default()
        })
    })
}

/// The first character of a label, for the icon's place.
fn monogram(label: &str) -> String {
    label
        .chars()
        .find(|c| !c.is_whitespace())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

impl App {
    pub(crate) fn bind_access(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_acc_back(move || a.access_back());
        let a = self.clone();
        ui.on_acc_toggle(move |name| {
            page().lock().unwrap().draft.toggle(&name);
            a.publish_access();
        });
        let a = self.clone();
        ui.on_acc_select_all(move || {
            {
                let mut p = page().lock().unwrap();
                let names = Self::visible_names(&p);
                p.draft.select_all(&names);
            }
            a.publish_access();
        });
        let a = self.clone();
        ui.on_acc_save(move || a.access_save());
        let a = self.clone();
        ui.on_acc_menu(move || a.access_menu());
        let a = self.clone();
        ui.on_acc_enable(move || a.access_update(|d| d.enable = true));
        let a = self.clone();
        ui.on_acc_grant(move || a.access_grant());
        let a = self.clone();
        ui.on_acc_search(move |q| {
            {
                let mut p = page().lock().unwrap();
                p.query = q.to_string();
                p.pinned = p.draft.current().clone();
            }
            a.publish_access();
        });
    }

    /// Opening the page: the saved choice becomes the draft, then the apps load.
    pub(crate) fn open_access(self: &Arc<Self>) {
        let saved = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .access_control
            .clone();
        {
            let mut p = page().lock().unwrap();
            p.pinned = saved.current().clone();
            p.draft = saved;
            p.query.clear();
        }
        self.ui(|ui| {
            ui.set_acc_searching(false);
            ui.set_acc_query("".into());
        });
        self.publish_access();
        self.spawn(|app| app.load_packages());
    }

    fn load_packages(self: &Arc<Self>) {
        let result = self
            .bridge()
            .and_then(|b| b.shell("packages.list", json!({}), Duration::from_secs(60)));
        {
            let mut p = page().lock().unwrap();
            if let Ok(r) = &result {
                p.packages = r
                    .get("packages")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                p.granted = r.get("granted").and_then(Value::as_bool).unwrap_or(true);
            }
            p.loaded = true;
        }
        if let Err(e) = result {
            self.toast(e);
        }
        self.publish_access();
    }

    fn visible_names(p: &Page) -> Vec<String> {
        p.draft
            .view(&p.packages, &p.pinned)
            .into_iter()
            .filter(|pkg| access::matches(pkg, &p.query))
            .map(|pkg| pkg.package_name.clone())
            .collect()
    }

    pub(crate) fn publish_access(self: &Arc<Self>) {
        let saved = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .access_control
            .sorted();
        let p = page().lock().unwrap();
        let d = &p.draft;
        let selected: std::collections::HashSet<&String> = d.current().iter().collect();
        let rows: Vec<AccessRow> = d
            .view(&p.packages, &p.pinned)
            .into_iter()
            .filter(|pkg| access::matches(pkg, &p.query))
            .map(|pkg| AccessRow {
                name: pkg.package_name.as_str().into(),
                label: pkg.label.as_str().into(),
                monogram: monogram(&pkg.label).into(),
                checked: selected.contains(&pkg.package_name),
            })
            .collect();
        let names: Vec<String> = rows.iter().map(|r| r.name.to_string()).collect();
        let installed = p
            .packages
            .iter()
            .filter(|pkg| selected.contains(&pkg.package_name))
            .count();
        let needs_permission = p.packages.is_empty() && !p.granted && p.loaded;
        let authorize = d.enable && !p.granted;
        let status = if authorize {
            "需要读取应用列表权限"
        } else if d.enable {
            match d.mode {
                Mode::Accept => "只允许选中应用进入VPN",
                Mode::Reject => "选中应用将会被排除在VPN之外",
            }
        } else {
            "应用访问控制已关闭"
        };
        let (enabled, changed, all, loading) = (
            d.enable,
            d.sorted() != saved,
            d.holds_all(&names),
            !p.loaded,
        );
        let count: SharedString = format!("已选 {installed}").into();
        let can_all = !names.is_empty();
        drop(p);
        self.ui(move |ui| {
            ui.set_acc_rows(ModelRc::new(VecModel::from(rows)));
            ui.set_acc_enabled(enabled);
            ui.set_acc_changed(changed);
            ui.set_acc_all_selected(all);
            ui.set_acc_can_select_all(can_all);
            ui.set_acc_loading(loading);
            ui.set_acc_needs_permission(needs_permission);
            ui.set_acc_authorize(authorize);
            ui.set_acc_status(status.into());
            ui.set_acc_count(count);
        });
    }

    fn access_update(self: &Arc<Self>, f: impl FnOnce(&mut AccessControl)) {
        {
            let mut p = page().lock().unwrap();
            let mode = p.draft.mode;
            f(&mut p.draft);
            if p.draft.mode != mode {
                p.pinned = p.draft.current().clone();
            }
        }
        self.publish_access();
    }

    /// `_handleSave`: the draft, lists sorted, goes into the settings; the VPN takes it the
    /// next time it starts.
    fn access_save(self: &Arc<Self>) {
        let draft = page().lock().unwrap().draft.sorted();
        {
            let mut store = self.store.lock().unwrap();
            store.state.settings.access_control = draft;
            store.save();
        }
        self.publish_access();
    }

    /// `_handleBack`: unsaved changes ask first.
    fn access_back(self: &Arc<Self>) {
        let saved = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .access_control
            .sorted();
        if page().lock().unwrap().draft.sorted() != saved {
            self.confirm(
                "提示",
                "是否保存更改？".into(),
                "access-save",
                String::new(),
            );
        } else {
            self.ui(|ui| ui.set_tools_page("".into()));
        }
    }

    pub(crate) fn access_save_answer(self: &Arc<Self>, yes: bool) {
        if yes {
            self.access_save();
        }
        self.ui(|ui| ui.set_tools_page("".into()));
    }

    fn access_menu(self: &Arc<Self>) {
        let d = page().lock().unwrap().draft.clone();
        self.show_access_menu(menu_items(&d, ""));
    }

    /// `_buildMenuItems` in a popup at the ⋯, a sub menu replacing it in place.
    fn show_access_menu(self: &Arc<Self>, items: Vec<ActionItem>) {
        self.ui(move |ui| {
            let at = ui.global::<crate::MenuAnchor>();
            ui.set_ctx_x(at.get_x());
            ui.set_ctx_y(at.get_y());
            ui.set_ctx_w(at.get_w());
            ui.set_ctx_h(at.get_h());
            ui.set_ctx_a("".into());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_kind("acc".into());
        });
    }

    pub(crate) fn access_menu_picked(self: &Arc<Self>, id: &str) {
        if matches!(id, "settings" | "action") {
            let d = page().lock().unwrap().draft.clone();
            return self.show_access_menu(menu_items(&d, id));
        }
        match id {
            "on" => self.access_update(|d| d.enable = true),
            "off" => self.access_update(|d| d.enable = false),
            "accept" => self.access_update(|d| d.mode = Mode::Accept),
            "reject" => self.access_update(|d| d.mode = Mode::Reject),
            "sort-none" => self.access_update(|d| d.sort = Sort::None),
            "sort-name" => self.access_update(|d| d.sort = Sort::Name),
            "sort-time" => self.access_update(|d| d.sort = Sort::Time),
            // `keepsOpen`: the sources toggle with the menu still showing them.
            "system" | "no-net" => {
                self.access_update(|d| match id {
                    "system" => d.is_filter_system_app = !d.is_filter_system_app,
                    _ => d.is_filter_non_internet_app = !d.is_filter_non_internet_app,
                });
                let d = page().lock().unwrap().draft.clone();
                self.show_access_menu(menu_items(&d, "settings"));
            }
            "smart" => self.spawn(|app| {
                if page().lock().unwrap().packages.is_empty() {
                    return;
                }
                match app
                    .bridge()
                    .and_then(|b| b.shell("packages.china", json!({}), Duration::from_secs(120)))
                {
                    Ok(r) => {
                        let china: Vec<String> = r
                            .get("packages")
                            .cloned()
                            .and_then(|v| serde_json::from_value(v).ok())
                            .unwrap_or_default();
                        {
                            let mut p = page().lock().unwrap();
                            let packages = p.packages.clone();
                            p.draft.intelligent(&packages, &china);
                        }
                        app.publish_access();
                    }
                    Err(e) => app.toast(e),
                }
            }),
            "export" => {
                let text = page().lock().unwrap().draft.current().join("\n");
                self.spawn(move |app| {
                    if let Err(e) = app
                        .bridge()
                        .and_then(|b| b.shell("clipboard.set", json!({"text": text}), CALL))
                    {
                        app.toast(e);
                    }
                });
            }
            "import" => self.spawn(|app| {
                let text = app
                    .bridge()
                    .and_then(|b| b.shell("clipboard.get", json!({}), CALL))
                    .map(|r| {
                        r.get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    });
                match text {
                    Ok(text) => {
                        let list = access::parse_package_names(&text);
                        if list.is_empty() {
                            app.toast("剪贴板中没有有效的包名");
                            return;
                        }
                        app.access_update(|d| *d.current_mut() = list);
                    }
                    Err(e) => app.toast(e),
                }
            }),
            _ => {}
        }
    }

    /// `_handleGrantInstalledAppsPermission`.
    fn access_grant(self: &Arc<Self>) {
        self.spawn(|app| {
            let granted = app
                .bridge()
                .and_then(|b| b.shell("packages.request", json!({}), Duration::from_secs(3600)))
                .map(|r| r.get("granted").and_then(Value::as_bool).unwrap_or(false));
            match granted {
                Ok(true) => app.load_packages(),
                Ok(false) => app.confirm(
                    "提示",
                    "读取应用列表权限已被拒绝，无法获取已安装的应用。请前往系统设置手动开启。"
                        .into(),
                    "app-settings",
                    String::new(),
                ),
                Err(e) => app.toast(e),
            }
        });
    }

    pub(crate) fn open_app_settings(self: &Arc<Self>) {
        self.spawn(|app| {
            if let Err(e) = app
                .bridge()
                .and_then(|b| b.shell("app.settings", json!({}), CALL))
            {
                app.toast(e);
            }
        });
    }
}

/// FlClash's access ⋯ menu (`_buildMenuItems`): 开启 / 关闭, then 设置 (模式, 排序 and 来源)
/// and 操作 as sub menus; `sub` names the sub menu open, "" for the top level.
pub fn menu_items(d: &AccessControl, sub: &str) -> Vec<ActionItem> {
    let item = |id: &str, glyph: &str, label: &str, checked: Option<bool>| ActionItem {
        id: id.into(),
        glyph: glyph.into(),
        label: label.into(),
        check: checked.is_some(),
        checked: checked.unwrap_or(false),
        enabled: true,
        ..Default::default()
    };
    let head = |mut a: ActionItem, h: &str| {
        a.heading = h.into();
        a
    };
    match sub {
        "settings" => vec![
            head(
                item("accept", "", "白名单模式", Some(d.mode == Mode::Accept)),
                "模式",
            ),
            item("reject", "", "黑名单模式", Some(d.mode == Mode::Reject)),
            head(
                item("sort-none", "", "默认", Some(d.sort == Sort::None)),
                "排序",
            ),
            item("sort-name", "", "名称", Some(d.sort == Sort::Name)),
            item("sort-time", "", "时间", Some(d.sort == Sort::Time)),
            head(
                item("system", "", "系统应用", Some(!d.is_filter_system_app)),
                "来源",
            ),
            item(
                "no-net",
                "",
                "无网络应用",
                Some(!d.is_filter_non_internet_app),
            ),
        ],
        "action" => vec![
            item("smart", "sparkle", "智能选择", None),
            item("export", "copy", "导出剪贴板", None),
            item("import", "paste", "剪贴板导入", None),
        ],
        _ => {
            let mut settings = head(item("settings", "sliders", "设置", None), "more");
            settings.subtitle = match d.mode {
                Mode::Accept => "白名单模式",
                Mode::Reject => "黑名单模式",
            }
            .into();
            settings.section = true;
            vec![
                head(item("on", "toggle-on", "开启", Some(d.enable)), "访问控制"),
                item("off", "toggle", "关闭", Some(!d.enable)),
                settings,
                head(item("action", "more-circle", "操作", None), "more"),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn monogram_takes_the_first_letter() {
        assert_eq!(super::monogram(" wechat"), "W");
        assert_eq!(super::monogram("微信"), "微");
        assert_eq!(super::monogram(""), "");
    }

    #[test]
    fn the_menu_follows_flclash() {
        let d = super::AccessControl::default();
        let labels = |sub| -> Vec<String> {
            super::menu_items(&d, sub)
                .iter()
                .map(|a| a.label.to_string())
                .collect()
        };
        assert_eq!(labels(""), ["开启", "关闭", "设置", "操作"]);
        let top = super::menu_items(&d, "");
        assert_eq!(top[0].heading, "访问控制");
        assert!(top[2].section && top[2].heading == "more" && top[3].heading == "more");
        assert_eq!(
            labels("settings"),
            [
                "白名单模式",
                "黑名单模式",
                "默认",
                "名称",
                "时间",
                "系统应用",
                "无网络应用"
            ]
        );
        assert!(super::menu_items(&d, "settings").iter().all(|a| a.check));
        assert_eq!(labels("action"), ["智能选择", "导出剪贴板", "剪贴板导入"]);
    }
}
