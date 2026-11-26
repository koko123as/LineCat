//! The added rules pages on `App`'s side (lib/views/config/rules.dart): the list, selecting
//! to delete, the rule form (lib/features/form/rule.dart) and the presets
//! (lib/features/form/rule_preset.dart).

use crate::rules::{self, Rule, ACTIONS, TARGETS};
use crate::{store, ActionItem, App, PageMeta, SetRow, SetSection};
use slint::{ModelRc, SharedString, VecModel};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// The form's rule, whether it is new, whether a save was tried, and whether its target is
/// a new exit being named.
#[derive(Default)]
struct Draft {
    rule: Rule,
    is_new: bool,
    attempted: bool,
    new_exit: bool,
}

#[derive(Default)]
struct RulesUi {
    draft: Draft,
    selected: BTreeSet<u64>,
    presets: BTreeSet<usize>,
    /// The profile whose own added rules the pages edit, None for the app's.
    scope: Option<String>,
    /// The pages show inside a profile's 扩展 rather than under 进阶配置.
    hosted: bool,
}

pub const PAGES: [&str; 4] = ["rules", "rules-select", "rule-form", "rule-presets"];

pub fn scope() -> Option<String> {
    state().lock().unwrap().scope.clone()
}

pub fn selected() -> BTreeSet<u64> {
    state().lock().unwrap().selected.clone()
}

fn state() -> &'static Mutex<RulesUi> {
    static S: std::sync::OnceLock<Mutex<RulesUi>> = std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

/// `RulePreset`: (label, rules).
pub const PRESETS: [(&str, &[&str]); 7] = [
    (
        "屏蔽 QUIC",
        &["AND,((NETWORK,UDP),(DST-PORT,443)),REJECT-DROP"],
    ),
    (
        "屏蔽 STUN",
        &["AND,((NETWORK,UDP),(DST-PORT,3478/19302)),REJECT-DROP"],
    ),
    ("屏蔽 DNS over TLS", &["DST-PORT,853,REJECT"]),
    (
        "防止代理回环",
        &[
            "IP-CIDR,127.0.0.0/8,REJECT,no-resolve",
            "IP-CIDR,0.0.0.0/32,REJECT,no-resolve",
            "IP-CIDR6,::1/128,REJECT,no-resolve",
            "IP-CIDR6,::/128,REJECT,no-resolve",
            "AND,((IN-TYPE,TUN),(DST-PORT,853)),REJECT",
        ],
    ),
    (
        "局域网直连",
        &["GEOSITE,private,DIRECT", "GEOIP,LAN,DIRECT,no-resolve"],
    ),
    (
        "Apple 与 Microsoft 直连",
        &["GEOSITE,apple,DIRECT", "GEOSITE,microsoft,DIRECT"],
    ),
    (
        "BT 下载直连",
        &[
            "GEOSITE,category-public-tracker,DIRECT",
            "GEOSITE,category-pt,DIRECT",
            r"PROCESS-NAME-REGEX,(?i)^(qbittorrent|transmission.*|deluge.*|aria2c|motrix|utorrent|bitcomet)(\.exe)?$,DIRECT",
        ],
    ),
];

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

fn section(title: &str, footer: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: footer.into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

fn new_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(1)
}

/// What the page shell shows for the rules pages.
pub fn page_meta(settings: &store::Settings, page: &str) -> Option<PageMeta> {
    let meta = |title: &str, parent: &str| PageMeta {
        known: true,
        title: title.into(),
        parent: parent.into(),
        ..Default::default()
    };
    let st = state().lock().unwrap();
    Some(match page {
        "rules" => PageMeta {
            action: "add-circle".into(),
            action2: "bolt".into(),
            empty: "暂无规则".into(),
            long_press: true,
            ..meta("附加规则", "advanced")
        },
        "rules-select" => PageMeta {
            action: "delete".into(),
            action2: "select-all".into(),
            ..meta("附加规则", "rules")
        },
        "rule-form" => PageMeta {
            action: "check".into(),
            ..meta(
                if st.draft.is_new {
                    "添加规则"
                } else {
                    "编辑规则"
                },
                "rules",
            )
        },
        "rule-presets" => PageMeta {
            action: if st.presets.is_empty() { "" } else { "check" }.into(),
            ..meta("快捷操作", "rules")
        },
        _ => {
            let _ = settings;
            return None;
        }
    })
}

/// The sections of a rules page.
pub fn sections(settings: &store::Settings, page: &str) -> Option<Vec<SetSection>> {
    let st = state().lock().unwrap();
    let list = &settings.added_rules;
    Some(match page {
        "rules" | "rules-select" => {
            if list.is_empty() {
                return Some(Vec::new());
            }
            let selecting = page == "rules-select";
            let rows = list
                .iter()
                .map(|r| SetRow {
                    on: st.selected.contains(&r.id),
                    ..row(
                        &format!("rule:{}", r.id),
                        if selecting { "check" } else { "action" },
                        &r.action,
                        &rule_body(r),
                    )
                })
                .collect();
            vec![section("", "", rows)]
        }
        "rule-form" => form_sections(&st.draft),
        "rule-presets" => {
            let rows = PRESETS
                .iter()
                .enumerate()
                .map(|(i, (label, lines))| SetRow {
                    on: st.presets.contains(&i),
                    ..row(&format!("preset:{i}"), "check", label, lines[0])
                })
                .collect();
            vec![section("", "", rows)]
        }
        _ => return None,
    })
}

/// A rule's row subtitle: what follows its type.
fn rule_body(r: &Rule) -> String {
    let line = r.line();
    line.strip_prefix(&format!("{},", r.action))
        .unwrap_or(&line)
        .to_string()
}

fn form_sections(d: &Draft) -> Vec<SetSection> {
    let r = &d.rule;
    let target = if d.new_exit {
        "自定义出口"
    } else {
        &r.target
    };
    let mut basic = vec![
        row("type", "value", "代理类型", &r.action),
        row(
            "content",
            "value",
            "内容",
            if r.content.is_empty() {
                "输入规则内容"
            } else {
                &r.content
            },
        ),
        row("target", "value", "分流策略", target),
    ];
    if d.new_exit {
        basic.push(row(
            "exit",
            "value",
            "名称",
            if r.target.is_empty() {
                "输入出口名称"
            } else {
                &r.target
            },
        ));
    }
    let issue = if d.attempted {
        r.issue().unwrap_or_default()
    } else {
        String::new()
    };
    let mut out = vec![section("基础信息", "", basic)];
    if rules::has_params(&r.action) {
        out.push(section(
            "附加参数",
            "",
            vec![
                SetRow {
                    on: r.no_resolve,
                    ..row("no-resolve", "toggle", "不解析主机名", "")
                },
                SetRow {
                    on: r.src,
                    ..row("src", "toggle", "匹配来源IP", "")
                },
            ],
        ));
    }
    if !issue.is_empty() {
        out.push(section("", &issue, Vec::new()));
    }
    out
}

impl App {
    fn bump_meta(self: &Arc<Self>) {
        self.ui(|ui| ui.set_meta_rev(ui.get_meta_rev() + 1));
    }

    pub(crate) fn host_rules(&self, scope: Option<String>) {
        let mut st = state().lock().unwrap();
        st.hosted = true;
        st.scope = scope;
    }

    fn rules_refresh(self: &Arc<Self>) {
        self.publish_settings();
        self.publish_ext();
    }

    fn scoped_rules(&self) -> Vec<Rule> {
        let scope = scope();
        let store = self.store.lock().unwrap();
        match scope {
            Some(pid) => crate::extend::own_rules(&store.state, &pid),
            None => store.state.settings.added_rules.clone(),
        }
    }

    fn open_rules_page(self: &Arc<Self>, page: &str) {
        if state().lock().unwrap().hosted {
            self.ext_rules_page(page);
            return;
        }
        let page: SharedString = page.into();
        self.ui(move |ui| {
            ui.set_tools_page(page);
            ui.set_meta_rev(ui.get_meta_rev() + 1);
        });
    }

    /// A profile's own rules wait for 立即应用 or leaving 扩展; the app's apply now.
    fn update_rules(self: &Arc<Self>, f: impl FnOnce(&mut Vec<Rule>)) {
        let scope = scope();
        {
            let mut store = self.store.lock().unwrap();
            match &scope {
                Some(pid) => {
                    let mut list = crate::extend::own_rules(&store.state, pid);
                    f(&mut list);
                    crate::extend::set_own_rules(&mut store.state, pid, &list);
                }
                None => f(&mut store.state.settings.added_rules),
            }
            store.save();
        }
        self.rules_refresh();
        if scope.is_none() {
            self.reapply();
        }
    }

    fn open_form(self: &Arc<Self>, rule: Option<Rule>) {
        {
            let mut st = state().lock().unwrap();
            st.draft = match rule {
                Some(rule) => Draft {
                    rule,
                    ..Default::default()
                },
                None => Draft {
                    rule: Rule::new(new_id()),
                    is_new: true,
                    ..Default::default()
                },
            };
        }
        self.open_rules_page("rule-form");
    }

    /// A tap on a rules page, its bar or the dialogs it opens.
    pub(crate) fn rules_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        let settings = self.store.lock().unwrap().state.settings.clone();
        let list = self.scoped_rules();
        match (page, id) {
            ("rules", "action") => self.open_form(None),
            ("rules", "action2") => {
                state().lock().unwrap().presets.clear();
                self.open_rules_page("rule-presets");
            }
            ("rules", id) => {
                if let Some(rid) = id
                    .strip_prefix("long:rule:")
                    .and_then(|i| i.parse::<u64>().ok())
                {
                    let mut st = state().lock().unwrap();
                    st.selected = BTreeSet::from([rid]);
                    drop(st);
                    self.open_rules_page("rules-select");
                } else if let Some(rid) =
                    id.strip_prefix("rule:").and_then(|i| i.parse::<u64>().ok())
                {
                    let rule = list.iter().find(|r| r.id == rid).cloned();
                    if rule.is_some() {
                        self.open_form(rule);
                    }
                }
            }
            ("rules-select", "action") => {
                let n = state().lock().unwrap().selected.len();
                if n > 0 {
                    self.confirm(
                        "提示",
                        "确定删除选中的规则吗？".into(),
                        "rules-delete",
                        String::new(),
                    );
                }
            }
            ("rules-select", "action2") => {
                let all: BTreeSet<u64> = list.iter().map(|r| r.id).collect();
                {
                    let mut st = state().lock().unwrap();
                    st.selected = if st.selected.is_superset(&all) {
                        BTreeSet::new()
                    } else {
                        all
                    };
                }
                self.after_select();
            }
            ("rules-select", id) => {
                if let Some(rid) = id
                    .strip_prefix("rule:")
                    .or_else(|| id.strip_prefix("long:rule:"))
                    .and_then(|i| i.parse::<u64>().ok())
                {
                    {
                        let mut st = state().lock().unwrap();
                        if !st.selected.remove(&rid) {
                            st.selected.insert(rid);
                        }
                    }
                    self.after_select();
                }
            }
            ("rule-presets", "action") => {
                let picked: Vec<usize> = state().lock().unwrap().presets.iter().copied().collect();
                let mut id = new_id();
                let added: Vec<Rule> = picked
                    .iter()
                    .flat_map(|i| PRESETS[*i].1.iter())
                    .map(|line| {
                        id += 1;
                        rules::parse(line, id)
                    })
                    .collect();
                let front = scope().is_some();
                self.update_rules(move |list| {
                    if front {
                        list.splice(0..0, added);
                    } else {
                        list.extend(added);
                    }
                });
                self.open_rules_page("rules");
            }
            ("rule-presets", id) => {
                if let Some(i) = id
                    .strip_prefix("preset:")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    {
                        let mut st = state().lock().unwrap();
                        if !st.presets.remove(&i) {
                            st.presets.insert(i);
                        }
                    }
                    self.rules_refresh();
                    self.bump_meta();
                }
            }
            ("rule-form", "action") => self.save_form(),
            ("rule-form", "content") => {
                let v = state().lock().unwrap().draft.rule.content.clone();
                self.open_input("rule-content", "内容", "", &v);
            }
            ("rule-form", "exit") => {
                let v = state().lock().unwrap().draft.rule.target.clone();
                self.open_input("rule-exit", "名称", "", &v);
            }
            ("rule-form", key @ ("target" | "type")) => self.form_choice(key, &settings),
            _ => return false,
        }
        true
    }

    fn after_select(self: &Arc<Self>) {
        if state().lock().unwrap().selected.is_empty() {
            self.open_rules_page("rules");
        } else {
            self.rules_refresh();
            self.bump_meta();
        }
    }

    /// `rules-delete` confirmed.
    pub(crate) fn delete_selected_rules(self: &Arc<Self>) {
        let selected = std::mem::take(&mut state().lock().unwrap().selected);
        self.update_rules(move |list| list.retain(|r| !selected.contains(&r.id)));
        self.open_rules_page("rules");
    }

    fn form_choice(self: &Arc<Self>, key: &str, settings: &store::Settings) {
        let (title, items): (&str, Vec<ActionItem>) = {
            let st = state().lock().unwrap();
            let d = &st.draft;
            let item = |id: String, label: &str, checked: bool, heading: &str| ActionItem {
                id: id.into(),
                label: label.into(),
                heading: heading.into(),
                check: true,
                checked,
                enabled: true,
                ..Default::default()
            };
            if key == "type" {
                (
                    "代理类型",
                    ACTIONS
                        .iter()
                        .map(|a| item(format!("rule-action:{a}"), a, d.rule.action == *a, ""))
                        .collect(),
                )
            } else {
                let mut items: Vec<ActionItem> = TARGETS
                    .iter()
                    .map(|t| {
                        let on = !d.new_exit && !d.rule.custom_exit && d.rule.target == *t;
                        item(format!("rule-target:{t}"), t, on, "")
                    })
                    .collect();
                let mut exits = match st.scope.clone() {
                    Some(pid) => {
                        let store = self.store.lock().unwrap();
                        let mut all = crate::extend::own_rules(&store.state, &pid);
                        all.extend(settings.added_rules.iter().cloned());
                        rules::custom_exits(&all)
                    }
                    None => rules::custom_exits(&settings.added_rules),
                };
                if d.rule.custom_exit && !d.new_exit && !exits.contains(&d.rule.target) {
                    exits.push(d.rule.target.clone());
                }
                for (i, e) in exits.iter().enumerate() {
                    let on = !d.new_exit && d.rule.custom_exit && d.rule.target == *e;
                    let mut it = item(format!("rule-exit:{e}"), e, on, "");
                    it.section = i == 0;
                    items.push(it);
                }
                let mut new = item("rule-new-exit:".into(), "自定义出口", d.new_exit, "");
                new.section = true;
                items.push(new);
                ("分流策略", items)
            }
        };
        let title: SharedString = title.into();
        self.ui(move |ui| {
            ui.set_choice_title(title);
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    /// A pick from the form's choices; false when it is not one of them.
    pub(crate) fn rules_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        {
            let mut st = state().lock().unwrap();
            let d = &mut st.draft;
            match key {
                "rule-action" => d.rule.action = value.into(),
                "rule-target" => {
                    d.rule.target = value.into();
                    d.rule.custom_exit = false;
                    d.new_exit = false;
                }
                "rule-exit" => {
                    d.rule.target = value.into();
                    d.rule.custom_exit = true;
                    d.new_exit = false;
                }
                "rule-new-exit" => {
                    if !d.new_exit {
                        d.rule.target.clear();
                    }
                    d.rule.custom_exit = true;
                    d.new_exit = true;
                }
                _ => return false,
            }
        }
        self.rules_refresh();
        if key == "rule-new-exit" {
            self.open_input("rule-exit", "名称", "", "");
        }
        true
    }

    /// The form's text inputs; false when the key is not one of them.
    pub(crate) fn rules_input(self: &Arc<Self>, key: &str, value: &str) -> bool {
        {
            let mut st = state().lock().unwrap();
            match key {
                "rule-content" => st.draft.rule.content = value.into(),
                "rule-exit" => st.draft.rule.target = value.replace(',', "").trim().into(),
                _ => return false,
            }
        }
        self.rules_refresh();
        true
    }

    pub(crate) fn rules_toggled(self: &Arc<Self>, page: &str, id: &str, on: bool) -> bool {
        if page != "rule-form" {
            return false;
        }
        {
            let mut st = state().lock().unwrap();
            match id {
                "no-resolve" => st.draft.rule.no_resolve = on,
                "src" => st.draft.rule.src = on,
                _ => return false,
            }
        }
        self.rules_refresh();
        true
    }

    fn save_form(self: &Arc<Self>) {
        let rule = {
            let mut st = state().lock().unwrap();
            if st.draft.rule.issue().is_some() {
                st.draft.attempted = true;
                None
            } else {
                Some(st.draft.rule.clone())
            }
        };
        let Some(rule) = rule else {
            self.rules_refresh();
            return;
        };
        let front = scope().is_some();
        self.update_rules(
            move |list| match list.iter_mut().find(|r| r.id == rule.id) {
                Some(r) => *r = rule,
                None if front => list.insert(0, rule),
                None => list.push(rule),
            },
        );
        self.open_rules_page("rules");
    }

    /// Leaving the selection by back clears it.
    pub(crate) fn rules_page_shown(&self, page: &str) {
        if page == "rules" {
            let mut st = state().lock().unwrap();
            st.selected.clear();
            st.hosted = false;
            st.scope = None;
        }
    }

    pub(crate) fn rules_unhost(&self, clear: bool) {
        let mut st = state().lock().unwrap();
        if clear {
            st.selected.clear();
        }
        st.hosted = false;
        st.scope = None;
    }

    pub(crate) fn clear_rules_selection(&self) {
        state().lock().unwrap().selected.clear();
    }
}
