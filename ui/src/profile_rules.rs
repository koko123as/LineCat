//! A custom profile's own rules: `Rule.parse`, its payload and target checks
//! (`customRuleIssues`), the rule sets it names, the quick edit text (`encodeRuleList` /
//! `decodeRuleList`) and `ProfileRules`' writes, over the `profile_rules` rows of the state.
//! FlClash: lib/views/profiles/custom/rules.dart, lib/models/references.dart.

use crate::custom_profile::{self as cp, ProfileRule};
use crate::proxy_groups::{self as pg, Data};
use crate::store::State;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// `RuleAction.values` with `getDesc`.
pub const ACTIONS: [(&str, &str); 37] = [
    ("DOMAIN", "匹配完整域名"),
    ("DOMAIN-SUFFIX", "匹配域名后缀"),
    ("DOMAIN-KEYWORD", "匹配域名关键字"),
    ("DOMAIN-REGEX", "使用域名正则表达式匹配"),
    ("DOMAIN-WILDCARD", "通配符匹配，仅支持*和?通配符"),
    ("GEOSITE", "匹配 Geosite 内的域名"),
    ("IP-CIDR", "匹配 IP 地址范围"),
    ("IP-CIDR6", "匹配 IP 地址范围，IP-CIDR6 只是一个别名"),
    ("IP-SUFFIX", "匹配 IP 后缀范围"),
    ("IP-ASN", "匹配 IP 所属 ASN"),
    ("GEOIP", "匹配 IP 所属国家代码"),
    ("SRC-GEOIP", "匹配来源 IP 所属国家代码"),
    ("SRC-IP-ASN", "匹配来源 IP 所属 ASN"),
    ("SRC-IP-CIDR", "匹配来源 IP 地址范围"),
    ("SRC-IP-SUFFIX", "匹配来源 IP 后缀范围"),
    ("DST-PORT", "匹配请求目标端口范围"),
    ("SRC-PORT", "匹配请求来源端口范围"),
    ("IN-PORT", "匹配入站端口"),
    ("IN-TYPE", "匹配入站类型"),
    ("IN-USER", "匹配入站用户名，支持使用 / 分隔多个用户名"),
    ("IN-NAME", "匹配入站名称"),
    ("REMATCH-NAME", "匹配重匹配名称，多个名称用/分隔"),
    ("PROCESS-PATH", "使用完整进程路径匹配"),
    ("PROCESS-PATH-REGEX", "使用进程路径正则表达式匹配"),
    (
        "PROCESS-PATH-WILDCARD",
        "使用进程路径通配符匹配，仅支持*和?通配符",
    ),
    ("PROCESS-NAME", "使用进程匹配，在Android平台可以匹配包名"),
    (
        "PROCESS-NAME-REGEX",
        "使用进程名称正则表达式匹配，在Android平台可以匹配包名",
    ),
    (
        "PROCESS-NAME-WILDCARD",
        "使用进程名称通配符匹配，仅支持*和?通配符",
    ),
    ("UID", "匹配 Linux USER ID"),
    ("NETWORK", "匹配TCP或者UDP"),
    ("DSCP", "匹配DSCP标记（仅限 tproxy udp 入站）"),
    ("RULE-SET", "引用规则集合，需配置rule-providers"),
    ("AND", "逻辑规则 AND"),
    ("OR", "逻辑规则 OR"),
    ("NOT", "逻辑规则 NOT"),
    ("SUB-RULE", "匹配至子规则，需要注意括号的使用"),
    ("MATCH", "匹配所有请求，无需条件"),
];

/// `RuleTarget.ruleTargetNames`: the base targets, then GLOBAL.
pub const TARGETS: [&str; 7] = [
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "PASS-RULE",
    "COMPATIBLE",
    "GLOBAL",
];

/// The type as a profile writes it, as FlClash shows it (`ruleAction.value`).
pub fn name(action: &str) -> String {
    action.to_string()
}

pub fn is_action(value: &str) -> bool {
    ACTIONS.iter().any(|(a, _)| *a == value)
}

pub fn has_params(action: &str) -> bool {
    crate::rules::has_params(action)
}

fn nests_rules(action: &str) -> bool {
    matches!(action, "AND" | "OR" | "NOT" | "SUB-RULE")
}

fn has_comma_payload(action: &str) -> bool {
    nests_rules(action)
        || matches!(
            action,
            "DOMAIN-REGEX" | "PROCESS-NAME-REGEX" | "PROCESS-PATH-REGEX"
        )
}

/// `Rule.init`.
pub fn new_rule(id: u64, profile_id: &str) -> ProfileRule {
    ProfileRule {
        id,
        profile_id: profile_id.into(),
        action: "DOMAIN".into(),
        target: "DIRECT".into(),
        ..Default::default()
    }
}

/// `Rule.parse`, as mihomo's ParseRulePayload reads a line with its target.
pub fn parse(line: &str, id: u64, profile_id: &str) -> ProfileRule {
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let kind = fields[0].to_uppercase();
    if kind.is_empty() {
        return new_rule(id, profile_id);
    }
    let action = if is_action(&kind) {
        kind
    } else {
        "DOMAIN".to_string()
    };
    let rest = &fields[1..];
    let (payload, target, params): (String, String, &[&str]) = if action == "MATCH" {
        (
            String::new(),
            rest.first().copied().unwrap_or("").into(),
            &[],
        )
    } else if has_comma_payload(&action) {
        let payload = if rest.len() > 1 {
            rest[..rest.len() - 1].join(",")
        } else {
            String::new()
        };
        (payload, rest.last().copied().unwrap_or("").into(), &[])
    } else {
        (
            rest.first().copied().unwrap_or("").into(),
            rest.get(1).copied().unwrap_or("").into(),
            rest.get(2..).unwrap_or(&[]),
        )
    };
    let set = action == "RULE-SET";
    let sub = action == "SUB-RULE";
    ProfileRule {
        id,
        profile_id: profile_id.into(),
        content: if set { String::new() } else { payload.clone() },
        rule_provider: if set { payload } else { String::new() },
        target: if sub { String::new() } else { target.clone() },
        sub_rule: if sub { target } else { String::new() },
        src: params.contains(&"src"),
        no_resolve: params.contains(&"no-resolve"),
        action,
        ..Default::default()
    }
}

/// `realContent`.
pub fn real_content(r: &ProfileRule) -> &str {
    match r.action.as_str() {
        "MATCH" => "",
        "RULE-SET" => &r.rule_provider,
        _ => &r.content,
    }
}

/// `realTarget`.
pub fn real_target(r: &ProfileRule) -> &str {
    if r.action == "SUB-RULE" {
        &r.sub_rule
    } else {
        &r.target
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PayloadError {
    Network,
    NumberRange,
    DscpRange,
}

impl PayloadError {
    pub fn message(self) -> &'static str {
        match self {
            PayloadError::Network => "仅支持 tcp 或 udp",
            PayloadError::NumberRange => "请输入数字或范围，如 80 或 8000-9000，多个用 / 分隔",
            PayloadError::DscpRange => "DSCP 标记不能超过 63",
        }
    }
}

/// `payloadError`: the payloads the core would turn the whole config down over.
pub fn payload_error(r: &ProfileRule) -> Option<PayloadError> {
    let payload = real_content(r).trim();
    if payload.is_empty() {
        return None;
    }
    match r.action.as_str() {
        "NETWORK" => (!["tcp", "udp"].contains(&payload.to_lowercase().as_str()))
            .then_some(PayloadError::Network),
        "DST-PORT" | "SRC-PORT" | "IN-PORT" | "UID" => int_ranges(payload)
            .is_none()
            .then_some(PayloadError::NumberRange),
        "DSCP" => match int_ranges(payload) {
            None => Some(PayloadError::NumberRange),
            Some(b) if b.iter().any(|v| *v > 63) => Some(PayloadError::DscpRange),
            _ => None,
        },
        _ => None,
    }
}

/// `_parseRanges`, as mihomo's newIntRanges: `80`, `80-90`, `/` or `,` between them.
fn int_ranges(payload: &str) -> Option<Vec<u64>> {
    if payload == "*" {
        return None;
    }
    let joined = payload.replace(',', "/");
    let segments: Vec<&str> = joined.split('/').collect();
    if segments.len() > 28 {
        return None;
    }
    let mut bounds = Vec::new();
    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let parts: Vec<&str> = segment.split('-').collect();
        if parts.len() > 2 {
            return None;
        }
        for part in parts {
            let digits: String = part.chars().filter(|c| !"[] ".contains(*c)).collect();
            bounds.push(digits.parse::<u64>().ok()?);
        }
    }
    (!bounds.is_empty()).then_some(bounds)
}

/// The spans of `(RULE-SET,<name>` in a logic rule's payload (`_nestedRuleSet`), the name
/// without the spaces around it.
pub(crate) fn nested_rule_sets(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let skip = |mut i: usize| {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    };
    let mut out = Vec::new();
    for (open, _) in text.match_indices('(') {
        let at = skip(open + 1);
        let word = "RULE-SET";
        if !text
            .get(at..at + word.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(word))
        {
            continue;
        }
        let comma = skip(at + word.len());
        if bytes.get(comma) != Some(&b',') {
            continue;
        }
        let start = skip(comma + 1);
        let Some(len) = text[start..].find([',', ')', '(']) else {
            continue;
        };
        if bytes[start + len] == b'(' {
            continue;
        }
        let end = start + text[start..start + len].trim_end().len();
        if end > start {
            out.push((start, end));
        }
    }
    out
}

/// `ruleSets`: mihomo's ProviderNames, each of which the config has to declare.
pub fn rule_sets(r: &ProfileRule) -> Vec<String> {
    match r.action.as_str() {
        "RULE-SET" if !r.rule_provider.is_empty() => vec![r.rule_provider.clone()],
        a if nests_rules(a) => nested_rule_sets(&r.content)
            .into_iter()
            .map(|(s, e)| r.content[s..e].to_string())
            .collect(),
        _ => Vec::new(),
    }
}

/// `renamedRuleSets` for one name.
pub fn rename_rule_set(r: &mut ProfileRule, old: &str, new: &str) {
    if r.action == "RULE-SET" {
        if r.rule_provider == old {
            r.rule_provider = new.into();
        }
    } else if nests_rules(&r.action) {
        let mut content = r.content.clone();
        for (s, e) in nested_rule_sets(&r.content).into_iter().rev() {
            if &content[s..e] == old {
                content.replace_range(s..e, new);
            }
        }
        r.content = content;
    }
}

/// `renamedRuleSets`: every name looked up once, so a rename never chains into another.
pub fn rename_rule_sets(r: &mut ProfileRule, names: &std::collections::HashMap<String, String>) {
    if r.action == "RULE-SET" {
        if let Some(n) = names.get(&r.rule_provider) {
            r.rule_provider = n.clone();
        }
    } else if nests_rules(&r.action) {
        let mut content = r.content.clone();
        for (s, e) in nested_rule_sets(&r.content).into_iter().rev() {
            if let Some(n) = names.get(&r.content[s..e]) {
                content.replace_range(s..e, n);
            }
        }
        r.content = content;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Issue {
    InvalidPayload(PayloadError),
    MissingRuleSet(String),
    MissingSubRule(String),
    MissingTarget(String),
}

impl Issue {
    pub fn message(&self) -> String {
        match self {
            Issue::InvalidPayload(e) => e.message().into(),
            Issue::MissingRuleSet(n) => format!("{n} 不是有效的规则集"),
            Issue::MissingSubRule(n) => format!("{n} 是一个无效的SUB_RULE"),
            Issue::MissingTarget(n) => format!("{n} 是一个无效的策略"),
        }
    }
}

/// What a rule's check reads of the profile, gathered once for all its rules.
#[derive(Clone, Debug, Default)]
pub struct Targets {
    /// `isRuleTarget`: the base targets, GLOBAL, the groups and the custom proxies they name.
    pub targets: BTreeSet<String>,
    /// The app's rule sets by label (`appProviderNames(ProviderKind.rule)`).
    pub rule_sets: BTreeSet<String>,
}

impl Targets {
    pub fn of(data: &Data, rule_sets: impl IntoIterator<Item = String>) -> Targets {
        let mut targets = data.rule_targets();
        targets.insert(pg::GLOBAL.into());
        targets.extend(data.named_proxies());
        Targets {
            targets,
            rule_sets: rule_sets.into_iter().collect(),
        }
    }

    /// `_isRuleLineTarget`: a name with a comma reads as cut short in a rule line.
    pub fn is_line_target(&self, n: &str) -> bool {
        self.targets.contains(n) && !n.contains(',')
    }
}

/// `customRuleIssues`.
pub fn issues(r: &ProfileRule, t: &Targets) -> Vec<Issue> {
    if let Some(e) = payload_error(r) {
        return vec![Issue::InvalidPayload(e)];
    }
    let sets = if r.action == "RULE-SET" {
        vec![r.rule_provider.clone()]
    } else {
        rule_sets(r)
    };
    if let Some(missing) = sets.into_iter().find(|s| !t.rule_sets.contains(s)) {
        return vec![Issue::MissingRuleSet(missing)];
    }
    let target = real_target(r);
    if r.action == "SUB-RULE" {
        return vec![Issue::MissingSubRule(target.into())];
    }
    if t.is_line_target(target) {
        Vec::new()
    } else {
        vec![Issue::MissingTarget(target.into())]
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

/// `customProfileData` for the rules: the groups' data and the app's rule sets.
pub fn targets(state: &State, profile_id: &str) -> Targets {
    Targets::of(
        &pg::data(state, profile_id),
        state.settings.rule_sets.iter().map(|s| s.label.clone()),
    )
}

/// `customProfileIssues.rules`: each rule with issues, by id.
pub fn profile_issues(state: &State, profile_id: &str) -> BTreeMap<u64, Vec<Issue>> {
    let t = targets(state, profile_id);
    cp::rules(state, profile_id)
        .into_iter()
        .filter_map(|r| {
            let found = issues(r, &t);
            (!found.is_empty()).then_some((r.id, found))
        })
        .collect()
}

/// `_ruleSaveError`: what keeps the form, or a quick edit line, from saving.
pub fn save_error(r: &ProfileRule) -> Option<String> {
    if r.action == "SUB-RULE" {
        return Some(format!("{} 是一个无效的SUB_RULE", r.sub_rule));
    }
    if let Some(e) = payload_error(r) {
        return Some(e.message().into());
    }
    if r.action != "MATCH" && real_content(r).is_empty() {
        return Some(if r.action == "RULE-SET" {
            "规则集不能为空".into()
        } else {
            "内容不能为空".into()
        });
    }
    if real_target(r).is_empty() {
        return Some("分流策略不能为空".into());
    }
    None
}

/// `_RuleItemState._checkInvalid`'s colour: 0 plain, 1 success, 2 warning, 3 tertiary,
/// 4 error, and whether the row carries the error mark.
pub fn tone(r: &ProfileRule, found: &[Issue]) -> (i32, bool) {
    if r.custom_exit {
        return (3, false);
    }
    if found.is_empty() && r.action != "SUB-RULE" {
        match r.target.to_uppercase().as_str() {
            "DIRECT" | "COMPATIBLE" => return (1, false),
            "REJECT" | "REJECT-DROP" => return (2, false),
            "PASS" | "PASS-RULE" | "GLOBAL" => return (3, false),
            _ => {}
        }
    }
    if found.is_empty() {
        (3, false)
    } else {
        (4, true)
    }
}

/// `searchFields`: the type's name and the line.
pub fn matches(r: &ProfileRule, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty()
        || name(&r.action).to_lowercase().contains(&q)
        || r.raw().to_lowercase().contains(&q)
}

/// `encodeRuleList`: a YAML list, quoting the lines YAML would read otherwise.
pub fn encode_list(rules: &[&ProfileRule]) -> String {
    rules
        .iter()
        .map(|r| {
            let v = r.raw();
            if v.contains(": ") || v.contains(" #") || v.ends_with(':') {
                format!("- '{}'\n", v.replace('\'', "''"))
            } else {
                format!("- {v}\n")
            }
        })
        .collect()
}

/// A quoted scalar's text, as `loadYaml` reads one; None when it does not close.
fn unquote(text: &str) -> Option<String> {
    if let Some(body) = text.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = body.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                    continue;
                }
                let rest: String = chars.collect();
                let rest = rest.trim();
                return (rest.is_empty() || rest.starts_with('#')).then_some(out);
            }
            out.push(c);
        }
        return None;
    }
    let body = text.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            '"' => {
                let rest = chars.as_str().trim();
                return (rest.is_empty() || rest.starts_with('#')).then_some(out);
            }
            c => out.push(c),
        }
    }
    None
}

/// `_ruleText`: a list line without its dash, quotes or trailing comment.
fn rule_text(line: &str) -> String {
    let text = line.strip_prefix('-').map(str::trim_start).unwrap_or(line);
    if text.starts_with('\'') || text.starts_with('"') {
        return unquote(text).unwrap_or_else(|| text.to_string());
    }
    match text.find(" #") {
        Some(i) => text[..i].trim_end().to_string(),
        None => text.to_string(),
    }
}

fn is_header(text: &str) -> bool {
    let lower = text.to_lowercase();
    let Some(rest) = lower.strip_prefix("rules") else {
        return false;
    };
    matches!(rest.trim_start(), ":" | "：")
}

/// `decodeRuleList`: each rule line with its line number; a rule whose line did not change
/// keeps its id. Err is the first line that starts with no known type.
pub fn decode_list(
    text: &str,
    previous: &[&ProfileRule],
    profile_id: &str,
    mut new_id: impl FnMut() -> u64,
) -> Result<Vec<(usize, ProfileRule)>, usize> {
    let mut unused: HashMap<String, Vec<u64>> = HashMap::new();
    for r in previous {
        unused.entry(r.raw()).or_default().push(r.id);
    }
    let mut out = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || is_header(trimmed) {
            continue;
        }
        let value = rule_text(trimmed);
        let kind = value.split(',').next().unwrap_or("").trim().to_uppercase();
        if !is_action(&kind) {
            return Err(i + 1);
        }
        let mut rule = parse(&value, 0, profile_id);
        rule.id = match unused.get_mut(&rule.raw()) {
            Some(ids) if !ids.is_empty() => ids.remove(0),
            _ => new_id(),
        };
        out.push((i + 1, rule));
    }
    Ok(out)
}

/// `_handleQuickEdit`'s apply: the list it saves, or the message that keeps it open.
pub fn quick_edit(
    text: &str,
    previous: &[&ProfileRule],
    profile_id: &str,
    new_id: impl FnMut() -> u64,
) -> Result<Vec<ProfileRule>, String> {
    let lines = decode_list(text, previous, profile_id, new_id).map_err(|line| {
        format!("第 {line} 行：规则必须以受支持的类型开头，例如 DOMAIN,example.com,DIRECT")
    })?;
    let mut rules = Vec::new();
    for (line, rule) in lines {
        if rule.action == "SUB-RULE" {
            continue;
        }
        if let Some(e) = save_error(&rule) {
            return Err(format!("第 {line} 行：{e}"));
        }
        rules.push(rule);
    }
    Ok(rules)
}

/// Every rule of the profile in the order given, renumbered so the order stays as shown.
fn renumber(state: &mut State, profile_id: &str, ordered: &[u64]) {
    for (i, id) in ordered.iter().enumerate() {
        if let Some(r) = state
            .profile_rules
            .iter_mut()
            .find(|r| r.id == *id && r.profile_id == profile_id)
        {
            r.order = Some(format!("{i:08}"));
        }
    }
}

fn ids(state: &State, profile_id: &str) -> Vec<u64> {
    cp::rules(state, profile_id).iter().map(|r| r.id).collect()
}

/// `put`: a stored rule in its place, a new one first.
pub fn put(state: &mut State, profile_id: &str, mut rule: ProfileRule) {
    rule.profile_id = profile_id.into();
    if let Some(r) = state
        .profile_rules
        .iter_mut()
        .find(|r| r.id == rule.id && r.profile_id == profile_id)
    {
        rule.order = r.order.clone();
        *r = rule;
        return;
    }
    put_all(state, profile_id, vec![rule]);
}

/// `putAll`: the rules before the profile's own, in their order.
pub fn put_all(state: &mut State, profile_id: &str, rules: Vec<ProfileRule>) {
    let mut order: Vec<u64> = rules.iter().map(|r| r.id).collect();
    order.extend(ids(state, profile_id));
    for mut r in rules {
        r.profile_id = profile_id.into();
        state.profile_rules.push(r);
    }
    renumber(state, profile_id, &order);
}

/// `setAll`: the quick edit's list in place of the profile's rules.
pub fn set_all(state: &mut State, profile_id: &str, rules: Vec<ProfileRule>) {
    state.profile_rules.retain(|r| r.profile_id != profile_id);
    let order: Vec<u64> = rules.iter().map(|r| r.id).collect();
    for mut r in rules {
        r.profile_id = profile_id.into();
        state.profile_rules.push(r);
    }
    renumber(state, profile_id, &order);
}

pub fn delete(state: &mut State, profile_id: &str, ids: &BTreeSet<u64>) {
    state
        .profile_rules
        .retain(|r| r.profile_id != profile_id || !ids.contains(&r.id));
}

/// `insertAfterEach` for 复制: a copy of each of `ids` right after it.
pub fn duplicate(
    state: &mut State,
    profile_id: &str,
    ids: &BTreeSet<u64>,
    mut new_id: impl FnMut() -> u64,
) {
    let current: Vec<ProfileRule> = cp::rules(state, profile_id).into_iter().cloned().collect();
    let mut order = Vec::new();
    for r in &current {
        order.push(r.id);
        if ids.contains(&r.id) {
            let mut copy = r.clone();
            copy.id = new_id();
            order.push(copy.id);
            state.profile_rules.push(copy);
        }
    }
    renumber(state, profile_id, &order);
}

/// `order(old, new)` from the sort list.
pub fn reorder(state: &mut State, profile_id: &str, from: usize, to: usize) {
    let mut order = ids(state, profile_id);
    if from >= order.len() || to >= order.len() || from == to {
        return;
    }
    let id = order.remove(from);
    order.insert(to, id);
    renumber(state, profile_id, &order);
}

/// `renameRuleTarget`: a renamed group keeps the rules that went to it.
pub fn rename_target(state: &mut State, profile_id: &str, old: &str, new: &str) {
    for r in state
        .profile_rules
        .iter_mut()
        .filter(|r| r.profile_id == profile_id && r.target == old)
    {
        r.target = new.into();
    }
}

fn custom_ids(state: &State) -> BTreeSet<String> {
    state
        .profiles
        .iter()
        .filter(|p| p.is_custom())
        .map(|p| p.id.clone())
        .collect()
}

/// `profilesUsing` a rule set: the custom profiles whose rules name it, by label.
pub fn profiles_using(state: &State, label: &str) -> Vec<String> {
    let custom = custom_ids(state);
    let using: BTreeSet<&str> = state
        .profile_rules
        .iter()
        .filter(|r| custom.contains(&r.profile_id) && rule_sets(r).iter().any(|s| s == label))
        .map(|r| r.profile_id.as_str())
        .collect();
    state
        .profiles
        .iter()
        .filter(|p| {
            using.contains(p.id.as_str())
                || p.is_custom()
                    && crate::profile_refs::rule_sets(&p.overrides)
                        .iter()
                        .any(|s| s == label)
        })
        .map(|p| p.label())
        .collect()
}

/// `renameRuleProvider`: the custom profiles' rules follow a rule set to its new label.
pub fn rename_rule_set_everywhere(state: &mut State, old: &str, new: &str) {
    if old == new {
        return;
    }
    let custom = custom_ids(state);
    for r in state
        .profile_rules
        .iter_mut()
        .filter(|r| custom.contains(&r.profile_id))
    {
        rename_rule_set(r, old, new);
    }
    let names = [(old.to_string(), new.to_string())].into();
    for p in state.profiles.iter_mut().filter(|p| p.is_custom()) {
        crate::profile_refs::rename_rule_sets(&mut p.overrides, &names);
    }
}

/// `customReferencedProviders.rule`, with the rule sets the DNS and sniffer settings name.
pub fn referenced_rule_sets(state: &State, profile_id: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let from_settings = state
        .profiles
        .iter()
        .find(|p| p.id == profile_id && p.is_custom())
        .map(|p| crate::profile_refs::rule_sets(&p.overrides))
        .unwrap_or_default();
    for s in cp::rules(state, profile_id)
        .into_iter()
        .flat_map(rule_sets)
        .chain(from_settings)
    {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Profile;
    use serde_json::json;

    fn rule(line: &str) -> ProfileRule {
        parse(line, 1, "p")
    }

    fn state() -> State {
        let mut s = State::default();
        s.profiles.push(cp::new_profile("p".into(), "自定义", 1));
        s.profiles.push(Profile {
            id: "f".into(),
            name: "订阅".into(),
            ..Default::default()
        });
        s.proxy_groups.push(cp::ProxyGroupRow {
            id: 1,
            profile_id: "p".into(),
            definition: json!({"name": "节点选择", "type": "select", "proxies": ["香港", "DIRECT"]}),
            order: None,
        });
        s.settings
            .custom_proxies
            .push(crate::custom_proxies::CustomProxy {
                id: "c".into(),
                definition: json!({"name": "香港", "type": "ss"}),
                ..Default::default()
            });
        s.settings
            .custom_proxies
            .push(crate::custom_proxies::CustomProxy {
                id: "d".into(),
                definition: json!({"name": "未用", "type": "ss"}),
                ..Default::default()
            });
        s.settings.rule_sets.push(crate::rule_sets::RuleSet {
            id: "r".into(),
            label: "ads".into(),
            ..Default::default()
        });
        s
    }

    #[test]
    fn lines_parse_as_mihomo_reads_them() {
        let r = rule("rule-set, ads ,REJECT,no-resolve,src");
        assert_eq!(
            (r.action.as_str(), r.rule_provider.as_str()),
            ("RULE-SET", "ads")
        );
        assert!(r.content.is_empty() && r.no_resolve && r.src);
        assert_eq!(r.raw(), "RULE-SET,ads,REJECT,src,no-resolve");
        let r = rule("SUB-RULE,(NETWORK,tcp),sub");
        assert_eq!(
            (r.content.as_str(), r.sub_rule.as_str()),
            ("(NETWORK,tcp)", "sub")
        );
        assert!(r.target.is_empty());
        let r = rule("MATCH,节点选择");
        assert_eq!((r.target.as_str(), real_content(&r)), ("节点选择", ""));
        let r = rule("NOPE,a,b");
        assert_eq!((r.action.as_str(), r.content.as_str()), ("DOMAIN", "a"));
        assert_eq!(rule("").target, "DIRECT");
        let r = rule("DOMAIN-REGEX,^a{1,3}$,DIRECT");
        assert_eq!(r.content, "^a{1,3}$");
        assert_eq!(name("IP-CIDR6"), "IP-CIDR6");
    }

    #[test]
    fn payloads_follow_new_int_ranges() {
        let bad = |l: &str| payload_error(&rule(l));
        assert_eq!(bad("DST-PORT,80/443,DIRECT"), None);
        assert_eq!(bad("DST-PORT,80-90,443,DIRECT"), None);
        assert_eq!(bad("DST-PORT,[80]-[90],DIRECT"), None);
        assert_eq!(bad("DST-PORT,*,DIRECT"), Some(PayloadError::NumberRange));
        assert_eq!(bad("UID,1-2-3,DIRECT"), Some(PayloadError::NumberRange));
        assert_eq!(bad("DSCP,64,DIRECT"), Some(PayloadError::DscpRange));
        assert_eq!(bad("DSCP,4/63,DIRECT"), None);
        assert_eq!(bad("NETWORK,UDP,DIRECT"), None);
        assert_eq!(bad("NETWORK,icmp,DIRECT"), Some(PayloadError::Network));
        assert_eq!(bad("DST-PORT,,DIRECT"), None);
        let many = vec!["1"; 29].join("/");
        assert_eq!(
            bad(&format!("SRC-PORT,{many},DIRECT")),
            Some(PayloadError::NumberRange)
        );
    }

    #[test]
    fn nested_rule_sets_are_found_and_renamed() {
        let mut r = rule("AND,((RULE-SET, ads ),(rule-set,cn,x),(DOMAIN,RULE-SET)),REJECT");
        assert_eq!(rule_sets(&r), ["ads", "cn"]);
        rename_rule_set(&mut r, "ads", "广告");
        assert_eq!(
            r.content,
            "((RULE-SET,广告 ),(rule-set,cn,x),(DOMAIN,RULE-SET))"
        );
        let mut spaced = ProfileRule {
            action: "OR".into(),
            content: "(( RULE-SET , ads ),(RULE-SET,(x)))".into(),
            ..Default::default()
        };
        assert_eq!(rule_sets(&spaced), ["ads"]);
        rename_rule_set(&mut spaced, "ads", "b");
        assert_eq!(spaced.content, "(( RULE-SET , b ),(RULE-SET,(x)))");
        let mut set = rule("RULE-SET,ads,REJECT");
        rename_rule_set(&mut set, "ads", "b");
        assert_eq!(rule_sets(&set), ["b"]);
        assert!(rule_sets(&rule("DOMAIN,RULE-SET,DIRECT")).is_empty());
    }

    #[test]
    fn issues_follow_custom_rule_issues() {
        let s = state();
        let t = targets(&s, "p");
        let found = |l: &str| issues(&rule(l), &t);
        assert!(found("DOMAIN,a,节点选择").is_empty());
        assert!(found("DOMAIN,a,香港").is_empty());
        assert!(found("DOMAIN,a,GLOBAL").is_empty());
        assert_eq!(
            found("DOMAIN,a,未用"),
            [Issue::MissingTarget("未用".into())]
        );
        assert_eq!(
            found("RULE-SET,cn,DIRECT"),
            [Issue::MissingRuleSet("cn".into())]
        );
        assert!(found("RULE-SET,ads,DIRECT").is_empty());
        assert_eq!(
            found("OR,((RULE-SET,ads),(RULE-SET,x)),DIRECT"),
            [Issue::MissingRuleSet("x".into())]
        );
        assert_eq!(
            found("DSCP,99,DIRECT"),
            [Issue::InvalidPayload(PayloadError::DscpRange)]
        );
        assert_eq!(
            found("SUB-RULE,(NETWORK,tcp),sub"),
            [Issue::MissingSubRule("sub".into())]
        );
        assert_eq!(
            found("RULE-SET,,DIRECT"),
            [Issue::MissingRuleSet("".into())]
        );
        assert_eq!(found("MATCH,节点选择").len() + found("MATCH,无").len(), 1);
        assert_eq!(
            Issue::MissingTarget("x".into()).message(),
            "x 是一个无效的策略"
        );
    }

    #[test]
    fn save_errors_and_tones() {
        assert_eq!(save_error(&rule("DOMAIN,,DIRECT")).unwrap(), "内容不能为空");
        assert_eq!(
            save_error(&rule("RULE-SET,,DIRECT")).unwrap(),
            "规则集不能为空"
        );
        assert_eq!(save_error(&rule("DOMAIN,a")).unwrap(), "分流策略不能为空");
        assert_eq!(
            save_error(&rule("SUB-RULE,(A),s")).unwrap(),
            "s 是一个无效的SUB_RULE"
        );
        assert!(save_error(&rule("MATCH,DIRECT")).is_none());
        assert!(save_error(&rule("DOMAIN,a,不存在")).is_none());
        assert_eq!(tone(&rule("DOMAIN,a,direct"), &[]), (1, false));
        assert_eq!(tone(&rule("DOMAIN,a,REJECT-DROP"), &[]), (2, false));
        assert_eq!(tone(&rule("DOMAIN,a,组"), &[]), (3, false));
        assert_eq!(
            tone(&rule("DOMAIN,a,x"), &[Issue::MissingTarget("x".into())]),
            (4, true)
        );
        assert!(matches(
            &rule("DOMAIN-SUFFIX,a.com,DIRECT"),
            "domain-suffix"
        ));
        assert!(!matches(
            &rule("DOMAIN-SUFFIX,a.com,DIRECT"),
            "domain_suffix"
        ));
        assert!(matches(&rule("DOMAIN-SUFFIX,a.com,DIRECT"), "A.COM"));
        assert!(!matches(&rule("DOMAIN,a,DIRECT"), "zzz"));
    }

    #[test]
    fn quick_edit_round_trips_and_keeps_ids() {
        let a = parse("DOMAIN,a.com,DIRECT", 10, "p");
        let b = parse("DOMAIN-KEYWORD,x: y,REJECT", 11, "p");
        let text = encode_list(&[&a, &b]);
        assert_eq!(
            text,
            "- DOMAIN,a.com,DIRECT\n- 'DOMAIN-KEYWORD,x: y,REJECT'\n"
        );
        let mut next = 100;
        let mut id = || {
            next += 1;
            next
        };
        let edited =
            format!("rules:\n{text}# note\n- \"IP-CIDR,1.1.1.1/32,DIRECT\" # c\n- MATCH,DIRECT\n");
        let out = quick_edit(&edited, &[&a, &b], "p", &mut id).unwrap();
        let got: Vec<(u64, String)> = out.iter().map(|r| (r.id, r.raw())).collect();
        assert_eq!(
            got,
            [
                (10, "DOMAIN,a.com,DIRECT".into()),
                (11, "DOMAIN-KEYWORD,x: y,REJECT".into()),
                (101, "IP-CIDR,1.1.1.1/32,DIRECT".into()),
                (102, "MATCH,DIRECT".into()),
            ]
        );
        assert_eq!(
            quick_edit("- DOMAIN,a,DIRECT\n- bogus line", &[], "p", &mut id).unwrap_err(),
            "第 2 行：规则必须以受支持的类型开头，例如 DOMAIN,example.com,DIRECT"
        );
        assert_eq!(
            quick_edit("- DST-PORT,x,DIRECT", &[], "p", &mut id).unwrap_err(),
            "第 1 行：请输入数字或范围，如 80 或 8000-9000，多个用 / 分隔"
        );
        let kept = quick_edit("- SUB-RULE,(A),s\n- DOMAIN,b,DIRECT", &[], "p", &mut id).unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(rule_text("- 'it''s, x' # tail"), "it's, x");
    }

    #[test]
    fn writes_keep_the_order_shown() {
        let mut s = state();
        let raws =
            |s: &State| -> Vec<String> { cp::rules(s, "p").iter().map(|r| r.raw()).collect() };
        put(&mut s, "p", parse("DOMAIN,a,DIRECT", 1, "p"));
        put(&mut s, "p", parse("DOMAIN,b,DIRECT", 2, "p"));
        assert_eq!(raws(&s), ["DOMAIN,b,DIRECT", "DOMAIN,a,DIRECT"]);
        put(&mut s, "p", parse("DOMAIN,a2,DIRECT", 1, "p"));
        assert_eq!(raws(&s), ["DOMAIN,b,DIRECT", "DOMAIN,a2,DIRECT"]);
        put_all(
            &mut s,
            "p",
            vec![
                parse("MATCH,DIRECT", 3, "p"),
                parse("GEOIP,CN,DIRECT", 4, "p"),
            ],
        );
        assert_eq!(raws(&s)[..2], ["MATCH,DIRECT", "GEOIP,CN,DIRECT"]);
        let mut n = 50;
        duplicate(&mut s, "p", &[3].into(), || {
            n += 1;
            n
        });
        assert_eq!(cp::rules(&s, "p")[1].id, 51);
        reorder(&mut s, "p", 0, 4);
        assert_eq!(cp::rules(&s, "p")[4].id, 3);
        delete(&mut s, "p", &[3, 51].into());
        assert_eq!(cp::rules(&s, "p").len(), 3);
        set_all(&mut s, "p", vec![parse("DOMAIN,z,DIRECT", 9, "p")]);
        assert_eq!(raws(&s), ["DOMAIN,z,DIRECT"]);
        rename_target(&mut s, "p", "DIRECT", "新");
        assert_eq!(raws(&s), ["DOMAIN,z,新"]);
    }

    #[test]
    fn rule_sets_in_use_and_renamed_for_custom_profiles_only() {
        let mut s = state();
        put(&mut s, "p", parse("RULE-SET,ads,REJECT", 1, "p"));
        put(&mut s, "p", parse("NOT,((RULE-SET,ads)),DIRECT", 2, "p"));
        s.profile_rules.push(parse("RULE-SET,ads,REJECT", 3, "f"));
        assert_eq!(profiles_using(&s, "ads"), ["自定义"]);
        assert!(profiles_using(&s, "cn").is_empty());
        assert_eq!(referenced_rule_sets(&s, "p"), ["ads"]);
        rename_rule_set_everywhere(&mut s, "ads", "广告");
        assert_eq!(referenced_rule_sets(&s, "p"), ["广告"]);
        assert_eq!(s.profile_rules[2].rule_provider, "ads");
        assert_eq!(profile_issues(&s, "p").len(), 2);
    }
}
