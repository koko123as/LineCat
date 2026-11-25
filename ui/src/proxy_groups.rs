//! A custom profile's proxy groups (lib/views/profiles/custom/groups.dart and the group
//! checks of lib/providers/state/custom.dart): types, the form's options, the config
//! definition a stored group gives, the issues the list and the form show, and loops.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::custom_profile::{self as cp, ProxyGroupRow};
use crate::store::State;

/// `reservedProxyNames`, also `RuleTarget.baseTargets`.
pub const RESERVED: &[&str] = &[
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "COMPATIBLE",
    "PASS",
    "PASS-RULE",
];

pub const DEFAULT_EMPTY_FALLBACK: &str = "COMPATIBLE";
const IN_USER_HASH_KEY: &str = "in-user";
const GLOBAL: &str = "GLOBAL";

/// `GroupType.selectableValues` (relay parses but the core refuses it): value and name.
pub const TYPES: &[(&str, &str)] = &[
    ("select", "Selector"),
    ("url-test", "URLTest"),
    ("fallback", "Fallback"),
    ("load-balance", "LoadBalance"),
    ("smart", "Smart"),
];

pub const STRATEGIES: &[&str] = &["consistent-hashing", "round-robin", "sticky-sessions"];

/// `GroupType.parse`, keeping what it does not know.
pub fn type_value(raw: &str) -> String {
    match raw.to_lowercase().as_str() {
        "url-test" | "urltest" => "url-test",
        "select" | "selector" => "select",
        "fallback" => "fallback",
        "load-balance" | "loadbalance" => "load-balance",
        "relay" => "relay",
        "smart" => "smart",
        _ => return raw.to_string(),
    }
    .to_string()
}

/// `type.name` as the list and the type sheet show it.
pub fn type_name(value: &str) -> String {
    let value = type_value(value);
    match value.as_str() {
        "relay" => "Relay".into(),
        v => TYPES
            .iter()
            .find(|(t, _)| *t == v)
            .map(|(_, n)| n.to_string())
            .unwrap_or(value),
    }
}

pub fn text(def: &Value, key: &str) -> String {
    match def.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

pub fn name(def: &Value) -> String {
    text(def, "name")
}

pub fn kind(def: &Value) -> String {
    type_value(&text(def, "type"))
}

pub fn names(def: &Value, key: &str) -> Vec<String> {
    def.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn flag(def: &Value, key: &str) -> Option<bool> {
    def.get(key).and_then(Value::as_bool)
}

/// `_readIncludeAll`: `include-all` stands for both.
fn includes_all(def: &Value, key: &str) -> bool {
    flag(def, key).or_else(|| flag(def, "include-all")) == Some(true)
}

pub fn strategy(def: &Value) -> Option<String> {
    def.get("strategy")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `effectiveUsePosition`: the providers placed before the end of `proxies`.
fn use_position(def: &Value) -> Option<Map<String, Value>> {
    let count = names(def, "proxies").len() as i64;
    let positions = def.get("use-position")?.as_object()?;
    let out: Map<String, Value> = names(def, "use")
        .into_iter()
        .filter_map(|n| {
            let p = match positions.get(&n)? {
                Value::Number(x) => x.as_i64()?,
                Value::String(s) => s.trim().parse().ok()?,
                _ => return None,
            };
            (p < count).then(|| (n, json!(p.max(0))))
        })
        .collect();
    (!out.is_empty()).then_some(out)
}

/// `ProxyGroupExt.definition`: only what the core reads of the group, by its type.
pub fn config_definition(def: &Value) -> Value {
    let t = kind(def);
    let mut out = Map::new();
    out.insert("name".into(), name(def).into());
    out.insert("type".into(), t.clone().into());
    for key in ["proxies", "use"] {
        let list = names(def, key);
        if !list.is_empty() {
            out.insert(key.into(), json!(list));
        }
    }
    if let Some(p) = use_position(def) {
        out.insert("use-position".into(), Value::Object(p));
    }
    let nonempty = |out: &mut Map<String, Value>, key: &str| {
        let v = text(def, key);
        if !v.is_empty() {
            out.insert(key.into(), v.into());
        }
    };
    nonempty(&mut out, "url");
    for key in ["interval", "timeout", "max-failed-times"] {
        if let Some(n) = def.get(key).and_then(Value::as_i64) {
            out.insert(key.into(), n.into());
        }
    }
    nonempty(&mut out, "empty-fallback");
    for key in ["lazy", "disable-udp"] {
        if let Some(b) = flag(def, key) {
            out.insert(key.into(), b.into());
        }
    }
    for key in [
        "filter",
        "exclude-filter",
        "exclude-type",
        "expected-status",
    ] {
        nonempty(&mut out, key);
    }
    for key in ["include-all-proxies", "include-all-providers"] {
        if includes_all(def, key) {
            out.insert(key.into(), true.into());
        }
    }
    if let Some(b) = flag(def, "hidden") {
        out.insert("hidden".into(), b.into());
    }
    nonempty(&mut out, "icon");
    if t == "url-test" || t == "smart" {
        if let Some(n) = def.get("tolerance").and_then(Value::as_i64) {
            out.insert("tolerance".into(), n.into());
        }
    }
    if t == "smart" {
        nonempty(&mut out, "policy-priority");
    }
    if t == "select" {
        nonempty(&mut out, "default-selected");
    }
    if t == "load-balance" {
        let s = strategy(def);
        if let Some(s) = &s {
            out.insert("strategy".into(), s.clone().into());
        }
        if s.as_deref() != Some("round-robin") {
            nonempty(&mut out, "hash-key");
        }
    }
    Value::Object(out)
}

/// `_GroupOption`: what a group leaves to the core until it is added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Opt {
    EmptyFallback,
    Url,
    Interval,
    Timeout,
    MaxFailedTimes,
    Lazy,
    ExpectedStatus,
    Tolerance,
    DefaultSelected,
    Strategy,
    HashKey,
    Hidden,
    DisableUdp,
}

pub const OPTS: [Opt; 13] = [
    Opt::EmptyFallback,
    Opt::Url,
    Opt::Interval,
    Opt::Timeout,
    Opt::MaxFailedTimes,
    Opt::Lazy,
    Opt::ExpectedStatus,
    Opt::Tolerance,
    Opt::DefaultSelected,
    Opt::Strategy,
    Opt::HashKey,
    Opt::Hidden,
    Opt::DisableUdp,
];

impl Opt {
    pub fn key(self) -> &'static str {
        match self {
            Opt::EmptyFallback => "empty-fallback",
            Opt::Url => "url",
            Opt::Interval => "interval",
            Opt::Timeout => "timeout",
            Opt::MaxFailedTimes => "max-failed-times",
            Opt::Lazy => "lazy",
            Opt::ExpectedStatus => "expected-status",
            Opt::Tolerance => "tolerance",
            Opt::DefaultSelected => "default-selected",
            Opt::Strategy => "strategy",
            Opt::HashKey => "hash-key",
            Opt::Hidden => "hidden",
            Opt::DisableUdp => "disable-udp",
        }
    }

    pub fn of(key: &str) -> Option<Opt> {
        OPTS.into_iter().find(|o| o.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Opt::EmptyFallback => "空组兜底",
            Opt::Url => "测速链接",
            Opt::Interval => "测试间隔",
            Opt::Timeout => "超时",
            Opt::MaxFailedTimes => "最大失败次数",
            Opt::Lazy => "使用时测试",
            Opt::ExpectedStatus => "预期状态",
            Opt::Tolerance => "容差",
            Opt::DefaultSelected => "默认选中",
            Opt::Strategy => "策略",
            Opt::HashKey => "按入站用户哈希",
            Opt::Hidden => "从列表中隐藏",
            Opt::DisableUdp => "禁用UDP",
        }
    }

    pub fn section(self) -> &'static str {
        match self {
            Opt::Url
            | Opt::Interval
            | Opt::Timeout
            | Opt::MaxFailedTimes
            | Opt::Lazy
            | Opt::ExpectedStatus
            | Opt::Tolerance => "健康检查",
            _ => "其他",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Opt::Interval => "s",
            Opt::Timeout | Opt::Tolerance => "ms",
            _ => "",
        }
    }

    pub fn is_number(self) -> bool {
        matches!(
            self,
            Opt::Interval | Opt::Timeout | Opt::MaxFailedTimes | Opt::Tolerance
        )
    }

    pub fn is_toggle(self) -> bool {
        matches!(
            self,
            Opt::Lazy | Opt::HashKey | Opt::Hidden | Opt::DisableUdp
        )
    }

    pub fn applies_to(self, kind: &str, strategy: Option<&str>) -> bool {
        match self {
            Opt::Tolerance => kind == "url-test" || kind == "smart",
            Opt::DefaultSelected => kind == "select",
            Opt::Strategy => kind == "load-balance",
            Opt::HashKey => kind == "load-balance" && strategy != Some("round-robin"),
            _ => true,
        }
    }

    pub fn is_set_in(self, def: &Value) -> bool {
        def.get(self.key()).is_some_and(|v| !v.is_null())
    }

    /// `reset`: the core's own default, set when the option is added.
    pub fn reset_value(self, test_url: &str) -> Value {
        match self {
            Opt::Url => test_url.into(),
            Opt::Interval => 300.into(),
            Opt::Timeout => 5000.into(),
            Opt::MaxFailedTimes => 5.into(),
            Opt::Lazy => true.into(),
            Opt::ExpectedStatus | Opt::DefaultSelected | Opt::HashKey => "".into(),
            Opt::Tolerance => 0.into(),
            Opt::Strategy => "consistent-hashing".into(),
            Opt::Hidden | Opt::DisableUdp => false.into(),
            Opt::EmptyFallback => DEFAULT_EMPTY_FALLBACK.into(),
        }
    }

    /// The row's value as the form shows it.
    pub fn shown(self, def: &Value) -> String {
        match self {
            Opt::EmptyFallback => match text(def, self.key()) {
                v if v.is_empty() => DEFAULT_EMPTY_FALLBACK.into(),
                v => v,
            },
            Opt::DefaultSelected => match text(def, self.key()) {
                v if v.is_empty() => "默认".into(),
                v => v,
            },
            Opt::Strategy => strategy(def).unwrap_or_else(|| "consistent-hashing".into()),
            _ => match text(def, self.key()) {
                v if v.is_empty() => "可选".into(),
                v if self.suffix().is_empty() => v,
                v => format!("{v} {}", self.suffix()),
            },
        }
    }

    /// A switch's state: `lazy` is on unless set off, `hash-key` on when by user.
    pub fn on(self, def: &Value) -> bool {
        match self {
            Opt::Lazy => flag(def, "lazy") != Some(false),
            Opt::HashKey => text(def, "hash-key") == IN_USER_HASH_KEY,
            _ => flag(def, self.key()) == Some(true),
        }
    }

    pub fn toggled(self, on: bool) -> Value {
        match self {
            Opt::HashKey => if on { IN_USER_HASH_KEY } else { "" }.into(),
            _ => on.into(),
        }
    }
}

/// What a group's check reads of the profile (`CustomProfileData`).
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub groups: Vec<(u64, Value)>,
    pub proxy_providers: BTreeSet<String>,
    /// The custom proxies' names.
    pub proxies: BTreeSet<String>,
    /// The custom proxies' types by name, for the members' subtitles.
    pub proxy_types: BTreeMap<String, String>,
}

impl Data {
    pub fn rule_targets(&self) -> BTreeSet<String> {
        RESERVED
            .iter()
            .map(|s| s.to_string())
            .chain(self.groups.iter().map(|(_, g)| name(g)))
            .collect()
    }

    /// `namedProxies`: the custom proxies some group names.
    pub fn named_proxies(&self) -> BTreeSet<String> {
        self.groups
            .iter()
            .flat_map(|(_, g)| names(g, "proxies"))
            .filter(|n| self.proxies.contains(n))
            .collect()
    }

    pub fn is_rule_target(&self, n: &str) -> bool {
        n == GLOBAL || self.rule_targets().contains(n) || self.named_proxies().contains(n)
    }
}

/// `customProfileData`; subscription and file profiles stand in as proxy providers.
pub fn data(state: &State, profile_id: &str) -> Data {
    Data {
        groups: cp::groups(state, profile_id)
            .into_iter()
            .map(|g| (g.id, g.definition.clone()))
            .collect(),
        proxy_providers: state
            .profiles
            .iter()
            .filter(|p| !p.is_custom())
            .map(|p| p.label())
            .collect(),
        proxies: state
            .settings
            .custom_proxies
            .iter()
            .map(|p| p.name())
            .collect(),
        proxy_types: state
            .settings
            .custom_proxies
            .iter()
            .map(|p| (p.name(), p.kind()))
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Issue {
    EmptyName,
    ReservedName(String),
    DuplicateName(String),
    NoProxySource,
    MissingProxies(Vec<String>),
    MissingProviders(Vec<String>),
    GroupLoop(Vec<String>),
    InvalidEmptyFallback(String),
    InvalidFilter(String, String),
}

impl Issue {
    pub fn message(&self) -> String {
        match self {
            Issue::EmptyName => "名称为空".into(),
            Issue::ReservedName(n) => format!("{n} 是内置策略名，不能在此使用"),
            Issue::DuplicateName(n) => format!("名称 {n} 已被其他代理或策略组使用"),
            Issue::NoProxySource => "未选择任何代理或代理集，内核会拒绝该策略组".into(),
            Issue::MissingProxies(v) => format!("以下代理或策略不存在：{}", v.join(", ")),
            Issue::MissingProviders(v) => format!("以下代理集不存在：{}", v.join(", ")),
            Issue::GroupLoop(v) => format!("策略组之间存在循环引用：{}", v.join(" › ")),
            Issue::InvalidEmptyFallback(n) => {
                format!("空组兜底只能是代理，{n} 不存在或是策略组")
            }
            Issue::InvalidFilter(t, m) => format!("{t}不是有效的正则：{m}"),
        }
    }

    /// A new form holds these back until the first save.
    pub fn is_incomplete(&self) -> bool {
        matches!(self, Issue::EmptyName | Issue::NoProxySource)
    }

    /// `_blocksSave`: a missing member is left to the list, since a profile update brings it.
    pub fn blocks_save(&self) -> bool {
        !matches!(self, Issue::MissingProxies(_) | Issue::MissingProviders(_))
    }

    pub fn is_name(&self) -> bool {
        matches!(
            self,
            Issue::EmptyName | Issue::ReservedName(_) | Issue::DuplicateName(_)
        )
    }
}

pub fn issues_message(issues: &[Issue]) -> String {
    match issues {
        [one] => one.message(),
        _ => issues
            .iter()
            .map(|i| format!("• {}", i.message()))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// `_loopFrom`: the path from `start` back to itself through the groups' members.
pub fn loop_from(start: &str, members: &HashMap<String, Vec<String>>) -> Option<Vec<String>> {
    fn walk(
        start: &str,
        name: &str,
        path: &mut Vec<String>,
        members: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) -> bool {
        for next in members.get(name).into_iter().flatten() {
            if next == start {
                path.push(next.clone());
                return true;
            }
            if !members.contains_key(next) || !visited.insert(next.clone()) {
                continue;
            }
            path.push(next.clone());
            if walk(start, next, path, members, visited) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = vec![start.to_string()];
    let mut visited = HashSet::new();
    walk(start, start, &mut path, members, &mut visited).then_some(path)
}

/// `proxyGroupLoop`: the group as edited, in place of its stored self.
pub fn group_loop(id: u64, def: &Value, groups: &[(u64, Value)]) -> Option<Vec<String>> {
    let mut members: HashMap<String, Vec<String>> = groups
        .iter()
        .filter(|(i, _)| *i != id)
        .map(|(_, g)| (name(g), names(g, "proxies")))
        .collect();
    let own = name(def);
    members.insert(own.clone(), names(def, "proxies"));
    loop_from(&own, &members)
}

/// `_namesOnLoops`: the groups on some loop, from Tarjan's components.
pub fn names_on_loops(members: &[(String, Vec<String>)]) -> BTreeSet<String> {
    let ids: HashMap<&str, usize> = members
        .iter()
        .enumerate()
        .map(|(i, (n, _))| (n.as_str(), i))
        .collect();
    let edges: Vec<Vec<usize>> = members
        .iter()
        .map(|(_, items)| {
            items
                .iter()
                .filter_map(|m| ids.get(m.as_str()).copied())
                .collect()
        })
        .collect();
    let component = components(&edges);
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for c in &component {
        *sizes.entry(*c).or_default() += 1;
    }
    members
        .iter()
        .enumerate()
        .filter(|(i, _)| sizes[&component[*i]] > 1 || edges[*i].contains(i))
        .map(|(_, (n, _))| n.clone())
        .collect()
}

/// `_componentsOf`, iterative so a long chain of groups keeps the stack small.
fn components(edges: &[Vec<usize>]) -> Vec<usize> {
    let n = edges.len();
    let (mut index, mut low, mut comp) = (vec![usize::MAX; n], vec![0; n], vec![usize::MAX; n]);
    let (mut stack, mut on_stack) = (Vec::new(), vec![false; n]);
    let (mut visited, mut closed) = (0, 0);
    for root in 0..n {
        if index[root] != usize::MAX {
            continue;
        }
        let mut calls: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = visited;
        low[root] = visited;
        visited += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (node, ref mut next)) = calls.last_mut() {
            if let Some(&to) = edges[node].get(*next) {
                *next += 1;
                if index[to] == usize::MAX {
                    index[to] = visited;
                    low[to] = visited;
                    visited += 1;
                    stack.push(to);
                    on_stack[to] = true;
                    calls.push((to, 0));
                } else if on_stack[to] {
                    low[node] = low[node].min(index[to]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(parent, _)) = calls.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                while let Some(item) = stack.pop() {
                    on_stack[item] = false;
                    comp[item] = closed;
                    if item == node {
                        break;
                    }
                }
                closed += 1;
            }
        }
    }
    comp
}

/// `proxyGroupIssues`, for the group as edited (`id` 0 for a new one).
pub fn issues(id: u64, def: &Value, data: &Data) -> Vec<Issue> {
    issues_with(id, def, data, None)
}

fn issues_with(
    id: u64,
    def: &Value,
    data: &Data,
    looping: Option<&BTreeSet<String>>,
) -> Vec<Issue> {
    let n = name(def);
    let proxies = names(def, "proxies");
    let used = names(def, "use");
    let has_source = !proxies.is_empty()
        || !used.is_empty()
        || includes_all(def, "include-all-proxies")
        || (includes_all(def, "include-all-providers")
            && data.groups.iter().any(|(i, g)| {
                *i != id
                    && names(g, "use")
                        .iter()
                        .any(|u| data.proxy_providers.contains(u))
            }));
    let targets = data.rule_targets();
    let missing: Vec<String> = proxies
        .iter()
        .filter(|p| !targets.contains(*p) && !data.proxies.contains(*p))
        .cloned()
        .collect();
    let fallback = text(def, "empty-fallback");
    let invalid_fallback = !fallback.is_empty()
        && !RESERVED.contains(&fallback.as_str())
        && !data.named_proxies().contains(&fallback);
    let missing_providers: Vec<String> = used
        .iter()
        .filter(|u| !data.proxy_providers.contains(*u))
        .cloned()
        .collect();
    let taken =
        data.proxies.contains(&n) || data.groups.iter().any(|(i, g)| *i != id && name(g) == n);
    let lp = match looping {
        Some(set) if !set.contains(&n) => None,
        _ => group_loop(id, def, &data.groups),
    };
    let mut out = Vec::new();
    if n.is_empty() {
        out.push(Issue::EmptyName);
    } else if RESERVED.contains(&n.as_str()) {
        out.push(Issue::ReservedName(n.clone()));
    } else if taken {
        out.push(Issue::DuplicateName(n.clone()));
    }
    if !has_source {
        out.push(Issue::NoProxySource);
    }
    if !missing.is_empty() {
        out.push(Issue::MissingProxies(missing));
    }
    if !missing_providers.is_empty() {
        out.push(Issue::MissingProviders(missing_providers));
    }
    if let Some(lp) = lp {
        out.push(Issue::GroupLoop(lp));
    }
    if invalid_fallback {
        out.push(Issue::InvalidEmptyFallback(fallback));
    }
    out
}

/// `customProfileIssues.proxyGroups`: each stored group with issues, by id.
pub fn profile_issues(data: &Data) -> BTreeMap<u64, Vec<Issue>> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, g) in &data.groups {
        *counts.entry(name(g)).or_default() += 1;
    }
    let mut members: Vec<(String, Vec<String>)> = Vec::new();
    for (_, g) in &data.groups {
        let n = name(g);
        match members.iter_mut().find(|(m, _)| *m == n) {
            Some(slot) => slot.1 = names(g, "proxies"),
            None => members.push((n, names(g, "proxies"))),
        }
    }
    let looping = names_on_loops(&members);
    data.groups
        .iter()
        .filter_map(|(id, g)| {
            let unique = counts.get(&name(g)) == Some(&1);
            let found = issues_with(*id, g, data, unique.then_some(&looping));
            (!found.is_empty()).then_some((*id, found))
        })
        .collect()
}

/// Every server the DNS keys list, for `dnsProxies`.
fn dns_servers(state: &State, profile_id: &str) -> Vec<String> {
    let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut take = |v: &Value| match v {
        Value::String(s) => out.extend(s.split(',').map(|s| s.trim().to_string())),
        Value::Array(a) => out.extend(a.iter().filter_map(Value::as_str).map(str::to_string)),
        _ => {}
    };
    for key in [
        "default-nameserver",
        "nameserver",
        "fallback",
        "proxy-server-nameserver",
        "direct-nameserver",
    ] {
        if let Some(v) = cp::PROFILE_DNS.picked_value(&p.overrides.dns, key) {
            take(&v);
        }
    }
    for key in ["nameserver-policy", "proxy-server-nameserver-policy"] {
        if let Some(Value::Object(m)) = cp::PROFILE_DNS.picked_value(&p.overrides.dns, key) {
            m.values().for_each(&mut take);
        }
    }
    out
}

/// The proxy a DNS server dials through: its `#` part that is not a `key=value`.
fn dns_dialer(server: &str) -> Option<String> {
    let (_, tail) = server.split_once('#')?;
    tail.split('&')
        .filter(|p| !p.contains('='))
        .last()
        .filter(|p| !p.is_empty())
        .map(str::to_string)
}

/// `groupsInUse` with `proxyNamesInUse`: the names of `ids` that the profile's other groups,
/// rules, dialers, DNS or NTP still name.
pub fn in_use(state: &State, profile_id: &str, ids: &BTreeSet<u64>) -> Vec<String> {
    let groups = cp::groups(state, profile_id);
    let names_of: Vec<String> = groups
        .iter()
        .filter(|g| ids.contains(&g.id))
        .map(|g| g.name().to_string())
        .collect();
    let mut referenced: HashSet<String> = HashSet::new();
    for g in groups.iter().filter(|g| !ids.contains(&g.id)) {
        referenced.extend(names(&g.definition, "proxies"));
        let selected = text(&g.definition, "default-selected");
        if !selected.is_empty() {
            referenced.insert(selected);
        }
    }
    referenced.extend(
        cp::rules(state, profile_id)
            .iter()
            .map(|r| r.target.clone()),
    );
    referenced.extend(
        state
            .proxy_dialers
            .iter()
            .filter(|d| d.profile_id == profile_id)
            .map(|d| d.target.clone()),
    );
    referenced.extend(
        dns_servers(state, profile_id)
            .iter()
            .filter_map(|s| dns_dialer(s)),
    );
    if let Some(p) = state.profiles.iter().find(|p| p.id == profile_id) {
        if let Some(Value::String(d)) =
            crate::tun_sniffer_ntp::NTP.picked_value(&p.overrides.ntp, "dialer-proxy")
        {
            referenced.insert(d);
        }
    }
    let mut out: Vec<String> = Vec::new();
    for n in names_of {
        if referenced.contains(&n) && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

/// `copiedProxyName`: the name without its number, numbered from 01 apart from `taken`.
pub fn copied_name(n: &str, taken: &mut HashSet<String>) -> String {
    let base = match n.rfind('-') {
        Some(i) if n.len() - i > 2 && n[i + 1..].bytes().all(|b| b.is_ascii_digit()) => &n[..i],
        _ => n,
    };
    let mut index = 1;
    loop {
        let candidate = format!("{base}-{index:02}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

/// Every row of the profile in the order given, renumbered so the order stays as shown.
pub fn renumber(rows: &mut [ProxyGroupRow], profile_id: &str, ordered: &[u64]) {
    for (i, id) in ordered.iter().enumerate() {
        if let Some(r) = rows
            .iter_mut()
            .find(|r| r.id == *id && r.profile_id == profile_id)
        {
            r.order = Some(format!("{i:08}"));
        }
    }
}

/// `insertAfterEach` for 复制: a copy of each of `ids` right after it.
pub fn duplicate(
    state: &mut State,
    profile_id: &str,
    ids: &BTreeSet<u64>,
    mut new_id: impl FnMut() -> u64,
) {
    let current: Vec<ProxyGroupRow> = cp::groups(state, profile_id).into_iter().cloned().collect();
    let mut taken: HashSet<String> = RESERVED.iter().map(|s| s.to_string()).collect();
    taken.extend(state.settings.custom_proxies.iter().map(|p| p.name()));
    taken.extend(current.iter().map(|g| g.name().to_string()));
    let mut order = Vec::new();
    for g in &current {
        order.push(g.id);
        if !ids.contains(&g.id) {
            continue;
        }
        let mut copy = g.clone();
        copy.id = new_id();
        if let Some(m) = copy.definition.as_object_mut() {
            m.insert("name".into(), copied_name(g.name(), &mut taken).into());
        }
        order.push(copy.id);
        state.proxy_groups.push(copy);
    }
    renumber(&mut state.proxy_groups, profile_id, &order);
}

/// `order(old, new)` from the sort list.
pub fn reorder(state: &mut State, profile_id: &str, from: usize, to: usize) {
    let mut order: Vec<u64> = cp::groups(state, profile_id).iter().map(|g| g.id).collect();
    if from >= order.len() || to >= order.len() {
        return;
    }
    let id = order.remove(from);
    order.insert(to, id);
    renumber(&mut state.proxy_groups, profile_id, &order);
}

/// `put`: false when another group of the profile already has the name.
pub fn put(state: &mut State, profile_id: &str, id: u64, def: Value) -> bool {
    let n = name(&def);
    if cp::groups(state, profile_id)
        .iter()
        .any(|g| g.id != id && g.name() == n)
    {
        return false;
    }
    match state
        .proxy_groups
        .iter_mut()
        .find(|g| g.id == id && g.profile_id == profile_id)
    {
        Some(g) => g.definition = def,
        None => {
            let mut order: Vec<u64> = cp::groups(state, profile_id).iter().map(|g| g.id).collect();
            state.proxy_groups.push(ProxyGroupRow {
                id,
                profile_id: profile_id.into(),
                definition: def,
                order: None,
            });
            order.push(id);
            renumber(&mut state.proxy_groups, profile_id, &order);
        }
    }
    true
}

pub fn delete(state: &mut State, profile_id: &str, ids: &BTreeSet<u64>) {
    state
        .proxy_groups
        .retain(|g| g.profile_id != profile_id || !ids.contains(&g.id));
}

/// A member row of the form (`ProxyGroupMembersSliver`, read only here): kind, name.
#[derive(Clone, Debug, PartialEq)]
pub enum Member {
    Proxy(String),
    Provider(String),
    AllProxies,
    AllProviders,
    Filter(String),
    ExcludeFilter(String),
}

/// `filterParts`: mihomo splits a filter on backticks.
pub fn filter_parts(v: &str) -> Vec<String> {
    v.split('`')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// `membersOf` without a layout: the proxies with the providers at their positions, then
/// the include-all keys and the filters.
pub fn members(def: &Value) -> Vec<Member> {
    let proxies = names(def, "proxies");
    let positions = use_position(def).unwrap_or_default();
    let mut before: Vec<Vec<Member>> = vec![Vec::new(); proxies.len() + 1];
    for u in names(def, "use") {
        let at = positions
            .get(&u)
            .and_then(Value::as_u64)
            .map(|p| p as usize)
            .unwrap_or(proxies.len())
            .min(proxies.len());
        before[at].push(Member::Provider(u));
    }
    let mut out = Vec::new();
    for (i, providers) in before.into_iter().enumerate() {
        out.extend(providers);
        if let Some(p) = proxies.get(i) {
            out.push(Member::Proxy(p.clone()));
        }
    }
    if includes_all(def, "include-all-proxies") {
        out.push(Member::AllProxies);
    }
    if includes_all(def, "include-all-providers") {
        out.push(Member::AllProviders);
    }
    out.extend(
        filter_parts(&text(def, "filter"))
            .into_iter()
            .map(Member::Filter),
    );
    out.extend(
        filter_parts(&text(def, "exclude-filter"))
            .into_iter()
            .map(Member::ExcludeFilter),
    );
    out
}

/// `GroupMonogram`: the first letter or digit, upper-cased.
pub fn monogram(n: &str) -> String {
    n.chars()
        .find(|c| c.is_alphanumeric())
        .or_else(|| n.chars().next())
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

/// The keys of [config_definition] in the order `definition` writes them.
const KEY_ORDER: [&str; 25] = [
    "name",
    "type",
    "proxies",
    "use",
    "use-position",
    "url",
    "interval",
    "timeout",
    "max-failed-times",
    "empty-fallback",
    "lazy",
    "disable-udp",
    "filter",
    "exclude-filter",
    "exclude-type",
    "expected-status",
    "include-all-proxies",
    "include-all-providers",
    "hidden",
    "icon",
    "tolerance",
    "policy-priority",
    "default-selected",
    "strategy",
    "hash-key",
];

/// `definitionYaml`: the config definition as block YAML.
pub fn to_yaml(def: &Value) -> String {
    let mut out = String::new();
    if let Value::Object(mut m) = config_definition(def) {
        for k in KEY_ORDER {
            let Some(v) = m.remove(k) else {
                continue;
            };
            match v {
                Value::Array(items) => {
                    out.push_str(&format!("{k}:\n"));
                    for item in items {
                        out.push_str(&format!("  - {}\n", scalar(&item)));
                    }
                }
                Value::Object(map) => {
                    out.push_str(&format!("{k}:\n"));
                    for (mk, mv) in map {
                        out.push_str(&format!("  {}: {}\n", quote(&mk), scalar(&mv)));
                    }
                }
                v => out.push_str(&format!("{k}: {}\n", scalar(&v))),
            }
        }
    }
    out
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => quote(s),
        other => other.to_string(),
    }
}

/// A plain scalar when YAML reads it back as the same string, else double quoted.
fn quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.trim() == s
        && !s.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.ends_with(':')
        && !s.chars().any(char::is_control)
        && !matches!(
            s.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~" | "y" | "n"
        )
        && s.parse::<f64>().is_err();
    if plain {
        s.to_string()
    } else {
        serde_json::to_string(s).unwrap_or_default()
    }
}

/// What quick edit wraps the text in so the Core's `config.read` parses it as a group.
pub fn yaml_document(text: &str) -> String {
    let mut out = String::from("proxy-groups:\n");
    let mut first = true;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str(if first { "  - " } else { "    " });
        out.push_str(line);
        out.push('\n');
        first = false;
    }
    out
}

/// `withDefinitionYaml`: the parsed mapping, refused unless it has a name and a type. What
/// the text never showed of `old`, such as an option added without a value, stays.
pub fn from_parsed(parsed: &Value, old: &Value) -> Result<Value, String> {
    let group = parsed
        .get("proxy-groups")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_object);
    let Some(group) = group.filter(|g| {
        matches!(g.get("name"), Some(Value::String(_) | Value::Number(_)))
            && matches!(g.get("type"), Some(Value::String(_)))
    }) else {
        return Err("配置必须是包含 name 和 type 的 YAML 映射".into());
    };
    let shown = config_definition(old);
    let mut m: Map<String, Value> = old
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| shown.get(k.as_str()).is_none() && k.as_str() != "use-position")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    m.extend(group.clone());
    for key in [
        "name",
        "url",
        "filter",
        "exclude-filter",
        "exclude-type",
        "expected-status",
        "hash-key",
        "policy-priority",
        "default-selected",
        "empty-fallback",
        "icon",
    ] {
        if let Some(v) = m.get(key).filter(|v| !v.is_string() && !v.is_null()) {
            let s = match v {
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            m.insert(key.into(), s.into());
        }
    }
    for key in ["proxies", "use"] {
        if let Some(Value::Array(a)) = m.get(key) {
            let list: Vec<Value> = a
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.clone().into(),
                    other => other.to_string().into(),
                })
                .collect();
            m.insert(key.into(), Value::Array(list));
        }
    }
    let t = type_value(m.get("type").and_then(Value::as_str).unwrap_or_default());
    m.insert("type".into(), t.into());
    Ok(Value::Object(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(id: u64, def: Value) -> (u64, Value) {
        (id, def)
    }

    fn data_of(groups: Vec<(u64, Value)>) -> Data {
        Data {
            groups,
            proxies: ["hk".to_string(), "jp".to_string()].into(),
            proxy_providers: ["sub".to_string()].into(),
            ..Default::default()
        }
    }

    #[test]
    fn types_parse_and_show_their_names() {
        assert_eq!(type_value("URLTest"), "url-test");
        assert_eq!(type_value("selector"), "select");
        assert_eq!(type_name("load-balance"), "LoadBalance");
        assert_eq!(type_name("relay"), "Relay");
        assert_eq!(type_name("odd"), "odd");
    }

    #[test]
    fn the_config_definition_keeps_what_the_type_reads() {
        let def = json!({
            "name": "G", "type": "select", "proxies": ["a"], "use": [],
            "tolerance": 50, "default-selected": "a", "url": "", "expected-status": "",
            "strategy": "round-robin", "hash-key": "in-user", "lazy": false, "extra": 1,
            "include-all": true
        });
        assert_eq!(
            config_definition(&def),
            json!({
                "name": "G", "type": "select", "proxies": ["a"], "lazy": false,
                "include-all-proxies": true, "include-all-providers": true,
                "default-selected": "a"
            })
        );
        let lb = json!({"name": "L", "type": "load-balance", "strategy": "round-robin", "hash-key": "in-user"});
        assert_eq!(config_definition(&lb).get("hash-key"), None);
        let lb =
            json!({"name": "L", "type": "load-balance", "hash-key": "in-user", "tolerance": 1});
        assert_eq!(config_definition(&lb)["hash-key"], "in-user");
        assert_eq!(config_definition(&lb).get("tolerance"), None);
        let ut = json!({"name": "U", "type": "url-test", "tolerance": 10, "use": ["p"], "proxies": ["a"], "use-position": {"p": 0, "q": 0}});
        assert_eq!(config_definition(&ut)["tolerance"], 10);
        assert_eq!(config_definition(&ut)["use-position"], json!({"p": 0}));
    }

    #[test]
    fn options_apply_by_type_and_strategy() {
        assert!(Opt::Tolerance.applies_to("smart", None));
        assert!(!Opt::Tolerance.applies_to("select", None));
        assert!(Opt::DefaultSelected.applies_to("select", None));
        assert!(Opt::HashKey.applies_to("load-balance", None));
        assert!(!Opt::HashKey.applies_to("load-balance", Some("round-robin")));
        assert_eq!(Opt::Url.reset_value("u"), json!("u"));
        assert_eq!(Opt::Interval.shown(&json!({"interval": 300})), "300 s");
        assert_eq!(Opt::Interval.shown(&json!({})), "可选");
        assert_eq!(
            Opt::EmptyFallback.shown(&json!({"empty-fallback": ""})),
            "COMPATIBLE"
        );
        assert!(Opt::Lazy.on(&json!({})));
        assert!(!Opt::Lazy.on(&json!({"lazy": false})));
        assert!(Opt::HashKey.on(&json!({"hash-key": "in-user"})));
        assert_eq!(Opt::HashKey.toggled(false), json!(""));
        assert!(Opt::ExpectedStatus.is_set_in(&json!({"expected-status": ""})));
        assert_eq!(Opt::of("disable-udp"), Some(Opt::DisableUdp));
    }

    #[test]
    fn names_missing_members_and_sources_are_issues() {
        let data = data_of(vec![
            g(
                1,
                json!({"name": "A", "type": "select", "proxies": ["hk", "B", "gone"], "use": ["sub", "nope"]}),
            ),
            g(
                2,
                json!({"name": "B", "type": "select", "proxies": ["DIRECT"]}),
            ),
        ]);
        let found = issues(1, &data.groups[0].1, &data);
        assert_eq!(
            found,
            [
                Issue::MissingProxies(vec!["gone".into()]),
                Issue::MissingProviders(vec!["nope".into()])
            ]
        );
        assert_eq!(
            issues(0, &json!({"name": "", "type": "select"}), &data),
            [Issue::EmptyName, Issue::NoProxySource]
        );
        assert_eq!(
            issues(
                0,
                &json!({"name": "DIRECT", "type": "select", "proxies": ["hk"]}),
                &data
            ),
            [Issue::ReservedName("DIRECT".into())]
        );
        assert_eq!(
            issues(
                0,
                &json!({"name": "B", "type": "select", "proxies": ["hk"]}),
                &data
            ),
            [Issue::DuplicateName("B".into())]
        );
        assert_eq!(
            issues(
                0,
                &json!({"name": "hk", "type": "select", "proxies": ["jp"]}),
                &data
            ),
            [Issue::DuplicateName("hk".into())]
        );
        assert!(issues(
            2,
            &json!({"name": "B", "type": "select", "include-all-proxies": true}),
            &data
        )
        .is_empty());
        assert_eq!(
            issues(
                0,
                &json!({"name": "C", "type": "select", "include-all-providers": true}),
                &data
            ),
            []
        );
        assert_eq!(
            issues(
                1,
                &json!({"name": "C", "type": "select", "include-all-providers": true}),
                &data
            ),
            [Issue::NoProxySource]
        );
    }

    #[test]
    fn the_empty_fallback_takes_a_named_proxy_only() {
        let data = data_of(vec![g(
            1,
            json!({"name": "A", "type": "select", "proxies": ["hk"]}),
        )]);
        let with = |v: &str| {
            issues(
                0,
                &json!({"name": "C", "type": "select", "proxies": ["DIRECT"], "empty-fallback": v}),
                &data,
            )
        };
        assert!(with("hk").is_empty());
        assert!(with("REJECT").is_empty());
        assert!(with("").is_empty());
        assert_eq!(with("jp"), [Issue::InvalidEmptyFallback("jp".into())]);
        assert_eq!(with("A"), [Issue::InvalidEmptyFallback("A".into())]);
    }

    #[test]
    fn loops_are_found_with_their_path() {
        let groups = vec![
            g(1, json!({"name": "A", "type": "select", "proxies": ["B"]})),
            g(
                2,
                json!({"name": "B", "type": "select", "proxies": ["C", "hk"]}),
            ),
            g(3, json!({"name": "C", "type": "select", "proxies": ["A"]})),
            g(4, json!({"name": "D", "type": "select", "proxies": ["A"]})),
            g(5, json!({"name": "E", "type": "select", "proxies": ["E"]})),
        ];
        assert_eq!(
            group_loop(1, &groups[0].1, &groups),
            Some(vec!["A".into(), "B".into(), "C".into(), "A".into()])
        );
        assert_eq!(group_loop(4, &groups[3].1, &groups), None);
        let members: Vec<(String, Vec<String>)> = groups
            .iter()
            .map(|(_, d)| (name(d), names(d, "proxies")))
            .collect();
        let on: Vec<String> = names_on_loops(&members).into_iter().collect();
        assert_eq!(on, ["A", "B", "C", "E"]);
        let edited = json!({"name": "C", "type": "select", "proxies": ["hk"]});
        assert_eq!(group_loop(3, &edited, &groups), None);
        let data = data_of(groups);
        let all = profile_issues(&data);
        assert_eq!(all.keys().copied().collect::<Vec<_>>(), [1, 2, 3, 5]);
        assert_eq!(all[&5], [Issue::GroupLoop(vec!["E".into(), "E".into()])]);
        assert!(issues_message(&all[&1]).starts_with("策略组之间存在循环引用：A › B"));
    }

    #[test]
    fn a_long_chain_checks_without_deep_recursion() {
        let groups: Vec<(u64, Value)> = (0..3000u64)
            .map(|i| g(i + 1, json!({"name": format!("g{i}"), "type": "select", "proxies": [format!("g{}", i + 1)]})))
            .collect();
        let members: Vec<(String, Vec<String>)> = groups
            .iter()
            .map(|(_, d)| (name(d), names(d, "proxies")))
            .collect();
        assert!(names_on_loops(&members).is_empty());
    }

    #[test]
    fn issues_split_by_what_blocks_saving() {
        assert!(Issue::GroupLoop(vec![]).blocks_save());
        assert!(!Issue::MissingProxies(vec![]).blocks_save());
        assert!(Issue::NoProxySource.is_incomplete());
        assert!(!Issue::DuplicateName("a".into()).is_incomplete());
        assert_eq!(
            issues_message(&[Issue::EmptyName, Issue::NoProxySource]),
            "• 名称为空\n• 未选择任何代理或代理集，内核会拒绝该策略组"
        );
    }

    #[test]
    fn copies_take_the_next_free_number() {
        let mut taken: HashSet<String> = ["a-01".to_string(), "b".to_string()].into();
        assert_eq!(copied_name("a", &mut taken), "a-02");
        assert_eq!(copied_name("a-01", &mut taken), "a-03");
        assert_eq!(copied_name("b-7", &mut taken), "b-7-01");
        assert_eq!(monogram("🇭🇰 香港"), "香");
        assert_eq!(monogram("auto"), "A");
        assert_eq!(monogram(""), "");
    }

    fn state_with(groups: &[(&str, Value)]) -> State {
        let mut s = State::default();
        s.profiles.push(cp::new_profile("p".into(), "c", 1));
        for (i, (_, def)) in groups.iter().enumerate() {
            s.proxy_groups.push(ProxyGroupRow {
                id: i as u64 + 1,
                profile_id: "p".into(),
                definition: def.clone(),
                order: Some(format!("{i:08}")),
            });
        }
        s
    }

    fn order(s: &State) -> Vec<String> {
        cp::groups(s, "p")
            .iter()
            .map(|g| g.name().to_string())
            .collect()
    }

    #[test]
    fn duplicate_reorder_put_and_delete() {
        let mut s = state_with(&[
            ("a", json!({"name": "a", "type": "select"})),
            ("b", json!({"name": "b", "type": "select"})),
        ]);
        let mut next = 10;
        duplicate(&mut s, "p", &[1].into(), || {
            next += 1;
            next
        });
        assert_eq!(order(&s), ["a", "a-01", "b"]);
        reorder(&mut s, "p", 2, 0);
        assert_eq!(order(&s), ["b", "a", "a-01"]);
        assert!(!put(
            &mut s,
            "p",
            99,
            json!({"name": "b", "type": "select"})
        ));
        assert!(put(&mut s, "p", 99, json!({"name": "c", "type": "select"})));
        assert!(put(&mut s, "p", 1, json!({"name": "a2", "type": "select"})));
        assert_eq!(order(&s), ["b", "a2", "a-01", "c"]);
        delete(&mut s, "p", &[11, 99].into());
        assert_eq!(order(&s), ["b", "a2"]);
    }

    #[test]
    fn groups_in_use_by_groups_rules_dialers_and_dns() {
        let mut s = state_with(&[
            (
                "a",
                json!({"name": "a", "type": "select", "proxies": ["b"]}),
            ),
            ("b", json!({"name": "b", "type": "select"})),
            (
                "c",
                json!({"name": "c", "type": "select", "default-selected": "d"}),
            ),
            ("d", json!({"name": "d", "type": "select"})),
            ("e", json!({"name": "e", "type": "select"})),
        ]);
        assert_eq!(in_use(&s, "p", &[2, 4, 5].into()), ["b", "d"]);
        assert!(in_use(&s, "p", &[1, 2].into()).is_empty());
        s.profile_rules.push(cp::ProfileRule {
            profile_id: "p".into(),
            action: "MATCH".into(),
            target: "e".into(),
            ..Default::default()
        });
        assert_eq!(in_use(&s, "p", &[5].into()), ["e"]);
        s.profile_rules.clear();
        let dns = &mut s.profiles[0].overrides.dns;
        dns.keys.push("nameserver".into());
        dns.model.insert(
            "nameserver".into(),
            json!(["https://1.1.1.1/dns-query#e&h3=true"]),
        );
        assert_eq!(in_use(&s, "p", &[5].into()), ["e"]);
        assert_eq!(dns_dialer("8.8.8.8#a&b=1"), Some("a".into()));
        assert_eq!(dns_dialer("8.8.8.8"), None);
    }

    #[test]
    fn members_follow_the_use_positions() {
        let def = json!({
            "name": "G", "type": "select", "proxies": ["a", "b"], "use": ["p", "q"],
            "use-position": {"p": 1}, "include-all-proxies": true, "filter": "HK`JP",
            "exclude-filter": "x"
        });
        assert_eq!(
            members(&def),
            [
                Member::Proxy("a".into()),
                Member::Provider("p".into()),
                Member::Proxy("b".into()),
                Member::Provider("q".into()),
                Member::AllProxies,
                Member::Filter("HK".into()),
                Member::Filter("JP".into()),
                Member::ExcludeFilter("x".into()),
            ]
        );
    }

    #[test]
    fn quick_edit_writes_yaml_and_reads_the_parsed_group_back() {
        let def = json!({"name": "🚀 节点", "type": "url-test", "proxies": ["a: b", "true", "x"], "interval": 300, "lazy": true, "url": "https://x/y?a=1#f"});
        let y = to_yaml(&def);
        assert_eq!(
            y,
            "name: 🚀 节点\ntype: url-test\nproxies:\n  - \"a: b\"\n  - \"true\"\n  - x\nurl: https://x/y?a=1#f\ninterval: 300\nlazy: true\n"
        );
        assert_eq!(
            yaml_document("name: a\ntype: select\n\nproxies:\n  - x\n"),
            "proxy-groups:\n  - name: a\n    type: select\n    proxies:\n      - x\n"
        );
        let parsed = json!({"proxy-groups": [{"name": 7, "type": "Selector", "proxies": ["a", 1], "url": 5}]});
        let old = json!({"name": "o", "type": "select", "expected-status": "", "proxies": ["z"]});
        assert_eq!(
            from_parsed(&parsed, &old).unwrap(),
            json!({"name": "7", "type": "select", "proxies": ["a", "1"], "url": "5", "expected-status": ""})
        );
        assert!(from_parsed(&json!({"proxy-groups": [{"name": "a"}]}), &old).is_err());
        assert!(from_parsed(&json!({}), &old).is_err());
    }
}
