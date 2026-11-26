//! A profile's extensions (lib/views/profiles/extend): its own added rules, the app's added
//! rules it leaves out (`disabled_rules`), and where each custom exit goes (`exitTargets`).

use crate::custom_profile::{self as cp, DisabledRule, ProfileRule};
use crate::rules::{self, Rule, TARGETS};
use crate::store::{Profile, State};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn to_rule(r: &ProfileRule) -> Rule {
    Rule {
        id: r.id,
        action: r.action.clone(),
        content: r.content.clone(),
        target: r.target.clone(),
        no_resolve: r.no_resolve,
        src: r.src,
        custom_exit: r.custom_exit,
    }
}

fn from_rule(r: &Rule, profile_id: &str) -> ProfileRule {
    ProfileRule {
        id: r.id,
        profile_id: profile_id.into(),
        action: r.action.clone(),
        content: r.content.clone(),
        target: r.target.clone(),
        no_resolve: r.no_resolve,
        src: r.src,
        custom_exit: r.custom_exit,
        ..Default::default()
    }
}

/// `queryProfileRules` for a profile that is not custom: its own added rules.
pub fn own_rules(state: &State, profile_id: &str) -> Vec<Rule> {
    cp::rules(state, profile_id)
        .into_iter()
        .map(to_rule)
        .collect()
}

pub fn set_own_rules(state: &mut State, profile_id: &str, list: &[Rule]) {
    let rows = list.iter().map(|r| from_rule(r, profile_id)).collect();
    crate::profile_rules::set_all(state, profile_id, rows);
}

pub fn disabled_ids(state: &State, profile_id: &str) -> BTreeSet<u64> {
    state
        .disabled_rules
        .iter()
        .filter(|d| d.profile_id == profile_id)
        .map(|d| d.rule_id)
        .collect()
}

pub fn set_disabled(state: &mut State, profile_id: &str, rule_id: u64, disabled: bool) {
    state
        .disabled_rules
        .retain(|d| d.profile_id != profile_id || d.rule_id != rule_id);
    if disabled {
        state.disabled_rules.push(DisabledRule {
            profile_id: profile_id.into(),
            rule_id,
        });
    }
}

pub fn enabled_global(state: &State, profile_id: &str) -> Vec<Rule> {
    let off = disabled_ids(state, profile_id);
    state
        .settings
        .added_rules
        .iter()
        .filter(|r| !off.contains(&r.id))
        .cloned()
        .collect()
}

/// `queryAddedRules`: the profile's own rules, then the app's it keeps; none for a custom
/// profile, whose own rules stand in place of the config's.
pub fn added_rules(state: &State, profile: &Profile) -> Vec<Rule> {
    if profile.is_custom() {
        return Vec::new();
    }
    let mut out = own_rules(state, &profile.id);
    out.extend(enabled_global(state, &profile.id));
    out
}

/// `_GlobalAddedRulesItem`: (kept, all).
pub fn enabled_count(state: &State, profile_id: &str) -> (usize, usize) {
    (
        enabled_global(state, profile_id).len(),
        state.settings.added_rules.len(),
    )
}

/// `matchTargetOf`: where the last MATCH line goes.
pub fn match_target(lines: &[String]) -> Option<String> {
    lines.iter().rev().find_map(|l| {
        let mut f = l.split(',').map(str::trim);
        (f.next()?.eq_ignore_ascii_case("MATCH"))
            .then(|| f.next().unwrap_or("").to_string())
            .filter(|t| !t.is_empty())
    })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outbounds {
    pub groups: Vec<(String, String)>,
    pub proxies: Vec<(String, String)>,
}

impl Outbounds {
    pub fn names(&self) -> BTreeSet<String> {
        self.groups
            .iter()
            .chain(&self.proxies)
            .map(|(n, _)| n.clone())
            .collect()
    }
}

fn refs(v: Option<&Value>) -> Vec<(String, String)> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|o| {
                    let name = o.get("name")?.as_str()?.to_string();
                    let kind = o.get("type").and_then(Value::as_str).unwrap_or("");
                    Some((name, kind.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn outbounds_of(answer: &Value) -> Outbounds {
    Outbounds {
        groups: refs(answer.get("proxyGroups")),
        proxies: refs(answer.get("proxies")),
    }
}

/// `outboundNamesOf`: what a config's `proxies` and `proxy-groups` name.
pub fn config_outbounds(config: &Value) -> Outbounds {
    Outbounds {
        groups: refs(config.get("proxy-groups")),
        proxies: refs(config.get("proxies")),
    }
}

/// `_watchOutbounds` for a custom profile: its groups and the app's proxies.
pub fn custom_outbounds(state: &State, profile_id: &str) -> Outbounds {
    let def = |d: &Value, k: &str| d.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Outbounds {
        groups: cp::groups(state, profile_id)
            .iter()
            .map(|g| (g.name().to_string(), def(&g.definition, "type")))
            .collect(),
        proxies: state
            .settings
            .custom_proxies
            .iter()
            .filter(|p| !def(&p.definition, "name").is_empty())
            .map(|p| (def(&p.definition, "name"), def(&p.definition, "type")))
            .collect(),
    }
}

fn usable(name: &str, outbounds: &BTreeSet<String>) -> bool {
    TARGETS.contains(&name) || outbounds.contains(name)
}

/// `exitTargetResolver`: the picked target when it still exists, else an outbound of the
/// exit's name, else where unmatched traffic goes.
pub fn resolve_exit(
    targets: &BTreeMap<String, String>,
    outbounds: &BTreeSet<String>,
    fallback: Option<&str>,
    exit: &str,
) -> String {
    if let Some(t) = targets.get(exit).filter(|t| usable(t, outbounds)) {
        return t.clone();
    }
    if outbounds.contains(exit) {
        return exit.to_string();
    }
    fallback.unwrap_or("DIRECT").to_string()
}

pub fn overlay_rules(
    added: &[Rule],
    targets: &BTreeMap<String, String>,
    outbounds: &BTreeSet<String>,
    fallback: Option<&str>,
) -> Vec<String> {
    added
        .iter()
        .map(|r| r.raw(|e| resolve_exit(targets, outbounds, fallback, e)))
        .collect()
}

/// `module-policy`: each policy the modules name, resolved as an exit.
pub fn module_policy(
    policies: &BTreeSet<String>,
    targets: &BTreeMap<String, String>,
    outbounds: &BTreeSet<String>,
    fallback: Option<&str>,
) -> Value {
    let m: Map<String, Value> = policies
        .iter()
        .map(|p| {
            (
                p.clone(),
                resolve_exit(targets, outbounds, fallback, p).into(),
            )
        })
        .collect();
    Value::Object(m)
}

/// `ExitsSection._watchExits`: the rules' exits, then the modules'.
pub fn exits(rules: &[Rule], modules: &[(String, Vec<String>)], modules_only: bool) -> Vec<String> {
    let mut out = if modules_only {
        Vec::new()
    } else {
        rules::custom_exits(rules)
    };
    for (m, _) in modules {
        if !out.contains(m) {
            out.push(m.clone());
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExitRow {
    pub exit: String,
    /// The picked target, None for 默认.
    pub target: Option<String>,
    pub invalid: bool,
    pub subtitle: String,
}

/// `names` is None until the profile's outbounds are known.
pub fn exit_row(
    exit: &str,
    target: Option<&str>,
    names: Option<&BTreeSet<String>>,
    fallback: Option<&str>,
    modules: Option<&Vec<String>>,
) -> ExitRow {
    let invalid = matches!((names, target), (Some(n), Some(t)) if !usable(t, n));
    let actual = match names {
        Some(n) if target.is_none() || invalid => {
            if n.contains(exit) {
                Some(exit.to_string())
            } else {
                fallback.filter(|f| usable(f, n)).map(str::to_string)
            }
        }
        _ => None,
    };
    let mut lines: Vec<String> = actual.into_iter().collect();
    if let Some(m) = modules {
        lines.push(format!("来自模块：{}", m.join(", ")));
    }
    ExitRow {
        exit: exit.into(),
        target: target.map(str::to_string),
        invalid,
        subtitle: lines.join("\n"),
    }
}

/// `_handleSelect`'s default line: the outbound of the exit's name, else the fallback.
pub fn default_target(exit: &str, outbounds: &Outbounds, fallback: Option<&str>) -> String {
    if outbounds.names().contains(exit) {
        exit.into()
    } else {
        fallback.unwrap_or("").into()
    }
}

/// `exitTargets` with the exit set to `target`, or back to 默认 when empty.
pub fn with_target(
    mut targets: BTreeMap<String, String>,
    exit: &str,
    target: &str,
) -> BTreeMap<String, String> {
    targets.remove(exit);
    if !target.is_empty() {
        targets.insert(exit.into(), target.into());
    }
    targets
}

/// `mitmModuleExits`: each named policy with the modules that name it, in module order.
pub fn module_exits(modules: &[(String, Vec<String>)]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for (module, policies) in modules {
        for p in policies {
            match out.iter_mut().find(|(e, _)| e == p) {
                Some((_, names)) => names.push(module.clone()),
                None => out.push((p.clone(), vec![module.clone()])),
            }
        }
    }
    out
}

/// `ExtendView._watchInputs`: what an apply reads of the page.
pub fn inputs(state: &State, profile_id: &str) -> Value {
    json!({
        "targets": state.profiles.iter().find(|p| p.id == profile_id).map(|p| &p.exit_targets),
        "rules": own_rules(state, profile_id),
        "disabled": disabled_ids(state, profile_id),
        "script": state.profiles.iter().find(|p| p.id == profile_id).map(|p| &p.script_id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: u64, content: &str, target: &str, exit: bool) -> Rule {
        Rule {
            content: content.into(),
            target: target.into(),
            custom_exit: exit,
            ..Rule::new(id)
        }
    }

    fn state() -> State {
        let mut s = State::default();
        s.profiles.push(Profile {
            id: "p".into(),
            url: "https://a".into(),
            ..Default::default()
        });
        s.settings.added_rules = vec![
            rule(1, "g1.com", "DIRECT", false),
            rule(2, "g2.com", "家里", true),
            rule(3, "g3.com", "REJECT", false),
        ];
        s
    }

    #[test]
    fn own_rules_come_first_and_disabled_ones_drop() {
        let mut s = state();
        set_own_rules(&mut s, "p", &[rule(10, "own.com", "DIRECT", false)]);
        set_disabled(&mut s, "p", 3, true);
        set_disabled(&mut s, "p", 3, true);
        set_disabled(&mut s, "other", 1, true);
        let p = s.profiles[0].clone();
        let ids: Vec<u64> = added_rules(&s, &p).iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![10, 1, 2]);
        assert_eq!(enabled_count(&s, "p"), (2, 3));
        assert_eq!(s.disabled_rules.len(), 2);
        set_disabled(&mut s, "p", 3, false);
        assert_eq!(enabled_count(&s, "p"), (3, 3));
        assert_eq!(enabled_count(&s, "other"), (2, 3));
        let mut custom = p.clone();
        custom.kind = cp::KIND.into();
        assert!(added_rules(&s, &custom).is_empty());
    }

    #[test]
    fn own_rules_keep_their_order() {
        let mut s = state();
        let list = vec![
            rule(5, "b.com", "DIRECT", false),
            rule(4, "a.com", "家里", true),
        ];
        set_own_rules(&mut s, "p", &list);
        assert_eq!(own_rules(&s, "p"), list);
        assert!(own_rules(&s, "q").is_empty());
    }

    #[test]
    fn exits_resolve_to_the_pick_the_outbound_or_the_match() {
        let names: BTreeSet<String> = ["节点选择".to_string(), "家里".to_string()].into();
        let mut targets = BTreeMap::new();
        assert_eq!(resolve_exit(&targets, &names, Some("X"), "家里"), "家里");
        assert_eq!(resolve_exit(&targets, &names, Some("X"), "公司"), "X");
        assert_eq!(resolve_exit(&targets, &names, None, "公司"), "DIRECT");
        targets.insert("公司".into(), "节点选择".into());
        assert_eq!(resolve_exit(&targets, &names, None, "公司"), "节点选择");
        targets.insert("公司".into(), "REJECT".into());
        assert_eq!(resolve_exit(&targets, &names, None, "公司"), "REJECT");
        targets.insert("公司".into(), "gone".into());
        assert_eq!(resolve_exit(&targets, &names, Some("Y"), "公司"), "Y");
        let added = vec![
            rule(1, "a.com", "公司", true),
            rule(2, "b.com", "DIRECT", false),
        ];
        assert_eq!(
            overlay_rules(&added, &targets, &names, Some("Y")),
            vec!["DOMAIN,a.com,Y", "DOMAIN,b.com,DIRECT"]
        );
        let policies: BTreeSet<String> = ["家里".to_string(), "广告".to_string()].into();
        assert_eq!(
            module_policy(&policies, &targets, &names, None),
            json!({"家里": "家里", "广告": "DIRECT"})
        );
    }

    #[test]
    fn match_target_reads_the_last_match() {
        let lines = vec![
            "MATCH,A".to_string(),
            "DOMAIN,x,B".into(),
            "match, C ".into(),
        ];
        assert_eq!(match_target(&lines).as_deref(), Some("C"));
        assert_eq!(match_target(&["MATCH".to_string()]), None);
        assert_eq!(match_target(&[]), None);
    }

    #[test]
    fn exit_rows_tell_the_actual_target() {
        let names: BTreeSet<String> = ["节点选择".to_string(), "家里".to_string()].into();
        let r = exit_row("公司", None, Some(&names), Some("节点选择"), None);
        assert_eq!((r.invalid, r.subtitle.as_str()), (false, "节点选择"));
        let r = exit_row("家里", None, Some(&names), Some("节点选择"), None);
        assert_eq!(r.subtitle, "家里");
        let r = exit_row("公司", Some("gone"), Some(&names), Some("missing"), None);
        assert!(r.invalid);
        assert_eq!(r.subtitle, "");
        let mods = vec!["A".to_string(), "B".to_string()];
        let r = exit_row("公司", Some("REJECT"), Some(&names), None, Some(&mods));
        assert_eq!((r.invalid, r.subtitle.as_str()), (false, "来自模块：A, B"));
        let r = exit_row("公司", Some("gone"), None, None, None);
        assert!(!r.invalid && r.subtitle.is_empty());
    }

    #[test]
    fn exits_list_rules_then_modules() {
        let rules = vec![rule(1, "a", "家里", true), rule(2, "b", "DIRECT", false)];
        let modules = vec![
            ("广告".to_string(), vec!["M".to_string()]),
            ("家里".to_string(), vec!["N".to_string()]),
        ];
        assert_eq!(exits(&rules, &modules, false), vec!["家里", "广告"]);
        assert_eq!(exits(&rules, &modules, true), vec!["广告", "家里"]);
        assert_eq!(
            with_target(BTreeMap::new(), "家里", "节点选择")
                .get("家里")
                .map(String::as_str),
            Some("节点选择")
        );
        assert!(with_target([("家里".to_string(), "X".to_string())].into(), "家里", "").is_empty());
    }

    #[test]
    fn module_exits_group_modules_by_policy() {
        let read = vec![
            (
                "A".to_string(),
                vec!["家里".to_string(), "广告".to_string()],
            ),
            ("B".to_string(), vec!["家里".to_string()]),
        ];
        assert_eq!(
            module_exits(&read),
            vec![
                ("家里".to_string(), vec!["A".to_string(), "B".to_string()]),
                ("广告".to_string(), vec!["A".to_string()]),
            ]
        );
    }

    #[test]
    fn outbounds_read_answers_and_configs() {
        let o = outbounds_of(&json!({
            "proxies": [{"name": "a", "type": "Trojan"}],
            "proxyGroups": [{"name": "G", "type": "Selector"}]
        }));
        assert_eq!(o.groups, vec![("G".to_string(), "Selector".to_string())]);
        assert_eq!(o.names().len(), 2);
        let c = config_outbounds(
            &json!({"proxies": [{"name": "b", "type": "ss"}], "proxy-groups": []}),
        );
        assert_eq!(c.proxies[0].0, "b");
        assert_eq!(default_target("b", &c, Some("F")), "b".to_string());
        assert_eq!(default_target("z", &c, Some("F")), "F".to_string());
    }
}
