//! The custom profile's pages past its overview, on `App`'s side: 外部资源
//! (`CustomProvidersView`), 更多 (`_MoreSectionsView`), 拨号代理 (`CustomProxyDialersView`)
//! and the profile's DNS, NTP, sniffer and TUN settings (`OverrideView` on a
//! `ProfileOverrideTarget`), with their list, map and sniffed-protocol pages. They stack over
//! the profile page as "pmore"; back takes the top one off.

use crate::custom_profile::{self as cp, ProfileOverrides};
use crate::overrides::{self, Kind, OverrideState, Spec};
use crate::store::State;
use crate::tun_sniffer_ntp::{self as tsn, NTP, SNIFFER, TUN};
use crate::{ActionItem, App, PageMeta, SetRow, SetSection};
use serde_json::json;
use slint::{ModelRc, VecModel};
use std::path::Path;
use std::sync::{Arc, Mutex};

static STACK: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The spec a profile page id names.
pub fn spec(id: &str) -> Option<&'static Spec> {
    match id {
        "dns" => Some(&cp::PROFILE_DNS),
        "ntp" => Some(&NTP),
        "sniffer" => Some(&SNIFFER),
        "tun" => Some(&TUN),
        _ => None,
    }
}

fn state_of<'a>(o: &'a ProfileOverrides, id: &str) -> &'a OverrideState {
    match id {
        "dns" => &o.dns,
        "ntp" => &o.ntp,
        "sniffer" => &o.sniffer,
        _ => &o.tun,
    }
}

fn state_mut<'a>(o: &'a mut ProfileOverrides, id: &str) -> &'a mut OverrideState {
    match id {
        "dns" => &mut o.dns,
        "ntp" => &mut o.ntp,
        "sniffer" => &mut o.sniffer,
        _ => &mut o.tun,
    }
}

/// `<spec>:<path>` as its spec, its state and the path.
fn target<'a, 'k>(
    o: &'a ProfileOverrides,
    key: &'k str,
) -> Option<(&'static Spec, &'a OverrideState, &'k str)> {
    let (id, path) = key.split_once(':')?;
    Some((spec(id)?, state_of(o, id), path))
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

fn section(title: &str, footer: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: footer.into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

fn meta(title: &str) -> PageMeta {
    PageMeta {
        known: true,
        title: title.into(),
        ..Default::default()
    }
}

/// A list a key names: the override's list, or a sniffed protocol's ports.
fn list_of(o: &ProfileOverrides, key: &str) -> Option<(Vec<String>, String, &'static str)> {
    let (spec, state, path) = target(o, key)?;
    let k = spec.key(path)?;
    let v = spec.value(state, path);
    match k.kind {
        Kind::List(_, desc) => Some((overrides::strings(&v), k.label.into(), desc)),
        Kind::Sniff => Some((tsn::ports(&v), "端口".into(), "")),
        _ => None,
    }
}

fn put_list(o: &mut ProfileOverrides, key: &str, items: Vec<String>) {
    let Some((id, path)) = key.split_once(':') else {
        return;
    };
    let Some(spec) = spec(id) else {
        return;
    };
    let state = state_mut(o, id);
    let value = match spec.key(path).map(|k| k.kind) {
        Some(Kind::Sniff) => tsn::with_ports(&spec.value(state, path), items),
        _ => json!(items),
    };
    spec.set(state, path, value);
}

fn map_of(
    o: &ProfileOverrides,
    key: &str,
) -> Option<(
    std::collections::BTreeMap<String, String>,
    String,
    &'static str,
)> {
    let (spec, state, path) = target(o, key)?;
    let k = spec.key(path)?;
    let Kind::Map(desc) = k.kind else {
        return None;
    };
    Some((
        overrides::pairs(&spec.value(state, path)),
        k.label.into(),
        desc,
    ))
}

/// The rule set's file age, compiled or not.
fn rule_set_age(dir: &Path, s: &crate::rule_sets::RuleSet) -> String {
    let file = s.file_name();
    [dir.join(&file), dir.join(format!("{file}.mrs"))]
        .iter()
        .find_map(|p| crate::rule_sets_app::modified_secs(p))
        .map(crate::ago_short)
        .unwrap_or_default()
}

/// `CustomProvidersView`: the proxy providers, then the rule sets, each marked when the app
/// lacks it.
fn providers_sections(state: &State, profile_id: &str, dir: &Path) -> Vec<SetSection> {
    let (proxy, rule) = crate::providers::referenced(state, profile_id);
    let profiles: Vec<&crate::store::Profile> =
        state.profiles.iter().filter(|p| !p.is_custom()).collect();
    let proxy_rows: Vec<SetRow> = proxy
        .iter()
        .map(|n| match profiles.iter().find(|p| p.label() == *n) {
            Some(p) => row(&format!("pp:{n}"), "open", n, &crate::ago(p.updated)),
            None => SetRow {
                invalid: true,
                ..row(&format!("pp-missing:{n}"), "value", n, "")
            },
        })
        .collect();
    let rule_rows: Vec<SetRow> = rule
        .iter()
        .map(
            |n| match state.settings.rule_sets.iter().find(|s| s.label == *n) {
                Some(s) => row(&format!("rp:{n}"), "value", n, &rule_set_age(dir, s)),
                None => SetRow {
                    invalid: true,
                    ..row(&format!("rp-missing:{n}"), "value", n, "")
                },
            },
        )
        .collect();
    [("代理", proxy_rows), ("规则", rule_rows)]
        .into_iter()
        .filter(|(_, rows)| !rows.is_empty())
        .map(|(t, rows)| section(t, "", rows))
        .collect()
}

fn dialers_sections(state: &State, profile_id: &str) -> Vec<SetSection> {
    let issues = crate::dialers::profile_issues(state, profile_id);
    let rows: Vec<SetRow> = crate::dialers::rows(state, profile_id)
        .into_iter()
        .map(|(id, name, target)| SetRow {
            invalid: issues.contains_key(&id),
            ..row(
                &format!("dl:{id}"),
                "action",
                &name,
                &target.unwrap_or_default(),
            )
        })
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    vec![section("", "", rows)]
}

fn more_sections(state: &State, p: &crate::store::Profile) -> Vec<SetSection> {
    let rows = cp::more_rows(state, p)
        .into_iter()
        .map(|(id, glyph, title, value, tone)| SetRow {
            glyph: glyph.into(),
            trailing: value.into(),
            invalid: tone == 2,
            ..row(id, "open", title, "")
        })
        .collect();
    vec![section("", "", rows)]
}

/// The page on top, as the shell shows it: its bar, its rows and its banner.
pub fn view(
    state: &State,
    profile_id: &str,
    page: &str,
    dir: &Path,
) -> (PageMeta, Vec<SetSection>, String) {
    let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
        return (PageMeta::default(), Vec::new(), String::new());
    };
    let o = &p.overrides;
    let issue =
        |found: Vec<crate::proxy_groups::Issue>| crate::proxy_groups::issues_message(&found);
    match page {
        "providers" => (
            PageMeta {
                empty: "暂无外部资源".into(),
                ..meta("外部资源")
            },
            providers_sections(state, profile_id, dir),
            String::new(),
        ),
        "dialers" => (
            PageMeta {
                empty: "暂无本地代理".into(),
                ..meta("拨号代理")
            },
            dialers_sections(state, profile_id),
            issue(
                crate::dialers::profile_issues(state, profile_id)
                    .into_values()
                    .flatten()
                    .collect(),
            ),
        ),
        "more" => (meta("更多"), more_sections(state, p), String::new()),
        id if spec(id).is_some() => {
            let spec = spec(id).expect("a spec");
            let st = state_of(o, id);
            let found = match id {
                "dns" => cp::dns_issues(state, p),
                "ntp" => cp::ntp_issues(state, p),
                _ => Vec::new(),
            };
            (
                PageMeta {
                    action: if spec.can_add(st) {
                        "add-circle".into()
                    } else {
                        "".into()
                    },
                    empty: format!("暂无{}", spec.entries).into(),
                    empty_desc: spec.description.into(),
                    empty_action: spec.add.into(),
                    ..meta(spec.title)
                },
                spec.sections(st),
                issue(found),
            )
        }
        _ => {
            if let Some(path) = page.strip_prefix("sniff:") {
                let title = SNIFFER.key(path).map_or("", |k| k.label);
                return (
                    meta(title),
                    tsn::sniff_sections(&SNIFFER.value(&o.sniffer, path)),
                    String::new(),
                );
            }
            if let Some(key) = page.strip_prefix("list:") {
                let (items, title, desc) = list_of(o, key).unwrap_or_default();
                return (
                    PageMeta {
                        action: "add-circle".into(),
                        empty: "暂无内容".into(),
                        empty_action: "添加".into(),
                        ..meta(&title)
                    },
                    crate::settings_view::list(&items, desc),
                    String::new(),
                );
            }
            if let Some(key) = page.strip_prefix("map:") {
                let (entries, title, desc) = map_of(o, key).unwrap_or_default();
                return (
                    PageMeta {
                        action: "add-circle".into(),
                        empty: "暂无内容".into(),
                        empty_action: "添加".into(),
                        ..meta(&title)
                    },
                    crate::settings_view::map(&entries, desc),
                    String::new(),
                );
            }
            (PageMeta::default(), Vec::new(), String::new())
        }
    }
}

fn top() -> Option<String> {
    STACK.lock().unwrap().last().cloned()
}

fn check_item(id: String, label: &str, subtitle: &str, checked: bool) -> ActionItem {
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

impl App {
    pub(crate) fn bind_profile_more(self: &Arc<Self>, ui: &crate::AppWindow) {
        let a = self.clone();
        ui.on_pm_clicked(move |id| a.pm_clicked(&id));
        let a = self.clone();
        ui.on_pm_toggled(move |id, on| a.pm_toggled(&id, on));
        let a = self.clone();
        ui.on_pm_back(move || a.pm_back());
    }

    /// The overview's rows past 规则: 外部资源, DNS and 更多.
    pub(crate) fn open_pm(self: &Arc<Self>, page: &str) {
        if page == "more" {
            self.refresh_module_exits();
        }
        STACK.lock().unwrap().push(page.to_string());
        self.publish_pm();
        self.ui(|ui| ui.set_profiles_page("pmore".into()));
    }

    pub(crate) fn reset_pm(&self) {
        STACK.lock().unwrap().clear();
    }

    pub(crate) fn pm_back(self: &Arc<Self>) {
        let empty = {
            let mut st = STACK.lock().unwrap();
            st.pop();
            st.is_empty()
        };
        if empty {
            self.publish_custom();
            self.ui(|ui| ui.set_profiles_page("custom".into()));
        } else {
            self.publish_pm();
        }
    }

    pub(crate) fn publish_pm(self: &Arc<Self>) {
        let (Some(pid), Some(page)) = (crate::custom_profile_app::page_id(), top()) else {
            return;
        };
        let dir = self.rule_sets_dir();
        let state = self.store.lock().unwrap().state.clone();
        let exits = if page == "more" {
            self.exit_rows(&state, &pid)
        } else {
            Vec::new()
        };
        self.ui(move |ui| {
            let (meta, mut sections, issue) = view(&state, &pid, &page, &dir);
            sections.extend(crate::extend_app::exit_section(&exits));
            ui.set_pm_meta(meta);
            ui.set_pm_sections(ModelRc::new(VecModel::from(sections)));
            ui.set_pm_issue(issue.into());
        });
    }

    /// Saves a change to the open profile and shows it.
    fn edit_profile_overrides(self: &Arc<Self>, f: impl FnOnce(&mut State, &str)) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state, &pid);
            store.save();
        }
        self.publish_pm();
        self.publish_custom();
    }

    fn edit_spec(self: &Arc<Self>, id: &str, f: impl FnOnce(&Spec, &mut OverrideState)) {
        let Some(spec) = spec(id) else {
            return;
        };
        let id = id.to_string();
        self.edit_profile_overrides(move |s, pid| {
            if let Some(p) = s.profiles.iter_mut().find(|p| p.id == pid) {
                f(spec, state_mut(&mut p.overrides, &id));
            }
        });
    }

    fn overrides_now(&self) -> ProfileOverrides {
        let pid = crate::custom_profile_app::page_id().unwrap_or_default();
        self.store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .find(|p| p.id == pid)
            .map(|p| p.overrides.clone())
            .unwrap_or_default()
    }

    pub(crate) fn pm_clicked(self: &Arc<Self>, id: &str) {
        let Some(page) = top() else {
            return;
        };
        match page.as_str() {
            "more" => match id.strip_prefix("exit:") {
                Some(exit) => {
                    if let Some(pid) = crate::custom_profile_app::page_id() {
                        self.open_exit_sheet(&pid, exit);
                    }
                }
                None => self.open_pm(id),
            },
            "providers" => self.provider_clicked(id),
            "dialers" => {
                if let Some(proxy) = id.strip_prefix("dl:") {
                    self.open_dialer_sheet(proxy);
                }
            }
            p if spec(p).is_some() => self.pm_override_clicked(p, id),
            p => {
                if let Some(path) = p.strip_prefix("sniff:") {
                    match id {
                        "ports" => self.open_pm(&format!("list:sniffer:{path}")),
                        "destination" => self.open_pm_destination(path),
                        _ => {}
                    }
                } else if let Some(key) = p.strip_prefix("list:") {
                    self.pm_list_clicked(key, id);
                } else if let Some(key) = p.strip_prefix("map:") {
                    self.pm_map_clicked(key, id);
                }
            }
        }
    }

    fn provider_clicked(self: &Arc<Self>, id: &str) {
        if let Some(n) = id.strip_prefix("pp:") {
            self.open_provider_form(n);
        } else if let Some(n) = id.strip_prefix("pp-missing:") {
            self.alert("提示", format!("{n} 是一个无效的代理集"));
        } else if let Some(n) = id.strip_prefix("rp-missing:") {
            self.alert("提示", format!("{n} 不是有效的规则集"));
        }
    }

    /// `_handleSelect`: 无, then the proxies and groups the dial would not loop through.
    fn open_dialer_sheet(self: &Arc<Self>, proxy_id: &str) {
        let Some(pid) = crate::custom_profile_app::page_id() else {
            return;
        };
        let (current, nodes, groups) = {
            let store = self.store.lock().unwrap();
            let s = &store.state;
            let Some((_, name, current)) = crate::dialers::rows(s, &pid)
                .into_iter()
                .find(|(id, _, _)| id == proxy_id)
            else {
                return;
            };
            let (nodes, groups) = crate::dialers::choices(s, &pid, &name);
            (current.unwrap_or_default(), nodes, groups)
        };
        let mut items = vec![check_item(
            format!("pm-dial:{proxy_id}\t"),
            "无",
            "",
            current.is_empty(),
        )];
        for (heading, list) in [("本地代理", nodes), ("策略组", groups)] {
            for (i, (n, t)) in list.iter().enumerate() {
                let mut it = check_item(format!("pm-dial:{proxy_id}\t{n}"), n, t, *n == current);
                if i == 0 {
                    it.heading = heading.into();
                }
                items.push(it);
            }
        }
        self.show_choice("拨号代理", items);
    }

    /// A tap on a settings page: the bar's or the empty page's add, a row's remove button,
    /// or the row itself.
    fn pm_override_clicked(self: &Arc<Self>, page: &str, id: &str) {
        let Some(spec) = spec(page) else {
            return;
        };
        let o = self.overrides_now();
        let state = state_of(&o, page);
        if id == "action" {
            let mut last = "";
            let items: Vec<ActionItem> = spec
                .addable(state)
                .into_iter()
                .map(|(sec, k)| {
                    let heading = if sec != last { sec } else { "" };
                    last = sec;
                    ActionItem {
                        id: format!("pm-add:{page}:{}", k.path).into(),
                        heading: heading.into(),
                        label: k.label.into(),
                        subtitle: k.desc.into(),
                        enabled: true,
                        ..Default::default()
                    }
                })
                .collect();
            if !items.is_empty() {
                self.show_choice(spec.add, items);
            }
            return;
        }
        if let Some(path) = id.strip_prefix("remove:") {
            let path = path.to_string();
            self.edit_spec(page, move |spec, st| spec.remove(st, &path));
            return;
        }
        let Some(k) = spec.key(id) else {
            return;
        };
        let key = format!("{page}:{id}");
        match k.kind {
            Kind::List(..) => self.open_pm(&format!("list:{key}")),
            Kind::Map(_) => self.open_pm(&format!("map:{key}")),
            Kind::Sniff => self.open_pm(&format!("sniff:{id}")),
            Kind::Options(options) => {
                let current = spec.text(state, id);
                let items = options
                    .iter()
                    .map(|v| check_item(format!("pm-opt:{key}:{v}"), v, "", *v == current))
                    .collect();
                self.show_choice(k.label, items);
            }
            Kind::Number { .. } | Kind::Text(_) => {
                self.open_input(&format!("pm-in:{key}"), k.label, "", &spec.text(state, id));
                if matches!(k.kind, Kind::Number { .. }) {
                    self.ui(|ui| ui.set_input_number(true));
                }
            }
            Kind::Toggle => {}
        }
    }

    fn open_pm_destination(self: &Arc<Self>, path: &str) {
        let o = self.overrides_now();
        let current = tsn::destination(&SNIFFER.value(&o.sniffer, path));
        let items = tsn::DESTINATIONS
            .iter()
            .map(|(id, label, v)| {
                check_item(format!("pm-dest:{path}\t{id}"), label, "", *v == current)
            })
            .collect();
        self.show_choice("覆盖目标地址", items);
    }

    fn pm_list_clicked(self: &Arc<Self>, key: &str, id: &str) {
        if id == "action" {
            return self.open_input(&format!("pm-list:{key}\tnew"), "添加", "", "");
        }
        let Some(i) = id
            .strip_prefix("item:")
            .and_then(|i| i.parse::<usize>().ok())
        else {
            return;
        };
        let value = list_of(&self.overrides_now(), key)
            .and_then(|(v, _, _)| v.get(i).cloned())
            .unwrap_or_default();
        self.open_input_with(
            &format!("pm-list:{key}\t{i}"),
            "编辑",
            "",
            &value,
            "",
            "",
            true,
        );
    }

    fn pm_map_clicked(self: &Arc<Self>, key: &str, id: &str) {
        if id == "action" {
            return self.open_input_with(
                &format!("pm-map:{key}\t"),
                "添加",
                "键",
                "",
                "值",
                "",
                false,
            );
        }
        let Some(name) = id.strip_prefix("entry:") else {
            return;
        };
        let value = map_of(&self.overrides_now(), key)
            .and_then(|(m, _, _)| m.get(name).cloned())
            .unwrap_or_default();
        self.open_input_with(
            &format!("pm-map:{key}\t{name}"),
            "编辑",
            "键",
            name,
            "值",
            &value,
            true,
        );
    }

    pub(crate) fn pm_toggled(self: &Arc<Self>, id: &str, on: bool) {
        if let Some(page) = top().filter(|p| spec(p).is_some()) {
            let id = id.to_string();
            self.edit_spec(&page, move |spec, st| spec.set(st, &id, on.into()));
        }
    }

    /// The sheets' picks: an added setting, an option, a destination, a dialer.
    pub(crate) fn pm_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match key {
            "pm-add" => {
                if let Some((id, path)) = value.split_once(':') {
                    let path = path.to_string();
                    self.edit_spec(id, move |spec, st| spec.add(st, &path));
                }
            }
            "pm-opt" => {
                if let Some((target, option)) = value.rsplit_once(':') {
                    if let Some((id, path)) = target.split_once(':') {
                        let (path, option) = (path.to_string(), option.to_string());
                        self.edit_spec(id, move |spec, st| spec.set(st, &path, option.into()));
                    }
                }
            }
            "pm-dest" => {
                let Some((path, dest)) = value.split_once('\t') else {
                    return true;
                };
                let Some(d) = tsn::DESTINATIONS.iter().find(|d| d.0 == dest) else {
                    return true;
                };
                let (path, d) = (path.to_string(), d.2);
                self.edit_spec("sniffer", move |spec, st| {
                    let v = tsn::with_destination(&spec.value(st, &path), d);
                    spec.set(st, &path, v);
                });
            }
            "pm-dial" => {
                let Some((proxy, target)) = value.split_once('\t') else {
                    return true;
                };
                let (proxy, target) = (proxy.to_string(), target.to_string());
                self.edit_profile_overrides(move |s, pid| {
                    crate::dialers::set(s, pid, &proxy, (!target.is_empty()).then_some(target));
                });
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn pm_check(&self, key: &str, v1: &str, v2: &str) -> Option<String> {
        if let Some(rest) = key.strip_prefix("pm-in:") {
            let (id, path) = rest.split_once(':')?;
            return Some(spec(id)?.check(path, v1).err().unwrap_or_default());
        }
        if let Some(rest) = key.strip_prefix("pm-list:") {
            let (list, _) = rest.split_once('\t')?;
            let (id, path) = list.split_once(':')?;
            return Some(spec(id)?.check(path, v1).err().unwrap_or_default());
        }
        key.starts_with("pm-map:").then(|| {
            if v1.trim().is_empty() {
                "键不能为空".into()
            } else if v2.trim().is_empty() {
                "值不能为空".into()
            } else {
                String::new()
            }
        })
    }

    pub(crate) fn pm_input(self: &Arc<Self>, key: &str, v1: &str, v2: &str) -> bool {
        if let Some(rest) = key.strip_prefix("pm-in:") {
            if let Some((id, path)) = rest.split_once(':') {
                if let Some(v) = spec(id).and_then(|s| s.check(path, v1).ok()) {
                    let path = path.to_string();
                    self.edit_spec(id, move |spec, st| spec.set(st, &path, v));
                }
            }
            return true;
        }
        if key.starts_with("pm-list:") {
            self.pm_list_set(key, Some(v1.trim().to_string()));
            return true;
        }
        if key.starts_with("pm-map:") {
            self.pm_map_set(key, Some((v1.trim().to_string(), v2.trim().to_string())));
            return true;
        }
        false
    }

    pub(crate) fn pm_removed(self: &Arc<Self>, key: &str) -> bool {
        if key.starts_with("pm-list:") {
            self.pm_list_set(key, None);
        } else if key.starts_with("pm-map:") {
            self.pm_map_set(key, None);
        } else {
            return false;
        }
        true
    }

    fn pm_list_set(self: &Arc<Self>, key: &str, value: Option<String>) {
        let Some((list, at)) = key
            .strip_prefix("pm-list:")
            .and_then(|k| k.split_once('\t'))
        else {
            return;
        };
        let (list, at) = (list.to_string(), at.to_string());
        self.edit_profile_overrides(move |s, pid| {
            let Some(p) = s.profiles.iter_mut().find(|p| p.id == pid) else {
                return;
            };
            let Some((mut items, _, _)) = list_of(&p.overrides, &list) else {
                return;
            };
            match (at.parse::<usize>().ok(), value) {
                (None, Some(v)) if !items.contains(&v) => items.push(v),
                (Some(i), Some(v)) if i < items.len() => items[i] = v,
                (Some(i), None) if i < items.len() => {
                    items.remove(i);
                }
                _ => {}
            }
            put_list(&mut p.overrides, &list, items);
        });
    }

    fn pm_map_set(self: &Arc<Self>, key: &str, entry: Option<(String, String)>) {
        let Some((map, old)) = key.strip_prefix("pm-map:").and_then(|k| k.split_once('\t')) else {
            return;
        };
        let (map, old) = (map.to_string(), old.to_string());
        self.edit_profile_overrides(move |s, pid| {
            let Some(p) = s.profiles.iter_mut().find(|p| p.id == pid) else {
                return;
            };
            let Some((mut entries, _, _)) = map_of(&p.overrides, &map) else {
                return;
            };
            if !old.is_empty() {
                entries.remove(&old);
            }
            if let Some((name, value)) = entry {
                entries.insert(name, value);
            }
            if let Some((id, path)) = map.split_once(':') {
                if let Some(spec) = spec(id) {
                    spec.set(state_mut(&mut p.overrides, id), path, json!(entries));
                }
            }
        });
    }

    /// The overview's 外部资源, DNS and 更多 rows.
    pub(crate) fn open_overview_row(self: &Arc<Self>, id: &str) -> bool {
        let page = match id {
            "providers" => "providers",
            "dns" => "dns",
            "more" => "more",
            _ => return false,
        };
        self.reset_pm();
        self.open_pm(page);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    fn state() -> State {
        let mut s = State::default();
        s.profiles.push(cp::new_profile("p".into(), "自定义", 1));
        s.profiles.push(crate::store::Profile {
            id: "f".into(),
            name: "订阅".into(),
            url: "https://example.com".into(),
            ..Default::default()
        });
        s.proxy_groups.push(cp::ProxyGroupRow {
            id: 1,
            profile_id: "p".into(),
            definition: json!({"name": "G", "type": "select", "use": ["订阅", "旧订阅"], "proxies": ["A"]}),
            order: None,
        });
        s.settings
            .custom_proxies
            .push(crate::custom_proxies::CustomProxy {
                id: "9".into(),
                definition: json!({"name": "A", "type": "ss"}),
                ..Default::default()
            });
        s
    }

    fn ids(secs: &[SetSection]) -> Vec<String> {
        secs.iter()
            .flat_map(|s| s.rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>())
            .collect()
    }

    #[test]
    fn providers_list_marks_what_the_app_lacks() {
        let mut s = state();
        s.profile_rules.push(cp::ProfileRule {
            profile_id: "p".into(),
            action: "RULE-SET".into(),
            rule_provider: "广告".into(),
            target: "G".into(),
            ..Default::default()
        });
        let (meta, secs, _) = view(&s, "p", "providers", Path::new("/nonexistent"));
        assert_eq!(meta.title, "外部资源");
        assert_eq!(
            ids(&secs),
            ["pp:订阅", "pp-missing:旧订阅", "rp-missing:广告"]
        );
        assert!(secs[1].rows.row_data(0).unwrap().invalid);
    }

    #[test]
    fn dialers_and_more_follow_the_profile() {
        let mut s = state();
        crate::dialers::set(&mut s, "p", "9", Some("Gone".into()));
        let (_, secs, issue) = view(&s, "p", "dialers", Path::new(""));
        let r = secs[0].rows.row_data(0).unwrap();
        assert_eq!(
            (r.title.as_str(), r.subtitle.as_str(), r.invalid),
            ("A", "Gone", true)
        );
        assert_eq!(issue, "拨号代理 Gone 不存在");
        let (_, secs, _) = view(&s, "p", "more", Path::new(""));
        let r = secs[0].rows.row_data(0).unwrap();
        assert_eq!((r.trailing.as_str(), r.invalid), ("1 个代理", true));
        assert_eq!(ids(&secs).len(), if cp::shows_tun() { 4 } else { 3 });
    }

    #[test]
    fn settings_pages_and_their_lists_edit_the_profile() {
        let mut s = state();
        let (meta, secs, _) = view(&s, "p", "ntp", Path::new(""));
        assert!(secs.is_empty());
        assert_eq!(meta.empty, "暂无设置项");
        assert_eq!(meta.empty_action, "添加设置项");
        let o = &mut s.profiles[0].overrides;
        NTP.add(&mut o.ntp, "enable");
        NTP.set(&mut o.ntp, "enable", true.into());
        NTP.add(&mut o.ntp, "dialer-proxy");
        NTP.set(&mut o.ntp, "dialer-proxy", "Gone".into());
        SNIFFER.add(&mut o.sniffer, "sniff.TLS");
        put_list(o, "sniffer:sniff.TLS", vec!["443".into()]);
        put_list(o, "dns:nameserver", vec!["https://a#Gone".into()]);
        let (_, _, issue) = view(&s, "p", "ntp", Path::new(""));
        assert_eq!(issue, "拨号代理 Gone 不存在");
        let (meta, secs, _) = view(&s, "p", "list:sniffer:sniff.TLS", Path::new(""));
        assert_eq!(meta.title, "端口");
        assert_eq!(ids(&secs), ["item:0"]);
        let (_, secs, _) = view(&s, "p", "sniff:sniff.TLS", Path::new(""));
        assert_eq!(secs[0].rows.row_data(0).unwrap().subtitle, "1 项");
        assert!(list_of(&s.profiles[0].overrides, "dns:nameserver").is_some());
        assert!(view(&s, "p", "dns", Path::new("")).2.is_empty());
        s.profiles[0].overrides.dns.keys.push("nameserver".into());
        assert_eq!(
            view(&s, "p", "dns", Path::new("")).2,
            "以下代理或策略不存在：Gone"
        );
    }
}
