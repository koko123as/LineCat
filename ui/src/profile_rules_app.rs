//! A custom profile's 规则 on `App`'s side (FlClash: lib/views/profiles/custom/rules.dart): the list
//! with search, selection, sort mode and menus (`ListEditorPage`), the presets
//! (`RulePresetSheet`), quick edit, and the form (`_AddOrEditRuleView`).

use crate::custom_profile::{self as cp, ProfileRule};
use crate::profile_rules::{self as pr, Issue, Targets};
use crate::rules_app::PRESETS;
use crate::{ActionItem, Anchor, App, AppWindow, RuleRow, SetRow, SetSection};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, OnceLock};

/// The form as edited and as it opened (`NestedFormSheet` compares them on the way out).
#[derive(Clone, Debug)]
pub struct Draft {
    pub rule: ProfileRule,
    pub origin: ProfileRule,
    pub is_new: bool,
}

impl Draft {
    pub fn open(rule: ProfileRule, is_new: bool) -> Draft {
        Draft {
            origin: rule.clone(),
            rule,
            is_new,
        }
    }
}

#[derive(Default)]
struct Page {
    selected: BTreeSet<u64>,
    sorting: bool,
    query: String,
    draft: Option<Draft>,
    /// `RulePresetSheet` while open: the presets picked.
    presets: Option<BTreeSet<usize>>,
    yaml: Option<String>,
}

fn page() -> &'static Mutex<Page> {
    static S: OnceLock<Mutex<Page>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn new_id() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_default()
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

fn item(id: String, label: &str, subtitle: &str, checked: bool) -> ActionItem {
    ActionItem {
        id: id.into(),
        label: label.into(),
        subtitle: subtitle.into(),
        check: true,
        checked,
        enabled: true,
        ..Default::default()
    }
}

/// The list's rows (`RuleItem` with `invalidMessageOf`).
pub fn rule_rows(rules: &[&ProfileRule], t: &Targets, selected: &BTreeSet<u64>) -> Vec<RuleRow> {
    rules
        .iter()
        .map(|r| {
            let (tone, invalid) = pr::tone(r, &pr::issues(r, t));
            RuleRow {
                id: r.id.to_string().into(),
                name: pr::name(&r.action).into(),
                content: pr::real_content(r).into(),
                target: pr::real_target(r).into(),
                tone,
                invalid,
                selected: selected.contains(&r.id),
            }
        })
        .collect()
}

/// The marks the form's rows carry: payload, rule set and target, each with its message.
fn form_marks(r: &ProfileRule, t: &Targets) -> (bool, bool, bool, Vec<Issue>) {
    let payload = pr::payload_error(r);
    let set_invalid = r.action == "RULE-SET"
        && !r.rule_provider.is_empty()
        && !t.rule_sets.contains(&r.rule_provider);
    let target = pr::real_target(r);
    let target_invalid = !t.is_line_target(target);
    let mut shown = Vec::new();
    if let Some(e) = payload {
        shown.push(Issue::InvalidPayload(e));
    }
    if set_invalid {
        shown.push(Issue::MissingRuleSet(r.rule_provider.clone()));
    }
    if target_invalid && !target.is_empty() {
        shown.push(Issue::MissingTarget(target.into()));
    }
    (payload.is_some(), set_invalid, target_invalid, shown)
}

/// `_AddOrEditRuleView`'s sections and the messages its marks carry.
pub fn form_sections(d: &Draft, t: &Targets) -> (Vec<SetSection>, String) {
    let r = &d.rule;
    let (payload_bad, set_bad, target_bad, shown) = form_marks(r, t);
    let mut basic = vec![row("pr-f:type", "open", "代理类型", &pr::name(&r.action))];
    if r.action == "RULE-SET" {
        let mut set = row(
            "pr-f:rule-set",
            "open",
            "规则集",
            if r.rule_provider.is_empty() {
                "请选择规则集"
            } else {
                &r.rule_provider
            },
        );
        set.invalid = set_bad;
        basic.push(set);
    } else if r.action != "MATCH" {
        let mut content = row(
            "pr-f:content",
            "value",
            "内容",
            if r.content.is_empty() {
                "输入规则内容"
            } else {
                &r.content
            },
        );
        content.invalid = payload_bad;
        basic.push(content);
    }
    let target = pr::real_target(r);
    let mut target_row = row(
        "pr-f:target",
        "open",
        "分流策略",
        if target.is_empty() {
            "请选择分流策略"
        } else {
            target
        },
    );
    target_row.invalid = target_bad;
    basic.push(target_row);
    let mut out = vec![section("基础信息", basic)];
    if pr::has_params(&r.action) {
        out.push(section(
            "附加参数",
            vec![
                SetRow {
                    on: r.no_resolve || r.src,
                    enabled: !r.src,
                    ..row("pr-no-resolve", "toggle", "不解析主机名", "")
                },
                SetRow {
                    on: r.src,
                    ..row("pr-src", "toggle", "匹配来源IP", "")
                },
            ],
        ));
    }
    if !d.is_new {
        out.push(section(
            "操作",
            vec![row("pr-delete", "danger", "删除", "")],
        ));
    }
    (out, pr::issues_message(&shown))
}

/// `RulePresetSheet`'s rows: each preset with its rules, picked ones checked.
pub fn preset_sections(picked: &BTreeSet<usize>) -> Vec<SetSection> {
    let rows = PRESETS
        .iter()
        .enumerate()
        .map(|(i, (label, lines))| SetRow {
            on: picked.contains(&i),
            ..row(&format!("preset:{i}"), "check", label, &lines.join("  "))
        })
        .collect();
    vec![section("", rows)]
}

impl App {
    pub(crate) fn bind_profile_rules(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_pr_act(move |action| a.pr_act(&action));
        let a = self.clone();
        ui.on_pr_form(move |id| a.pr_form(&id));
        let a = self.clone();
        ui.on_pr_form_toggled(move |id, on| a.pr_form_toggled(&id, on));
    }

    fn rule_targets(&self, pid: &str) -> Targets {
        pr::targets(&self.store.lock().unwrap().state, pid)
    }

    /// The profile's rule issue count, for the page's banner.
    pub(crate) fn profile_rule_issues(&self, pid: &str) -> usize {
        pr::profile_issues(&self.store.lock().unwrap().state, pid).len()
    }

    pub(crate) fn reset_rules_page(&self) {
        *page().lock().unwrap() = Page::default();
    }

    fn publish_rules(self: &Arc<Self>) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let (rows, titles, subtitles, total, selecting, sorting) = {
            let store = self.store.lock().unwrap();
            let all = cp::rules(&store.state, &pid);
            let t = pr::targets(&store.state, &pid);
            let mut st = page().lock().unwrap();
            let ids: BTreeSet<u64> = all.iter().map(|r| r.id).collect();
            st.selected = st.selected.intersection(&ids).copied().collect();
            if all.len() < 2 {
                st.sorting = false;
            }
            let shown: Vec<&ProfileRule> = all
                .iter()
                .copied()
                .filter(|r| pr::matches(r, &st.query))
                .collect();
            let rows = rule_rows(&shown, &t, &st.selected);
            let titles: Vec<SharedString> = if st.sorting {
                all.iter().map(|r| pr::name(&r.action).into()).collect()
            } else {
                Vec::new()
            };
            let subtitles: Vec<SharedString> = if st.sorting {
                all.iter()
                    .map(|r| {
                        let raw = r.raw();
                        raw.split_once(',')
                            .map_or("", |(_, b)| b)
                            .to_string()
                            .into()
                    })
                    .collect()
            } else {
                Vec::new()
            };
            (
                rows,
                titles,
                subtitles,
                all.len() as i32,
                !st.selected.is_empty(),
                st.sorting,
            )
        };
        self.ui(move |ui| {
            ui.set_pr_rows(ModelRc::new(VecModel::from(rows)));
            ui.set_pr_sort_titles(ModelRc::new(VecModel::from(titles)));
            ui.set_pr_sort_subtitles(ModelRc::new(VecModel::from(subtitles)));
            ui.set_pr_total(total);
            ui.set_pr_selecting(selecting);
            ui.set_pr_sorting(sorting);
        });
    }

    fn edit_rules(self: &Arc<Self>, f: impl FnOnce(&mut crate::store::State)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state);
            store.save();
        }
        self.publish_rules();
        self.publish_custom();
    }

    fn show_rules_page(self: &Arc<Self>, name: &'static str) {
        self.ui(move |ui| ui.set_profiles_page(name.into()));
    }

    /// The list's bar and rows: `open`, `back`, `search:<q>`, `sort`, `sort-done`,
    /// `select:<id>`, `select-all`, `delete`, `open:<id>`, `long:<id>`, `add`,
    /// `move:<from>:<to>` and `yaml-closed`.
    pub(crate) fn pr_act(self: &Arc<Self>, action: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let (verb, arg) = action.split_once(':').unwrap_or((action, ""));
        let id = arg.parse::<u64>().ok();
        match verb {
            "open" if arg.is_empty() => {
                self.reset_rules_page();
                self.publish_rules();
                self.ui(|ui| {
                    ui.set_pr_query("".into());
                    ui.set_pr_searching(false);
                    ui.set_profiles_page("rules".into());
                });
            }
            "back" => {
                let left = {
                    let mut st = page().lock().unwrap();
                    if !st.selected.is_empty() || st.sorting {
                        st.selected.clear();
                        st.sorting = false;
                        false
                    } else {
                        true
                    }
                };
                if left {
                    self.reset_rules_page();
                    self.publish_custom();
                    self.show_rules_page("custom");
                } else {
                    self.publish_rules();
                }
            }
            "search" => {
                page().lock().unwrap().query = arg.to_string();
                self.publish_rules();
            }
            "sort" | "sort-done" => {
                page().lock().unwrap().sorting = verb == "sort";
                self.publish_rules();
            }
            "select" => {
                if let Some(id) = id {
                    let mut st = page().lock().unwrap();
                    if !st.selected.remove(&id) {
                        st.selected.insert(id);
                    }
                }
                self.publish_rules();
            }
            "select-all" => {
                let visible = self.visible_rule_ids(&pid);
                {
                    let mut st = page().lock().unwrap();
                    st.selected = if st.selected.is_superset(&visible) {
                        BTreeSet::new()
                    } else {
                        visible
                    };
                }
                self.publish_rules();
            }
            "delete" => {
                let visible = self.visible_rule_ids(&pid);
                let ids: Vec<String> = page()
                    .lock()
                    .unwrap()
                    .selected
                    .intersection(&visible)
                    .map(u64::to_string)
                    .collect();
                if !ids.is_empty() {
                    self.confirm(
                        "提示",
                        "确定删除选中的规则吗？".into(),
                        "pr-delete",
                        ids.join(","),
                    );
                }
            }
            "open" => {
                let rule = id.and_then(|id| {
                    cp::rules(&self.store.lock().unwrap().state, &pid)
                        .into_iter()
                        .find(|r| r.id == id)
                        .cloned()
                });
                if let Some(rule) = rule {
                    self.open_rule_form(Draft::open(rule, false));
                }
            }
            "long" => {
                if let Some(id) = id {
                    self.rules_menu(
                        "pr",
                        id.to_string(),
                        &[
                            ("copy", "copy", "复制", false),
                            ("delete", "delete", "删除", true),
                        ],
                    );
                }
            }
            "add" => self.rules_menu(
                "pr-add",
                String::new(),
                &[
                    ("add", "plus", "添加", false),
                    ("presets", "bolt", "快捷操作", false),
                    ("quick-edit", "compose", "快速编辑", false),
                ],
            ),
            "move" => {
                let mut it = arg.split(':').filter_map(|n| n.parse::<usize>().ok());
                if let (Some(from), Some(to)) = (it.next(), it.next()) {
                    self.edit_rules(|s| pr::reorder(s, &pid, from, to));
                }
            }
            "yaml-closed" => {
                page().lock().unwrap().yaml = None;
                self.show_rules_page("rules");
            }
            _ => {}
        }
    }

    fn visible_rule_ids(&self, pid: &str) -> BTreeSet<u64> {
        let query = page().lock().unwrap().query.clone();
        cp::rules(&self.store.lock().unwrap().state, pid)
            .into_iter()
            .filter(|r| pr::matches(r, &query))
            .map(|r| r.id)
            .collect()
    }

    fn rules_menu(
        self: &Arc<Self>,
        kind: &'static str,
        arg: String,
        entries: &[(&str, &str, &str, bool)],
    ) {
        let items: Vec<ActionItem> = entries
            .iter()
            .map(|(id, glyph, label, danger)| ActionItem {
                id: (*id).into(),
                glyph: (*glyph).into(),
                label: (*label).into(),
                danger: *danger,
                enabled: true,
                ..Default::default()
            })
            .collect();
        self.ui(move |ui| {
            let anchor = ui.global::<Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(arg.into());
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind(kind.into());
        });
    }

    /// The row menu (`pr`) and the add menu (`pr-add`).
    pub(crate) fn rules_menu_picked(self: &Arc<Self>, kind: &str, arg: &str, action: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        match (kind, action) {
            ("pr", "copy") => {
                if let Ok(id) = arg.parse::<u64>() {
                    self.edit_rules(|s| {
                        let mut next = new_id();
                        pr::duplicate(s, &pid, &[id].into(), || {
                            next += 1;
                            next
                        })
                    });
                }
            }
            ("pr", "delete") => self.confirm(
                "提示",
                "确定删除当前规则吗？".into(),
                "pr-delete",
                arg.into(),
            ),
            ("pr-add", "add") => {
                self.open_rule_form(Draft::open(pr::new_rule(0, &pid), true));
            }
            ("pr-add", "presets") => {
                page().lock().unwrap().presets = Some(BTreeSet::new());
                self.publish_rule_form();
                self.show_rules_page("rule-presets");
            }
            ("pr-add", "quick-edit") => self.open_rules_yaml(&pid),
            _ => {}
        }
    }

    /// `pr-delete` confirmed: the rules in `arg`, and the form when it shows one of them.
    pub(crate) fn delete_profile_rules(self: &Arc<Self>, arg: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let ids: BTreeSet<u64> = arg.split(',').filter_map(|s| s.parse().ok()).collect();
        let closing = {
            let mut st = page().lock().unwrap();
            st.selected.retain(|id| !ids.contains(id));
            let closing = st.draft.as_ref().is_some_and(|d| ids.contains(&d.rule.id));
            if closing {
                st.draft = None;
            }
            closing
        };
        self.edit_rules(|s| pr::delete(s, &pid, &ids));
        if closing {
            self.show_rules_page("rules");
        }
    }

    fn open_rule_form(self: &Arc<Self>, d: Draft) {
        {
            let mut st = page().lock().unwrap();
            st.draft = Some(d);
            st.presets = None;
        }
        self.publish_rule_form();
        self.show_rules_page("rule");
    }

    fn publish_rule_form(self: &Arc<Self>) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let t = self.rule_targets(&pid);
        let (presets, draft) = {
            let st = page().lock().unwrap();
            (st.presets.clone(), st.draft.clone())
        };
        if presets.is_none() && draft.is_none() {
            return;
        }
        self.ui(move |ui| {
            let (title, sections, issue, action) = match (&presets, &draft) {
                (Some(picked), _) => (
                    "快捷操作",
                    preset_sections(picked),
                    String::new(),
                    if picked.is_empty() { "" } else { "check" },
                ),
                (None, Some(d)) => {
                    let (sections, issue) = form_sections(d, &t);
                    (
                        if d.is_new {
                            "添加规则"
                        } else {
                            "编辑规则"
                        },
                        sections,
                        issue,
                        "check",
                    )
                }
                (None, None) => return,
            };
            ui.set_pr_form_title(title.into());
            ui.set_pr_form_sections(ModelRc::new(VecModel::from(sections)));
            ui.set_pr_form_issue(issue.into());
            ui.set_pr_form_action(action.into());
        });
    }

    fn edit_rule_draft(self: &Arc<Self>, f: impl FnOnce(&mut ProfileRule)) {
        {
            let mut st = page().lock().unwrap();
            let Some(d) = st.draft.as_mut() else {
                return;
            };
            f(&mut d.rule);
        }
        self.publish_rule_form();
    }

    /// A tap on the form or the presets: the bar's ✓ (`action`), `back` and the rows.
    pub(crate) fn pr_form(self: &Arc<Self>, id: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        if page().lock().unwrap().presets.is_some() {
            return self.presets_clicked(&pid, id);
        }
        let Some(d) = page().lock().unwrap().draft.clone() else {
            return;
        };
        match id {
            "back" => {
                if d.rule != d.origin {
                    self.confirm("提示", "是否保存更改？".into(), "pr-save", String::new());
                } else {
                    self.close_rule_form();
                }
            }
            "action" => self.save_rule(),
            "pr-f:type" => {
                let items = pr::ACTIONS
                    .iter()
                    .filter(|(a, _)| *a != "SUB-RULE")
                    .map(|(a, desc)| {
                        item(
                            format!("pr-type:{a}"),
                            &pr::name(a),
                            desc,
                            *a == d.rule.action,
                        )
                    })
                    .collect();
                self.show_choice("代理类型", items);
            }
            "pr-f:content" => self.open_input("pr-in:content", "内容", "", &d.rule.content),
            "pr-f:rule-set" => {
                let t = self.rule_targets(&pid);
                let mut items: Vec<ActionItem> = t
                    .rule_sets
                    .iter()
                    .map(|s| item(format!("pr-set:{s}"), s, "", *s == d.rule.rule_provider))
                    .collect();
                if items.is_empty() {
                    items.push(ActionItem {
                        id: "pr-set-none:".into(),
                        label: "暂无规则集".into(),
                        ..Default::default()
                    });
                }
                self.show_choice("规则集", items);
            }
            "pr-f:target" => self.show_rule_targets(&pid, &d),
            "pr-delete" => self.confirm(
                "提示",
                "确定删除当前规则吗？".into(),
                "pr-delete",
                d.rule.id.to_string(),
            ),
            _ => {}
        }
    }

    /// `_handleSelectedTarget`: 基础策略 (those no group takes), 规则目标 (the groups with
    /// their type) and 本地代理 (the custom proxies some group names, with their type).
    fn show_rule_targets(self: &Arc<Self>, pid: &str, d: &Draft) {
        let data = crate::proxy_groups::data(&self.store.lock().unwrap().state, pid);
        let current = pr::real_target(&d.rule).to_string();
        let groups: Vec<(String, String)> = data
            .groups
            .iter()
            .map(|(_, g)| {
                (
                    crate::proxy_groups::name(g),
                    crate::proxy_groups::type_name(&crate::proxy_groups::kind(g)),
                )
            })
            .collect();
        let named = data.named_proxies();
        let mut items: Vec<ActionItem> = Vec::new();
        let mut push = |heading: &str, list: Vec<(String, String)>| {
            for (i, (n, sub)) in list.into_iter().enumerate() {
                let mut it = item(format!("pr-target:{n}"), &n, &sub, n == current);
                if i == 0 {
                    it.heading = heading.into();
                }
                items.push(it);
            }
        };
        push(
            "基础策略",
            pr::TARGETS
                .iter()
                .filter(|t| !groups.iter().any(|(n, _)| n == *t))
                .map(|t| (t.to_string(), String::new()))
                .collect(),
        );
        push("规则目标", groups);
        push(
            "本地代理",
            data.proxy_types
                .iter()
                .filter(|(n, _)| named.contains(*n))
                .map(|(n, k)| (n.clone(), k.clone()))
                .collect(),
        );
        self.show_choice("分流策略", items);
    }

    fn presets_clicked(self: &Arc<Self>, pid: &str, id: &str) {
        match id {
            "back" => {
                page().lock().unwrap().presets = None;
                self.show_rules_page("rules");
            }
            "action" => {
                let picked = page().lock().unwrap().presets.take().unwrap_or_default();
                if picked.is_empty() {
                    return;
                }
                let mut next = new_id();
                let rules: Vec<ProfileRule> = picked
                    .iter()
                    .flat_map(|i| PRESETS[*i].1.iter())
                    .map(|line| {
                        next += 1;
                        pr::parse(line, next, pid)
                    })
                    .collect();
                let pid = pid.to_string();
                self.edit_rules(move |s| pr::put_all(s, &pid, rules));
                self.show_rules_page("rules");
            }
            id => {
                if let Some(i) = id
                    .strip_prefix("preset:")
                    .and_then(|i| i.parse::<usize>().ok())
                {
                    if let Some(picked) = page().lock().unwrap().presets.as_mut() {
                        if !picked.remove(&i) {
                            picked.insert(i);
                        }
                    }
                    self.publish_rule_form();
                }
            }
        }
    }

    pub(crate) fn pr_form_toggled(self: &Arc<Self>, id: &str, on: bool) {
        match id {
            "pr-no-resolve" => self.edit_rule_draft(|r| r.no_resolve = on),
            "pr-src" => self.edit_rule_draft(|r| r.src = on),
            _ => {}
        }
    }

    pub(crate) fn profile_rules_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        let v = value.to_string();
        match key {
            "pr-type" => self.edit_rule_draft(move |r| r.action = v),
            "pr-set" => self.edit_rule_draft(move |r| r.rule_provider = v),
            "pr-target" => self.edit_rule_draft(move |r| {
                if r.action == "SUB-RULE" {
                    r.sub_rule = v;
                } else {
                    r.target = v;
                }
            }),
            "pr-set-none" => {}
            _ => return false,
        }
        true
    }

    pub(crate) fn profile_rules_input(self: &Arc<Self>, key: &str, v1: &str) -> bool {
        if key != "pr-in:content" {
            return false;
        }
        let v = v1.to_string();
        self.edit_rule_draft(move |r| r.content = v);
        true
    }

    /// `_handleSaveRule`: `_ruleSaveError`, then `put` (a new rule goes first).
    fn save_rule(self: &Arc<Self>) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let Some(d) = page().lock().unwrap().draft.clone() else {
            return;
        };
        if let Some(e) = pr::save_error(&d.rule) {
            return self.alert("无法保存", e);
        }
        let mut rule = d.rule;
        if d.is_new {
            rule.id = new_id();
        }
        page().lock().unwrap().draft = None;
        self.edit_rules(move |s| pr::put(s, &pid, rule));
        self.show_rules_page("rules");
    }

    pub(crate) fn pr_save_answer(self: &Arc<Self>, yes: bool) {
        if yes {
            self.save_rule();
        } else {
            self.close_rule_form();
        }
    }

    fn close_rule_form(self: &Arc<Self>) {
        page().lock().unwrap().draft = None;
        self.show_rules_page("rules");
    }

    /// `_handleQuickEdit`: the rules as a YAML list in the editor.
    fn open_rules_yaml(self: &Arc<Self>, pid: &str) {
        let text = pr::encode_list(&cp::rules(&self.store.lock().unwrap().state, pid));
        page().lock().unwrap().yaml = Some(text.clone());
        self.ui(move |ui| {
            ui.set_editor_title("规则".into());
            ui.set_editor_text(text.into());
            ui.set_editor_readonly(false);
            ui.set_editor_parent("rules".into());
            ui.set_profiles_page("editor".into());
        });
    }

    /// The editor's save while the rules' quick edit is open; false otherwise.
    pub(crate) fn rules_yaml_save(self: &Arc<Self>, text: &str) -> bool {
        let Some(original) = page().lock().unwrap().yaml.clone() else {
            return false;
        };
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return true;
        };
        if text != original {
            let result = {
                let store = self.store.lock().unwrap();
                let previous = cp::rules(&store.state, &pid);
                let mut next = new_id();
                pr::quick_edit(text, &previous, &pid, || {
                    next += 1;
                    next
                })
            };
            match result {
                Ok(rules) => self.edit_rules(move |s| pr::set_all(s, &pid, rules)),
                Err(e) => {
                    self.alert("无法保存", e);
                    return true;
                }
            }
        }
        page().lock().unwrap().yaml = None;
        self.ui(|ui| {
            ui.set_editor_parent("".into());
            ui.set_profiles_page("rules".into());
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_groups::Data;
    use serde_json::json;
    use slint::Model;

    fn targets() -> Targets {
        let data = Data {
            groups: vec![(
                1,
                json!({"name": "节点选择", "type": "select", "proxies": ["香港"]}),
            )],
            proxies: ["香港".to_string()].into(),
            ..Default::default()
        };
        Targets::of(&data, ["ads".to_string()])
    }

    fn titles(s: &SetSection) -> Vec<String> {
        s.rows.iter().map(|r| r.title.to_string()).collect()
    }

    #[test]
    fn rows_carry_names_tones_marks_and_selection() {
        let a = pr::parse("DOMAIN-SUFFIX,a.com,DIRECT", 1, "p");
        let b = pr::parse("RULE-SET,cn,节点选择", 2, "p");
        let c = pr::parse("MATCH,香港", 3, "p");
        let rows = rule_rows(&[&a, &b, &c], &targets(), &[2].into());
        assert_eq!(rows[0].name, "DOMAIN-SUFFIX");
        assert_eq!((rows[0].tone, rows[0].invalid), (1, false));
        assert_eq!(
            (rows[1].content.as_str(), rows[1].invalid, rows[1].selected),
            ("cn", true, true)
        );
        assert_eq!((rows[2].content.as_str(), rows[2].tone), ("", 3));
    }

    #[test]
    fn the_form_follows_the_type() {
        let t = targets();
        let d = Draft::open(pr::new_rule(0, "p"), true);
        let (s, issue) = form_sections(&d, &t);
        assert_eq!(titles(&s[0]), ["代理类型", "内容", "分流策略"]);
        assert_eq!(s.len(), 1);
        assert!(issue.is_empty());
        let mut d = Draft::open(pr::parse("IP-CIDR,1.0.0.0/8,无,src", 5, "p"), false);
        let (s, issue) = form_sections(&d, &t);
        assert_eq!(titles(&s[1]), ["不解析主机名", "匹配来源IP"]);
        let nr = s[1].rows.row_data(0).unwrap();
        assert!(nr.on && !nr.enabled);
        assert_eq!(titles(&s[2]), ["删除"]);
        assert_eq!(issue, "无 是一个无效的策略");
        d.rule = pr::parse("RULE-SET,cn,DIRECT", 5, "p");
        let (s, issue) = form_sections(&d, &t);
        assert_eq!(titles(&s[0]), ["代理类型", "规则集", "分流策略"]);
        assert!(s[0].rows.row_data(1).unwrap().invalid);
        assert_eq!(issue, "cn 不是有效的规则集");
        d.rule = pr::parse("MATCH", 5, "p");
        let (s, _) = form_sections(&d, &t);
        assert_eq!(titles(&s[0]), ["代理类型", "分流策略"]);
        assert_eq!(s[0].rows.row_data(1).unwrap().subtitle, "请选择分流策略");
        assert!(s[0].rows.row_data(1).unwrap().invalid);
    }

    #[test]
    fn presets_list_every_preset_with_its_rules() {
        let s = preset_sections(&[1].into());
        let rows: Vec<SetRow> = s[0].rows.iter().collect();
        assert_eq!(rows.len(), PRESETS.len());
        assert!(rows[1].on && !rows[0].on);
        assert!(rows[4].subtitle.contains("GEOIP,LAN,DIRECT,no-resolve"));
    }
}
