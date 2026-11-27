//! What a config brings into a custom profile (FlClash: lib/providers/state/custom_import.dart,
//! `readCustomProfileImport`), read from the mapping the Core's `config.read` gives: its
//! names moved off the app's. A provider whose source the app has is used under the app's
//! label, a proxy the app holds unchanged under the app's name, one keeping an app proxy's
//! name, server and credentials as that proxy, and any other name the app already uses
//! takes a free one, with every reference to it following. A name the config repeats keeps
//! its references on its first holder.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::custom_profile::{ProfileOverrides, ProfileRule};
use crate::custom_proxies::{text_of, CustomProxy, RESERVED_NAMES};
use crate::overrides::{Kind, OverrideState, Spec};
use crate::profile::{free_proxy_name, link_proxy_name};

/// `ImportedProvider`: an empty `url` is a local provider, saved with `content`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Provider {
    pub label: String,
    pub url: String,
    pub age_key: String,
    pub content: String,
}

/// `CustomImportApp`: what the app already holds.
#[derive(Clone, Debug, Default)]
pub struct AppSide {
    pub profile_labels: HashSet<String>,
    /// (url, age key) → label, in the profiles' order.
    pub profile_sources: Vec<((String, String), String)>,
    pub rule_set_labels: HashSet<String>,
    pub rule_set_sources: Vec<((String, String), String)>,
    pub proxies: Vec<Value>,
    /// The groups of the other profiles.
    pub group_names: HashSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Import {
    pub proxy_providers: Vec<Provider>,
    pub rule_providers: Vec<Provider>,
    pub proxies: Vec<Map<String, Value>>,
    /// The dialer of each imported proxy, by the name it is imported as.
    pub dialers: BTreeMap<String, String>,
    pub groups: Vec<Value>,
    /// Rules with id 0 and no profile.
    pub rules: Vec<ProfileRule>,
    pub overrides: ProfileOverrides,
    pub dropped_sub_rules: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportError {
    Unreadable,
    RuleType(String),
    Entry(String, String),
}

impl ImportError {
    pub fn message(&self) -> String {
        match self {
            ImportError::Unreadable => "此文件不是可导入的配置".into(),
            ImportError::RuleType(item) => {
                format!("{item}：规则必须以受支持的类型开头，例如 DOMAIN,example.com,DIRECT")
            }
            ImportError::Entry(item, detail) => format!("{item}：{detail}"),
        }
    }
}

const SECTIONS: [&str; 8] = [
    "proxies",
    "proxy-groups",
    "proxy-providers",
    "rule-providers",
    "rules",
    "dns",
    "ntp",
    "sniffer",
];

/// `customImportDroppedSubRules`.
pub fn dropped_tip(count: usize) -> String {
    format!(
        "自定义配置不支持子规则，已丢弃 {} 条 SUB-RULE 规则",
        crate::icon_sets::compact(count)
    )
}

pub fn read(document: &Value, app: &AppSide) -> Result<Import, ImportError> {
    let Some(doc) = document
        .as_object()
        .filter(|d| SECTIONS.iter().any(|s| d.contains_key(*s)))
    else {
        return Err(ImportError::Unreadable);
    };
    let (proxy_names, proxy_providers) = import_providers(
        doc.get("proxy-providers"),
        &app.profile_labels,
        &app.profile_sources,
        "proxies",
        true,
    );
    let (rule_sets, rule_providers) = import_providers(
        doc.get("rule-providers"),
        &app.rule_set_labels,
        &app.rule_set_sources,
        "payload",
        false,
    );
    let groups: Vec<Map<String, Value>> = list_of(doc.get("proxy-groups"))
        .iter()
        .filter_map(|g| g.as_object().cloned())
        .collect();
    let proxies: Vec<Map<String, Value>> = list_of(doc.get("proxies"))
        .iter()
        .filter_map(|p| p.as_object().cloned())
        .collect();
    let imported = import_proxies(&proxies, &groups, &app.proxies, &app.group_names);
    let mut rules = Vec::new();
    let mut dropped = 0;
    for item in list_of(doc.get("rules")) {
        let rule = import_rule(&text_of(item), &imported.names, &rule_sets)?;
        if rule.action == "SUB-RULE" {
            dropped += 1;
        } else {
            rules.push(rule);
        }
    }
    let groups = groups
        .iter()
        .zip(&imported.group_names)
        .map(|(g, name)| import_group(g, name, &imported.names, &proxy_names))
        .collect::<Result<Vec<_>, _>>()?;
    let options = provider_options(doc.get("proxy-providers"), &proxy_names)?;
    let overrides = import_overrides(doc, &imported.names, &rule_sets, options)?;
    Ok(Import {
        proxy_providers,
        rule_providers,
        proxies: imported.added,
        dialers: imported.dialers,
        groups,
        rules,
        overrides,
        dropped_sub_rules: dropped,
    })
}

fn list_of(v: Option<&Value>) -> &[Value] {
    v.and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn name_of(item: &Map<String, Value>) -> String {
    item.get("name").map(text_of).unwrap_or_default()
}

fn import_providers(
    section: Option<&Value>,
    labels: &HashSet<String>,
    sources: &[((String, String), String)],
    payload_key: &str,
    reads_age_key: bool,
) -> (HashMap<String, String>, Vec<Provider>) {
    let mut taken = labels.clone();
    let mut by_source: HashMap<(String, String), String> = HashMap::new();
    let mut by_url: HashMap<String, String> = HashMap::new();
    for (source, label) in sources {
        by_source
            .entry(source.clone())
            .or_insert_with(|| label.clone());
        by_url
            .entry(source.0.clone())
            .or_insert_with(|| label.clone());
    }
    let mut names = HashMap::new();
    let mut added = Vec::new();
    let entries = section
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (key, value) in &entries {
        let Some(provider) = value.as_object() else {
            continue;
        };
        if key.trim().is_empty() {
            continue;
        }
        let kind = provider.get("type").map(text_of).unwrap_or_default();
        let url = if kind == "http" {
            provider
                .get("url")
                .map(|u| text_of(u).trim().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let age_key = if reads_age_key && !url.is_empty() {
            provider
                .get("age-secret-key")
                .map(|k| text_of(k).trim().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let source = (url.clone(), age_key.clone());
        let known = if url.is_empty() {
            None
        } else if age_key.is_empty() {
            by_url.get(&url).cloned()
        } else {
            by_source.get(&source).cloned()
        };
        let label = match known {
            Some(label) => label,
            None => {
                let label = crate::rule_sets::unique_label(key, key, |l| taken.contains(l));
                taken.insert(label.clone());
                if !url.is_empty() {
                    by_source.insert(source, label.clone());
                    by_url.entry(url.clone()).or_insert_with(|| label.clone());
                }
                let content = match provider.get("payload") {
                    Some(Value::Array(payload)) if kind == "inline" => {
                        json!({ payload_key: payload }).to_string()
                    }
                    _ => String::new(),
                };
                added.push(Provider {
                    label: label.clone(),
                    url,
                    age_key,
                    content,
                });
                label
            }
        };
        names.insert(key.clone(), label);
    }
    (names, added)
}

fn entry_error(item: &str, path: &str) -> ImportError {
    ImportError::Entry(item.into(), format!("{path} 的值无效"))
}

/// `_proxyProviderOptions`: the options a provider of the config sets, by its label.
fn provider_options(
    section: Option<&Value>,
    names: &HashMap<String, String>,
) -> Result<BTreeMap<String, Value>, ImportError> {
    let mut out = BTreeMap::new();
    let entries = section
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (key, value) in &entries {
        let (Some(label), Some(provider)) = (names.get(key), value.as_object()) else {
            continue;
        };
        let mut options = Map::new();
        let pick = |from: Option<&Value>,
                    keys: &[(&str, char)],
                    to: &mut Map<String, Value>|
         -> Result<(), ImportError> {
            let Some(from) = from.and_then(Value::as_object) else {
                return Ok(());
            };
            for (k, kind) in keys {
                let Some(v) = from.get(*k).filter(|v| !v.is_null()) else {
                    continue;
                };
                let typed = match kind {
                    's' => Some(text_of(v).into()),
                    'n' => as_int(v),
                    _ => v.is_boolean().then(|| v.clone()),
                };
                to.insert((*k).into(), typed.ok_or_else(|| entry_error(key, k))?);
            }
            Ok(())
        };
        let health = provider
            .get("health-check")
            .filter(|h| h.get("enable") == Some(&Value::Bool(true)));
        let mut hc = Map::new();
        pick(
            health,
            &[
                ("url", 's'),
                ("interval", 'n'),
                ("timeout", 'n'),
                ("lazy", 'b'),
                ("expected-status", 's'),
            ],
            &mut hc,
        )?;
        let mut over = Map::new();
        pick(
            provider.get("override"),
            &[
                ("additional-prefix", 's'),
                ("additional-suffix", 's'),
                ("ip-version", 's'),
                ("udp", 'b'),
                ("skip-cert-verify", 'b'),
            ],
            &mut over,
        )?;
        options.insert("health-check".into(), Value::Object(hc));
        options.insert("override".into(), Value::Object(over));
        for k in ["filter", "exclude-filter"] {
            if let Some(v) = provider.get(k).filter(|v| !v.is_null()) {
                options.insert(k.into(), text_of(v).into());
            }
        }
        if let Some(v) = crate::providers::stored(&Value::Object(options)) {
            out.insert(label.clone(), v);
        }
    }
    Ok(out)
}

fn as_int(v: &Value) -> Option<Value> {
    match v {
        Value::Number(n) => n.as_i64().map(Value::from),
        Value::String(s) => s.trim().parse::<i64>().ok().map(Value::from),
        _ => None,
    }
}

/// `_weaklyTyped`: numbers compare as their text, so `443` and `"443"` are one value.
fn weak(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), weak(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(weak).collect()),
        Value::Number(n) => Value::String(n.to_string()),
        other => other.clone(),
    }
}

/// `_comparable`: the definition apart from its name and dialer.
fn comparable(definition: &Map<String, Value>) -> String {
    let mut d = definition.clone();
    d.remove("name");
    d.remove("dialer-proxy");
    weak(&Value::Object(d)).to_string()
}

const ACCOUNT_FIELDS: [&str; 7] = [
    "username",
    "password",
    "uuid",
    "psk",
    "key",
    "privateKey",
    "authStr",
];

/// `_identityOf`: name, type, server, port and credentials, for a proxy that has them.
fn identity_of(definition: &Map<String, Value>) -> Option<String> {
    let proxy = CustomProxy::new(String::new(), definition.clone());
    let server = proxy.server().filter(|s| !s.is_empty())?;
    let accounts: Vec<_> = proxy
        .fields()
        .into_iter()
        .filter(|f| ACCOUNT_FIELDS.contains(&f.name))
        .collect();
    if accounts.is_empty() {
        return None;
    }
    let mut parts = vec![
        json!(proxy.name()),
        json!(proxy.kind()),
        json!(server),
        json!(proxy.port()),
    ];
    parts.extend(
        accounts
            .iter()
            .map(|f| json!(proxy.value_of(f).map(text_of))),
    );
    Some(Value::Array(parts).to_string())
}

struct Proxies {
    names: HashMap<String, String>,
    group_names: Vec<String>,
    added: Vec<Map<String, Value>>,
    dialers: BTreeMap<String, String>,
}

/// `_importProxies`. Proxies and groups share one namespace in mihomo, and in a custom
/// profile that namespace takes in the app's proxies, so a group is moved off an app proxy's
/// name as a proxy is, and a proxy off other profiles' `group_names`.
fn import_proxies(
    proxies: &[Map<String, Value>],
    groups: &[Map<String, Value>],
    existing: &[Value],
    group_names: &HashSet<String>,
) -> Proxies {
    let mut held: HashMap<String, String> = HashMap::new();
    let mut identities: HashSet<String> = HashSet::new();
    let mut existing_names: HashSet<String> = HashSet::new();
    for definition in existing.iter().filter_map(Value::as_object) {
        let name = name_of(definition);
        existing_names.insert(name.clone());
        if !name.is_empty() {
            held.entry(comparable(definition)).or_insert(name);
            if let Some(identity) = identity_of(definition) {
                identities.insert(identity);
            }
        }
    }
    let mut kept: HashSet<String> = HashSet::new();
    for p in proxies {
        let name = link_proxy_name(&Value::Object(p.clone()));
        if !existing_names.contains(&name) && !group_names.contains(&name) {
            kept.insert(name);
        }
    }
    for g in groups {
        let name = name_of(g).trim().to_string();
        if !existing_names.contains(&name) {
            kept.insert(name);
        }
    }
    let mut taken: HashSet<String> = RESERVED_NAMES.iter().map(|s| s.to_string()).collect();
    taken.extend(existing_names.iter().cloned());
    taken.extend(group_names.iter().cloned());
    taken.extend(kept.iter().cloned());
    let mut used: HashSet<String> = HashSet::new();
    let free_name = |name: &str, taken: &mut HashSet<String>, used: &mut HashSet<String>| {
        if kept.contains(name) && used.insert(name.to_string()) {
            name.to_string()
        } else {
            free_proxy_name(name, taken)
        }
    };
    let mut names: HashMap<String, String> = HashMap::new();
    let mut proxy_names = Vec::new();
    let mut added = Vec::new();
    let mut repeats: HashMap<(String, String), String> = HashMap::new();
    for p in proxies {
        let name = link_proxy_name(&Value::Object(p.clone()));
        let mut definition = p.clone();
        definition.remove("dialer-proxy");
        definition.insert("name".into(), name.clone().into());
        let same = comparable(&definition);
        let imported = if let Some(held) = held.get(&same) {
            held.clone()
        } else if identity_of(&definition).is_some_and(|i| identities.contains(&i)) {
            name.clone()
        } else if let Some(first) = repeats.get(&(name.clone(), same.clone())) {
            first.clone()
        } else {
            let imported = if RESERVED_NAMES.contains(&name.as_str()) || group_names.contains(&name)
            {
                free_proxy_name(&name, &mut taken)
            } else {
                free_name(&name, &mut taken, &mut used)
            };
            repeats.insert((name, same), imported.clone());
            definition.insert("name".into(), imported.clone().into());
            added.push(definition);
            imported
        };
        names.entry(name_of(p)).or_insert_with(|| imported.clone());
        proxy_names.push(imported);
    }
    let mut imported_groups = Vec::new();
    for g in groups {
        let imported = free_name(name_of(g).trim(), &mut taken, &mut used);
        names.entry(name_of(g)).or_insert_with(|| imported.clone());
        imported_groups.push(imported);
    }
    let mut dialers = BTreeMap::new();
    for (p, imported) in proxies.iter().zip(&proxy_names) {
        let dialer = p.get("dialer-proxy").map(text_of).unwrap_or_default();
        if !dialer.is_empty() {
            dialers
                .entry(imported.clone())
                .or_insert_with(|| names.get(&dialer).cloned().unwrap_or(dialer));
        }
    }
    Proxies {
        names,
        group_names: imported_groups,
        added,
        dialers,
    }
}

/// `importProxy`: the name taken, and the proxy to add unless a local one holds it already.
pub fn import_proxy(
    definition: &Map<String, Value>,
    existing: &[Value],
    group_names: &HashSet<String>,
) -> (String, Option<Map<String, Value>>) {
    let imported = import_proxies(std::slice::from_ref(definition), &[], existing, group_names);
    let name = imported.names.into_values().next().unwrap_or_default();
    (name, imported.added.into_iter().next())
}

/// `_renamedList`: each name once, renamed.
fn renamed_list(v: &Value, names: &HashMap<String, String>) -> Value {
    let mut out: Vec<String> = Vec::new();
    for item in v.as_array().map(Vec::as_slice).unwrap_or(&[]) {
        let n = text_of(item);
        let n = names.get(&n).cloned().unwrap_or(n);
        if !out.contains(&n) {
            out.push(n);
        }
    }
    json!(out)
}

const GROUP_TEXT_KEYS: [&str; 11] = [
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
];

fn import_group(
    group: &Map<String, Value>,
    name: &str,
    names: &HashMap<String, String>,
    use_names: &HashMap<String, String>,
) -> Result<Value, ImportError> {
    let raw = group.get("type").map(text_of).unwrap_or_default();
    let kind = crate::proxy_groups::type_value(&raw);
    let known = kind == "relay" || crate::proxy_groups::TYPES.iter().any(|(t, _)| *t == kind);
    if !known {
        return Err(ImportError::Entry(
            name_of(group),
            format!("不支持的类型 {raw}"),
        ));
    }
    let mut m = group.clone();
    m.remove("id");
    m.insert("name".into(), name.into());
    m.insert("type".into(), kind.into());
    if let Some(v) = group.get("proxies") {
        m.insert("proxies".into(), renamed_list(v, names));
    }
    if let Some(v) = group.get("use") {
        m.insert("use".into(), renamed_list(v, use_names));
    }
    if let Some(v) = group.get("use-position") {
        let positions: Map<String, Value> = v
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, p)| {
                let p = text_of(p).trim().parse::<i64>().ok()?;
                Some((use_names.get(k).cloned().unwrap_or(k.clone()), p.into()))
            })
            .collect();
        m.insert("use-position".into(), Value::Object(positions));
    }
    if let Some(v) = group.get("default-selected").filter(|v| !v.is_null()) {
        let s = text_of(v);
        m.insert(
            "default-selected".into(),
            names.get(&s).cloned().unwrap_or(s).into(),
        );
    }
    for key in GROUP_TEXT_KEYS {
        if let Some(v) = m.get(key).filter(|v| !v.is_string() && !v.is_null()) {
            let s = text_of(v);
            m.insert(key.into(), s.into());
        }
    }
    let effective = crate::proxy_groups::config_definition(&Value::Object(m.clone()))
        .get("use-position")
        .cloned();
    match effective {
        Some(p) => m.insert("use-position".into(), p),
        None => m.remove("use-position"),
    };
    Ok(Value::Object(m))
}

fn import_rule(
    line: &str,
    names: &HashMap<String, String>,
    rule_sets: &HashMap<String, String>,
) -> Result<ProfileRule, ImportError> {
    let kind = line.split(',').next().unwrap_or("").trim().to_uppercase();
    if !crate::profile_rules::is_action(&kind) {
        return Err(ImportError::RuleType(line.into()));
    }
    let mut rule = crate::profile_rules::parse(line, 0, "");
    crate::profile_rules::rename_rule_sets(&mut rule, rule_sets);
    if let Some(n) = names.get(&rule.target) {
        rule.target = n.clone();
    }
    Ok(rule)
}

/// The value a key of an override page takes from a config, as its field reads it.
fn typed(kind: Kind, v: &Value) -> Option<Value> {
    match kind {
        Kind::Toggle => v.is_boolean().then(|| v.clone()),
        Kind::Number { .. } => as_int(v),
        Kind::Text(_) => (!v.is_object() && !v.is_array()).then(|| text_of(v).into()),
        Kind::Options(options) => {
            let s = text_of(v);
            options.contains(&s.as_str()).then(|| s.into())
        }
        Kind::List(..) => v
            .as_array()
            .map(|a| Value::Array(a.iter().map(|i| text_of(i).into()).collect())),
        Kind::Map(_) => v.as_object().map(|m| {
            Value::Object(
                m.iter()
                    .map(|(k, v)| {
                        let s = match v {
                            Value::Array(a) => a.iter().map(text_of).collect::<Vec<_>>().join(", "),
                            other => text_of(other),
                        };
                        (k.clone(), s.into())
                    })
                    .collect(),
            )
        }),
        Kind::Sniff => v.is_object().then(|| v.clone()),
    }
}

/// `_knownKeys` and `applyOverrideYaml`: the keys of `spec` the section sets, picked with
/// their values.
fn import_section(
    spec: &Spec,
    item: &str,
    section: &Map<String, Value>,
    upper_fields: bool,
) -> Result<OverrideState, ImportError> {
    let mut state = OverrideState::default();
    for k in spec.values() {
        let raw = match k.path.split_once('.') {
            None => section.get(k.path),
            Some((parent, field)) => section
                .get(parent)
                .and_then(Value::as_object)
                .and_then(|m| {
                    m.iter()
                        .find(|(f, _)| {
                            if upper_fields {
                                f.to_uppercase() == field
                            } else {
                                f.as_str() == field
                            }
                        })
                        .map(|(_, v)| v)
                }),
        };
        let Some(raw) = raw else {
            continue;
        };
        state.keys.push(k.path.to_string());
        if !raw.is_null() {
            let v = typed(k.kind, raw).ok_or_else(|| entry_error(item, k.path))?;
            state.model.insert(k.path.to_string(), v);
        }
    }
    Ok(state)
}

fn import_overrides(
    doc: &Map<String, Value>,
    names: &HashMap<String, String>,
    rule_sets: &HashMap<String, String>,
    proxy_providers: BTreeMap<String, Value>,
) -> Result<ProfileOverrides, ImportError> {
    use crate::tun_sniffer_ntp::{NTP, SNIFFER, TUN};
    let mut o = ProfileOverrides {
        proxy_providers,
        ..Default::default()
    };
    let section = |key: &str| doc.get(key).and_then(Value::as_object);
    if let Some(s) = section("dns") {
        o.dns = import_section(&crate::custom_profile::PROFILE_DNS, "DNS", s, false)?;
    }
    if let Some(s) = section("ntp") {
        o.ntp = import_section(&NTP, "NTP", s, false)?;
    }
    if let Some(s) = section("sniffer") {
        o.sniffer = import_section(&SNIFFER, "Sniffer", s, true)?;
    }
    if let Some(s) = section("tun") {
        o.tun = import_section(&TUN, "TUN", s, false)?;
    }
    let changed = |m: &HashMap<String, String>| -> HashMap<String, String> {
        m.iter()
            .filter(|(k, v)| k != v)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };
    crate::profile_refs::rename_proxies(&mut o, &changed(names));
    crate::profile_refs::rename_rule_sets(&mut o, &changed(rule_sets));
    Ok(o)
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_node_is_added_once_and_named_apart() {
        let def = |name: &str, port: u16| {
            json!({"name": name, "type": "ss", "server": "a", "port": port, "cipher": "aes-128-gcm", "password": "p"})
                .as_object()
                .unwrap()
                .clone()
        };
        let groups: HashSet<String> = ["G".to_string()].into();
        let (name, added) = import_proxy(&def("hk", 1), &[], &groups);
        assert_eq!(name, "hk");
        assert_eq!(added.unwrap()["name"], "hk");
        let held = [Value::Object(def("hk", 1))];
        assert_eq!(
            import_proxy(&def("hk", 1), &held, &groups),
            ("hk".into(), None)
        );
        let (name, added) = import_proxy(&def("hk", 2), &held, &groups);
        assert_ne!(name, "hk");
        assert_eq!(added.unwrap()["name"], json!(name));
        let (name, _) = import_proxy(&def("G", 3), &[], &groups);
        assert_ne!(name, "G");
    }
    use super::*;

    fn app() -> AppSide {
        AppSide {
            profile_labels: ["sub".to_string()].into(),
            profile_sources: vec![(
                ("https://a.example/sub".into(), String::new()),
                "sub".into(),
            )],
            rule_set_labels: ["ads".to_string()].into(),
            rule_set_sources: vec![(
                ("https://r.example/ads.yaml".into(), String::new()),
                "ads".into(),
            )],
            proxies: vec![
                json!({"name": "hk", "type": "ss", "server": "1.1.1.1", "port": 443, "cipher": "aes-128-gcm", "password": "p"}),
                json!({"name": "jp", "type": "trojan", "server": "2.2.2.2", "port": 443, "password": "x"}),
            ],
            group_names: ["Other".to_string()].into(),
        }
    }

    #[test]
    fn a_document_without_any_section_is_unreadable() {
        assert_eq!(
            read(&json!({"port": 7890}), &app()).unwrap_err(),
            ImportError::Unreadable
        );
        assert_eq!(
            read(&json!("text"), &app()).unwrap_err(),
            ImportError::Unreadable
        );
        assert_eq!(ImportError::Unreadable.message(), "此文件不是可导入的配置");
    }

    #[test]
    fn proxies_held_by_the_app_are_reused_and_others_named_apart() {
        let doc = json!({
            "proxies": [
                {"name": "hk", "type": "ss", "server": "1.1.1.1", "port": "443", "cipher": "aes-128-gcm", "password": "p"},
                {"name": "jp", "type": "trojan", "server": "2.2.2.2", "port": 443, "password": "x", "sni": "new"},
                {"name": "hk", "type": "vless", "server": "3.3.3.3", "port": 1, "uuid": "u"},
                {"name": "Other", "type": "socks5", "server": "4.4.4.4", "port": 1080},
                {"name": "", "type": "http", "server": "5.5.5.5", "port": 80, "dialer-proxy": "hk"},
                {"name": "DIRECT", "type": "http", "server": "6.6.6.6", "port": 80},
            ],
            "proxy-groups": [
                {"name": "Proxy", "type": "select", "proxies": ["hk", "jp", "Other", "", "DIRECT", "hk"]},
                {"name": "jp", "type": "url-test", "proxies": ["hk"]},
            ],
            "rules": ["MATCH,Proxy", "DOMAIN,x.com,jp"],
        });
        let got = read(&doc, &app()).unwrap();
        let added: Vec<String> = got.proxies.iter().map(name_of).collect();
        assert_eq!(added, ["hk-01", "Other-01", "5.5.5.5", "DIRECT-01"]);
        assert_eq!(
            got.groups[0]["proxies"],
            json!(["hk", "jp", "Other-01", "5.5.5.5", "DIRECT-01"])
        );
        assert_eq!(name_of(got.groups[1].as_object().unwrap()), "jp-01");
        assert_eq!(got.rules[1].target, "jp");
        assert_eq!(
            got.dialers,
            [("5.5.5.5".to_string(), "hk".to_string())].into()
        );
        assert!(got.proxies.iter().all(|p| !p.contains_key("dialer-proxy")));
    }

    #[test]
    fn providers_take_the_app_labels_or_free_ones() {
        let doc = json!({
            "proxy-providers": {
                "a": {"type": "http", "url": "https://a.example/sub", "health-check": {"enable": true, "interval": 600, "url": "https://t"}},
                "sub": {"type": "http", "url": "https://b.example/sub", "age-secret-key": "AGE-KEY", "override": {"udp": true}, "filter": "HK"},
                "local": {"type": "inline", "payload": [{"name": "x", "type": "direct"}]},
            },
            "rule-providers": {
                "r": {"type": "http", "url": "https://r.example/ads.yaml", "behavior": "domain"},
                "ads": {"type": "inline", "payload": ["DOMAIN,a.com"]},
            },
            "proxy-groups": [{"name": "G", "type": "select", "use": ["a", "sub", "local"], "use-position": {"sub": 0, "a": 9}, "proxies": ["DIRECT"]}],
            "rules": ["RULE-SET,r,G", "RULE-SET,ads,DIRECT", "AND,((RULE-SET,r),(NETWORK,udp)),G"],
        });
        let got = read(&doc, &app()).unwrap();
        let labels: Vec<&str> = got
            .proxy_providers
            .iter()
            .map(|p| p.label.as_str())
            .collect();
        assert_eq!(labels, ["local", "sub(1)"]);
        assert_eq!(got.proxy_providers[1].age_key, "AGE-KEY");
        assert_eq!(
            got.proxy_providers[0].content,
            r#"{"proxies":[{"name":"x","type":"direct"}]}"#
        );
        assert_eq!(got.groups[0]["use"], json!(["sub", "sub(1)", "local"]));
        assert_eq!(got.groups[0]["use-position"], json!({"sub(1)": 0}));
        assert_eq!(
            got.rule_providers,
            vec![Provider {
                label: "ads(1)".into(),
                content: r#"{"payload":["DOMAIN,a.com"]}"#.into(),
                ..Default::default()
            }]
        );
        assert_eq!(got.rules[0].rule_provider, "ads");
        assert_eq!(got.rules[1].rule_provider, "ads(1)");
        assert_eq!(got.rules[2].content, "((RULE-SET,ads),(NETWORK,udp))");
        assert_eq!(
            got.overrides.proxy_providers,
            [
                (
                    "sub".to_string(),
                    json!({"health-check": {"interval": 600, "url": "https://t"}})
                ),
                (
                    "sub(1)".to_string(),
                    json!({"override": {"udp": true}, "filter": "HK"})
                ),
            ]
            .into()
        );
    }

    #[test]
    fn sub_rules_are_dropped_and_unknown_types_refused() {
        let doc = json!({"rules": ["SUB-RULE,(NETWORK,tcp),s", "MATCH,DIRECT", "SUB-RULE,(NETWORK,udp),s"]});
        let got = read(&doc, &app()).unwrap();
        assert_eq!((got.rules.len(), got.dropped_sub_rules), (1, 2));
        assert_eq!(
            dropped_tip(2),
            "自定义配置不支持子规则，已丢弃 2 条 SUB-RULE 规则"
        );
        let bad = read(&json!({"rules": ["FOO,x,DIRECT"]}), &app()).unwrap_err();
        assert_eq!(bad, ImportError::RuleType("FOO,x,DIRECT".into()));
        let bad = read(
            &json!({"proxy-groups": [{"name": "g", "type": "chain"}]}),
            &app(),
        );
        assert!(matches!(bad, Err(ImportError::Entry(item, _)) if item == "g"));
    }

    #[test]
    fn settings_take_the_keys_the_pages_know() {
        let doc = json!({
            "proxies": [{"name": "Other", "type": "socks5", "server": "4.4.4.4", "port": 1080}],
            "dns": {
                "enable": true, "ipv6": false, "unknown": 1,
                "nameserver": ["https://1.1.1.1/dns-query#Other", 8],
                "nameserver-policy": {"+.cn": ["223.5.5.5", "119.29.29.29"]},
                "fallback-filter": {"geoip": true, "extra": 1},
                "fake-ip-filter": ["rule-set:ads"], "fake-ip-filter-mode": "rule",
            },
            "ntp": {"enable": true, "port": "123", "dialer-proxy": "Other"},
            "sniffer": {"enable": true, "sniff": {"tls": {"ports": [443]}}},
            "rule-providers": {"ads": {"type": "file", "path": "x"}},
        });
        let o = read(&doc, &app()).unwrap().overrides;
        let dns = &crate::custom_profile::PROFILE_DNS;
        assert_eq!(
            dns.picked(&o.dns).map(|k| k.path).collect::<Vec<_>>(),
            [
                "enable",
                "ipv6",
                "fake-ip-filter",
                "fake-ip-filter-mode",
                "nameserver-policy",
                "nameserver",
                "fallback-filter.geoip",
            ]
        );
        assert_eq!(
            o.dns.model["nameserver"],
            json!(["https://1.1.1.1/dns-query#Other-01", "8"])
        );
        assert_eq!(
            o.dns.model["nameserver-policy"],
            json!({"+.cn": "223.5.5.5, 119.29.29.29"})
        );
        assert_eq!(o.dns.model["fake-ip-filter"], json!(["rule-set:ads(1)"]));
        assert_eq!(o.ntp.model["port"], json!(123));
        assert_eq!(o.ntp.model["dialer-proxy"], json!("Other-01"));
        assert_eq!(o.sniffer.keys, ["enable", "sniff.TLS"]);
        let bad = read(&json!({"ntp": {"port": "x"}}), &app()).unwrap_err();
        assert_eq!(bad.message(), "NTP：port 的值无效");
    }
}
