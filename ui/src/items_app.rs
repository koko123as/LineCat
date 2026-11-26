//! The filters and scripts pages on `App`'s side (lib/views/config/filters.dart,
//! lib/views/config/scripts.dart).

use crate::items::{self, Filter, Script};
use crate::{profile, store, ActionItem, App, PageMeta, SetRow, SetSection, CALL};
use serde_json::{json, Value};
use slint::{ModelRc, SharedString, VecModel};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The script the editor is open on (None for a new one) and what a new one holds until it
/// is named.
#[derive(Default)]
struct Editing {
    script: Option<String>,
    pending: String,
}

fn syncing() -> &'static Mutex<BTreeSet<String>> {
    static S: std::sync::OnceLock<Mutex<BTreeSet<String>>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

fn editing() -> &'static Mutex<Editing> {
    static E: std::sync::OnceLock<Mutex<Editing>> = std::sync::OnceLock::new();
    E.get_or_init(Default::default)
}

/// `scriptTemplate`: what a new script and a missing script file open on.
const SCRIPT_TEMPLATE: &str = "const main = (config) => {\n  return config;\n}";

fn row(id: &str, title: &str, subtitle: &str) -> SetRow {
    SetRow {
        id: id.into(),
        kind: "action".into(),
        title: title.into(),
        subtitle: subtitle.into(),
        enabled: true,
        ..Default::default()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn page_meta(page: &str) -> Option<PageMeta> {
    let (title, empty) = match page {
        "filters" => ("筛选", "暂无筛选"),
        "scripts" => ("脚本", "暂无脚本"),
        _ => return None,
    };
    Some(PageMeta {
        known: true,
        title: title.into(),
        parent: "advanced".into(),
        action: "add-circle".into(),
        empty: empty.into(),
        action_empty: true,
        ..Default::default()
    })
}

pub fn sections(settings: &store::Settings, page: &str) -> Option<Vec<SetSection>> {
    let rows: Vec<SetRow> = match page {
        "filters" => settings
            .filters
            .iter()
            .enumerate()
            .map(|(i, f)| row(&format!("filter:{i}"), &f.label, &f.regex))
            .collect(),
        "scripts" => settings
            .scripts
            .iter()
            .map(|s| {
                let kind = if s.url.is_some() { "远程" } else { "本地" };
                row(&format!("script:{}", s.id), &s.label, kind)
            })
            .collect(),
        _ => return None,
    };
    if rows.is_empty() {
        return Some(Vec::new());
    }
    Some(vec![SetSection {
        title: "".into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }])
}

impl App {
    pub(crate) fn script_path(&self, id: &str) -> PathBuf {
        self.store
            .lock()
            .unwrap()
            .dir
            .join("scripts")
            .join(format!("{id}.js"))
    }

    fn update_items(self: &Arc<Self>, f: impl FnOnce(&mut store::Settings)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings);
            store.save();
        }
        self.publish_settings();
    }

    fn sheet(self: &Arc<Self>, title: &str, items: Vec<(String, &str, &str, bool)>) {
        let items: Vec<ActionItem> = items
            .into_iter()
            .map(|(id, glyph, label, danger)| ActionItem {
                id: id.into(),
                glyph: glyph.into(),
                label: label.into(),
                danger,
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

    /// A tap on the filters or scripts page; false when it is neither.
    pub(crate) fn items_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        let settings = self.store.lock().unwrap().state.settings.clone();
        match (page, id) {
            ("filters", "action") => {
                self.open_input_with("filter:new", "添加", "标签", "", "正则", "", false)
            }
            ("filters", id) => {
                let Some(i) = id
                    .strip_prefix("filter:")
                    .and_then(|i| i.parse::<usize>().ok())
                else {
                    return true;
                };
                if let Some(f) = settings.filters.get(i) {
                    self.open_input_with(
                        &format!("filter:{i}"),
                        "编辑",
                        "标签",
                        &f.label,
                        "正则",
                        &f.regex,
                        true,
                    );
                }
            }
            ("scripts", "action") => self.sheet(
                "",
                vec![
                    ("script-add:new".into(), "edit", "从零开始", false),
                    ("script-add:url".into(), "sync", "通过URL导入", false),
                    ("script-add:file".into(), "copy", "通过文件导入", false),
                ],
            ),
            ("scripts", id) => {
                let Some(sid) = id.strip_prefix("script:") else {
                    return true;
                };
                let Some(s) = settings.scripts.iter().find(|s| s.id == sid) else {
                    return true;
                };
                let mut items = vec![
                    (format!("script-menu:edit:{sid}"), "edit", "编辑", false),
                    (format!("script-menu:rename:{sid}"), "edit", "重命名", false),
                ];
                if s.url.is_some() {
                    items.push((format!("script-menu:url:{sid}"), "copy", "URL", false));
                    items.push((format!("script-menu:sync:{sid}"), "sync", "同步", false));
                }
                items.push((format!("script-menu:delete:{sid}"), "block", "删除", true));
                self.sheet(&s.label, items);
            }
            _ => return false,
        }
        true
    }

    /// The input dialog's checks for these pages; None when the key is not theirs.
    pub(crate) fn items_check(&self, key: &str, v1: &str, v2: &str) -> Option<String> {
        let settings = &self.store.lock().unwrap().state.settings;
        if let Some(at) = key.strip_prefix("filter:") {
            let at = at.parse::<usize>().ok();
            return Some(items::filter_issue(&settings.filters, at, v1, v2).unwrap_or_default());
        }
        if key == "script-name" {
            return Some(items::script_name_issue(&settings.scripts, None, v1).unwrap_or_default());
        }
        if let Some(id) = key.strip_prefix("script-rename:") {
            return Some(
                items::script_name_issue(&settings.scripts, Some(id), v1).unwrap_or_default(),
            );
        }
        if key == "script-url" || key.starts_with("script-url:") {
            let v = v1.trim();
            return Some(if v.starts_with("http://") || v.starts_with("https://") {
                String::new()
            } else {
                "请输入有效的 URL".into()
            });
        }
        None
    }

    pub(crate) fn items_input(self: &Arc<Self>, key: &str, v1: &str, v2: &str) -> bool {
        if let Some(at) = key.strip_prefix("filter:") {
            let filter = Filter {
                label: v1.trim().into(),
                regex: v2.trim().into(),
            };
            let at = at.parse::<usize>().ok();
            self.spawn(move |app| {
                // `checkRegex`: the core's own regex engine has the last word.
                let issue = app
                    .bridge()
                    .and_then(|b| {
                        b.call(
                            "validate.filters",
                            json!({"filters": [filter.regex.clone()]}),
                            CALL,
                        )
                    })
                    .ok()
                    .and_then(|r| {
                        r.as_object()
                            .and_then(|m| m.values().next())
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                if let Some(e) = issue {
                    return app.toast(e);
                }
                app.update_items(move |s| match at {
                    Some(i) if i < s.filters.len() => s.filters[i] = filter,
                    _ => s.filters.push(filter),
                });
            });
            return true;
        }
        match key {
            "script-name" => {
                let content = std::mem::take(&mut editing().lock().unwrap().pending);
                self.add_script(v1.trim(), None, content.as_bytes());
                self.ui(|ui| ui.set_tools_page("scripts".into()));
            }
            "script-url" => {
                let url = v1.trim().to_string();
                self.spawn(move |app| app.import_script_url(&url, None));
            }
            k if k.starts_with("script-rename:") => {
                let (id, label) = (
                    k["script-rename:".len()..].to_string(),
                    v1.trim().to_string(),
                );
                self.update_items(move |s| {
                    if let Some(x) = s.scripts.iter_mut().find(|x| x.id == id) {
                        x.label = label;
                    }
                });
            }
            k if k.starts_with("script-url:") => {
                let (id, url) = (k["script-url:".len()..].to_string(), v1.trim().to_string());
                self.spawn(move |app| app.import_script_url(&url, Some(id)));
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn items_removed(self: &Arc<Self>, key: &str) -> bool {
        if let Some(i) = key
            .strip_prefix("filter:")
            .and_then(|i| i.parse::<usize>().ok())
        {
            self.update_items(move |s| {
                if i < s.filters.len() {
                    s.filters.remove(i);
                }
            });
            return true;
        }
        false
    }

    pub(crate) fn items_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match (key, value) {
            ("script-add", "new") => {
                *editing().lock().unwrap() = Editing::default();
                self.open_tools_editor("脚本", SCRIPT_TEMPLATE, "scripts");
            }
            ("script-add", "url") => self.open_input("script-url", "通过URL导入", "URL", ""),
            ("script-add", "file") => self.spawn(|app| app.import_script_file()),
            ("script-menu", v) => {
                let (action, id) = v.split_once(':').unwrap_or((v, ""));
                let settings = self.store.lock().unwrap().state.settings.clone();
                let Some(s) = settings.scripts.iter().find(|s| s.id == id).cloned() else {
                    return true;
                };
                match action {
                    "edit" => {
                        let text = std::fs::read_to_string(self.script_path(id))
                            .unwrap_or_else(|_| SCRIPT_TEMPLATE.into());
                        editing().lock().unwrap().script = Some(id.to_string());
                        self.open_tools_editor(&s.label, &text, "scripts");
                    }
                    "rename" => self.open_input(
                        &format!("script-rename:{id}"),
                        "重命名",
                        "请输入脚本名称",
                        &s.label,
                    ),
                    "url" => self.open_input(
                        &format!("script-url:{id}"),
                        "URL",
                        "",
                        s.url.as_deref().unwrap_or(""),
                    ),
                    "sync" => self.sync_script(id),
                    "delete" => self.confirm(
                        "提示",
                        "确定删除当前脚本吗？".into(),
                        "script-delete",
                        id.to_string(),
                    ),
                    _ => {}
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn script_syncing(&self, id: &str) -> bool {
        syncing().lock().unwrap().contains(id)
    }

    /// `updateScript`: the script read again from its URL, once at a time.
    pub(crate) fn sync_script(self: &Arc<Self>, id: &str) {
        let url = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .scripts
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.url.clone());
        let Some(url) = url else { return };
        if !syncing().lock().unwrap().insert(id.to_string()) {
            return;
        }
        self.republish_resources();
        let id = id.to_string();
        self.spawn(move |app| {
            app.import_script_url(&url, Some(id.clone()));
            syncing().lock().unwrap().remove(&id);
            app.republish_resources();
        });
    }

    /// `script-delete` confirmed.
    pub(crate) fn delete_script(self: &Arc<Self>, id: &str) {
        let _ = std::fs::remove_file(self.script_path(id));
        let in_use = {
            let mut store = self.store.lock().unwrap();
            let in_use = items::drop_script(&mut store.state, id);
            store.state.settings.scripts.retain(|x| x.id != id);
            store.save();
            in_use
        };
        self.publish_settings();
        if in_use {
            self.reapply();
        }
    }

    /// `_applyIfInUse`: the current profile runs again with the script's new content.
    fn reapply_if_running(self: &Arc<Self>, id: &str) {
        if items::script_in_use(&self.store.lock().unwrap().state, id) {
            self.reapply();
        }
    }

    pub(crate) fn open_tools_editor(self: &Arc<Self>, title: &str, text: &str, parent: &str) {
        crate::rule_sets_app::release_editor();
        let (title, text, parent): (SharedString, SharedString, SharedString) =
            (title.into(), text.into(), parent.into());
        self.ui(move |ui| {
            ui.set_tools_editor_title(title);
            ui.set_tools_editor_text(text);
            ui.set_tools_editor_guard(false);
            ui.set_tools_editor_parent(parent);
            ui.set_tools_editor_readonly(false);
            ui.set_tools_page("editor".into());
        });
    }

    /// The tools editor's save: a script's new content, or a new script to name.
    pub(crate) fn tools_editor_save(self: &Arc<Self>, text: String) {
        if self.override_yaml_save(&text, false)
            || self.share_links_editor_save(&text)
            || self.custom_proxy_editor_save(&text)
            || self.rule_set_editor_save(&text)
            || self.mitm_editor_save(&text)
        {
            return;
        }
        let id = editing().lock().unwrap().script.clone();
        match id {
            Some(id) => {
                let path = self.script_path(&id);
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = std::fs::write(&path, text) {
                    return self.toast(e.to_string());
                }
                self.update_items({
                    let id = id.clone();
                    move |s| {
                        if let Some(x) = s.scripts.iter_mut().find(|x| x.id == id) {
                            x.updated = now_ms();
                        }
                    }
                });
                self.reapply_if_running(&id);
                self.ui(|ui| ui.set_tools_page("scripts".into()));
            }
            None => {
                editing().lock().unwrap().pending = text;
                self.open_input("script-name", "保存", "请输入脚本名称", "");
            }
        }
    }

    fn add_script(self: &Arc<Self>, label: &str, url: Option<String>, content: &[u8]) {
        let id = now_ms().to_string();
        let path = self.script_path(&id);
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&path, content) {
            return self.toast(e.to_string());
        }
        let script = Script {
            id,
            label: label.to_string(),
            url,
            updated: now_ms(),
        };
        self.update_items(move |s| s.scripts.push(script));
    }

    /// Imports a script from a URL, or refreshes the script `id` from it.
    fn import_script_url(self: &Arc<Self>, url: &str, id: Option<String>) {
        let check = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .general
            .check_certificate;
        match profile::download(url, check) {
            Ok(d) => match id {
                Some(id) => {
                    if let Err(e) = self.write_script(&id, url, &d.bytes) {
                        self.toast(e);
                    }
                }
                None => {
                    let scripts = self.store.lock().unwrap().state.settings.scripts.clone();
                    let name = items::import_name(&scripts, d.name.as_deref().unwrap_or(url));
                    self.add_script(&name, Some(url.to_string()), &d.bytes);
                }
            },
            Err(e) => self.toast(e),
        }
    }

    fn write_script(self: &Arc<Self>, id: &str, url: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(self.script_path(id), bytes).map_err(|e| e.to_string())?;
        let (id, url) = (id.to_string(), url.to_string());
        self.update_items({
            let id = id.clone();
            move |s| {
                if let Some(x) = s.scripts.iter_mut().find(|x| x.id == id) {
                    x.url = Some(url);
                    x.updated = now_ms();
                }
            }
        });
        self.reapply_if_running(&id);
        Ok(())
    }

    /// `updateScript` for the resources' auto update, which logs rather than toasts.
    pub(crate) fn update_script(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let settings = self.store.lock().unwrap().state.settings.clone();
        let Some(url) = settings
            .scripts
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.url.clone())
        else {
            return Ok(());
        };
        let d = profile::download(&url, settings.general.check_certificate)?;
        self.write_script(id, &url, &d.bytes)
    }

    fn import_script_file(self: &Arc<Self>) {
        let tmp = self.store.lock().unwrap().dir.join("script.part.js");
        let picked = self.bridge().and_then(|b| {
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
            Err(e) => return self.toast(e),
        };
        let bytes = std::fs::read(&tmp).unwrap_or_default();
        let _ = std::fs::remove_file(&tmp);
        let scripts = self.store.lock().unwrap().state.settings.scripts.clone();
        self.add_script(&items::import_name(&scripts, &name), None, &bytes);
    }
}
