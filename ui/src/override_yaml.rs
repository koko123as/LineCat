//! Quick edit of an override page as YAML (`OverrideQuickEdit`: `overrideYaml`,
//! `applyOverrideYaml`, `_handleQuickEditPop`), for the DNS, NTP and sniffer specs.
//! The text goes out through the emitter here and comes back through the Core's
//! `config.read`, under the section's own key.

use crate::overrides::{Kind, OverrideState, Spec};
use crate::proxy_groups::{quote, scalar};
use serde_json::{Map, Value};

/// The specs with a quick edit, by page id, and the name their messages use.
pub fn section_of(spec: &Spec) -> Option<&'static str> {
    match spec.id {
        "dns" | "profile-dns" => Some("DNS"),
        "ntp" => Some("NTP"),
        "sniffer" => Some("sniffer"),
        _ => None,
    }
}

/// The top-level key the text is read under.
pub fn document_key(spec: &Spec) -> &'static str {
    match spec.id {
        "ntp" => "ntp",
        "sniffer" => "sniffer",
        _ => "dns",
    }
}

fn parent(path: &str) -> Option<(&str, &str)> {
    path.split_once('.')
}

/// `_splitPolicyServers`: a policy's servers as a list once there are two.
fn split_servers(v: &Value) -> Value {
    let Some(m) = v.as_object() else {
        return v.clone();
    };
    let split = m.iter().map(|(k, v)| {
        let s = v.as_str().unwrap_or_default();
        let parts: Vec<Value> = s
            .split([',', ' ', ';'])
            .filter(|p| !p.is_empty())
            .map(|p| Value::String(p.into()))
            .collect();
        let v = if parts.len() > 1 {
            Value::Array(parts)
        } else {
            Value::String(s.into())
        };
        (k.clone(), v)
    });
    Value::Object(split.collect())
}

/// `_overrideJson`: the picked keys in the spec's order, written as they are, nested under
/// their parent.
pub fn override_json(spec: &Spec, state: &OverrideState) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    for k in spec.picked(state) {
        let mut v = spec.value(state, k.path);
        if matches!(k.kind, Kind::Map(_)) {
            v = split_servers(&v);
        }
        match parent(k.path) {
            Some((p, field)) => {
                let at = match out.iter().position(|(k, _)| k == p) {
                    Some(at) => at,
                    None => {
                        out.push((p.to_string(), Value::Object(Map::new())));
                        out.len() - 1
                    }
                };
                if let Some(m) = out[at].1.as_object_mut() {
                    m.insert(field.into(), v);
                }
            }
            None => out.push((k.path.into(), v)),
        }
    }
    out
}

/// Block YAML for a map; a map inside a list stays in flow style, which YAML reads alike.
pub fn emit<'a>(entries: impl IntoIterator<Item = (&'a String, &'a Value)>) -> String {
    let mut out = String::new();
    emit_map(&mut out, entries, 0);
    out
}

fn emit_map<'a>(
    out: &mut String,
    entries: impl IntoIterator<Item = (&'a String, &'a Value)>,
    indent: usize,
) {
    let pad = " ".repeat(indent);
    for (k, v) in entries {
        let k = quote(k);
        match v {
            Value::Object(m) if !m.is_empty() => {
                out.push_str(&format!("{pad}{k}:\n"));
                emit_map(out, m, indent + 2);
            }
            Value::Array(items) if !items.is_empty() => {
                out.push_str(&format!("{pad}{k}:\n"));
                for item in items {
                    out.push_str(&format!("{pad}  - {}\n", inline(item)));
                }
            }
            v => out.push_str(&format!("{pad}{k}: {}\n", inline(v))),
        }
    }
}

fn inline(v: &Value) -> String {
    match v {
        Value::Array(_) | Value::Object(_) => v.to_string(),
        v => scalar(v),
    }
}

pub fn override_yaml(spec: &Spec, state: &OverrideState) -> String {
    emit(override_json(spec, state).iter().map(|(k, v)| (k, v)))
}

/// The text under the section's key, so the Core reads it as that section.
pub fn document(spec: &Spec, text: &str) -> String {
    let mut out = format!("{}:\n", document_key(spec));
    for line in text.lines() {
        if !line.trim().is_empty() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn strings(v: &Value) -> Option<Value> {
    v.as_array()?
        .iter()
        .map(|x| text_of(x).map(Value::String))
        .collect::<Option<Vec<_>>>()
        .map(Value::Array)
}

/// The value as the model holds it, or None when the model cannot hold it.
fn typed(kind: Kind, v: &Value) -> Option<Value> {
    match kind {
        Kind::Toggle => v.as_bool().map(Value::Bool),
        Kind::Number { .. } => v.as_i64().map(Value::from),
        Kind::Text(_) => text_of(v).map(Value::String),
        Kind::Options(options) => {
            let s = text_of(v)?;
            options.contains(&s.as_str()).then_some(Value::String(s))
        }
        Kind::List(..) => strings(v),
        Kind::Map(_) => {
            let m = v.as_object()?;
            let joined = m.iter().map(|(k, v)| {
                let s = match v {
                    Value::Array(items) => items
                        .iter()
                        .map(text_of)
                        .collect::<Option<Vec<_>>>()?
                        .join(", "),
                    v => text_of(v)?,
                };
                Some((k.clone(), Value::String(s)))
            });
            joined.collect::<Option<Map<_, _>>>().map(Value::Object)
        }
        Kind::Sniff => {
            let m = v.as_object()?;
            let mut out = Map::new();
            out.insert(
                "ports".into(),
                match m.get("ports") {
                    None | Some(Value::Null) => Value::Array(Vec::new()),
                    Some(p) => strings(p)?,
                },
            );
            match m.get("override-destination") {
                None | Some(Value::Null) => {}
                Some(d) => {
                    out.insert("override-destination".into(), Value::Bool(d.as_bool()?));
                }
            }
            Some(Value::Object(out))
        }
    }
}

/// `_applyOverrideYaml`: the keys an edited document names with their values, or None
/// for an empty document.
pub fn apply_document(
    spec: &Spec,
    doc: &Value,
) -> Result<Option<(Vec<String>, Map<String, Value>)>, String> {
    let section = section_of(spec).unwrap_or(spec.title);
    if doc.is_null() {
        return Ok(None);
    }
    let Some(obj) = doc.as_object() else {
        return Err(format!("覆写内容必须是 {section} 键的映射"));
    };
    let parents: Vec<&str> = spec
        .values()
        .filter_map(|k| parent(k.path).map(|(p, _)| p))
        .collect();
    let mut keys = Vec::new();
    let mut model = Map::new();
    let mut take = |path: String, v: &Value| -> Result<(), String> {
        let k = spec
            .key(&path)
            .ok_or_else(|| format!("未知的 {section} 键：{path}"))?;
        let v = typed(k.kind, v).ok_or_else(|| format!("{path} 的值无效"))?;
        if !keys.contains(&path) {
            keys.push(path.clone());
        }
        model.insert(path, v);
        Ok(())
    };
    for (name, v) in obj {
        if !parents.contains(&name.as_str()) {
            take(name.clone(), v)?;
            continue;
        }
        let Some(fields) = v.as_object() else {
            return Err(format!("{name} 必须是映射"));
        };
        for (field, v) in fields {
            let field = if spec.id == "sniffer" {
                field.to_uppercase()
            } else {
                field.clone()
            };
            take(format!("{name}.{field}"), v)?;
        }
    }
    Ok(Some((keys, model)))
}

/// `_handleQuickEditPop`: the document's keys replace the listed ones; keys the page did
/// not list stay.
pub fn quick_edit(
    spec: &Spec,
    state: &OverrideState,
    doc: &Value,
) -> Result<OverrideState, String> {
    let listed: Vec<&str> = spec.picked(state).map(|k| k.path).collect();
    let (keys, model) = apply_document(spec, doc)?.unwrap_or_default();
    let mut next = state.clone();
    next.keys
        .retain(|p| !listed.contains(&p.as_str()) || keys.contains(p));
    for p in &listed {
        if !keys.iter().any(|k| k == p) {
            next.model.remove(*p);
        }
    }
    for k in keys {
        if !next.keys.contains(&k) {
            next.keys.push(k);
        }
    }
    next.model.extend(model);
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_profile::PROFILE_DNS;
    use crate::overrides::DNS;
    use crate::tun_sniffer_ntp::{NTP, SNIFFER};
    use serde_json::json;

    fn state(keys: &[&str], model: Value) -> OverrideState {
        OverrideState {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            model: serde_json::from_value(model).unwrap(),
        }
    }

    #[test]
    fn writes_the_picked_keys_nested() {
        let s = state(
            &["enable", "nameserver-policy", "fallback-filter.geoip"],
            json!({"nameserver-policy": {"+.cn": "1.1.1.1, 8.8.8.8", "a.com": "9.9.9.9"}}),
        );
        assert_eq!(
            override_yaml(&PROFILE_DNS, &s),
            "enable: false\nnameserver-policy:\n  +.cn:\n    - 1.1.1.1\n    - 8.8.8.8\n  a.com: 9.9.9.9\nfallback-filter:\n  geoip: false\n"
        );
        let ntp = state(&["server", "dialer-proxy"], json!({}));
        assert_eq!(
            override_yaml(&NTP, &ntp),
            "server: time.apple.com\ndialer-proxy: \"\"\n"
        );
        assert_eq!(document(&NTP, "server: a\n\n"), "ntp:\n  server: a\n");
    }

    #[test]
    fn reads_a_document_back() {
        let doc = json!({"sniff": {"http": {"ports": [80, "8080-8090"]}}, "enable": true});
        let (keys, model) = apply_document(&SNIFFER, &doc).unwrap().unwrap();
        assert_eq!(keys, ["enable", "sniff.HTTP"]);
        assert_eq!(model["sniff.HTTP"], json!({"ports": ["80", "8080-8090"]}));
        let policy = json!({"nameserver-policy": {"a": ["1.1.1.1", "2.2.2.2"]}});
        let (_, model) = apply_document(&PROFILE_DNS, &policy).unwrap().unwrap();
        assert_eq!(model["nameserver-policy"], json!({"a": "1.1.1.1, 2.2.2.2"}));
        assert!(apply_document(&NTP, &json!({"nope": 1}))
            .unwrap_err()
            .contains("nope"));
        assert!(apply_document(&NTP, &json!({"port": "x"})).is_err());
        assert!(apply_document(&DNS, &json!({"cache-algorithm": "fifo"})).is_err());
        assert!(apply_document(&NTP, &json!(["a"])).is_err());
        assert!(apply_document(&NTP, &Value::Null).unwrap().is_none());
    }

    #[test]
    fn quick_edit_replaces_the_listed_keys() {
        let s = state(&["server", "port"], json!({"server": "a.com", "port": 1}));
        let next = quick_edit(&NTP, &s, &json!({"port": 123, "enable": true})).unwrap();
        assert_eq!(next.keys, ["port", "enable"]);
        assert_eq!(next.model.get("server"), None);
        assert_eq!(next.model["port"], json!(123));
        let cleared = quick_edit(&NTP, &s, &Value::Null).unwrap();
        assert!(cleared.keys.is_empty() && cleared.model.is_empty());
    }
}
