//! The custom profile's 策略组 on `App`'s side (lib/views/profiles/custom/groups.dart): the
//! section's rows, selection, sort mode and menu (`CustomProxyGroupsSection`), and the form
//! (`EditProxyGroupView`) with its options, quick edit and save.

use crate::custom_profile as cp;
use crate::proxy_groups::{self as pg, Data, Issue, Member, Opt, OPTS};
use crate::{profile_ref, ActionItem, Anchor, App, GroupRow, SetRow, SetSection, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The form as edited: `shown` keeps an option's row while its value is cleared.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub id: u64,
    pub is_new: bool,
    pub def: Value,
    pub origin: Value,
    pub shown: BTreeSet<Opt>,
    pub submitted: bool,
    pub filter_errors: BTreeMap<usize, String>,
}

impl Draft {
    pub fn open(id: u64, def: Value, is_new: bool) -> Draft {
        Draft {
            id,
            is_new,
            shown: shown_in(&def),
            origin: def.clone(),
            def,
            ..Default::default()
        }
    }

    fn applicable(&self) -> Vec<Opt> {
        let kind = pg::kind(&self.def);
        let strategy = pg::strategy(&self.def);
        OPTS.into_iter()
            .filter(|o| o.applies_to(&kind, strategy.as_deref()))
            .collect()
    }

    pub fn listed(&self) -> Vec<Opt> {
        self.applicable()
            .into_iter()
            .filter(|o| self.shown.contains(o))
            .collect()
    }

    pub fn addable(&self) -> Vec<Opt> {
        self.applicable()
            .into_iter()
            .filter(|o| !self.shown.contains(o))
            .collect()
    }

    fn set(&mut self, key: &str, value: Option<Value>) {
        let Some(m) = self.def.as_object_mut() else {
            return;
        };
        match value {
            Some(v) => {
                m.insert(key.into(), v);
            }
            None => {
                m.remove(key);
            }
        }
    }

    pub fn add_option(&mut self, o: Opt, test_url: &str) {
        self.set(o.key(), Some(o.reset_value(test_url)));
        self.shown.insert(o);
    }

    pub fn remove_option(&mut self, o: Opt) {
        self.set(o.key(), None);
        self.shown.remove(&o);
    }

    fn filter_issues(&self) -> Vec<Issue> {
        ["筛选", "排除筛选"]
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                self.filter_errors
                    .get(&i)
                    .map(|m| Issue::InvalidFilter(t.to_string(), m.clone()))
            })
            .collect()
    }

    /// The banner's issues: the incomplete ones wait for the first save on a new group.
    pub fn issues(&self, data: &Data) -> Vec<Issue> {
        let show_incomplete = !self.is_new || self.submitted;
        pg::issues(self.id, &self.def, data)
            .into_iter()
            .chain(self.filter_issues())
            .filter(|i| show_incomplete || !i.is_incomplete())
            .collect()
    }

    pub fn filters(&self) -> Vec<String> {
        vec![
            pg::text(&self.def, "filter"),
            pg::text(&self.def, "exclude-filter"),
        ]
    }
}

pub fn shown_in(def: &Value) -> BTreeSet<Opt> {
    OPTS.into_iter().filter(|o| o.is_set_in(def)).collect()
}

/// The section's rows (`_ProxyGroupItem`).
pub fn group_rows(data: &Data, selected: &BTreeSet<u64>) -> Vec<GroupRow> {
    let issues = pg::profile_issues(data);
    data.groups
        .iter()
        .map(|(id, def)| {
            let name = pg::name(def);
            let icon = pg::text(def, "icon");
            GroupRow {
                id: id.to_string().into(),
                mark: pg::monogram(&name).into(),
                name: name.into(),
                kind: pg::type_name(&pg::kind(def)).into(),
                icon: if crate::icon_sets::is_http_url(&icon) {
                    icon
                } else {
                    String::new()
                }
                .into(),
                invalid: issues.contains_key(id),
                selected: selected.contains(id),
            }
        })
        .collect()
}

fn row(id: &str, kind: &str, title: &str, subtitle: &str) -> SetRow {
    SetRow {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        enabled: true,
        ..Default::default()
    }
}

fn section(title: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

/// A member's title and subtitle as `ProxyGroupMembersSliver` gives them, and whether it
/// names something the profile lacks.
fn member_row(i: usize, m: &Member, data: &Data, presets: &[(String, String)]) -> SetRow {
    let preset = |v: &str| {
        presets
            .iter()
            .find(|(regex, _)| regex == v)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| v.to_string())
    };
    let group_type = |n: &str| {
        data.groups
            .iter()
            .find(|(_, g)| pg::name(g) == n)
            .map(|(_, g)| pg::kind(g))
    };
    let (title, subtitle, invalid) = match m {
        Member::Proxy(n) => (
            n.clone(),
            group_type(n)
                .or_else(|| data.proxy_types.get(n).cloned())
                .or_else(|| {
                    pg::RESERVED
                        .contains(&n.as_str())
                        .then(|| "基础策略".into())
                })
                .unwrap_or_default(),
            !data.rule_targets().contains(n) && !data.proxies.contains(n),
        ),
        Member::Provider(n) => (
            n.clone(),
            "代理集".into(),
            !data.proxy_providers.contains(n),
        ),
        Member::AllProxies => ("全部被引用的代理".into(), "自动包含".into(), false),
        Member::AllProviders => ("全部被引用的代理集".into(), "自动包含".into(), false),
        Member::Filter(v) => (preset(v), "筛选".into(), false),
        Member::ExcludeFilter(v) => (preset(v), "排除筛选".into(), false),
    };
    let mut r = row(&format!("pg-m:{i}"), "", &title, &subtitle);
    r.invalid = invalid;
    r
}

/// The form's sections: 常规, 节点 (read only until the member picker), 选项 and 操作.
pub fn form_sections(d: &Draft, data: &Data, presets: &[(String, String)]) -> Vec<SetSection> {
    let issues = d.issues(data);
    let name = pg::name(&d.def);
    let mut name_row = row(
        "pg-f:name",
        "value",
        "名称",
        if name.is_empty() {
            "输入策略组名称"
        } else {
            &name
        },
    );
    name_row.invalid = issues.iter().any(Issue::is_name);
    let icon = pg::text(&d.def, "icon");
    let mut icon_row = row(
        "pg-f:icon",
        "open",
        "图标",
        if icon.is_empty() { "可选" } else { "" },
    );
    if crate::icon_sets::is_http_url(&icon) {
        icon_row.image = icon.into();
    } else if !icon.is_empty() {
        icon_row.subtitle = pg::monogram(&icon).into();
    }
    let general = vec![
        name_row,
        row(
            "pg-f:type",
            "open",
            "代理类型",
            &pg::type_name(&pg::kind(&d.def)),
        ),
        icon_row,
    ];
    let mut members: Vec<SetRow> = pg::members(&d.def)
        .iter()
        .enumerate()
        .map(|(i, m)| member_row(i, m, data, presets))
        .collect();
    let mut add = row("pg-members-add", "action", "添加节点", "");
    add.lead = "add".into();
    add.enabled = false;
    members.push(add);
    let fallback_invalid = issues
        .iter()
        .any(|i| matches!(i, Issue::InvalidEmptyFallback(_)));
    let mut options: Vec<SetRow> = d
        .listed()
        .into_iter()
        .map(|o| {
            let mut r = row(
                &format!("pg-o:{}", o.key()),
                if o.is_toggle() {
                    "toggle"
                } else if matches!(o, Opt::EmptyFallback | Opt::DefaultSelected | Opt::Strategy) {
                    "open"
                } else {
                    "value"
                },
                o.label(),
                "",
            );
            if o.is_toggle() {
                r.on = o.on(&d.def);
            } else {
                r.subtitle = o.shown(&d.def).into();
            }
            r.lead = "remove".into();
            r.invalid = o == Opt::EmptyFallback && fallback_invalid;
            r
        })
        .collect();
    if !d.addable().is_empty() {
        let mut r = row("pg-add", "action", "添加设置项", "");
        r.lead = "add".into();
        options.push(r);
    }
    let mut out = vec![
        section("常规", general),
        section("节点", members),
        section("选项", options),
    ];
    if !d.is_new {
        out.push(section(
            "操作",
            vec![row("pg-delete", "danger", "删除", "")],
        ));
    }
    out
}

#[derive(Default)]
struct Section {
    selected: BTreeSet<u64>,
    sorting: bool,
    draft: Option<Draft>,
    rev: u64,
    yaml_open: bool,
}

fn section_state() -> &'static Mutex<Section> {
    static S: OnceLock<Mutex<Section>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn new_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_default()
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

impl App {
    fn group_data(&self, profile_id: &str) -> Data {
        pg::data(&self.store.lock().unwrap().state, profile_id)
    }

    fn filter_presets(&self) -> Vec<(String, String)> {
        self.store
            .lock()
            .unwrap()
            .state
            .settings
            .filters
            .iter()
            .map(|f| (f.regex.clone(), f.label.clone()))
            .collect()
    }

    /// The section's rows and the profile's group issue count, for `publish_custom`.
    pub(crate) fn publish_custom_groups(self: &Arc<Self>, profile_id: &str) -> usize {
        let data = self.group_data(profile_id);
        let (selected, sorting) = {
            let mut st = section_state().lock().unwrap();
            let ids: BTreeSet<u64> = data.groups.iter().map(|(id, _)| *id).collect();
            st.selected = st.selected.intersection(&ids).copied().collect();
            if data.groups.is_empty() {
                st.sorting = false;
            }
            (st.selected.clone(), st.sorting)
        };
        let issues = pg::profile_issues(&data).len();
        let rows = group_rows(&data, &selected);
        let titles: Vec<SharedString> = data
            .groups
            .iter()
            .map(|(_, g)| pg::name(g).into())
            .collect();
        let kinds: Vec<SharedString> = data
            .groups
            .iter()
            .map(|(_, g)| pg::type_name(&pg::kind(g)).into())
            .collect();
        let selecting = !selected.is_empty();
        self.ui(move |ui| {
            ui.set_custom_groups(ModelRc::new(VecModel::from(rows)));
            ui.set_custom_group_titles(ModelRc::new(VecModel::from(titles)));
            ui.set_custom_group_kinds(ModelRc::new(VecModel::from(kinds)));
            ui.set_custom_group_selecting(selecting);
            ui.set_custom_group_sorting(sorting);
        });
        issues
    }

    pub(crate) fn reset_groups_section(&self) {
        *section_state().lock().unwrap() = Section::default();
    }

    fn edit_groups(self: &Arc<Self>, f: impl FnOnce(&mut crate::store::State)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state);
            store.save();
        }
        self.publish_custom();
    }

    /// The section's header actions and rows: `add`, `sort`, `sort-done`, `delete`,
    /// `select-all`, `clear`, `open:<id>`, `select:<id>`, `long:<id>`, `move:<from>:<to>`.
    pub(crate) fn custom_group(self: &Arc<Self>, action: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let (verb, arg) = action.split_once(':').unwrap_or((action, ""));
        let id = arg.parse::<u64>().ok();
        match verb {
            "add" => self.open_group(&pid, None),
            "open" => self.open_group(&pid, id),
            "sort" | "sort-done" => {
                {
                    let mut st = section_state().lock().unwrap();
                    st.sorting = verb == "sort";
                }
                self.publish_custom();
            }
            "clear" => {
                {
                    let mut st = section_state().lock().unwrap();
                    st.selected.clear();
                    st.sorting = false;
                }
                self.publish_custom();
            }
            "select" => {
                if let Some(id) = id {
                    let mut st = section_state().lock().unwrap();
                    if !st.selected.remove(&id) {
                        st.selected.insert(id);
                    }
                }
                self.publish_custom();
            }
            "select-all" => {
                let ids: BTreeSet<u64> = self
                    .group_data(&pid)
                    .groups
                    .iter()
                    .map(|(id, _)| *id)
                    .collect();
                {
                    let mut st = section_state().lock().unwrap();
                    st.selected = if st.selected.is_superset(&ids) {
                        BTreeSet::new()
                    } else {
                        ids
                    };
                }
                self.publish_custom();
            }
            "delete" => {
                let ids = section_state().lock().unwrap().selected.clone();
                self.ask_delete_groups(&pid, ids, "确定删除选中的策略组吗？");
            }
            "long" => {
                if let Some(id) = id {
                    self.group_menu(id);
                }
            }
            "move" => {
                let mut it = arg.split(':').filter_map(|n| n.parse::<usize>().ok());
                if let (Some(from), Some(to)) = (it.next(), it.next()) {
                    self.edit_groups(|s| pg::reorder(s, &pid, from, to));
                }
            }
            _ => {}
        }
    }

    fn group_menu(self: &Arc<Self>, id: u64) {
        let item = |id: &str, glyph: &str, label: &str, danger: bool| ActionItem {
            id: id.into(),
            glyph: glyph.into(),
            label: label.into(),
            danger,
            enabled: true,
            ..Default::default()
        };
        let items = vec![
            item("copy", "copy", "复制", false),
            item("delete", "delete", "删除", true),
        ];
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(id.to_string().into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind("pg".into());
        });
    }

    pub(crate) fn group_menu_picked(self: &Arc<Self>, id: &str, action: &str) {
        let (Some(pid), Ok(id)) = (crate::custom_profile_app::page_id(), id.parse::<u64>()) else {
            return;
        };
        match action {
            "copy" => self.edit_groups(|s| {
                let mut next = new_id();
                pg::duplicate(s, &pid, &[id].into(), || {
                    next += 1;
                    next
                })
            }),
            "delete" => self.ask_delete_groups(&pid, [id].into(), "确定要删除当前策略组吗？"),
            _ => {}
        }
    }

    /// `_handleDelete`: names still in use change the question.
    fn ask_delete_groups(self: &Arc<Self>, pid: &str, ids: BTreeSet<u64>, message: &str) {
        if ids.is_empty() {
            return;
        }
        let in_use = pg::in_use(&self.store.lock().unwrap().state, pid, &ids);
        let text = if in_use.is_empty() {
            message.to_string()
        } else {
            format!(
                "{} 仍被当前配置的规则、策略组、拨号代理、DNS 或 NTP 引用，删除后这些引用会失效。确定删除吗？",
                in_use.join(", ")
            )
        };
        let arg = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        self.confirm("提示", text, "pg-delete", arg);
    }

    pub(crate) fn delete_groups(self: &Arc<Self>, arg: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let ids: BTreeSet<u64> = arg.split(',').filter_map(|s| s.parse().ok()).collect();
        let closing = {
            let mut st = section_state().lock().unwrap();
            st.selected.clear();
            let closing = st.draft.as_ref().is_some_and(|d| ids.contains(&d.id));
            if closing {
                st.draft = None;
            }
            closing
        };
        self.edit_groups(|s| pg::delete(s, &pid, &ids));
        if closing {
            self.ui(|ui| ui.set_profiles_page("custom".into()));
        }
    }

    fn open_group(self: &Arc<Self>, pid: &str, id: Option<u64>) {
        let existing = id.and_then(|id| {
            cp::groups(&self.store.lock().unwrap().state, pid)
                .into_iter()
                .find(|g| g.id == id)
                .map(|g| (g.id, g.definition.clone()))
        });
        let draft = match existing {
            Some((id, def)) => Draft::open(id, def, false),
            None => Draft::open(0, json!({"name": "", "type": "select"}), true),
        };
        {
            let mut st = section_state().lock().unwrap();
            st.draft = Some(draft);
            st.yaml_open = false;
        }
        self.publish_group_form();
        self.check_group_filters();
        self.ui(|ui| ui.set_profiles_page("group".into()));
    }

    fn publish_group_form(self: &Arc<Self>) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let Some(d) = section_state().lock().unwrap().draft.clone() else {
            return;
        };
        let data = self.group_data(&pid);
        let presets = self.filter_presets();
        let issue: SharedString = pg::issues_message(&d.issues(&data)).into();
        let title: SharedString = if d.is_new {
            "添加策略组"
        } else {
            "编辑策略组"
        }
        .into();
        self.ui(move |ui| {
            let sections = form_sections(&d, &data, &presets);
            ui.set_pg_title(title);
            ui.set_pg_sections(ModelRc::new(VecModel::from(sections)));
            ui.set_pg_issue(issue);
        });
    }

    fn edit_group_draft(self: &Arc<Self>, f: impl FnOnce(&mut Draft)) {
        {
            let mut st = section_state().lock().unwrap();
            let Some(d) = st.draft.as_mut() else {
                return;
            };
            f(d);
        }
        self.publish_group_form();
    }

    /// `validate.filters`: the issue of each filter by index, None when the Core is away.
    fn validate_filters(&self, filters: &[String]) -> Option<BTreeMap<usize, String>> {
        if filters.iter().all(String::is_empty) {
            return Some(BTreeMap::new());
        }
        let r = self
            .bridge()
            .ok()?
            .call("validate.filters", json!({ "filters": filters }), CALL)
            .ok()?;
        Some(
            crate::custom_proxies_app::parse_verdicts(&r, filters.len())
                .into_iter()
                .enumerate()
                .filter_map(|(i, v)| v.map(|v| (i, v)))
                .collect(),
        )
    }

    /// `_scheduleFilterCheck`: once the filters settle, the Core's word on them.
    fn check_group_filters(self: &Arc<Self>) {
        let rev = {
            let mut st = section_state().lock().unwrap();
            st.rev += 1;
            st.rev
        };
        self.spawn(move |app| {
            std::thread::sleep(Duration::from_millis(400));
            let filters = {
                let st = section_state().lock().unwrap();
                match (&st.draft, st.rev == rev) {
                    (Some(d), true) => d.filters(),
                    _ => return,
                }
            };
            let Some(found) = app.validate_filters(&filters) else {
                return;
            };
            {
                let mut st = section_state().lock().unwrap();
                if st.rev != rev {
                    return;
                }
                match st.draft.as_mut() {
                    Some(d) => d.filter_errors = found,
                    None => return,
                }
            }
            app.publish_group_form();
        });
    }

    fn test_url(&self) -> String {
        crate::profile::test_url(&self.store.lock().unwrap().state.settings)
    }

    /// A tap on the form: its rows and the bar's ✓ (`action`) and quick edit (`action2`).
    pub(crate) fn pg_clicked(self: &Arc<Self>, id: &str) {
        let Some(d) = section_state().lock().unwrap().draft.clone() else {
            return;
        };
        match id {
            "action" => self.spawn(|app| app.save_group()),
            "action2" => self.open_group_yaml(&d),
            "pg-f:name" => self.open_input("pg-in:name", "名称", "", &pg::name(&d.def)),
            "pg-f:type" => {
                let kind = pg::kind(&d.def);
                let items = pg::TYPES
                    .iter()
                    .map(|(v, n)| check_item(format!("pg-type:{v}"), n, *v == kind))
                    .collect();
                self.show_choice("代理类型", items);
            }
            "pg-f:icon" => self.ui(|ui| {
                ui.set_placeholder_title("图标".into());
                ui.set_placeholder_parent("group".into());
                ui.set_profiles_page("placeholder".into());
            }),
            "pg-add" => {
                let mut last = "";
                let items = d
                    .addable()
                    .into_iter()
                    .map(|o| {
                        let heading = if last != o.section() { o.section() } else { "" };
                        last = o.section();
                        ActionItem {
                            id: format!("pg-add:{}", o.key()).into(),
                            heading: heading.into(),
                            label: o.label().into(),
                            enabled: true,
                            ..Default::default()
                        }
                    })
                    .collect();
                self.show_choice("添加设置项", items);
            }
            "pg-delete" => {
                if let Some(pid) = crate::custom_profile_app::page_id() {
                    self.ask_delete_groups(&pid, [d.id].into(), "确定要删除当前策略组吗？");
                }
            }
            id => {
                if let Some(o) = id.strip_prefix("remove:pg-o:").and_then(Opt::of) {
                    self.edit_group_draft(|d| d.remove_option(o));
                } else if let Some(o) = id.strip_prefix("pg-o:").and_then(Opt::of) {
                    self.open_group_option(&d, o);
                }
            }
        }
    }

    fn open_group_option(self: &Arc<Self>, d: &Draft, o: Opt) {
        let key = format!("pg-in:{}", o.key());
        match o {
            Opt::Strategy => {
                let current = pg::strategy(&d.def).unwrap_or_else(|| "consistent-hashing".into());
                let items = pg::STRATEGIES
                    .iter()
                    .map(|s| check_item(format!("pg-pick:strategy\t{s}"), s, *s == current))
                    .collect();
                self.show_choice("策略", items);
            }
            Opt::EmptyFallback => {
                let current = o.shown(&d.def);
                let named = self
                    .group_data(&crate::custom_profile_app::page_id().unwrap_or_default())
                    .named_proxies();
                let mut items: Vec<ActionItem> = Vec::new();
                for (i, r) in pg::RESERVED.iter().enumerate() {
                    let mut it =
                        check_item(format!("pg-pick:empty-fallback\t{r}"), r, *r == current);
                    if i == 0 {
                        it.heading = "基础策略".into();
                    }
                    items.push(it);
                }
                for (i, n) in named.iter().enumerate() {
                    let mut it =
                        check_item(format!("pg-pick:empty-fallback\t{n}"), n, *n == current);
                    if i == 0 {
                        it.heading = "代理".into();
                    }
                    items.push(it);
                }
                self.show_choice("空组兜底", items);
            }
            Opt::DefaultSelected => {
                let current = pg::text(&d.def, "default-selected");
                let mut items = vec![check_item(
                    "pg-pick:default-selected\t".into(),
                    "默认",
                    current.is_empty(),
                )];
                for (i, n) in pg::names(&d.def, "proxies").iter().enumerate() {
                    let mut it =
                        check_item(format!("pg-pick:default-selected\t{n}"), n, *n == current);
                    if i == 0 {
                        it.heading = "代理".into();
                    }
                    items.push(it);
                }
                self.show_choice("默认选中", items);
            }
            _ => {
                self.open_input(&key, o.label(), "", &pg::text(&d.def, o.key()));
                if o.is_number() {
                    self.ui(|ui| ui.set_input_number(true));
                }
            }
        }
    }

    pub(crate) fn pg_toggled(self: &Arc<Self>, id: &str, on: bool) {
        if let Some(o) = id.strip_prefix("pg-o:").and_then(Opt::of) {
            self.edit_group_draft(|d| d.set(o.key(), Some(o.toggled(on))));
        }
    }

    pub(crate) fn groups_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match key {
            "pg-type" => {
                let v = value.to_string();
                self.edit_group_draft(move |d| d.set("type", Some(v.into())));
            }
            "pg-add" => {
                if let Some(o) = Opt::of(value) {
                    let url = self.test_url();
                    self.edit_group_draft(move |d| d.add_option(o, &url));
                }
            }
            "pg-pick" => {
                if let Some((k, v)) = value.split_once('\t') {
                    let (k, v) = (k.to_string(), v.to_string());
                    self.edit_group_draft(move |d| d.set(&k, Some(v.into())));
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn groups_check(&self, key: &str, v1: &str) -> Option<String> {
        let target = key.strip_prefix("pg-in:")?;
        let v = v1.trim();
        let number = Opt::of(target).is_some_and(Opt::is_number);
        Some(if number && !v.is_empty() && v.parse::<u32>().is_err() {
            "请输入数字".into()
        } else {
            String::new()
        })
    }

    pub(crate) fn groups_input(self: &Arc<Self>, key: &str, v1: &str) -> bool {
        let Some(target) = key.strip_prefix("pg-in:") else {
            return false;
        };
        let v = v1.trim().to_string();
        if target == "name" {
            self.edit_group_draft(move |d| d.set("name", Some(v.into())));
            return true;
        }
        let Some(o) = Opt::of(target) else {
            return true;
        };
        let value = if v.is_empty() {
            None
        } else if o.is_number() {
            v.parse::<u32>().ok().map(Value::from)
        } else {
            Some(v.into())
        };
        self.edit_group_draft(move |d| d.set(o.key(), value));
        true
    }

    /// `_handleSaveProxyGroup`: the blocking issues and the filters' verdicts, then `put`.
    fn save_group(self: &Arc<Self>) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let Some(d) = section_state().lock().unwrap().draft.clone() else {
            return;
        };
        let data = self.group_data(&pid);
        let mut blocking: Vec<Issue> = pg::issues(d.id, &d.def, &data)
            .into_iter()
            .filter(Issue::blocks_save)
            .collect();
        let mut checked = d.clone();
        if let Some(errors) = self.validate_filters(&d.filters()) {
            checked.filter_errors = errors;
        }
        blocking.extend(checked.filter_issues());
        let errors = checked.filter_errors.clone();
        self.edit_group_draft(move |d| {
            d.submitted = true;
            d.filter_errors = errors;
        });
        if !blocking.is_empty() {
            return self.alert("无法保存", pg::issues_message(&blocking));
        }
        let id = if d.is_new { new_id() } else { d.id };
        let def = d.def.clone();
        let saved = {
            let mut store = self.store.lock().unwrap();
            let ok = pg::put(&mut store.state, &pid, id, def);
            if ok {
                store.save();
            }
            ok
        };
        if !saved {
            return self.alert("无法保存", Issue::DuplicateName(pg::name(&d.def)).message());
        }
        section_state().lock().unwrap().draft = None;
        self.publish_custom();
        self.ui(|ui| ui.set_profiles_page("custom".into()));
    }

    /// `NestedFormSheet._handleExit`: a changed form asks whether to save.
    pub(crate) fn pg_back(self: &Arc<Self>) {
        let changed = section_state()
            .lock()
            .unwrap()
            .draft
            .as_ref()
            .is_some_and(|d| d.def != d.origin);
        if changed {
            self.confirm("提示", "是否保存更改？".into(), "pg-save", String::new());
        } else {
            self.close_group_form();
        }
    }

    pub(crate) fn pg_save_answer(self: &Arc<Self>, yes: bool) {
        if yes {
            self.spawn(|app| app.save_group());
        } else {
            self.close_group_form();
        }
    }

    fn close_group_form(self: &Arc<Self>) {
        section_state().lock().unwrap().draft = None;
        self.ui(|ui| ui.set_profiles_page("custom".into()));
    }

    /// `_handleQuickEdit`: the definition as YAML in the editor.
    fn open_group_yaml(self: &Arc<Self>, d: &Draft) {
        section_state().lock().unwrap().yaml_open = true;
        let name = pg::name(&d.def);
        let title: SharedString = if name.is_empty() {
            "快速编辑".into()
        } else {
            name
        }
        .into();
        let text: SharedString = pg::to_yaml(&d.def).into();
        self.ui(move |ui| {
            ui.set_editor_title(title);
            ui.set_editor_text(text);
            ui.set_editor_readonly(false);
            ui.set_editor_parent("group".into());
            ui.set_profiles_page("editor".into());
        });
    }

    /// The editor's save while quick edit is open: read through the Core's `config.read`,
    /// as there is no YAML parser here.
    pub(crate) fn group_yaml_save(self: &Arc<Self>, text: &str) -> bool {
        if !section_state().lock().unwrap().yaml_open {
            return false;
        }
        let text = text.to_string();
        self.spawn(move |app| {
            let path = app.store.lock().unwrap().dir.join("quick-edit.yaml");
            let parsed = std::fs::write(&path, pg::yaml_document(&text))
                .map_err(|e| e.to_string())
                .and_then(|_| app.bridge())
                .and_then(|b| b.call("config.read", json!({"profile": profile_ref(&path)}), CALL));
            let _ = std::fs::remove_file(&path);
            let old = section_state()
                .lock()
                .unwrap()
                .draft
                .as_ref()
                .map(|d| d.def.clone())
                .unwrap_or_default();
            let group =
                parsed.and_then(|r| pg::from_parsed(r.get("config").unwrap_or(&Value::Null), &old));
            match group {
                Ok(def) => {
                    {
                        let mut st = section_state().lock().unwrap();
                        st.yaml_open = false;
                        if let Some(d) = st.draft.as_mut() {
                            d.shown = shown_in(&def);
                            d.def = def;
                        }
                    }
                    app.publish_group_form();
                    app.check_group_filters();
                    app.ui(|ui| {
                        ui.set_editor_parent("".into());
                        ui.set_profiles_page("group".into());
                    });
                }
                Err(e) => app.alert("无法保存", e),
            }
        });
        true
    }

    pub(crate) fn group_yaml_closed(&self) {
        section_state().lock().unwrap().yaml_open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    fn data() -> Data {
        Data {
            groups: vec![
                (
                    1,
                    json!({"name": "A", "type": "select", "proxies": ["hk", "B"], "icon": "https://x/a.png"}),
                ),
                (
                    2,
                    json!({"name": "B", "type": "url-test", "proxies": ["A"]}),
                ),
            ],
            proxies: ["hk".to_string()].into(),
            proxy_types: [("hk".to_string(), "ss".to_string())].into(),
            ..Default::default()
        }
    }

    #[test]
    fn rows_carry_type_icon_issue_and_selection() {
        let rows = group_rows(&data(), &[2].into());
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (rows[0].name.as_str(), rows[0].kind.as_str()),
            ("A", "Selector")
        );
        assert_eq!(rows[0].icon, "https://x/a.png");
        assert_eq!(rows[1].mark, "B");
        assert!(rows[0].invalid && rows[1].invalid);
        assert!(!rows[0].selected && rows[1].selected);
    }

    #[test]
    fn the_form_lists_general_members_options_and_actions() {
        let data = data();
        let mut d = Draft::open(1, data.groups[0].1.clone(), false);
        let s = form_sections(&d, &data, &[]);
        let ids = |s: &[SetSection], i: usize| -> Vec<String> {
            s[i].rows.iter().map(|r| r.id.to_string()).collect()
        };
        assert_eq!(ids(&s, 0), ["pg-f:name", "pg-f:type", "pg-f:icon"]);
        assert_eq!(ids(&s, 1), ["pg-m:0", "pg-m:1", "pg-members-add"]);
        assert_eq!(s[1].rows.row_data(0).unwrap().subtitle, "ss");
        assert_eq!(s[1].rows.row_data(1).unwrap().subtitle, "url-test");
        assert_eq!(ids(&s, 2), ["pg-add"]);
        assert_eq!(ids(&s, 3), ["pg-delete"]);
        d.add_option(Opt::Interval, "u");
        d.add_option(Opt::Lazy, "u");
        d.add_option(Opt::DefaultSelected, "u");
        let s = form_sections(&d, &data, &[]);
        assert_eq!(
            ids(&s, 2),
            [
                "pg-o:interval",
                "pg-o:lazy",
                "pg-o:default-selected",
                "pg-add"
            ]
        );
        assert_eq!(s[2].rows.row_data(0).unwrap().subtitle, "300 s");
        assert!(s[2].rows.row_data(1).unwrap().on);
        d.set("type", Some("load-balance".into()));
        assert_eq!(d.listed(), [Opt::Interval, Opt::Lazy]);
        assert!(d.addable().contains(&Opt::Strategy));
        d.remove_option(Opt::Interval);
        assert!(d.def.get("interval").is_none());
        assert_eq!(d.listed(), [Opt::Lazy]);
    }

    #[test]
    fn a_new_form_holds_back_incomplete_issues_until_saved() {
        let data = data();
        let mut d = Draft::open(0, json!({"name": "", "type": "select"}), true);
        assert!(d.issues(&data).is_empty());
        assert_eq!(form_sections(&d, &data, &[]).len(), 3);
        d.submitted = true;
        assert_eq!(d.issues(&data), [Issue::EmptyName, Issue::NoProxySource]);
        d.filter_errors.insert(1, "bad".into());
        assert_eq!(
            d.issues(&data).last(),
            Some(&Issue::InvalidFilter("排除筛选".into(), "bad".into()))
        );
    }

    #[test]
    fn filters_show_their_preset_labels() {
        let data = data();
        let d = Draft::open(
            9,
            json!({"name": "F", "type": "select", "filter": "(?i)hk`jp"}),
            false,
        );
        let presets = vec![("(?i)hk".to_string(), "香港".to_string())];
        let s = form_sections(&d, &data, &presets);
        let titles: Vec<String> = s[1].rows.iter().map(|r| r.title.to_string()).collect();
        assert_eq!(titles, ["香港", "jp", "添加节点"]);
    }
}
