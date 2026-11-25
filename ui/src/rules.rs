//! The added rules (lib/views/config/rules.dart, lib/features/form/rule.dart): rules put
//! before every profile's own, each a type, a payload and where it goes.

use serde::{Deserialize, Serialize};

/// `RuleAction.addedRuleActions`: every type but MATCH, RULE-SET and SUB-RULE.
pub const ACTIONS: [&str; 34] = [
    "DOMAIN",
    "DOMAIN-SUFFIX",
    "DOMAIN-KEYWORD",
    "DOMAIN-REGEX",
    "DOMAIN-WILDCARD",
    "GEOSITE",
    "IP-CIDR",
    "IP-CIDR6",
    "IP-SUFFIX",
    "IP-ASN",
    "GEOIP",
    "SRC-GEOIP",
    "SRC-IP-ASN",
    "SRC-IP-CIDR",
    "SRC-IP-SUFFIX",
    "DST-PORT",
    "SRC-PORT",
    "IN-PORT",
    "IN-TYPE",
    "IN-USER",
    "IN-NAME",
    "REMATCH-NAME",
    "PROCESS-PATH",
    "PROCESS-PATH-REGEX",
    "PROCESS-PATH-WILDCARD",
    "PROCESS-NAME",
    "PROCESS-NAME-REGEX",
    "PROCESS-NAME-WILDCARD",
    "UID",
    "NETWORK",
    "DSCP",
    "AND",
    "OR",
    "NOT",
];

/// `RuleTarget.ruleTargetNames`.
pub const TARGETS: [&str; 7] = [
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "PASS-RULE",
    "COMPATIBLE",
    "GLOBAL",
];

/// `Rule`, as an added rule keeps it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Rule {
    pub id: u64,
    pub action: String,
    pub content: String,
    pub target: String,
    pub no_resolve: bool,
    pub src: bool,
    /// The target names an exit of the user's own, resolved per profile.
    pub custom_exit: bool,
}

/// `hasParams`: the types that take `src` and `no-resolve`.
pub fn has_params(action: &str) -> bool {
    matches!(
        action,
        "GEOIP" | "IP-ASN" | "IP-CIDR" | "IP-CIDR6" | "IP-SUFFIX" | "RULE-SET"
    )
}

impl Rule {
    /// `Rule.init`.
    pub fn new(id: u64) -> Self {
        Rule {
            id,
            action: ACTIONS[0].into(),
            target: TARGETS[0].into(),
            ..Default::default()
        }
    }

    /// `rawValue`, with a custom exit's target as `exit` resolves it.
    pub fn raw(&self, exit: impl Fn(&str) -> String) -> String {
        let target = if self.custom_exit {
            exit(&self.target)
        } else {
            self.target.clone()
        };
        let mut parts = vec![self.action.clone()];
        if !self.content.is_empty() {
            parts.push(self.content.clone());
        }
        if !target.is_empty() {
            parts.push(target);
        }
        if has_params(&self.action) {
            if self.src {
                parts.push("src".into());
            }
            if self.no_resolve {
                parts.push("no-resolve".into());
            }
        }
        parts.join(",")
    }

    /// The row's subtitle: the rule as the core reads it.
    pub fn line(&self) -> String {
        self.raw(|t| t.to_string())
    }

    /// `_RuleFormState._issue`: what keeps the form from saving.
    pub fn issue(&self) -> Option<String> {
        let content = self.content.trim();
        if content.is_empty() {
            return Some("内容不能为空".into());
        }
        if let Some(e) = payload_error(&self.action, content) {
            return Some(e);
        }
        if self.target.trim().is_empty() {
            return Some(if self.custom_exit {
                "名称不能为空".into()
            } else {
                "分流策略不能为空".into()
            });
        }
        if self.custom_exit && TARGETS.contains(&self.target.trim()) {
            return Some("名称当前已存在".into());
        }
        None
    }
}

/// `payloadError`: the payloads the core would reject the whole config over.
pub fn payload_error(action: &str, payload: &str) -> Option<String> {
    match action {
        "NETWORK" => (!["tcp", "udp"].contains(&payload.to_lowercase().as_str()))
            .then(|| "仅支持 tcp 或 udp".into()),
        "DST-PORT" | "SRC-PORT" | "IN-PORT" | "UID" => ranges(payload)
            .is_none()
            .then(|| "请输入数字或范围，如 80 或 8000-9000，多个用 / 分隔".into()),
        "DSCP" => match ranges(payload) {
            None => Some("请输入数字或范围，如 80 或 8000-9000，多个用 / 分隔".into()),
            Some(b) if b.iter().any(|v| *v > 63) => Some("DSCP 标记不能超过 63".into()),
            _ => None,
        },
        _ => None,
    }
}

/// `_parseRanges`: `80`, `80-90` or several joined by `/`; None when any part is not.
fn ranges(payload: &str) -> Option<Vec<u64>> {
    let mut out = Vec::new();
    for part in payload.split('/') {
        let part = part.trim();
        let (a, b) = part.split_once('-').unwrap_or((part, part));
        let (a, b): (u64, u64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        if a > b || b > 65535 {
            return None;
        }
        out.extend([a, b]);
    }
    Some(out)
}

/// `Rule.parse`: a rule line as mihomo's ParseRulePayload reads it.
pub fn parse(line: &str, id: u64) -> Rule {
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    let action = fields[0].to_uppercase();
    let action = if ACTIONS.contains(&action.as_str()) {
        action
    } else {
        ACTIONS[0].to_string()
    };
    let rest = &fields[1..];
    let comma = matches!(
        action.as_str(),
        "AND" | "OR" | "NOT" | "DOMAIN-REGEX" | "PROCESS-NAME-REGEX" | "PROCESS-PATH-REGEX"
    );
    let (content, target, params): (String, String, &[&str]) = if comma {
        let n = rest.len();
        (
            if n > 1 {
                rest[..n - 1].join(",")
            } else {
                String::new()
            },
            rest.last().map(|s| s.to_string()).unwrap_or_default(),
            &[],
        )
    } else {
        (
            rest.first().map(|s| s.to_string()).unwrap_or_default(),
            rest.get(1).map(|s| s.to_string()).unwrap_or_default(),
            rest.get(2..).unwrap_or(&[]),
        )
    };
    Rule {
        id,
        action,
        content,
        target,
        src: params.contains(&"src"),
        no_resolve: params.contains(&"no-resolve"),
        custom_exit: false,
    }
}

/// `customExitsOf`: the exits the added rules already go to.
pub fn custom_exits(rules: &[Rule]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in rules {
        if r.custom_exit && !r.target.is_empty() && !out.contains(&r.target) {
            out.push(r.target.clone());
        }
    }
    out
}

/// The overlay's `rules`: the added rules, each custom exit sent where the profile's last
/// MATCH goes (`exitTargetResolver` with no exit picked), DIRECT when it has none.
pub fn overlay_rules(rules: &[Rule], match_rules: &[String]) -> Vec<String> {
    let fallback = match_rules
        .last()
        .and_then(|m| m.split(',').nth(1))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "DIRECT".into());
    rules.iter().map(|r| r.raw(|_| fallback.clone())).collect()
}

/// Whether a search matches a rule (`searchFields`).
pub fn matches(rule: &Rule, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    q.is_empty() || rule.line().to_lowercase().contains(&q)
}

/// Moves the rule at `from` to `to`.
pub fn reorder(rules: &mut Vec<Rule>, from: usize, to: usize) {
    if from < rules.len() && to < rules.len() && from != to {
        let r = rules.remove(from);
        rules.insert(to, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_issues() {
        let mut r = Rule::new(1);
        assert_eq!(r.issue().unwrap(), "内容不能为空");
        r.content = "example.com".into();
        assert_eq!(r.line(), "DOMAIN,example.com,DIRECT");
        assert!(r.issue().is_none());
        r.action = "IP-CIDR".into();
        r.content = "10.0.0.0/8".into();
        r.no_resolve = true;
        r.src = true;
        assert_eq!(r.line(), "IP-CIDR,10.0.0.0/8,DIRECT,src,no-resolve");
        r.action = "DST-PORT".into();
        r.content = "80-x".into();
        assert!(r.issue().is_some());
        r.content = "80/443/1000-2000".into();
        assert!(r.issue().is_none());
        assert_eq!(r.line(), "DST-PORT,80/443/1000-2000,DIRECT");
        r.action = "DSCP".into();
        r.content = "64".into();
        assert!(r.issue().unwrap().contains("63"));
        r.action = "NETWORK".into();
        r.content = "UDP".into();
        assert!(r.issue().is_none());
        r.custom_exit = true;
        r.target = "DIRECT".into();
        assert_eq!(r.issue().unwrap(), "名称当前已存在");
    }

    #[test]
    fn exits_go_where_match_goes() {
        let mut a = Rule::new(1);
        a.content = "a.com".into();
        a.custom_exit = true;
        a.target = "家里".into();
        let mut b = Rule::new(2);
        b.content = "b.com".into();
        let rules = vec![a, b];
        assert_eq!(custom_exits(&rules), vec!["家里"]);
        assert_eq!(
            overlay_rules(&rules, &["MATCH,节点选择".into()]),
            vec!["DOMAIN,a.com,节点选择", "DOMAIN,b.com,DIRECT"]
        );
        assert_eq!(overlay_rules(&rules, &[])[0], "DOMAIN,a.com,DIRECT");
        let mut rules = rules;
        reorder(&mut rules, 0, 1);
        assert_eq!(rules[0].id, 2);
        assert!(matches(&rules[0], "b.COM"));
        assert!(!matches(&rules[0], "zzz"));
        let p = parse("AND,((NETWORK,UDP),(DST-PORT,443)),REJECT-DROP", 3);
        assert_eq!(p.content, "((NETWORK,UDP),(DST-PORT,443))");
        assert_eq!(p.target, "REJECT-DROP");
        assert_eq!(p.line(), "AND,((NETWORK,UDP),(DST-PORT,443)),REJECT-DROP");
        let p = parse("IP-CIDR6,::1/128,REJECT,no-resolve", 4);
        assert!(p.no_resolve && !p.src);
        assert_eq!(p.line(), "IP-CIDR6,::1/128,REJECT,no-resolve");
    }
}
