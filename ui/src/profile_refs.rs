//! What a custom profile's DNS, NTP and sniffer settings name (FlClash: lib/models/references.dart):
//! the proxies a DNS server dials through or sits behind, the NTP dialer, and the rule sets
//! the DNS policies, the Fake-IP filter and the sniffer lists read. mihomo refuses the whole
//! config for a rule set it lacks, so these join the rules' own in every check.

use crate::custom_profile::{ProfileOverrides, PROFILE_DNS};
use crate::tun_sniffer_ntp::{NTP, SNIFFER};
use serde_json::Value;
use std::collections::HashMap;

/// Percent-decoded, or the text itself when it does not decode (`Uri.decodeComponent`).
fn decoded_or_self(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let hex = text
            .get(i + 1..i + 3)
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(b) => out.push(b),
            None => return text.to_string(),
        }
        i += 3;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

const PROXY_HOST_SCHEMES: [&str; 4] = ["ts", "tailscale", "et", "easytier"];

/// `_proxyHostRange`: a ts or et server's host names a proxy.
fn proxy_host_range(head: &str) -> Option<(usize, usize)> {
    let sep = head.find("://").filter(|s| *s > 0)?;
    if !PROXY_HOST_SCHEMES.contains(&head[..sep].to_lowercase().as_str()) {
        return None;
    }
    let start = sep + 3;
    let end = head[start..]
        .find(['/', '?'])
        .map_or(head.len(), |e| start + e);
    Some((start, end))
}

/// `dnsServerProxies`, as mihomo's parseNameServer reads a server: the fragment decoded
/// before it is split, its last part without `=` the dialer, and a ts or et host.
pub fn dns_server_proxies(server: &str) -> Vec<String> {
    let (head, fragment) = match server.split_once('#') {
        Some((h, f)) => (h, Some(f)),
        None => (server, None),
    };
    let dialer = fragment.and_then(|f| {
        decoded_or_self(f)
            .split('&')
            .filter(|p| !p.contains('='))
            .last()
            .map(str::to_string)
    });
    let mut out = Vec::new();
    if let Some((s, e)) = proxy_host_range(head).filter(|(s, e)| e > s) {
        out.push(decoded_or_self(&head[s..e]));
    }
    if let Some(d) = dialer.filter(|d| !d.is_empty()) {
        out.push(d);
    }
    out
}

/// `_fragmentName`: mihomo splits on `&` and `=` only after decoding, so those stay.
fn fragment_name(name: &str) -> String {
    name.replace('%', "%25")
        .replace(' ', "%20")
        .replace(',', "%2C")
        .replace(';', "%3B")
}

pub fn renamed_dns_server(server: &str, names: &HashMap<String, String>) -> String {
    let (head, fragment) = match server.split_once('#') {
        Some((h, f)) => (h, Some(f)),
        None => (server, None),
    };
    let mut head = head.to_string();
    if let Some((s, e)) = proxy_host_range(&head) {
        if let Some(n) = names.get(&decoded_or_self(&head[s..e])) {
            head.replace_range(s..e, n);
        }
    }
    let Some(fragment) = fragment else {
        return head;
    };
    let parts: Vec<String> = fragment
        .split('&')
        .map(|p| match names.get(&decoded_or_self(p)) {
            Some(n) if !p.contains('=') => fragment_name(n),
            _ => p.to_string(),
        })
        .collect();
    format!("{head}#{}", parts.join("&"))
}

/// `splitByMultipleSeparators`: a policy's servers apart on commas, spaces and semicolons.
fn policy_servers(value: &str) -> Vec<String> {
    let parts: Vec<String> = value
        .split([',', ' ', ';'])
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if parts.len() > 1 {
        parts
    } else {
        vec![value.to_string()]
    }
}

fn renamed_policy_servers(value: &str, names: &HashMap<String, String>) -> String {
    let servers = policy_servers(value);
    let renamed: Vec<String> = servers
        .iter()
        .map(|s| renamed_dns_server(s, names))
        .collect();
    if renamed == servers {
        value.to_string()
    } else {
        renamed.join(", ")
    }
}

const RULE_SET_PREFIX: &str = "rule-set:";

/// `_ruleSetRange`: `rule-set:a,b` up to a second colon, the prefix in any case; a policy key
/// naming one set is taken whole.
fn rule_set_range(entry: &str, policy_key: bool) -> Option<(usize, usize)> {
    let start = RULE_SET_PREFIX.len();
    if !entry
        .get(..start)
        .is_some_and(|p| p.eq_ignore_ascii_case(RULE_SET_PREFIX))
    {
        return None;
    }
    if policy_key && !entry[start..].contains(',') {
        return Some((start, entry.len()));
    }
    let end = entry[start..].find(':').map_or(entry.len(), |c| start + c);
    Some((start, end))
}

fn rule_sets_in(entry: &str, policy_key: bool) -> Vec<String> {
    rule_set_range(entry, policy_key)
        .map(|(s, e)| entry[s..e].split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

fn renamed_rule_sets_in(entry: &str, names: &HashMap<String, String>, policy_key: bool) -> String {
    let Some((s, e)) = rule_set_range(entry, policy_key) else {
        return entry.to_string();
    };
    let renamed: Vec<&str> = entry[s..e]
        .split(',')
        .map(|n| names.get(n).map_or(n, String::as_str))
        .collect();
    let mut out = entry.to_string();
    out.replace_range(s..e, &renamed.join(","));
    out
}

/// `_renamedRuleText`: a Fake-IP filter in rule mode is a rule line, its own RULE-SET and
/// the ones nested in a logic rule.
fn renamed_rule_text(text: &str, names: &HashMap<String, String>) -> String {
    let mut spans = crate::profile_rules::nested_rule_sets(text);
    if let Some(span) = leading_rule_set(text) {
        spans.push(span);
    }
    spans.sort();
    let mut out = text.to_string();
    for (s, e) in spans.into_iter().rev() {
        if let Some(n) = names.get(&text[s..e]) {
            out.replace_range(s..e, n);
        }
    }
    out
}

/// `_leadingRuleSet`: the name after a leading `RULE-SET,`, without its spaces.
fn leading_rule_set(text: &str) -> Option<(usize, usize)> {
    let start_word = text.len() - text.trim_start().len();
    let word = "RULE-SET";
    if !text
        .get(start_word..start_word + word.len())
        .is_some_and(|w| w.eq_ignore_ascii_case(word))
    {
        return None;
    }
    let rest = &text[start_word + word.len()..];
    let comma = rest.len() - rest.trim_start().len();
    if !rest[comma..].starts_with(',') {
        return None;
    }
    let after = start_word + word.len() + comma + 1;
    let start = after + (text[after..].len() - text[after..].trim_start().len());
    let end_all = text[start..].find(',').map_or(text.len(), |c| start + c);
    let end = start + text[start..end_all].trim_end().len();
    (end > start).then_some((start, end))
}

fn strings(v: &Value) -> Vec<String> {
    crate::overrides::strings(v)
}

const SERVER_LISTS: [&str; 5] = [
    "default-nameserver",
    "nameserver",
    "fallback",
    "proxy-server-nameserver",
    "direct-nameserver",
];

const POLICIES: [&str; 2] = ["nameserver-policy", "proxy-server-nameserver-policy"];

const SNIFFER_MATCHERS: [&str; 4] = [
    "force-domain",
    "skip-domain",
    "skip-src-address",
    "skip-dst-address",
];

/// `ProfileOverridesReferences.dnsProxies`: every proxy the picked DNS servers name.
pub fn dns_proxies(o: &ProfileOverrides) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |server: &str| {
        for n in dns_server_proxies(server) {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    };
    for key in SERVER_LISTS {
        if let Some(v) = PROFILE_DNS.picked_value(&o.dns, key) {
            strings(&v).iter().for_each(|s| add(s));
        }
    }
    for key in POLICIES {
        if let Some(Value::Object(m)) = PROFILE_DNS.picked_value(&o.dns, key) {
            for v in m.values().filter_map(Value::as_str) {
                policy_servers(v).iter().for_each(|s| add(s));
            }
        }
    }
    out
}

/// `ntpProxy`: the picked NTP dialer, unless empty.
pub fn ntp_proxy(o: &ProfileOverrides) -> Option<String> {
    NTP.picked_value(&o.ntp, "dialer-proxy")
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|d| !d.is_empty())
}

/// `ProfileOverridesReferences.ruleSets`.
pub fn rule_sets(o: &ProfileOverrides) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |names: Vec<String>| {
        for n in names {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    };
    for key in POLICIES {
        if let Some(Value::Object(m)) = PROFILE_DNS.picked_value(&o.dns, key) {
            m.keys().for_each(|k| add(rule_sets_in(k, true)));
        }
    }
    if let Some(v) = PROFILE_DNS.picked_value(&o.dns, "fake-ip-filter") {
        let as_rules = PROFILE_DNS
            .picked_value(&o.dns, "fake-ip-filter-mode")
            .is_some_and(|m| m == "rule");
        for entry in strings(&v) {
            add(if as_rules {
                crate::profile_rules::rule_sets(&crate::profile_rules::parse(&entry, 0, ""))
            } else {
                rule_sets_in(&entry, false)
            });
        }
    }
    for key in SNIFFER_MATCHERS {
        if let Some(v) = SNIFFER.picked_value(&o.sniffer, key) {
            strings(&v).iter().for_each(|e| add(rule_sets_in(e, false)));
        }
    }
    out
}

fn map_strings(o: &mut crate::overrides::OverrideState, key: &str, f: impl Fn(&str) -> String) {
    if let Some(Value::Array(items)) = o.model.get_mut(key) {
        for item in items.iter_mut() {
            if let Some(s) = item.as_str() {
                *item = f(s).into();
            }
        }
    }
}

/// `renamedProxies`: the DNS servers and the NTP dialer follow a renamed group.
pub fn rename_proxies(o: &mut ProfileOverrides, names: &HashMap<String, String>) {
    for key in SERVER_LISTS {
        map_strings(&mut o.dns, key, |s| renamed_dns_server(s, names));
    }
    for key in POLICIES {
        if let Some(Value::Object(m)) = o.dns.model.get_mut(key) {
            for v in m.values_mut() {
                if let Some(s) = v.as_str() {
                    *v = renamed_policy_servers(s, names).into();
                }
            }
        }
    }
    if let Some(Value::String(d)) = o.ntp.model.get_mut("dialer-proxy") {
        if let Some(n) = names.get(d.as_str()) {
            *d = n.clone();
        }
    }
}

/// `renamedRuleSets`: the policy keys, the Fake-IP filter and the sniffer lists follow a
/// renamed rule set.
pub fn rename_rule_sets(o: &mut ProfileOverrides, names: &HashMap<String, String>) {
    for key in POLICIES {
        if let Some(Value::Object(m)) = o.dns.model.get_mut(key) {
            let renamed = m
                .iter()
                .map(|(k, v)| (renamed_rule_sets_in(k, names, true), v.clone()))
                .collect();
            *m = renamed;
        }
    }
    map_strings(&mut o.dns, "fake-ip-filter", |e| {
        renamed_rule_text(&renamed_rule_sets_in(e, names, false), names)
    });
    for key in SNIFFER_MATCHERS {
        map_strings(&mut o.sniffer, key, |e| {
            renamed_rule_sets_in(e, names, false)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn dns_servers_name_their_dialer_and_proxy_hosts() {
        assert_eq!(
            dns_server_proxies("https://1.1.1.1/dns-query#Proxy"),
            ["Proxy"]
        );
        assert_eq!(
            dns_server_proxies("tls://8.8.8.8#h3=true&%E8%8A%82%E7%82%B9"),
            ["节点"]
        );
        assert_eq!(
            dns_server_proxies("ts://Home%20Net/dns#Relay"),
            ["Home Net", "Relay"]
        );
        assert_eq!(dns_server_proxies("EasyTier://edge?x=1"), ["edge"]);
        assert!(dns_server_proxies("223.5.5.5").is_empty());
        assert!(dns_server_proxies("https://a#ecs=1.2.3.4/24").is_empty());
        assert_eq!(dns_server_proxies("https://a#bad%zz"), ["bad%zz"]);
    }

    #[test]
    fn renamed_servers_keep_their_options() {
        let n = names(&[("Old", "新 组,1"), ("edge", "core")]);
        assert_eq!(
            renamed_dns_server("https://a#h3=true&Old", &n),
            "https://a#h3=true&新%20组%2C1"
        );
        assert_eq!(renamed_dns_server("et://edge/x", &n), "et://core/x");
        assert_eq!(renamed_dns_server("udp://edge", &n), "udp://edge");
        assert_eq!(
            renamed_policy_servers("tls://a#Old;8.8.8.8", &n),
            "tls://a#新%20组%2C1, 8.8.8.8"
        );
        assert_eq!(renamed_policy_servers("8.8.8.8", &n), "8.8.8.8");
    }

    fn overrides() -> ProfileOverrides {
        let mut o = ProfileOverrides::default();
        for k in [
            "nameserver",
            "nameserver-policy",
            "fake-ip-filter",
            "fake-ip-filter-mode",
        ] {
            o.dns.keys.push(k.into());
        }
        o.dns
            .model
            .insert("nameserver".into(), json!(["https://a#G", "ts://T"]));
        o.dns.model.insert(
            "nameserver-policy".into(),
            json!({"rule-set:ads": "tls://b#H, 1.1.1.1", "rule-set:a,b:x": "c", "geosite:cn": "d"}),
        );
        o.dns.model.insert(
            "fake-ip-filter".into(),
            json!([
                "RULE-SET,cn,DIRECT",
                "AND,((RULE-SET,lan),(NETWORK,UDP)),DIRECT"
            ]),
        );
        o.dns
            .model
            .insert("fake-ip-filter-mode".into(), json!("rule"));
        o.ntp.keys = vec!["dialer-proxy".into()];
        o.ntp.model.insert("dialer-proxy".into(), json!("N"));
        o.sniffer.keys = vec!["skip-domain".into()];
        o.sniffer
            .model
            .insert("skip-domain".into(), json!(["Rule-Set:media", "+.x.com"]));
        o
    }

    #[test]
    fn overrides_name_proxies_and_rule_sets() {
        let o = overrides();
        assert_eq!(dns_proxies(&o), ["G", "T", "H"]);
        assert_eq!(ntp_proxy(&o).as_deref(), Some("N"));
        assert_eq!(rule_sets(&o), ["a", "b", "ads", "cn", "lan", "media"]);
        let mut unpicked = o.clone();
        unpicked.dns.keys.retain(|k| k != "nameserver-policy");
        unpicked.sniffer.keys.clear();
        assert_eq!(rule_sets(&unpicked), ["cn", "lan"]);
        unpicked.dns.keys.retain(|k| k != "fake-ip-filter-mode");
        assert!(rule_sets(&unpicked).is_empty());
    }

    #[test]
    fn renames_follow_into_the_overrides() {
        let mut o = overrides();
        rename_proxies(&mut o, &names(&[("G", "G2"), ("H", "H2"), ("N", "N2")]));
        assert_eq!(dns_proxies(&o), ["G2", "T", "H2"]);
        assert_eq!(ntp_proxy(&o).as_deref(), Some("N2"));
        rename_rule_sets(
            &mut o,
            &names(&[
                ("ads", "广告"),
                ("lan", "局域网"),
                ("cn", "CN"),
                ("media", "m"),
            ]),
        );
        assert_eq!(rule_sets(&o), ["a", "b", "广告", "CN", "局域网", "m"]);
        assert_eq!(
            o.dns.model["fake-ip-filter"][1],
            "AND,((RULE-SET,局域网),(NETWORK,UDP)),DIRECT"
        );
        assert_eq!(
            o.sniffer.model["skip-domain"],
            json!(["Rule-Set:m", "+.x.com"])
        );
    }
}
