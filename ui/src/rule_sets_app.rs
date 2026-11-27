//! 规则集 on `App`'s side. FlClash: lib/views/config/providers.dart (`ClashProvidersAction`).

use crate::items::ScriptSave;
use crate::rule_sets::{self, RuleSet};
use crate::{ActionItem, Anchor, App, PageMeta, SetRow, SetSection, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The set the tools editor is open on: Some(None) for a new one, None when the editor
/// belongs to another page. A new set's content waits here for its name.
#[derive(Default)]
struct Editing {
    set: Option<Option<String>>,
    pending: String,
    /// The title and text the editor opened with, for `_isDirty`.
    opened: (String, String),
    pending_title: String,
}

fn editing() -> &'static Mutex<Editing> {
    static E: OnceLock<Mutex<Editing>> = OnceLock::new();
    E.get_or_init(Default::default)
}

/// Another page takes the tools editor: neither a rule set nor a MITM text saves its text.
pub(crate) fn release_editor() {
    editing().lock().unwrap().set = None;
    crate::mitm_app::release_editor();
}

pub fn page_meta(sets: &[RuleSet], page: &str) -> Option<PageMeta> {
    (page == "rule-sets").then(|| PageMeta {
        known: true,
        title: "规则集".into(),
        parent: "advanced".into(),
        action: "add-circle".into(),
        action2: if sets.len() > 1 {
            "sort".into()
        } else {
            "".into()
        },
        empty: "暂无规则集".into(),
        action_empty: true,
        long_press: true,
        ..Default::default()
    })
}

pub(crate) fn modified_secs(path: &std::path::Path) -> Option<u64> {
    let t = std::fs::metadata(path).ok()?.modified().ok()?;
    t.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

pub fn sections(sets: &[RuleSet], dir: &std::path::Path, page: &str) -> Option<Vec<SetSection>> {
    if page != "rule-sets" {
        return None;
    }
    if sets.is_empty() {
        return Some(Vec::new());
    }
    let rows: Vec<SetRow> = sets
        .iter()
        .map(|s| SetRow {
            id: format!("rs:{}", s.id).into(),
            kind: "action".into(),
            title: s.label.as_str().into(),
            subtitle: if s.is_remote() { "远程" } else { "本地" }.into(),
            chip: s.behavior.as_str().into(),
            trailing: modified_secs(&dir.join(s.file_name()))
                .map(crate::ago_short)
                .unwrap_or_default()
                .into(),
            enabled: true,
            ..Default::default()
        })
        .collect();
    Some(vec![SetSection {
        title: "".into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }])
}

impl App {
    pub(crate) fn rule_sets_dir(&self) -> PathBuf {
        self.core_home
            .lock()
            .unwrap()
            .join("providers")
            .join("rules")
    }

    fn rule_sets(&self) -> Vec<RuleSet> {
        self.store.lock().unwrap().state.settings.rule_sets.clone()
    }

    fn update_rule_sets(self: &Arc<Self>, f: impl FnOnce(&mut Vec<RuleSet>)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings.rule_sets);
            store.save();
        }
        self.publish_settings();
    }

    fn rule_set_titles(&self) -> Vec<SharedString> {
        self.rule_sets()
            .iter()
            .map(|s| s.label.as_str().into())
            .collect()
    }

    pub(crate) fn rule_sets_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        if page != "rule-sets" {
            return false;
        }
        let sets = self.rule_sets();
        if id == "action" {
            let item = |id: &str, glyph: &str, label: &str| ActionItem {
                id: format!("rs-add:{id}").into(),
                glyph: glyph.into(),
                label: label.into(),
                enabled: true,
                ..Default::default()
            };
            let items = vec![
                item("new", "edit", "从零开始"),
                item("url", "cloud", "通过URL导入"),
                item("file", "import-file", "通过文件导入"),
            ];
            self.ui(move |ui| {
                ui.set_choice_title("".into());
                ui.set_choice_items(ModelRc::new(VecModel::from(items)));
                ui.set_choice_open(true);
            });
        } else if id == "action2" {
            let titles = self.rule_set_titles();
            self.ui(move |ui| {
                ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles)));
                ui.set_set_sorting(true);
            });
        } else if let Some(rest) = id.strip_prefix("sort-move:") {
            let mut it = rest.split(':').filter_map(|n| n.parse::<usize>().ok());
            if let (Some(from), Some(to)) = (it.next(), it.next()) {
                self.update_rule_sets(|s| crate::reorder::move_item(s, from, to));
                let titles = self.rule_set_titles();
                self.ui(move |ui| ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles))));
            }
        } else if let Some(sid) = id.strip_prefix("long:rs:") {
            let Some(i) = sets.iter().position(|s| s.id == sid) else {
                return true;
            };
            self.rule_set_menu(i, &sets[i]);
        } else if let Some(sid) = id.strip_prefix("rs:") {
            if let Some(s) = sets.iter().find(|s| s.id == sid).cloned() {
                if s.is_editable() {
                    self.open_rule_set_editor(Some(&s));
                } else {
                    self.preview_rule_set(s);
                }
            }
        }
        true
    }

    /// `_buildMenuItems`, anchored at the row.
    fn rule_set_menu(self: &Arc<Self>, index: usize, s: &RuleSet) {
        let item = |id: &str, glyph: &str, label: &str, danger: bool| ActionItem {
            id: id.into(),
            glyph: glyph.into(),
            label: label.into(),
            danger,
            enabled: true,
            ..Default::default()
        };
        let mut items = Vec::new();
        if s.is_editable() {
            items.push(item("edit", "edit", "编辑", false));
        }
        if !s.is_text() {
            items.push(item("preview", "eye", "预览", false));
        }
        if s.is_remote() {
            items.push(item("sync", "sync", "同步", false));
        }
        items.push(item("options", "settings", "选项", false));
        items.push(item("delete", "delete", "删除", true));
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(index.to_string().into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind("rs".into());
        });
    }

    pub(crate) fn rule_set_menu_picked(self: &Arc<Self>, index: usize, action: &str) {
        let Some(s) = self.rule_sets().get(index).cloned() else {
            return;
        };
        match action {
            "edit" => self.open_rule_set_editor(Some(&s)),
            "preview" => self.preview_rule_set(s),
            "sync" => self.spawn(move |app| {
                app.put_rule_set(s.clone(), Some(s), None, true);
            }),
            "options" => {
                let (label2, value2) = if s.is_remote() {
                    ("URL", s.url.as_str())
                } else {
                    ("", "")
                };
                self.open_input_with(
                    &format!("rs-opt:{}", s.id),
                    "选项",
                    "名称",
                    &s.label,
                    label2,
                    value2,
                    false,
                );
            }
            "delete" => {
                let users = crate::profile_rules::profiles_using(
                    &self.store.lock().unwrap().state,
                    &s.label,
                );
                let text = if users.is_empty() {
                    format!("确定删除当前{}吗？", s.label)
                } else {
                    format!(
                        "{} 仍被 {} 使用，删除后这些配置会异常。确定删除吗？",
                        s.label,
                        users.join(", ")
                    )
                };
                self.confirm("提示", text, "rs-delete", s.id)
            }
            _ => {}
        }
    }

    fn open_rule_set_editor(self: &Arc<Self>, s: Option<&RuleSet>) {
        let text = s
            .map(|s| {
                std::fs::read_to_string(self.rule_sets_dir().join(s.file_name()))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        crate::mitm_app::release_editor();
        let label = s.map(|s| s.label.clone()).unwrap_or_default();
        *editing().lock().unwrap() = Editing {
            set: Some(s.map(|s| s.id.clone())),
            opened: (label.clone(), text.clone()),
            ..Default::default()
        };
        let title: SharedString = label.into();
        self.ui(move |ui| {
            ui.set_tools_editor_title(title);
            ui.set_tools_editor_text(text.into());
            ui.set_tools_editor_schema("provider".into());
            ui.set_tools_editor_guard(true);
            ui.set_tools_editor_readonly(false);
            ui.set_tools_editor_title_editable(true);
            ui.set_tools_editor_parent("rule-sets".into());
            ui.set_tools_page("editor".into());
        });
    }

    /// `_handlePreview`: the text itself, or an mrs set dumped by the Core.
    fn preview_rule_set(self: &Arc<Self>, s: RuleSet) {
        release_editor();
        let title: SharedString = s.label.as_str().into();
        self.ui(move |ui| {
            ui.set_tools_editor_title(title);
            ui.set_tools_editor_text(SharedString::new());
            ui.set_tools_editor_schema(SharedString::new());
            ui.set_tools_editor_guard(false);
            ui.set_tools_editor_readonly(true);
            ui.set_tools_editor_title_editable(false);
            ui.set_tools_editor_parent("rule-sets".into());
            ui.set_tools_editor_error(SharedString::new());
            ui.set_tools_editor_loading(true);
            ui.set_tools_page("editor".into());
        });
        let load = crate::items_app::editor_load();
        self.spawn(move |app| {
            let path = app.rule_sets_dir().join(s.file_name());
            let text = if s.is_text() {
                std::fs::read_to_string(&path).map_err(|e| e.to_string())
            } else {
                app.bridge()
                    .and_then(|b| {
                        b.call(
                            "ruleSet.dump",
                            json!({"profile": {"path": path.to_string_lossy()}}),
                            CALL,
                        )
                    })
                    .map(|r| {
                        r.get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    })
            };
            app.tools_editor_loaded(load, text, || {});
        });
    }

    /// The tools editor's save while it holds a rule set; false when it does not.
    pub(crate) fn rule_set_editor_save(self: &Arc<Self>, text: &str, title: &str) -> bool {
        let (target, opened) = {
            let e = editing().lock().unwrap();
            (e.set.clone(), e.opened.clone())
        };
        let Some(target) = target else {
            return false;
        };
        let sets = self.rule_sets();
        let opened = (opened.0.as_str(), opened.1.as_str());
        match rule_sets::editor_save(&sets, target.as_deref(), opened, title, text) {
            ScriptSave::Unchanged => {}
            ScriptSave::AskName => {
                editing().lock().unwrap().pending = text.to_string();
                self.open_input("rs-name", "保存", "名称", "");
            }
            ScriptSave::Exists => self.alert("提示", "名称当前已存在"),
            ScriptSave::Save(label) => {
                let old = target.and_then(|id| sets.into_iter().find(|s| s.id == id));
                let s = match &old {
                    Some(o) => RuleSet { label, ..o.clone() },
                    None => new_set(&label, ""),
                };
                let content = text.as_bytes().to_vec();
                self.spawn(move |app| {
                    if app.put_rule_set(s, old, Some(content), false) {
                        app.ui(|ui| ui.set_tools_page("rule-sets".into()));
                    }
                });
            }
        }
        true
    }

    /// `_handleEditorPop`: a changed set asks 是否保存更改？; false when the editor
    /// does not hold a rule set.
    pub(crate) fn rule_set_editor_back(self: &Arc<Self>, text: &str, title: &str) -> bool {
        let mut e = editing().lock().unwrap();
        if e.set.is_none() {
            return false;
        }
        if crate::items::script_dirty((&e.opened.0, &e.opened.1), title, text) {
            e.pending = text.to_string();
            e.pending_title = title.to_string();
            drop(e);
            self.confirm("提示", "是否保存更改？".into(), "rs-save", String::new());
        } else {
            drop(e);
            self.ui(|ui| ui.set_tools_page("rule-sets".into()));
        }
        true
    }

    pub(crate) fn rule_set_save_answer(self: &Arc<Self>, yes: bool) {
        if !yes {
            return self.ui(|ui| ui.set_tools_page("rule-sets".into()));
        }
        let (text, title) = {
            let mut e = editing().lock().unwrap();
            (
                std::mem::take(&mut e.pending),
                std::mem::take(&mut e.pending_title),
            )
        };
        self.rule_set_editor_save(&text, &title);
    }

    pub(crate) fn rule_sets_check(&self, key: &str, v1: &str, v2: &str) -> Option<String> {
        let sets = self.rule_sets();
        let issue = match key {
            "rs-name" => {
                let id = editing().lock().unwrap().set.clone().flatten();
                rule_sets::label_issue(&sets, id.as_deref(), v1, false)
            }
            "rs-url" if crate::mitm::module_urls(v2).len() > 1 => None,
            "rs-url" => {
                rule_sets::label_issue(&sets, None, v1, true).or_else(|| rule_sets::url_issue(v2))
            }
            k => {
                let id = k.strip_prefix("rs-opt:")?;
                let remote = sets.iter().any(|s| s.id == id && s.is_remote());
                rule_sets::label_issue(&sets, Some(id), v1, false)
                    .or_else(|| remote.then(|| rule_sets::url_issue(v2)).flatten())
            }
        };
        Some(issue.unwrap_or_default())
    }

    pub(crate) fn rule_sets_input(self: &Arc<Self>, key: &str, v1: &str, v2: &str) -> bool {
        let sets = self.rule_sets();
        match key {
            "rs-name" => {
                let (id, content) = {
                    let mut e = editing().lock().unwrap();
                    (e.set.clone().flatten(), std::mem::take(&mut e.pending))
                };
                let old = id.and_then(|id| sets.iter().find(|s| s.id == id).cloned());
                let s = match &old {
                    Some(o) => RuleSet {
                        label: v1.trim().to_string(),
                        ..o.clone()
                    },
                    None => new_set(v1.trim(), ""),
                };
                let content = content.into_bytes();
                self.spawn(move |app| {
                    if app.put_rule_set(s, old, Some(content), false) {
                        app.ui(|ui| ui.set_tools_page("rule-sets".into()));
                    }
                });
            }
            "rs-url" if crate::mitm::module_urls(v2).len() > 1 => {
                let batch = rule_sets::batch_imports(&sets, v2);
                if batch.is_empty() {
                    self.toast("规则集当前已存在");
                }
                self.spawn(move |app| {
                    for (label, url) in batch {
                        app.put_rule_set(new_set(&label, &url), None, None, false);
                    }
                });
            }
            "rs-url" => {
                let url = v2.trim().to_string();
                let label = match v1.trim() {
                    "" => rule_sets::unique_label(&rule_sets::url_stem(&url), "规则集", |l| {
                        sets.iter().any(|s| s.label == l)
                    }),
                    l => l.to_string(),
                };
                let s = new_set(&label, &url);
                self.spawn(move |app| {
                    app.put_rule_set(s, None, None, false);
                });
            }
            k if k.starts_with("rs-opt:") => {
                let Some(old) = sets.iter().find(|s| s.id == k["rs-opt:".len()..]).cloned() else {
                    return true;
                };
                let mut s = old.clone();
                s.label = v1.trim().to_string();
                if s.is_remote() {
                    s.url = v2.trim().to_string();
                }
                self.spawn(move |app| {
                    app.put_rule_set(s, Some(old), None, false);
                });
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn rule_sets_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        if key != "rs-add" {
            return false;
        }
        match value {
            "new" => self.open_rule_set_editor(None),
            "url" => self.open_input_with("rs-url", "通过URL导入", "名称", "", "URL", "", false),
            "file" => self.spawn(|app| app.import_rule_set_file()),
            _ => {}
        }
        true
    }

    fn import_rule_set_file(self: &Arc<Self>) {
        let dir = self.rule_sets_dir();
        let _ = std::fs::create_dir_all(&dir);
        let tmp = dir.join("import.part");
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
        let sets = self.rule_sets();
        let label = rule_sets::unique_label(&rule_sets::file_stem(&name), "规则集", |l| {
            sets.iter().any(|s| s.label == l)
        });
        self.put_rule_set(new_set(&label, ""), None, Some(bytes), false);
    }

    /// `putProvider`: compiles the new content or downloads the set, then keeps it; false
    /// (with a toast) when the Core turned it down.
    fn put_rule_set(
        self: &Arc<Self>,
        s: RuleSet,
        previous: Option<RuleSet>,
        content: Option<Vec<u8>>,
        refresh: bool,
    ) -> bool {
        let reread = content.is_some() || refresh;
        match self.compile_rule_set(s, previous.as_ref(), content, refresh) {
            Ok(next) => {
                let dir = self.rule_sets_dir();
                if let Some(p) = &previous {
                    if p.label != next.label {
                        let mut store = self.store.lock().unwrap();
                        crate::profile_rules::rename_rule_set_everywhere(
                            &mut store.state,
                            &p.label,
                            &next.label,
                        );
                    }
                }
                let unchanged = previous.as_ref() == Some(&next);
                let mut labels = vec![next.label.clone()];
                labels.extend(previous.as_ref().map(|p| p.label.clone()));
                if let Some(p) = previous.filter(|p| p.file_name() != next.file_name()) {
                    clear_files(&dir, &p);
                }
                let label = next.label.clone();
                self.update_rule_sets(move |sets| {
                    match sets.iter_mut().find(|x| x.id == next.id) {
                        Some(x) => *x = next,
                        None => sets.push(next),
                    }
                });
                if reread && unchanged {
                    self.reload_if_rule_set_named(&label);
                } else {
                    self.apply_if_rule_set_named(&labels);
                }
                true
            }
            Err(e) => {
                self.toast(e);
                false
            }
        }
    }

    /// `updateRuleProvider` from the resources page: the remote set downloaded again.
    pub(crate) fn update_rule_set(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let Some(s) = self
            .rule_sets()
            .into_iter()
            .find(|x| x.id == id && x.is_remote())
        else {
            return Ok(());
        };
        let next = self.compile_rule_set(s.clone(), Some(&s), None, true)?;
        if next == s {
            self.reload_if_rule_set_named(&s.label);
            return Ok(());
        }
        let label = next.label.clone();
        self.update_rule_sets(move |sets| {
            if let Some(x) = sets.iter_mut().find(|x| x.id == next.id) {
                *x = next;
            }
        });
        self.apply_if_rule_set_named(&[label]);
        Ok(())
    }

    fn compile_rule_set(
        &self,
        s: RuleSet,
        previous: Option<&RuleSet>,
        content: Option<Vec<u8>>,
        refresh: bool,
    ) -> Result<RuleSet, String> {
        let dir = self.rule_sets_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = s.file_name();
        let compile = |name: &str, url: Option<&str>| -> Result<RuleSet, String> {
            let mut params = json!({ "name": name });
            if let Some(url) = url {
                params["url"] = json!(url);
            }
            let info = self.bridge()?.call("ruleSet.compile", params, CALL)?;
            let field =
                |k: &str, d: &str| info.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
            Ok(RuleSet {
                behavior: field("behavior", "classical"),
                format: field("format", "yaml"),
                ..s.clone()
            })
        };
        let mrs = |name: &str| dir.join(format!("{name}.mrs"));
        if let Some(content) = content {
            let staged = format!("{file}.new");
            let result = std::fs::write(dir.join(&staged), &content)
                .map_err(|e| e.to_string())
                .and_then(|_| compile(&staged, None))
                .and_then(|next| {
                    if compiled(&next) {
                        std::fs::rename(mrs(&staged), mrs(&file)).map_err(|e| e.to_string())?;
                    } else {
                        let _ = std::fs::remove_file(mrs(&file));
                    }
                    std::fs::rename(dir.join(&staged), dir.join(&file))
                        .map_err(|e| e.to_string())?;
                    Ok(next)
                });
            let _ = std::fs::remove_file(dir.join(&staged));
            let _ = std::fs::remove_file(mrs(&staged));
            return result;
        }
        let download =
            s.is_remote() && (refresh || previous.map(RuleSet::file_name) != Some(file.clone()));
        if !download {
            return Ok(s);
        }
        let next = compile(&file, Some(&s.url))?;
        if !compiled(&next) {
            let _ = std::fs::remove_file(mrs(&file));
        }
        Ok(next)
    }

    /// `_resolveInjectedProviders`' rule sets: each one `prepare`d (fetched or compiled again
    /// when its file is gone) and declared by the file the Core reads.
    pub(crate) fn rule_providers(&self, sets: Vec<RuleSet>) -> serde_json::Map<String, Value> {
        let dir = self.rule_sets_dir();
        let mut out = serde_json::Map::new();
        for s in sets {
            let core_path = |s: &RuleSet| {
                let file = s.file_name();
                dir.join(if compiled(s) {
                    format!("{file}.mrs")
                } else {
                    file
                })
            };
            let s = if core_path(&s).exists() {
                s
            } else {
                match self.compile_rule_set(s.clone(), None, None, true) {
                    Ok(next) => {
                        if next != s {
                            let mut store = self.store.lock().unwrap();
                            if let Some(x) = store
                                .state
                                .settings
                                .rule_sets
                                .iter_mut()
                                .find(|x| x.id == next.id)
                            {
                                *x = next.clone();
                            }
                            store.save();
                        }
                        next
                    }
                    Err(_) => s,
                }
            };
            out.insert(
                s.label.clone(),
                json!({
                    "type": "file",
                    "path": core_path(&s).to_string_lossy(),
                    "behavior": s.behavior,
                    "format": if compiled(&s) { "mrs" } else { s.format.as_str() },
                }),
            );
        }
        out
    }

    /// `remoteProviderOf`: the app's remote rule set the Core reads from `path`.
    pub(crate) fn remote_rule_set_at(&self, path: &str) -> Option<String> {
        let dir = self.rule_sets_dir();
        let core = |s: &RuleSet| {
            let file = s.file_name();
            dir.join(if compiled(s) {
                format!("{file}.mrs")
            } else {
                file
            })
        };
        self.rule_sets()
            .into_iter()
            .find(|s| s.is_remote() && core(s).to_string_lossy() == path)
            .map(|s| s.id)
    }

    pub(crate) fn delete_rule_set(self: &Arc<Self>, id: &str) {
        let Some(s) = self.rule_sets().into_iter().find(|s| s.id == id) else {
            return;
        };
        clear_files(&self.rule_sets_dir(), &s);
        let id = id.to_string();
        self.update_rule_sets(move |sets| sets.retain(|x| x.id != id));
    }

    /// `applyIfReferenced`: the active custom profile applied again when it names a set.
    fn apply_if_rule_set_named(self: &Arc<Self>, labels: &[String]) {
        if self.active_uses_rule_set(labels) {
            self.reapply();
        }
    }

    /// `_reloadIfReferenced`: the Core reads the set again, or the profile is applied.
    fn reload_if_rule_set_named(self: &Arc<Self>, label: &str) {
        if !self.active_uses_rule_set(&[label.to_string()]) {
            return;
        }
        let label = label.to_string();
        self.spawn(move |app| {
            let reloaded = app
                .bridge()
                .and_then(|b| b.call("provider.update", json!({"name": label}), CALL));
            if reloaded.is_err() {
                app.reapply();
            }
        });
    }

    fn active_uses_rule_set(&self, labels: &[String]) -> bool {
        let store = self.store.lock().unwrap();
        crate::custom_profile::active_uses(&store.state, |s, id| {
            crate::profile_rules::referenced_rule_sets(s, id)
                .iter()
                .any(|n| labels.contains(n))
        })
    }

    /// An imported rule provider: kept under `label`, downloaded or compiled from `content`.
    pub(crate) fn import_rule_set(
        self: &Arc<Self>,
        label: &str,
        url: &str,
        content: Option<Vec<u8>>,
    ) {
        self.put_rule_set(new_set(label, url), None, content, false);
    }
}

/// `isCompiled`: a text set of domains or addresses also keeps an mrs copy for the Core.
fn compiled(s: &RuleSet) -> bool {
    s.is_text() && s.behavior != "classical"
}

fn clear_files(dir: &std::path::Path, s: &RuleSet) {
    let file = s.file_name();
    let _ = std::fs::remove_file(dir.join(&file));
    let _ = std::fs::remove_file(dir.join(format!("{file}.mrs")));
}

fn new_set(label: &str, url: &str) -> RuleSet {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros().to_string())
        .unwrap_or_default();
    RuleSet {
        id,
        label: label.to_string(),
        url: url.to_string(),
        ..Default::default()
    }
}
