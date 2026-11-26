//! Custom profiles (lib/views/profiles/custom): a profile with no file of its own, built from
//! the proxy groups, rules and settings kept for it. The tables mirror lib/database's
//! `proxy_groups`, `rules` (those with a profile id), `proxy_dialers` and `disabled_rules`,
//! each row keyed by its profile's id.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use crate::overrides::{OverrideState, Spec};
use crate::store::{Profile, State};

pub const KIND: &str = "custom";

/// `ProfileOverrides` in lib/models/clash_config.dart: each section with its picked keys.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfileOverrides {
    pub dns: OverrideState,
    pub ntp: OverrideState,
    pub sniffer: OverrideState,
    pub tun: OverrideState,
    /// `proxy-providers`: options by provider name, as `ProxyProviderOptions` writes them.
    pub proxy_providers: BTreeMap<String, Value>,
}

impl ProfileOverrides {
    pub fn is_default(&self) -> bool {
        *self == ProfileOverrides::default()
    }
}

/// A row of `proxy_groups`: the group as the config takes it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProxyGroupRow {
    pub id: u64,
    pub profile_id: String,
    pub definition: Value,
    pub order: Option<String>,
}

impl ProxyGroupRow {
    pub fn name(&self) -> &str {
        self.definition
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

/// A row of `rules` that belongs to a profile (`queryProfileRules`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfileRule {
    pub id: u64,
    pub profile_id: String,
    pub action: String,
    pub content: String,
    pub target: String,
    pub rule_provider: String,
    pub sub_rule: String,
    pub no_resolve: bool,
    pub src: bool,
    pub order: Option<String>,
    pub custom_exit: bool,
}

impl ProfileRule {
    /// `rawValue`: RULE-SET reads its provider and SUB-RULE its sub rule.
    pub fn raw(&self) -> String {
        let content = if self.action == "RULE-SET" {
            &self.rule_provider
        } else {
            &self.content
        };
        let target = if self.action == "SUB-RULE" {
            &self.sub_rule
        } else {
            &self.target
        };
        let mut parts = vec![self.action.clone()];
        for part in [content, target] {
            if !part.is_empty() {
                parts.push(part.clone());
            }
        }
        if crate::rules::has_params(&self.action) {
            if self.src {
                parts.push("src".into());
            }
            if self.no_resolve {
                parts.push("no-resolve".into());
            }
        }
        parts.join(",")
    }
}

/// A row of `proxy_dialers`: the custom proxy a profile dials through `target`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProxyDialer {
    pub profile_id: String,
    pub proxy_id: String,
    pub target: String,
}

/// A row of `disabled_rules`: an added rule the profile leaves out.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DisabledRule {
    pub profile_id: String,
    pub rule_id: u64,
}

/// `Profile.custom`: no file, no auto update, updated now.
pub fn new_profile(id: String, label: &str, now: u64) -> Profile {
    Profile {
        id,
        name: label.to_string(),
        kind: KIND.into(),
        updated: now,
        auto_update: false,
        ..Default::default()
    }
}

/// Rows in their `order`, those without one last, as the DAOs sort them.
fn ordered<'a, T>(
    rows: impl Iterator<Item = &'a T>,
    order: impl Fn(&T) -> &Option<String>,
) -> Vec<&'a T>
where
    T: 'a,
{
    let mut rows: Vec<&T> = rows.collect();
    rows.sort_by(|a, b| match (order(a), order(b)) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    rows
}

pub fn groups<'a>(state: &'a State, profile_id: &str) -> Vec<&'a ProxyGroupRow> {
    ordered(
        state
            .proxy_groups
            .iter()
            .filter(|g| g.profile_id == profile_id),
        |g| &g.order,
    )
}

pub fn rules<'a>(state: &'a State, profile_id: &str) -> Vec<&'a ProfileRule> {
    ordered(
        state
            .profile_rules
            .iter()
            .filter(|r| r.profile_id == profile_id),
        |r| &r.order,
    )
}

/// `profileGroupsCount`.
pub fn groups_count(state: &State, profile_id: &str) -> usize {
    state
        .proxy_groups
        .iter()
        .filter(|g| g.profile_id == profile_id)
        .count()
}

/// The `ON DELETE CASCADE` of every table that names the profile.
pub fn remove_profile(state: &mut State, profile_id: &str) {
    state.proxy_groups.retain(|g| g.profile_id != profile_id);
    state.profile_rules.retain(|r| r.profile_id != profile_id);
    state.proxy_dialers.retain(|d| d.profile_id != profile_id);
    state.disabled_rules.retain(|d| d.profile_id != profile_id);
}

/// `_isUntouched`: the label it was created with, default settings, no groups and no rules.
pub fn is_untouched(state: &State, profile_id: &str, initial_label: &str) -> bool {
    let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
        return false;
    };
    p.name == initial_label
        && p.overrides.is_default()
        && groups_count(state, profile_id) == 0
        && rules(state, profile_id).is_empty()
}

pub fn is_empty(state: &State, profile_id: &str) -> bool {
    state
        .profiles
        .iter()
        .find(|p| p.id == profile_id)
        .is_none_or(|p| p.overrides.is_default())
        && groups_count(state, profile_id) == 0
        && rules(state, profile_id).is_empty()
        && !state
            .proxy_dialers
            .iter()
            .any(|d| d.profile_id == profile_id)
}

/// `_currentUses`: the active profile is a custom one and `uses` holds for it.
pub fn active_uses(state: &State, uses: impl Fn(&State, &str) -> bool) -> bool {
    state
        .active
        .as_deref()
        .and_then(|id| state.profiles.iter().find(|p| p.id == id))
        .is_some_and(|p| p.is_custom() && uses(state, &p.id))
}

pub fn names_proxies(
    state: &State,
    profile_id: &str,
    names: &std::collections::BTreeSet<String>,
) -> bool {
    groups(state, profile_id).iter().any(|g| {
        crate::proxy_groups::names(&g.definition, "proxies")
            .iter()
            .any(|n| names.contains(n))
    })
}

/// What the profile page's `_inputs` compare to tell an edit on the way out.
pub fn snapshot(state: &State, profile_id: &str) -> Value {
    json!({
        "profile": state.profiles.iter().find(|p| p.id == profile_id),
        "groups": groups(state, profile_id),
        "rules": rules(state, profile_id),
        "dialers": state.proxy_dialers.iter().filter(|d| d.profile_id == profile_id).collect::<Vec<_>>(),
    })
}

/// What a profile sets over the config besides the app's settings: its own groups and rules
/// (custom profiles only) and its overrides.
#[derive(Default)]
pub struct Own {
    pub rules: Vec<String>,
    pub groups: Vec<Value>,
    pub overrides: Option<ProfileOverrides>,
    pub rule_providers: Map<String, Value>,
    pub proxy_providers: Map<String, Value>,
    pub dialers: BTreeMap<String, String>,
}

pub fn own(state: &State, profile: &Profile) -> Own {
    if !profile.is_custom() {
        return Own::default();
    }
    Own {
        rules: rules(state, &profile.id).iter().map(|r| r.raw()).collect(),
        groups: groups(state, &profile.id)
            .iter()
            .map(|g| crate::proxy_groups::config_definition(&g.definition))
            .collect(),
        overrides: Some(profile.overrides.clone()),
        rule_providers: Map::new(),
        proxy_providers: Map::new(),
        dialers: crate::dialers::by_name(state, &profile.id),
    }
}

/// The app's rule sets the profile's rules name, in the order they are first named.
pub fn rule_sets_used(state: &State, profile: &Profile) -> Vec<crate::rule_sets::RuleSet> {
    if !profile.is_custom() {
        return Vec::new();
    }
    crate::providers::referenced(state, &profile.id)
        .1
        .into_iter()
        .filter_map(|n| {
            state
                .settings
                .rule_sets
                .iter()
                .find(|s| s.label == n)
                .cloned()
        })
        .collect()
}

/// `appProxiesPayload`: the custom proxies the groups name, with their dialers that do not loop.
pub fn proxies_payload(
    proxies: &[crate::custom_proxies::CustomProxy],
    groups: &[Value],
    dialers: &BTreeMap<String, String>,
) -> Vec<Value> {
    let kept = crate::dialers::payload_dialers(groups, dialers);
    let named: Vec<&str> = groups
        .iter()
        .filter_map(|g| g.get("proxies").and_then(Value::as_array))
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    proxies
        .iter()
        .filter(|p| {
            p.definition
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| named.contains(&n))
        })
        .map(|p| {
            let mut definition = p.definition.clone();
            if let Some(map) = definition.as_object_mut() {
                match kept.get(&p.name()) {
                    Some(d) => map.insert("dialer-proxy".into(), d.as_str().into()),
                    None => map.remove("dialer-proxy"),
                };
            }
            definition
        })
        .collect()
}

/// The file a custom profile is applied from: no config of its own but the proxies its
/// groups use (`clashConfig` is `const ClashConfig()` for a custom profile).
pub fn base_file(proxies: &[crate::custom_proxies::CustomProxy], own: &Own) -> Value {
    json!({"proxies": proxies_payload(proxies, &own.groups, &own.dialers)})
}

/// `DnsView.profile`: a custom profile may set any DNS key.
pub static PROFILE_DNS: Spec = Spec {
    id: "profile-dns",
    title: "DNS",
    keys: &crate::overrides::DNS_KEYS,
    allowed: &[],
    defaults: crate::overrides::dns_defaults,
    description: crate::tun_sniffer_ntp::PROFILE_DESC,
    entries: "设置项",
    add: "添加设置项",
};

/// `customDnsEnabled`: on unless the profile's own `enable` turns it off.
pub fn dns_enabled(o: &ProfileOverrides) -> bool {
    PROFILE_DNS
        .picked_value(&o.dns, "enable")
        .is_none_or(|v| v.as_bool() == Some(true))
}

fn enabled(spec: &Spec, state: &OverrideState) -> bool {
    spec.picked_value(state, "enable")
        .is_some_and(|v| v.as_bool() == Some(true))
}

pub fn ntp_enabled(o: &ProfileOverrides) -> bool {
    enabled(&crate::tun_sniffer_ntp::NTP, &o.ntp)
}

pub fn sniffer_enabled(o: &ProfileOverrides) -> bool {
    enabled(&crate::tun_sniffer_ntp::SNIFFER, &o.sniffer)
}

/// The steps of `_makeRealProfileTask` that read the profile: its DNS in place of the app's,
/// its TUN keys over the app's (`overAppTun`), its sniffer and NTP, then `proxy-groups` and
/// `rules` replaced by its own. `added` are the app's added rules, which only go in when the
/// profile has no rules of its own.
pub fn apply_own(c: &mut Map<String, Value>, own: &Own, added: Vec<String>) {
    if let Some(o) = &own.overrides {
        use crate::tun_sniffer_ntp::{apply_section, NTP, SNIFFER, TUN};
        for (key, spec, state) in [
            ("tun", &TUN, &o.tun),
            ("sniffer", &SNIFFER, &o.sniffer),
            ("ntp", &NTP, &o.ntp),
        ] {
            if let Some(v) = apply_section(spec, state, c.get(key)) {
                c.insert(key.into(), v);
            }
        }
    }
    if !own.groups.is_empty() {
        c.insert("proxy-groups".into(), Value::Array(own.groups.clone()));
    }
    if !own.proxy_providers.is_empty() {
        c.insert(
            "proxy-providers".into(),
            Value::Object(own.proxy_providers.clone()),
        );
    }
    if !own.rule_providers.is_empty() {
        c.insert(
            "rule-providers".into(),
            Value::Object(own.rule_providers.clone()),
        );
    }
    let rules = if own.rules.is_empty() {
        added
    } else {
        own.rules.clone()
    };
    c.insert("rules".into(), json!(rules));
}

/// The profile's DNS over the overlay's, for a custom profile, in place of the app's DNS
/// page (`profileOverrides` gives a custom profile none of it).
pub fn apply_dns(own: &Own, global: &OverrideState, dns: &mut Map<String, Value>) {
    match &own.overrides {
        Some(o) => PROFILE_DNS.apply(&o.dns, dns),
        None => crate::overrides::DNS.apply(global, dns),
    }
}

/// `_CountText`: the count, or the issues over it in the error tone.
fn count_row(
    id: &'static str,
    glyph: &'static str,
    title: &'static str,
    count: usize,
    issues: usize,
) -> (&'static str, &'static str, &'static str, String, i32) {
    if issues == 0 {
        (id, glyph, title, count.to_string(), 0)
    } else {
        (id, glyph, title, format!("{issues} / {count}"), 2)
    }
}

/// `customReferencedProviders`: the providers the profile names, and how many the app lacks.
pub fn referenced_providers(state: &State, profile_id: &str) -> (usize, usize) {
    let data = crate::proxy_groups::data(state, profile_id);
    let (proxy, rule) = crate::providers::referenced(state, profile_id);
    let missing = proxy
        .iter()
        .filter(|n| !data.proxy_providers.contains(*n))
        .count()
        + rule
            .iter()
            .filter(|n| !state.settings.rule_sets.iter().any(|s| &s.label == *n))
            .count();
    (proxy.len() + rule.len(), missing)
}

/// `showsTun`: a profile's own TUN settings take effect on the desktop only.
pub fn shows_tun() -> bool {
    !cfg!(target_os = "android")
}

pub fn dns_issues(state: &State, profile: &Profile) -> Vec<crate::proxy_groups::Issue> {
    if !dns_enabled(&profile.overrides) {
        return Vec::new();
    }
    let data = crate::proxy_groups::data(state, &profile.id);
    let missing: Vec<String> = crate::profile_refs::dns_proxies(&profile.overrides)
        .into_iter()
        .filter(|n| !data.is_rule_target(n))
        .collect();
    if missing.is_empty() {
        Vec::new()
    } else {
        vec![crate::proxy_groups::Issue::MissingProxies(missing)]
    }
}

pub fn ntp_issues(state: &State, profile: &Profile) -> Vec<crate::proxy_groups::Issue> {
    let o = &profile.overrides;
    match crate::profile_refs::ntp_proxy(o).filter(|_| ntp_enabled(o)) {
        Some(n) if !crate::proxy_groups::data(state, &profile.id).is_rule_target(&n) => {
            vec![crate::proxy_groups::Issue::MissingDialer(n)]
        }
        _ => Vec::new(),
    }
}

pub fn other_issue_count(state: &State, profile: &Profile) -> usize {
    crate::dialers::profile_issues(state, &profile.id).len()
        + dns_issues(state, profile).len()
        + ntp_issues(state, profile).len()
}

pub fn more_rows(
    state: &State,
    profile: &Profile,
) -> Vec<(&'static str, &'static str, &'static str, String, i32)> {
    let o = &profile.overrides;
    let dialers = crate::dialers::count(state, &profile.id);
    let dialer_issues = !crate::dialers::profile_issues(state, &profile.id).is_empty();
    let on = |b: bool| if b { "已开启" } else { "已关闭" }.to_string();
    let mut rows = vec![
        (
            "dialers",
            "link",
            "拨号代理",
            if dialers > 0 {
                format!("{dialers} 个代理")
            } else {
                "无".into()
            },
            if dialer_issues { 2 } else { 0 },
        ),
        (
            "ntp",
            "clock",
            "NTP",
            on(ntp_enabled(o)),
            if ntp_issues(state, profile).is_empty() {
                0
            } else {
                2
            },
        ),
        ("sniffer", "eye", "域名嗅探", on(sniffer_enabled(o)), 0),
    ];
    if shows_tun() {
        let n = tun_settings(o);
        rows.push((
            "tun",
            "shield",
            "虚拟网卡",
            if n > 0 {
                format!("{n} 项设置")
            } else {
                "默认".into()
            },
            0,
        ));
    }
    rows
}

pub fn tun_settings(o: &ProfileOverrides) -> usize {
    crate::tun_sniffer_ntp::TUN.picked(&o.tun).count()
}

/// `_Overview`'s rows: (id, glyph, title, state, tone), tone 0 plain, 1 in effect, 2 invalid.
pub fn overview(
    state: &State,
    profile: &Profile,
) -> Vec<(&'static str, &'static str, &'static str, String, i32)> {
    let o = &profile.overrides;
    let rule_count = rules(state, &profile.id).len();
    let rule_issues = crate::profile_rules::profile_issues(state, &profile.id).len();
    let (providers, missing) = referenced_providers(state, &profile.id);
    let dialers = crate::dialers::count(state, &profile.id);
    let more = [
        dialers > 0,
        ntp_enabled(o),
        sniffer_enabled(o),
        shows_tun() && tun_settings(o) > 0,
    ]
    .iter()
    .filter(|on| **on)
    .count();
    let more_invalid = !crate::dialers::profile_issues(state, &profile.id).is_empty()
        || !ntp_issues(state, profile).is_empty();
    let dns = dns_enabled(o);
    let dns_invalid = !dns_issues(state, profile).is_empty();
    vec![
        count_row("rules", "rules", "规则", rule_count, rule_issues),
        count_row("providers", "resources", "外部资源", providers, missing),
        (
            "dns",
            "dns",
            "DNS",
            if dns { "已开启" } else { "已关闭" }.into(),
            if dns_invalid {
                2
            } else if dns {
                1
            } else {
                0
            },
        ),
        (
            "more",
            "sliders",
            "更多",
            if more > 0 {
                format!("{more} 项生效")
            } else {
                "默认".into()
            },
            if more_invalid {
                2
            } else if more > 0 {
                1
            } else {
                0
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: u64, profile: &str, name: &str, order: Option<&str>) -> ProxyGroupRow {
        ProxyGroupRow {
            id,
            profile_id: profile.into(),
            definition: json!({"name": name, "type": "select", "proxies": ["a", "DIRECT"]}),
            order: order.map(str::to_string),
        }
    }

    fn custom_state() -> (State, Profile) {
        let mut state = State::default();
        let p = new_profile("9".into(), "未命名", 100);
        state.profiles.push(p.clone());
        (state, p)
    }

    #[test]
    fn a_new_custom_profile_takes_no_updates() {
        let p = new_profile("1".into(), "未命名", 42);
        assert!(p.is_custom() && !p.is_url() && !p.auto_update);
        assert_eq!(p.updated, 42);
        assert!(!p.due(u64::MAX / 2));
        let back: Profile = serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn older_state_reads_with_empty_tables() {
        let s: State =
            serde_json::from_str(r#"{"profiles":[{"id":"1","url":"https://a"}]}"#).unwrap();
        assert!(s.proxy_groups.is_empty() && s.profile_rules.is_empty());
        assert!(s.proxy_dialers.is_empty() && s.disabled_rules.is_empty());
        assert!(s.profiles[0].overrides.is_default());
    }

    #[test]
    fn rows_follow_their_order_with_unordered_last() {
        let (mut s, p) = custom_state();
        s.proxy_groups = vec![
            group(1, &p.id, "c", None),
            group(2, &p.id, "b", Some("a1")),
            group(3, "other", "x", Some("a0")),
            group(4, &p.id, "a", Some("a0")),
        ];
        let names: Vec<&str> = groups(&s, &p.id).iter().map(|g| g.name()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert_eq!(groups_count(&s, &p.id), 3);
    }

    #[test]
    fn removing_a_profile_cascades() {
        let (mut s, p) = custom_state();
        s.proxy_groups.push(group(1, &p.id, "g", None));
        s.proxy_groups.push(group(2, "keep", "g", None));
        s.profile_rules.push(ProfileRule {
            profile_id: p.id.clone(),
            ..Default::default()
        });
        s.proxy_dialers.push(ProxyDialer {
            profile_id: p.id.clone(),
            ..Default::default()
        });
        s.disabled_rules.push(DisabledRule {
            profile_id: p.id.clone(),
            rule_id: 3,
        });
        remove_profile(&mut s, &p.id);
        assert_eq!(s.proxy_groups.len(), 1);
        assert!(
            s.profile_rules.is_empty() && s.proxy_dialers.is_empty() && s.disabled_rules.is_empty()
        );
    }

    #[test]
    fn untouched_means_label_settings_groups_and_rules() {
        let (mut s, p) = custom_state();
        assert!(is_untouched(&s, &p.id, "未命名"));
        s.profiles[0].name = "新名字".into();
        assert!(!is_untouched(&s, &p.id, "未命名"));
        s.profiles[0].name = "未命名".into();
        s.profiles[0].overrides.ntp.keys.push("enable".into());
        assert!(!is_untouched(&s, &p.id, "未命名"));
        s.profiles[0].overrides = Default::default();
        s.proxy_groups.push(group(1, &p.id, "g", None));
        assert!(!is_untouched(&s, &p.id, "未命名"));
        assert!(!is_untouched(&s, "missing", "未命名"));
    }

    #[test]
    fn raw_rules_read_providers_and_sub_rules() {
        let r = |action: &str| ProfileRule {
            action: action.into(),
            content: "c".into(),
            target: "T".into(),
            rule_provider: "rp".into(),
            sub_rule: "sr".into(),
            no_resolve: true,
            ..Default::default()
        };
        assert_eq!(r("RULE-SET").raw(), "RULE-SET,rp,T,no-resolve");
        assert_eq!(r("SUB-RULE").raw(), "SUB-RULE,c,sr");
        assert_eq!(r("DOMAIN").raw(), "DOMAIN,c,T");
        let m = ProfileRule {
            action: "MATCH".into(),
            target: "G".into(),
            ..Default::default()
        };
        assert_eq!(m.raw(), "MATCH,G");
    }

    #[test]
    fn own_rules_replace_and_added_rules_only_fill_an_empty_list() {
        let (mut s, p) = custom_state();
        let mut c = Map::new();
        apply_own(&mut c, &own(&s, &p), vec!["DOMAIN,a,DIRECT".into()]);
        assert_eq!(c["rules"], json!(["DOMAIN,a,DIRECT"]));
        assert!(!c.contains_key("proxy-groups"));
        s.profile_rules.push(ProfileRule {
            id: 1,
            profile_id: p.id.clone(),
            action: "MATCH".into(),
            target: "g".into(),
            ..Default::default()
        });
        s.proxy_groups.push(group(1, &p.id, "g", None));
        let mut c = Map::new();
        c.insert("proxy-groups".into(), json!([{"name": "old"}]));
        apply_own(&mut c, &own(&s, &p), vec!["DOMAIN,a,DIRECT".into()]);
        assert_eq!(c["rules"], json!(["MATCH,g"]));
        assert_eq!(c["proxy-groups"][0]["name"], "g");
        assert_eq!(c["proxy-groups"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn other_profiles_bring_nothing_of_their_own() {
        let (mut s, _) = custom_state();
        let file = Profile {
            id: "5".into(),
            kind: "file".into(),
            ..Default::default()
        };
        s.proxy_groups.push(group(1, "5", "g", None));
        let o = own(&s, &file);
        assert!(o.groups.is_empty() && o.rules.is_empty() && o.overrides.is_none());
    }

    #[test]
    fn the_base_file_carries_the_proxies_the_groups_name() {
        let proxies = vec![
            crate::custom_proxies::CustomProxy {
                id: "1".into(),
                definition: json!({"name": "a", "type": "ss", "dialer-proxy": "x"}),
            },
            crate::custom_proxies::CustomProxy {
                id: "2".into(),
                definition: json!({"name": "unused", "type": "ss"}),
            },
        ];
        let own = Own {
            groups: vec![json!({"name": "g", "proxies": ["a", "DIRECT"]})],
            ..Default::default()
        };
        assert_eq!(
            base_file(&proxies, &own),
            json!({"proxies": [{"name": "a", "type": "ss"}]})
        );
        assert_eq!(base_file(&proxies, &Own::default()), json!({"proxies": []}));
    }

    #[test]
    fn profile_overrides_merge_over_the_sections() {
        let mut o = ProfileOverrides::default();
        o.ntp.keys = vec!["enable".into(), "server".into()];
        o.ntp.model.insert("enable".into(), json!(true));
        o.ntp
            .model
            .insert("server".into(), json!("time.example.com"));
        o.tun.keys = vec!["udp-timeout".into()];
        o.tun.model.insert("udp-timeout".into(), json!(60));
        let own = Own {
            overrides: Some(o.clone()),
            ..Default::default()
        };
        let mut c = Map::new();
        c.insert("ntp".into(), json!({"port": 123, "server": "old"}));
        c.insert("tun".into(), json!({"udp-timeout": 300, "stack": "mixed"}));
        apply_own(&mut c, &own, vec![]);
        assert_eq!(
            c["ntp"],
            json!({"enable": true, "port": 123, "server": "time.example.com"})
        );
        assert_eq!(c["tun"], json!({"udp-timeout": 60, "stack": "mixed"}));
        assert!(ntp_enabled(&o) && !sniffer_enabled(&o) && dns_enabled(&o));
    }

    #[test]
    fn a_custom_profile_takes_its_own_dns_not_the_app_page() {
        let mut global = OverrideState::default();
        global.keys = vec!["ipv6".into()];
        global.model.insert("ipv6".into(), json!(true));
        let mut o = ProfileOverrides::default();
        o.dns.keys = vec!["enable".into()];
        o.dns.model.insert("enable".into(), json!(false));
        let mut dns = Map::new();
        apply_dns(
            &Own {
                overrides: Some(o.clone()),
                ..Default::default()
            },
            &global,
            &mut dns,
        );
        assert_eq!(Value::Object(dns), json!({"enable": false}));
        assert!(!dns_enabled(&o));
        let mut dns = Map::new();
        apply_dns(&Own::default(), &global, &mut dns);
        assert_eq!(Value::Object(dns), json!({"ipv6": true}));
    }

    #[test]
    fn the_overview_counts_and_states() {
        let (mut s, p) = custom_state();
        let rows = overview(&s, &p);
        let states: Vec<(&str, i32)> = rows.iter().map(|r| (r.3.as_str(), r.4)).collect();
        assert_eq!(states, [("0", 0), ("0", 0), ("已开启", 1), ("默认", 0)]);
        s.profiles[0].overrides.sniffer.keys.push("enable".into());
        s.profiles[0]
            .overrides
            .sniffer
            .model
            .insert("enable".into(), json!(true));
        s.proxy_dialers.push(ProxyDialer {
            profile_id: p.id.clone(),
            proxy_id: "7".into(),
            target: "DIRECT".into(),
        });
        let p = s.profiles[0].clone();
        assert_eq!(overview(&s, &p)[3].3, "1 项生效");
        s.settings
            .custom_proxies
            .push(crate::custom_proxies::CustomProxy {
                id: "7".into(),
                definition: json!({"name": "a", "type": "ss"}),
            });
        s.proxy_groups.push(group(1, &p.id, "g", None));
        assert_eq!(overview(&s, &p)[3].3, "2 项生效");
    }

    #[test]
    fn edits_show_in_the_snapshot() {
        let (mut s, p) = custom_state();
        let before = snapshot(&s, &p.id);
        s.profiles[0].name = "x".into();
        assert_ne!(before, snapshot(&s, &p.id));
    }
}
