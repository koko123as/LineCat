//! 代理 on `App`'s side (lib/views/profiles/custom/custom_proxies.dart): the list, its search
//! and sort, the form (`_EditCustomProxyView`) and the Core's verdicts (`validate.proxies`).

use crate::custom_proxies::{
    self as cp, issues, issues_message, CustomProxy, Draft, Issue, Kind, Section,
};
use crate::{ActionItem, Anchor, App, PageMeta, SetRow, SetSection, CALL};
use serde_json::{json, Map, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

pub const LIST: &str = "custom-proxies";
pub const FORM: &str = "custom-proxy";

#[derive(Default)]
struct State {
    draft: Option<Draft>,
    query: String,
    editing: Option<&'static str>,
    rev: u64,
    selected: BTreeSet<String>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// `customProxyVerdicts`: the Core's word on each definition, empty when it parses.
fn verdicts() -> &'static Mutex<HashMap<String, String>> {
    static V: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    V.get_or_init(Default::default)
}

/// What the list held when it opened and every name seen since (`_opened`, `_seenNames`).
static SESSION: Mutex<Option<(Vec<Value>, BTreeSet<String>)>> = Mutex::new(None);

fn key_of(definition: &Value) -> String {
    serde_json::to_string(definition).unwrap_or_default()
}

pub fn page_meta(proxies: &[CustomProxy], page: &str) -> Option<PageMeta> {
    let st = state().lock().unwrap();
    let searching = !st.query.trim().is_empty();
    match page {
        LIST => Some(PageMeta {
            known: true,
            title: "代理".into(),
            parent: "advanced".into(),
            action: "add-circle".into(),
            action2: if proxies.len() > 1 && !searching {
                "sort".into()
            } else {
                "".into()
            },
            empty: if searching {
                "暂无搜索结果"
            } else {
                "暂无代理"
            }
            .into(),
            empty_action: if searching { "" } else { "添加" }.into(),
            long_press: true,
            search: true,
            action_empty: !searching,
            selecting: st
                .selected
                .iter()
                .any(|id| proxies.iter().any(|p| &p.id == id)),
            ..Default::default()
        }),
        FORM => Some(PageMeta {
            known: true,
            title: match &st.draft {
                Some(d) if !d.is_new => "编辑代理",
                _ => "添加代理",
            }
            .into(),
            parent: LIST.into(),
            action: "check".into(),
            action2: "compose".into(),
            ..Default::default()
        }),
        _ => None,
    }
}

/// The list's rows: name, type, and the issue marker (`_NodeItem`).
pub fn list_sections(
    proxies: &[CustomProxy],
    query: &str,
    errors: &HashMap<String, String>,
    selected: &BTreeSet<String>,
) -> Vec<SetSection> {
    let rows: Vec<SetRow> = cp::search(proxies, query)
        .into_iter()
        .map(|p| {
            let error = errors
                .get(&key_of(&p.definition))
                .filter(|e| !e.is_empty())
                .map(String::as_str);
            let name = p.name();
            SetRow {
                id: format!("cp:{}", p.id).into(),
                kind: "select".into(),
                on: selected.contains(&p.id),
                title: if name.is_empty() { "-".into() } else { name }.into(),
                subtitle: p.kind().into(),
                invalid: !issues(p, proxies, &BTreeSet::new(), error).is_empty(),
                enabled: true,
                ..Default::default()
            }
        })
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    vec![section("", rows)]
}

fn section(title: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

fn value_row(id: &str, title: &str, value: &str) -> SetRow {
    SetRow {
        id: id.into(),
        kind: "value".into(),
        title: title.into(),
        subtitle: value.into(),
        enabled: true,
        ..Default::default()
    }
}

fn field_row(d: &Draft, f: &cp::Field, lead: &str) -> SetRow {
    let p = &d.proxy;
    let id = format!("cp-f:{}", f.name);
    let revealed = d.revealed.contains(f.name);
    let mut row = value_row(&id, f.label, &cp::shown_value(p, f, revealed));
    row.lead = lead.into();
    match f.kind {
        Kind::Toggle => {
            row.kind = "toggle".into();
            row.on = p.value_of(f) == Some(&Value::Bool(true));
        }
        Kind::List | Kind::Map => row.kind = "open".into(),
        Kind::Secret if p.value_of(f).is_some() => {
            row.glyph = if revealed { "eye-off" } else { "eye" }.into();
        }
        _ => {}
    }
    row
}

/// The form's sections: 基础信息, 认证, then 选项 with 添加设置项.
pub fn editor_sections(d: &Draft) -> Vec<SetSection> {
    let p = &d.proxy;
    let hint = |key: &str| {
        if p.is_required(key) {
            "必填"
        } else {
            "可选"
        }
    };
    let name = p.name();
    let mut basic = vec![
        value_row(
            "cp-f:name",
            "名称",
            if name.is_empty() { "名称" } else { &name },
        ),
        value_row("cp-f:type", "代理类型", &p.kind()),
    ];
    if p.kind() != "easytier" {
        let server = p.server().filter(|s| !s.is_empty());
        basic.push(value_row(
            "cp-f:server",
            "服务器",
            server.as_deref().unwrap_or(hint("server")),
        ));
        let port = p.port().map(|n| n.to_string());
        basic.push(value_row(
            "cp-f:port",
            "端口",
            port.as_deref().unwrap_or(hint("port")),
        ));
    }
    let fields = p.fields();
    basic.extend(
        fields
            .iter()
            .filter(|f| !cp::is_credential(f))
            .map(|f| field_row(d, f, "")),
    );
    basic.push(SetRow {
        id: "cp-f:udp".into(),
        kind: "toggle".into(),
        title: "UDP".into(),
        on: p.definition.get("udp") == Some(&Value::Bool(true)),
        enabled: true,
        ..Default::default()
    });
    let auth: Vec<SetRow> = fields
        .iter()
        .filter(|f| cp::is_credential(f))
        .map(|f| field_row(d, f, ""))
        .collect();
    let mut options: Vec<SetRow> = d
        .listed()
        .into_iter()
        .map(|f| field_row(d, f, "remove"))
        .collect();
    if !d.addable().is_empty() {
        options.push(SetRow {
            id: "cp-add".into(),
            kind: "action".into(),
            title: "添加设置项".into(),
            lead: "add".into(),
            enabled: true,
            ..Default::default()
        });
    }
    let mut out = vec![section("基础信息", basic)];
    if p.kind() == "easytier" {
        let network = cp::EASYTIER.iter().map(|f| field_row(d, f, "")).collect();
        out.push(section("网络", network));
    }
    out.push(section("认证", auth));
    out.push(section("选项", options));
    if !d.is_new {
        let delete = SetRow {
            id: "cp-delete".into(),
            kind: "danger".into(),
            title: "删除".into(),
            enabled: true,
            ..Default::default()
        };
        out.push(section("操作", vec![delete]));
    }
    out
}

/// The issues the form's banner shows.
pub fn editor_issue(d: &Draft, proxies: &[CustomProxy]) -> String {
    let others: Vec<CustomProxy> = proxies.to_vec();
    issues_message(&d.issues(&others))
}

/// The sections of the page shown, from the state kept here.
pub fn sections(proxies: &[CustomProxy], page: &str) -> Option<Vec<SetSection>> {
    match page {
        LIST => {
            let (query, selected) = {
                let st = state().lock().unwrap();
                (st.query.clone(), st.selected.clone())
            };
            let errors = verdicts().lock().unwrap().clone();
            Some(list_sections(proxies, &query, &errors, &selected))
        }
        FORM => Some(
            state()
                .lock()
                .unwrap()
                .draft
                .as_ref()
                .map(editor_sections)
                .unwrap_or_default(),
        ),
        _ => None,
    }
}

/// `validate.proxies`: one verdict per definition, None when it parses.
pub(crate) fn parse_verdicts(r: &Value, n: usize) -> Vec<Option<String>> {
    let mut out = vec![None; n];
    let mut put = |i: usize, m: &Value| {
        if let (Some(slot), Some(m)) = (out.get_mut(i), m.as_str()) {
            *slot = Some(m.to_string());
        }
    };
    if let Some(items) = r.get("issues").and_then(Value::as_array) {
        for item in items {
            if let Some(i) = item.get("index").and_then(Value::as_u64) {
                put(i as usize, item.get("message").unwrap_or(&Value::Null));
            }
        }
    } else if let Some(map) = r.as_object() {
        for (k, v) in map {
            if let Ok(i) = k.parse::<usize>() {
                put(i, v);
            }
        }
    }
    out
}

pub(crate) fn new_id() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros().to_string())
        .unwrap_or_default()
}

/// `_handleSelectAll`: every shown row, or none once all of them are selected.
fn select_all(selected: &BTreeSet<String>, visible: &BTreeSet<String>) -> BTreeSet<String> {
    if visible.is_subset(selected) {
        BTreeSet::new()
    } else {
        visible.clone()
    }
}

fn field_of(id: &str) -> Option<&'static cp::Field> {
    cp::field(id.strip_prefix("cp-f:")?)
}

impl App {
    pub(crate) fn custom_proxies(&self) -> Vec<CustomProxy> {
        self.store
            .lock()
            .unwrap()
            .state
            .settings
            .custom_proxies
            .clone()
    }

    pub(crate) fn update_custom_proxies(self: &Arc<Self>, f: impl FnOnce(&mut Vec<CustomProxy>)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings.custom_proxies);
            store.save();
            if let Some((_, seen)) = SESSION.lock().unwrap().as_mut() {
                seen.extend(
                    store
                        .state
                        .settings
                        .custom_proxies
                        .iter()
                        .map(CustomProxy::name),
                );
            }
        }
        self.publish_settings();
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
    }

    pub(crate) fn validate_proxies(
        &self,
        definitions: &[Value],
    ) -> Result<Vec<Option<String>>, String> {
        let r = self
            .bridge()?
            .call("validate.proxies", json!({ "proxies": definitions }), CALL)?;
        Ok(parse_verdicts(&r, definitions.len()))
    }

    fn publish_custom_proxy(self: &Arc<Self>) {
        let proxies = self.custom_proxies();
        let issue: SharedString = state()
            .lock()
            .unwrap()
            .draft
            .as_ref()
            .map(|d| editor_issue(d, &proxies))
            .unwrap_or_default()
            .into();
        self.ui(move |ui| {
            ui.set_cp_issue(issue);
            ui.set_meta_rev(ui.get_meta_rev() + 1);
        });
        self.publish_settings();
    }

    /// Changes the form, then asks the Core about it once the edits pause (`_scheduleValidate`).
    pub(crate) fn edit_draft(self: &Arc<Self>, f: impl FnOnce(&mut Draft)) {
        let rev = {
            let mut st = state().lock().unwrap();
            let Some(d) = st.draft.as_mut() else {
                return;
            };
            f(d);
            st.rev += 1;
            st.rev
        };
        self.publish_custom_proxy();
        self.spawn(move |app| {
            std::thread::sleep(Duration::from_millis(400));
            let definition = {
                let st = state().lock().unwrap();
                match (&st.draft, st.rev == rev) {
                    (Some(d), true) => d.proxy.definition.clone(),
                    _ => return,
                }
            };
            let Ok(found) = app.validate_proxies(&[definition]) else {
                return;
            };
            {
                let mut st = state().lock().unwrap();
                if st.rev != rev {
                    return;
                }
                match st.draft.as_mut() {
                    Some(d) => d.core_error = found.into_iter().next().flatten(),
                    None => return,
                }
            }
            app.publish_custom_proxy();
        });
    }

    pub(crate) fn open_custom_proxy(self: &Arc<Self>, id: Option<&str>) {
        let existing = id.and_then(|id| self.custom_proxies().into_iter().find(|p| p.id == id));
        let draft = match existing {
            Some(p) => Draft::open(p, false),
            None => {
                let mut m = Map::new();
                m.insert("name".into(), "".into());
                m.insert("type".into(), "ss".into());
                Draft::open(CustomProxy::new(String::new(), m), true)
            }
        };
        state().lock().unwrap().draft = Some(draft);
        self.edit_draft(|_| {});
        self.ui(|ui| ui.set_tools_page(FORM.into()));
    }

    /// The tools page moved: the search ends off the list, an editor target off the editor.
    pub(crate) fn custom_proxies_page_shown(self: &Arc<Self>, page: &str) {
        {
            let mut st = state().lock().unwrap();
            if page != "editor" {
                st.editing = None;
            }
            if page != LIST {
                st.selected.clear();
            }
            if ![LIST, FORM, "editor"].contains(&page) && !st.query.is_empty() {
                st.query.clear();
                self.ui(|ui| {
                    ui.set_set_query("".into());
                    ui.set_set_searching(false);
                });
            }
        }
        self.share_links_page_shown(page);
        if page == LIST {
            self.spawn(|app| app.check_custom_proxies());
        }
        self.track_custom_proxies(page);
    }

    fn track_custom_proxies(self: &Arc<Self>, page: &str) {
        let shown = [LIST, FORM, "editor"].contains(&page);
        let mut session = SESSION.lock().unwrap();
        if shown {
            if session.is_none() && page == LIST {
                let proxies = self.custom_proxies();
                *session = Some((
                    proxies.iter().map(|p| p.definition.clone()).collect(),
                    proxies.iter().map(CustomProxy::name).collect(),
                ));
            }
            return;
        }
        let Some((opened, seen)) = session.take() else {
            return;
        };
        drop(session);
        let latest: Vec<Value> = self
            .custom_proxies()
            .into_iter()
            .map(|p| p.definition)
            .collect();
        if latest == opened {
            return;
        }
        let uses = {
            let store = self.store.lock().unwrap();
            crate::custom_profile::active_uses(&store.state, |s, id| {
                crate::custom_profile::names_proxies(s, id, &seen)
            })
        };
        if uses {
            self.reapply();
        }
    }

    /// `customProxyCoreErrors`: the definitions the Core has not judged yet.
    fn check_custom_proxies(self: &Arc<Self>) {
        let unchecked: Vec<Value> = {
            let seen = verdicts().lock().unwrap();
            self.custom_proxies()
                .into_iter()
                .map(|p| p.definition)
                .filter(|d| !seen.contains_key(&key_of(d)))
                .collect()
        };
        if unchecked.is_empty() {
            return;
        }
        let Ok(found) = self.validate_proxies(&unchecked) else {
            return;
        };
        {
            let mut seen = verdicts().lock().unwrap();
            for (d, v) in unchecked.iter().zip(found) {
                seen.insert(key_of(d), v.unwrap_or_default());
            }
        }
        self.publish_settings();
    }

    fn custom_proxy_titles(&self) -> Vec<SharedString> {
        self.custom_proxies()
            .iter()
            .map(|p| p.name().into())
            .collect()
    }

    pub(crate) fn custom_proxies_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        match page {
            LIST => self.custom_proxies_list_clicked(id),
            FORM => self.custom_proxy_form_clicked(id),
            _ => return false,
        }
        true
    }

    fn custom_proxies_list_clicked(self: &Arc<Self>, id: &str) {
        if id == "action" {
            self.custom_proxies_add_menu();
        } else if id == "empty-action" {
            self.open_custom_proxy(None);
        } else if id == "action2" {
            let titles = self.custom_proxy_titles();
            self.ui(move |ui| {
                ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles)));
                ui.set_set_sorting(true);
            });
        } else if let Some(q) = id.strip_prefix("search:") {
            state().lock().unwrap().query = q.to_string();
            self.publish_settings();
            self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
        } else if let Some(rest) = id.strip_prefix("sort-move:") {
            let mut it = rest.split(':').filter_map(|n| n.parse::<usize>().ok());
            if let (Some(from), Some(to)) = (it.next(), it.next()) {
                self.update_custom_proxies(|s| crate::reorder::move_item(s, from, to));
                let titles = self.custom_proxy_titles();
                self.ui(move |ui| ui.set_set_sort_titles(ModelRc::new(VecModel::from(titles))));
            }
        } else if let Some(pid) = id.strip_prefix("select:cp:") {
            self.toggle_custom_proxy(pid);
        } else if id == "sel-clear" {
            self.set_custom_proxies_selected(BTreeSet::new());
        } else if id == "sel-all" {
            let visible = self.visible_custom_proxies();
            let next = select_all(&state().lock().unwrap().selected, &visible);
            self.set_custom_proxies_selected(next);
        } else if id == "sel-delete" {
            self.delete_selected_custom_proxies();
        } else if let Some(pid) = id.strip_prefix("long:cp:") {
            if let Some(i) = self.custom_proxies().iter().position(|p| p.id == pid) {
                self.custom_proxy_menu(i);
            }
        } else if let Some(pid) = id.strip_prefix("cp:") {
            if state().lock().unwrap().selected.is_empty() {
                self.open_custom_proxy(Some(pid));
            } else {
                self.toggle_custom_proxy(pid);
            }
        }
    }

    fn visible_custom_proxies(&self) -> BTreeSet<String> {
        let query = state().lock().unwrap().query.clone();
        cp::search(&self.custom_proxies(), &query)
            .into_iter()
            .map(|p| p.id.clone())
            .collect()
    }

    fn toggle_custom_proxy(self: &Arc<Self>, id: &str) {
        let mut next = state().lock().unwrap().selected.clone();
        if !next.remove(id) {
            next.insert(id.to_string());
        }
        self.set_custom_proxies_selected(next);
    }

    fn set_custom_proxies_selected(self: &Arc<Self>, selected: BTreeSet<String>) {
        state().lock().unwrap().selected = selected;
        self.publish_settings();
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
    }

    /// `_handleDelete` on the shown selected rows; the hidden ones stay selected.
    fn delete_selected_custom_proxies(self: &Arc<Self>) {
        let visible = self.visible_custom_proxies();
        let ids: BTreeSet<String> = state()
            .lock()
            .unwrap()
            .selected
            .intersection(&visible)
            .cloned()
            .collect();
        if ids.is_empty() {
            return;
        }
        let text = self.delete_message(&ids, "确定删除选中的代理吗？");
        let arg = ids.into_iter().collect::<Vec<_>>().join(",");
        self.confirm_labeled("提示", text, "cp-delete", arg, ("删除", "", true));
    }

    /// `_deleteMessage`: the plain question, or which custom profiles still use the proxies.
    fn delete_message(&self, ids: &BTreeSet<String>, fallback: &str) -> String {
        let names: BTreeSet<String> = self
            .custom_proxies()
            .iter()
            .filter(|p| ids.contains(&p.id))
            .map(CustomProxy::name)
            .collect();
        let users = crate::proxy_groups::proxy_users(&self.store.lock().unwrap().state, &names);
        if users.is_empty() {
            return fallback.to_string();
        }
        let names: Vec<String> = names.into_iter().collect();
        format!(
            "{} 仍被 {} 使用，删除后这些配置会异常。确定删除吗？",
            names.join(", "),
            users.join(", ")
        )
    }

    /// `addToLocalProxies`: the node's definition from the Core, kept unless a local proxy
    /// already holds it.
    pub(crate) fn add_to_local_proxies(self: &Arc<Self>, name: &str) {
        let name = name.to_string();
        self.spawn(move |app| {
            let found = app.bridge().and_then(|b| {
                b.call("proxy.definition", json!({ "name": name }), CALL)
                    .map(|r| r.get("proxy").and_then(Value::as_object).cloned())
                    .or_else(|e| {
                        let missing = e.contains("not_found") || e.contains("not found");
                        if missing {
                            Ok(None)
                        } else {
                            Err(e)
                        }
                    })
            });
            let definition = match found {
                Ok(Some(d)) => d,
                Ok(None) => return app.alert("提示", format!("以下代理或策略不存在：{name}")),
                Err(e) => return app.alert("提示", e),
            };
            let proxies = app.custom_proxies();
            let existing: Vec<Value> = proxies.iter().map(|p| p.definition.clone()).collect();
            let groups = app.group_names().into_iter().collect();
            match crate::custom_import::import_proxy(&definition, &existing, &groups) {
                (name, None) => app.toast(format!("本地代理中已有 {name}")),
                (name, Some(added)) => {
                    app.update_custom_proxies(|s| s.push(CustomProxy::new(new_id(), added)));
                    app.toast(format!("已将 {name} 添加到本地代理"));
                }
            }
        });
    }

    fn custom_proxy_menu(self: &Arc<Self>, index: usize) {
        let item = |id: &str, glyph: &str, label: &str, danger: bool| ActionItem {
            id: id.into(),
            glyph: glyph.into(),
            label: label.into(),
            danger,
            enabled: true,
            ..Default::default()
        };
        let items = vec![
            item("edit", "edit", "编辑", false),
            item("delete", "delete", "删除", true),
        ];
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(index.to_string().into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind("cp".into());
        });
    }

    pub(crate) fn custom_proxy_menu_picked(self: &Arc<Self>, index: usize, action: &str) {
        let Some(p) = self.custom_proxies().get(index).cloned() else {
            return;
        };
        match action {
            "edit" => self.open_custom_proxy(Some(&p.id)),
            "delete" => {
                let text = self.delete_message(&[p.id.clone()].into(), "确定删除当前代理吗？");
                self.confirm_labeled("提示", text, "cp-delete", p.id, ("删除", "", true))
            }
            _ => {}
        }
    }

    /// The confirmed delete of the proxies `ids` names, comma separated.
    pub(crate) fn delete_custom_proxy(self: &Arc<Self>, ids: &str) {
        let ids: BTreeSet<String> = ids.split(',').map(String::from).collect();
        let closing = {
            let mut st = state().lock().unwrap();
            st.selected.retain(|id| !ids.contains(id));
            let closing = st.draft.as_ref().is_some_and(|d| ids.contains(&d.proxy.id));
            if closing {
                st.draft = None;
            }
            closing
        };
        self.update_custom_proxies(move |s| s.retain(|p| !ids.contains(&p.id)));
        if closing {
            self.ui(|ui| ui.set_tools_page(LIST.into()));
        }
    }

    pub(crate) fn draft(&self) -> Option<Draft> {
        state().lock().unwrap().draft.clone()
    }

    fn custom_proxy_form_clicked(self: &Arc<Self>, id: &str) {
        let Some(d) = self.draft() else {
            return;
        };
        let p = &d.proxy;
        match id {
            "action" => self.spawn(|app| app.save_custom_proxy()),
            "action2" => self.open_link_dialog(),
            "cp-add" => self.open_option_sheet(&d),
            "cp-delete" => {
                let id = p.id.clone();
                let text = self.delete_message(&[id.clone()].into(), "确定删除当前代理吗？");
                self.confirm_labeled("提示", text, "cp-delete", id, ("删除", "", true));
            }
            "cp-f:name" => self.open_input("cp-in:name", "名称", "", &p.name()),
            "cp-f:server" => self.open_input(
                "cp-in:server",
                "服务器",
                "",
                &p.server().unwrap_or_default(),
            ),
            "cp-f:port" => {
                let port = p.port().map(|n| n.to_string()).unwrap_or_default();
                self.open_input("cp-in:port", "端口", "", &port);
                self.ui(|ui| ui.set_input_number(true));
            }
            "cp-f:type" => {
                let kind = p.kind();
                let items = cp::TYPES
                    .iter()
                    .map(|t| check_item(format!("cp-type:{t}"), t, *t == kind))
                    .collect();
                self.show_choice("代理类型", items);
            }
            id => {
                if let Some(f) = id.strip_prefix("remove:").and_then(field_of) {
                    self.edit_draft(|d| d.remove_option(f));
                } else if let Some(f) = id.strip_prefix("reveal:").and_then(field_of) {
                    self.edit_draft(|d| {
                        if !d.revealed.remove(f.name) {
                            d.revealed.insert(f.name.to_string());
                        }
                    });
                } else if let Some(f) = field_of(id) {
                    self.open_field(&d, f);
                }
            }
        }
    }

    /// `_handleAddOption`: the options not listed yet, under their sections.
    fn open_option_sheet(self: &Arc<Self>, d: &Draft) {
        let mut last: Option<Section> = None;
        let items = d
            .addable()
            .into_iter()
            .map(|(s, f)| {
                let heading = if last != Some(s) { s.label() } else { "" };
                last = Some(s);
                ActionItem {
                    id: format!("cp-add:{}", f.name).into(),
                    heading: heading.into(),
                    label: f.label.into(),
                    enabled: true,
                    ..Default::default()
                }
            })
            .collect();
        self.show_choice("添加设置项", items);
    }

    fn open_field(self: &Arc<Self>, d: &Draft, f: &'static cp::Field) {
        let p = &d.proxy;
        let key = format!("cp-in:{}", f.name);
        let text = p.value_of(f).map(cp::text_of).unwrap_or_default();
        match f.kind {
            Kind::Text | Kind::Secret => self.open_input(&key, f.label, "", &text),
            Kind::Number => {
                self.open_input(&key, f.label, "", &text);
                self.ui(|ui| ui.set_input_number(true));
            }
            Kind::Choice => {
                let items = p
                    .choices(f)
                    .into_iter()
                    .map(|c| check_item(format!("cp-choice:{}\t{c}", f.name), c, c == text))
                    .collect();
                self.show_choice(f.label, items);
            }
            Kind::List => self.open_field_editor(f, &p.strings_of(f.path).join("\n")),
            Kind::Map => self.open_field_editor(f, &cp::map_text(&p.string_map_of(f))),
            Kind::Toggle => {}
        }
    }

    fn open_field_editor(self: &Arc<Self>, f: &'static cp::Field, text: &str) {
        crate::rule_sets_app::release_editor();
        state().lock().unwrap().editing = Some(f.name);
        let (title, text): (SharedString, SharedString) = (f.label.into(), text.into());
        self.ui(move |ui| {
            ui.set_tools_editor_title(title);
            ui.set_tools_editor_text(text);
            ui.set_tools_editor_guard(false);
            ui.set_tools_editor_parent(FORM.into());
            ui.set_tools_editor_readonly(false);
            ui.set_tools_editor_title_editable(false);
            ui.set_tools_page("editor".into());
        });
    }

    /// The tools editor's save for a list (an item per line or comma) or a map (`key: value`).
    pub(crate) fn custom_proxy_editor_save(self: &Arc<Self>, text: &str) -> bool {
        let Some(f) = state().lock().unwrap().editing.and_then(cp::field) else {
            return false;
        };
        if f.kind == Kind::Map {
            match cp::parse_map(text) {
                Ok(entries) => {
                    let next_of = move |d: &mut Draft| {
                        let next = d.proxy.with_entries(f, &entries);
                        d.proxy = next;
                    };
                    self.edit_draft(next_of);
                }
                Err(e) => {
                    self.toast(e);
                    return true;
                }
            }
        } else {
            let items = cp::parse_list(text);
            let value = (!items.is_empty()).then(|| json!(items));
            self.edit_draft(|d| d.proxy = d.proxy.with_value_at(f.path, value));
        }
        state().lock().unwrap().editing = None;
        self.ui(|ui| ui.set_tools_page(FORM.into()));
        true
    }

    pub(crate) fn custom_proxies_toggled(self: &Arc<Self>, page: &str, id: &str, on: bool) -> bool {
        if page != FORM {
            return false;
        }
        if id == "cp-f:udp" {
            self.edit_draft(|d| d.proxy = d.proxy.with_value("udp", Some(on.into())));
        } else if let Some(f) = field_of(id) {
            self.edit_draft(|d| d.set_option(f, Some(on.into())));
        }
        true
    }

    pub(crate) fn custom_proxies_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match key {
            "cp-type" => {
                let kind = value.to_string();
                self.edit_draft(move |d| d.set_type(&kind));
            }
            "cp-choice" => {
                let Some((name, choice)) = value.split_once('\t') else {
                    return true;
                };
                if let Some(f) = cp::field(name) {
                    let choice = choice.to_string();
                    self.edit_draft(move |d| d.set_option(f, Some(choice.into())));
                }
            }
            "cp-add" => {
                if let Some(f) = cp::field(value) {
                    self.edit_draft(|d| d.add_option(f));
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn custom_proxies_check(&self, key: &str, v1: &str) -> Option<String> {
        let target = key.strip_prefix("cp-in:")?;
        let v = v1.trim();
        let number = target == "port" || cp::field(target).is_some_and(|f| f.kind == Kind::Number);
        let issue = if v.is_empty() || !number {
            None
        } else if target == "port" {
            match v.parse::<u32>() {
                Ok(1..=65535) => None,
                _ => Some("端口必须在 1 到 65535 之间".to_string()),
            }
        } else if v.parse::<i64>().is_err() {
            Some("请输入数字".to_string())
        } else {
            None
        };
        Some(issue.unwrap_or_default())
    }

    pub(crate) fn custom_proxies_input(self: &Arc<Self>, key: &str, v1: &str) -> bool {
        let Some(target) = key.strip_prefix("cp-in:") else {
            return false;
        };
        let text = |v: &str| (!v.is_empty()).then(|| Value::from(v));
        match target {
            "name" | "server" => {
                let (k, v) = (target.to_string(), text(v1.trim()));
                self.edit_draft(move |d| d.proxy = d.proxy.with_value(&k, v));
            }
            "port" => {
                let v = v1.trim().parse::<i64>().ok().map(Value::from);
                self.edit_draft(move |d| d.proxy = d.proxy.with_value("port", v));
            }
            name => {
                let Some(f) = cp::field(name) else {
                    return true;
                };
                let v = match f.kind {
                    Kind::Number => v1.trim().parse::<i64>().ok().map(Value::from),
                    Kind::Secret => text(v1),
                    _ => text(v1.trim()),
                };
                self.edit_draft(move |d| d.set_option(f, v));
            }
        }
        true
    }

    /// `_handleSaveCustomProxy`: the name first, then the Core when it is there to ask.
    fn save_custom_proxy(self: &Arc<Self>) {
        let Some(d) = self.draft() else {
            return;
        };
        let proxies = self.custom_proxies();
        let mut found = issues(&d.proxy, &proxies, &BTreeSet::new(), None);
        if found.is_empty() {
            if let Ok(v) = self.validate_proxies(&[d.proxy.definition.clone()]) {
                found.extend(v.into_iter().flatten().map(Issue::CoreRejected));
            }
        }
        if !found.is_empty() {
            let core = found.iter().find_map(|i| match i {
                Issue::CoreRejected(m) => Some(m.clone()),
                _ => None,
            });
            self.edit_draft(move |d| {
                d.submitted = true;
                if core.is_some() {
                    d.core_error = core;
                }
            });
            return self.alert("无法保存", issues_message(&found));
        }
        let mut saved = d.finished();
        if saved.id.is_empty() {
            saved.id = new_id();
        }
        verdicts()
            .lock()
            .unwrap()
            .insert(key_of(&saved.definition), String::new());
        state().lock().unwrap().draft = None;
        self.update_custom_proxies(move |s| match s.iter_mut().find(|p| p.id == saved.id) {
            Some(p) => *p = saved,
            None => s.push(saved),
        });
        self.ui(|ui| ui.set_tools_page(LIST.into()));
    }
}

fn check_item(id: String, label: &str, checked: bool) -> ActionItem {
    ActionItem {
        id: id.into(),
        label: label.into(),
        check: true,
        checked,
        enabled: true,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    fn p(id: &str, def: Value) -> CustomProxy {
        CustomProxy {
            id: id.into(),
            definition: def,
            ..Default::default()
        }
    }

    #[test]
    fn list_rows_mark_issues_and_follow_the_search() {
        let all = vec![
            p(
                "1",
                json!({"name": "hk", "type": "ss", "server": "a.example"}),
            ),
            p("2", json!({"name": "hk", "type": "vmess"})),
            p("3", json!({"name": "jp", "type": "trojan"})),
        ];
        let mut errors = HashMap::new();
        errors.insert(key_of(&all[2].definition), "bad".to_string());
        let s = list_sections(&all, "", &errors, &BTreeSet::new());
        let rows: Vec<SetRow> = s[0].rows.iter().collect();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.invalid));
        assert_eq!(rows[1].subtitle, "vmess");
        assert_eq!(
            list_sections(&all, "a.example", &errors, &BTreeSet::new())[0]
                .rows
                .row_count(),
            1
        );
        assert!(list_sections(&all, "zz", &errors, &BTreeSet::new()).is_empty());
        assert!(
            list_sections(&all[2..], "", &HashMap::new(), &["3".to_string()].into())[0]
                .rows
                .iter()
                .all(|r| !r.invalid && r.on && r.kind == "select")
        );
    }

    #[test]
    fn select_all_takes_the_shown_rows_or_clears() {
        let set = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>();
        assert_eq!(
            select_all(&set(&["1"]), &set(&["1", "2"])),
            set(&["1", "2"])
        );
        assert!(select_all(&set(&["1", "2", "3"]), &set(&["1", "2"])).is_empty());
    }

    #[test]
    fn the_form_lists_basics_credentials_and_options() {
        let mut d = Draft::open(
            p(
                "1",
                json!({"name": "a", "type": "vmess", "uuid": "u", "tls": true}),
            ),
            false,
        );
        let s = editor_sections(&d);
        let ids =
            |i: usize| -> Vec<String> { s[i].rows.iter().map(|r| r.id.to_string()).collect() };
        assert_eq!(
            ids(0),
            [
                "cp-f:name",
                "cp-f:type",
                "cp-f:server",
                "cp-f:port",
                "cp-f:cipher",
                "cp-f:alterId",
                "cp-f:udp"
            ]
        );
        assert_eq!(ids(1), ["cp-f:uuid"]);
        assert_eq!(ids(2), ["cp-f:tls", "cp-add"]);
        assert_eq!(s[2].rows.row_data(0).unwrap().lead, "remove");
        d.proxy = d.proxy.with_value("password", Some("x".into()));
        d.set_type("trojan");
        let s = editor_sections(&d);
        let pw = s[1].rows.row_data(0).unwrap();
        assert_eq!((pw.subtitle.as_str(), pw.glyph.as_str()), ("••••••", "eye"));
        assert_eq!(page_meta(&[], FORM).unwrap().action, "check");
    }

    #[test]
    fn only_a_saved_proxy_offers_delete() {
        let saved = Draft::open(p("1", json!({"name": "a", "type": "ss"})), false);
        let last = editor_sections(&saved).pop().unwrap();
        assert_eq!(last.title, "操作");
        let row = last.rows.row_data(0).unwrap();
        assert_eq!(
            (row.id.as_str(), row.kind.as_str()),
            ("cp-delete", "danger")
        );
        let new = Draft::open(p("", json!({"name": "", "type": "ss"})), true);
        assert!(editor_sections(&new).iter().all(|s| s.title != "操作"));
    }

    #[test]
    fn an_easytier_proxy_shows_the_network_section() {
        let d = Draft::open(p("1", json!({"name": "e", "type": "easytier"})), false);
        let s = editor_sections(&d);
        let titles: Vec<String> = s.iter().map(|s| s.title.to_string()).collect();
        assert_eq!(titles, ["基础信息", "网络", "认证", "选项", "操作"]);
        let ids: Vec<String> = s[1].rows.iter().map(|r| r.id.to_string()).collect();
        assert_eq!(ids[0], "cp-f:networkName");
        assert_eq!(s[1].rows.row_data(2).unwrap().kind, "open");
        assert!(!s[0].rows.iter().any(|r| r.id == "cp-f:server"));
    }

    #[test]
    fn verdicts_come_as_a_list_or_by_index() {
        let r = json!({"issues": [{"index": 1, "message": "bad"}]});
        assert_eq!(parse_verdicts(&r, 2), [None, Some("bad".into())]);
        assert_eq!(parse_verdicts(&json!({"0": "x"}), 1), [Some("x".into())]);
        assert_eq!(parse_verdicts(&json!({}), 1), [None]);
    }
}
