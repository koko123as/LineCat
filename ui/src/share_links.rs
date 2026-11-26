//! Share link quick edit of 进阶配置 → 自定义 → 代理 (`ShareLinkEdit`, `withUnlinkedFields` in
//! lib/providers/state/custom.dart, `_confirmReplaced` and `_fillFromLink` in
//! lib/views/profiles/custom/custom_proxies.dart). The Core's `convert.encodeLinks` and
//! `convert.decodeLinks` do the encoding; this is what the page makes of their answers.

use crate::custom_proxies::{CustomProxy, RESERVED_NAMES};
use crate::profile::{free_proxy_name, link_proxy_name};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashMap, HashSet};

/// "第 {line} 行：{message}" (`lineIssueTip`).
pub fn line_issue(line: usize, message: &str) -> String {
    format!("第 {line} 行：{message}")
}

pub const INVALID: &str = "无法从链接中读取代理";
pub const SINGLE_ONLY: &str = "这里只能填写一条链接，多条请在列表的快速编辑中填写";

/// `withUnlinkedFields`: `next`, read from an edited link, plus what `previous` holds that its
/// own link, read back as `linked`, cannot carry.
pub fn with_unlinked_fields(
    next: &Map<String, Value>,
    previous: &Map<String, Value>,
    linked: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    match linked {
        Some(linked) if next.get("type") == previous.get("type") => {
            with_unlinked(next, previous, linked)
        }
        _ => next.clone(),
    }
}

fn with_unlinked(
    next: &Map<String, Value>,
    previous: &Map<String, Value>,
    linked: &Map<String, Value>,
) -> Map<String, Value> {
    let mut result = next.clone();
    for (key, value) in previous {
        match (linked.get(key), value, result.get(key)) {
            (None, _, _) => {
                result.entry(key.clone()).or_insert_with(|| value.clone());
            }
            (Some(Value::Object(carried)), Value::Object(value), Some(Value::Object(edited))) => {
                let merged = with_unlinked(edited, value, carried);
                result.insert(key.clone(), Value::Object(merged));
            }
            _ => {}
        }
    }
    result
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub line: usize,
    pub link: String,
    pub kept: Option<CustomProxy>,
}

/// A line that repeats a proxy's own link keeps that proxy as it is, since a link carries fewer
/// fields than the mapping.
#[derive(Clone, Debug)]
pub struct ShareLinkEdit {
    pub previous: Vec<CustomProxy>,
    pub links: Vec<String>,
    pub lines: Vec<Line>,
}

impl ShareLinkEdit {
    pub fn read(text: &str, previous: &[CustomProxy], links: &[String]) -> Self {
        let mut unused: HashMap<&str, Vec<&CustomProxy>> = HashMap::new();
        for (proxy, link) in previous.iter().zip(links) {
            if !link.is_empty() {
                unused.entry(link.as_str()).or_default().push(proxy);
            }
        }
        let mut lines = Vec::new();
        for (index, line) in text.split('\n').enumerate() {
            let link = line.trim();
            if link.is_empty() {
                continue;
            }
            let kept = unused
                .get_mut(link)
                .filter(|m| !m.is_empty())
                .map(|m| m.remove(0).clone());
            lines.push(Line {
                line: index + 1,
                link: link.to_string(),
                kept,
            });
        }
        ShareLinkEdit {
            previous: previous.to_vec(),
            links: links.to_vec(),
            lines,
        }
    }

    pub fn text_of(links: &[String]) -> String {
        links
            .iter()
            .filter(|l| !l.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn pending(&self) -> Vec<&Line> {
        self.lines.iter().filter(|l| l.kept.is_none()).collect()
    }

    fn kept_ids(&self) -> HashSet<&str> {
        self.lines
            .iter()
            .filter_map(|l| l.kept.as_ref().map(|p| p.id.as_str()))
            .collect()
    }

    fn linked_previous(&self) -> impl Iterator<Item = (&CustomProxy, &String)> {
        self.previous.iter().zip(&self.links)
    }

    /// The proxies whose link no line repeats, with that link, in the list's order.
    pub fn dropped_links(&self) -> Vec<(String, String)> {
        let kept = self.kept_ids();
        self.linked_previous()
            .filter(|(p, l)| !l.is_empty() && !kept.contains(p.id.as_str()))
            .map(|(p, l)| (p.id.clone(), l.clone()))
            .collect()
    }

    /// `apply`: a proxy named after one the edit dropped takes over its id, so each profile's
    /// dialer for it stays, and the options it listed without a value, which no link carries.
    /// Proxies without a link keep their place after the linked proxy they followed.
    pub fn apply(
        &self,
        decoded: &HashMap<usize, Vec<Value>>,
        group_names: &BTreeSet<String>,
        linked: &HashMap<String, Map<String, Value>>,
        mut new_id: impl FnMut() -> String,
    ) -> Vec<CustomProxy> {
        let kept = self.kept_ids();
        let mut dropped: HashMap<String, &CustomProxy> = self
            .linked_previous()
            .filter(|(p, l)| !l.is_empty() && !kept.contains(p.id.as_str()))
            .map(|(p, _)| (p.name(), p))
            .collect();
        let mut taken: HashSet<String> = RESERVED_NAMES.iter().map(|s| s.to_string()).collect();
        taken.extend(group_names.iter().cloned());
        taken.extend(
            self.linked_previous()
                .filter(|(_, l)| l.is_empty())
                .map(|(p, _)| p.name()),
        );
        taken.extend(
            self.lines
                .iter()
                .filter_map(|l| l.kept.as_ref().map(CustomProxy::name)),
        );
        let mut edited = Vec::new();
        for line in &self.lines {
            if let Some(kept) = &line.kept {
                edited.push(kept.clone());
                continue;
            }
            for definition in decoded.get(&line.line).into_iter().flatten() {
                let Some(map) = definition.as_object() else {
                    continue;
                };
                let base = link_proxy_name(definition);
                let replaced = if taken.contains(&base) {
                    None
                } else {
                    dropped.remove(&base)
                };
                let mut named = map.clone();
                named.insert("name".into(), free_proxy_name(&base, &mut taken).into());
                let proxy = match replaced {
                    Some(r) => {
                        let merged = with_unlinked_fields(&named, &r.map(), linked.get(&r.id));
                        CustomProxy::new(r.id.clone(), merged)
                    }
                    None => CustomProxy::new(new_id(), named),
                };
                edited.push(proxy);
            }
        }
        let mut following: HashMap<Option<String>, Vec<CustomProxy>> = HashMap::new();
        let mut anchor: Option<String> = None;
        for (proxy, link) in self.linked_previous() {
            if link.is_empty() {
                following
                    .entry(anchor.clone())
                    .or_default()
                    .push(proxy.clone());
            } else if kept.contains(proxy.id.as_str()) {
                anchor = Some(proxy.id.clone());
            }
        }
        let mut out = following.remove(&None).unwrap_or_default();
        for proxy in edited {
            let after = following.remove(&Some(proxy.id.clone()));
            out.push(proxy);
            out.extend(after.into_iter().flatten());
        }
        out
    }
}

/// `_confirmReplaced`: the question a quick edit that removes proxies asks, None when it
/// removes none.
pub fn replaced_message(previous: &[CustomProxy], next: &[CustomProxy]) -> Option<String> {
    let kept: HashSet<&str> = next.iter().map(|p| p.id.as_str()).collect();
    let removed: Vec<String> = previous
        .iter()
        .filter(|p| !kept.contains(p.id.as_str()))
        .map(CustomProxy::name)
        .collect();
    if removed.is_empty() {
        return None;
    }
    let existed: HashSet<&str> = previous.iter().map(|p| p.id.as_str()).collect();
    let added: Vec<String> = next
        .iter()
        .filter(|p| !existed.contains(p.id.as_str()))
        .map(CustomProxy::name)
        .collect();
    const TAIL: &str =
        "改了名字的行按新代理处理，自定义配置中对被删除代理的引用会标为缺失。确定应用吗？";
    Some(if added.is_empty() {
        format!("以下代理将被删除：{}。{TAIL}", removed.join(", "))
    } else {
        format!(
            "以下代理将被删除：{}。以下代理将新增：{}。{TAIL}",
            removed.join(", "),
            added.join(", ")
        )
    })
}

/// `_decodeShareLinks`' first check: every pending line read into at least one proxy.
pub fn decoded_lines(
    lines: &[&Line],
    results: &[Vec<Value>],
) -> Result<HashMap<usize, Vec<Value>>, String> {
    let mut out = HashMap::new();
    for (i, line) in lines.iter().enumerate() {
        match results.get(i) {
            Some(r) if !r.is_empty() => {
                out.insert(line.line, r.clone());
            }
            _ => return Err(line_issue(line.line, INVALID)),
        }
    }
    Ok(out)
}

/// The definitions of `decoded` in line order, each with its line, for `validate.proxies`.
pub fn entries(decoded: &HashMap<usize, Vec<Value>>) -> Vec<(usize, Value)> {
    let mut lines: Vec<_> = decoded.keys().copied().collect();
    lines.sort_unstable();
    lines
        .into_iter()
        .flat_map(|l| decoded[&l].iter().map(move |d| (l, d.clone())))
        .collect()
}

/// `convert.decodeLinks`' `results`: one list of definitions per line.
pub fn parse_decoded(r: &Value) -> Vec<Vec<Value>> {
    r.get("results")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .map(|l| l.as_array().cloned().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

/// `convert.encodeLinks`' `links`, one per proxy ("" for a proxy no link can carry).
pub fn parse_encoded(r: &Value, n: usize) -> Vec<String> {
    let mut out: Vec<String> = r
        .get("links")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|l| l.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default();
    out.resize(n, String::new());
    out
}

/// `_ShareLinkDialog.read`: the single proxy of an edited link, with what the form's own link
/// could not carry. `results` reads the edited link, then the form's own link when it had one.
pub fn single_definition(
    results: &[Vec<Value>],
    previous: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let first = results.first().cloned().unwrap_or_default();
    let definition = match first.as_slice() {
        [] => return Err(INVALID.into()),
        [one] => one
            .as_object()
            .cloned()
            .ok_or_else(|| INVALID.to_string())?,
        _ => return Err(SINGLE_ONLY.into()),
    };
    let linked = results
        .get(1)
        .filter(|r| r.len() == 1)
        .and_then(|r| r[0].as_object());
    Ok(with_unlinked_fields(&definition, previous, linked))
}

/// `_fillFromLink`'s name: an unnamed link keeps the form's name, otherwise the link's name
/// apart from the other proxies and the groups.
pub fn named_from_link(
    definition: Map<String, Value>,
    current: &CustomProxy,
    proxies: &[CustomProxy],
    group_names: &BTreeSet<String>,
) -> Map<String, Value> {
    let unnamed = matches!(definition.get("name"), None | Some(Value::Null))
        || definition.get("name") == Some(&Value::from(""));
    if unnamed && !current.name().is_empty() {
        let mut d = definition;
        d.insert("name".into(), current.name().into());
        return d;
    }
    let others: Vec<CustomProxy> = proxies
        .iter()
        .filter(|p| p.id != current.id)
        .cloned()
        .collect();
    crate::custom_proxies::imported(&[Value::Object(definition)], &others, group_names, || {
        String::new()
    })
    .pop()
    .map(|p| p.map())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(id: &str, def: Value) -> CustomProxy {
        CustomProxy::new(id.into(), def.as_object().unwrap().clone())
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn unlinked_fields_survive_an_edit_of_the_same_type() {
        let previous = obj(json!({"type": "ss", "name": "a", "udp": true,
            "plugin-opts": {"mode": "ws", "mux": true}}));
        let linked = obj(json!({"type": "ss", "name": "a", "plugin-opts": {"mode": "ws"}}));
        let next = obj(json!({"type": "ss", "name": "b", "plugin-opts": {"mode": "grpc"}}));
        let merged = with_unlinked_fields(&next, &previous, Some(&linked));
        assert_eq!(merged["name"], "b");
        assert_eq!(merged["udp"], true);
        assert_eq!(merged["plugin-opts"], json!({"mode": "grpc", "mux": true}));
        let other = obj(json!({"type": "vmess", "name": "b"}));
        assert_eq!(
            with_unlinked_fields(&other, &previous, Some(&linked)),
            other
        );
        assert_eq!(with_unlinked_fields(&next, &previous, None), next);
    }

    #[test]
    fn repeated_links_keep_their_proxies_and_the_rest_are_pending() {
        let previous = vec![
            p("1", json!({"name": "a", "type": "ss"})),
            p("2", json!({"name": "b", "type": "ss"})),
            p("3", json!({"name": "w", "type": "wireguard"})),
        ];
        let links = vec!["ss://a".to_string(), "ss://b".into(), "".into()];
        assert_eq!(ShareLinkEdit::text_of(&links), "ss://a\nss://b");
        let edit = ShareLinkEdit::read("ss://b\n\n trojan://c \nss://b", &previous, &links);
        assert_eq!(edit.lines.len(), 3);
        assert_eq!(edit.lines[0].kept.as_ref().unwrap().id, "2");
        assert_eq!(edit.lines[1].line, 3);
        assert_eq!(edit.lines[1].link, "trojan://c");
        assert!(edit.lines[2].kept.is_none());
        assert_eq!(edit.pending().len(), 2);
        assert_eq!(
            edit.dropped_links(),
            [("1".to_string(), "ss://a".to_string())]
        );
    }

    #[test]
    fn apply_names_apart_reuses_dropped_ids_and_keeps_unlinked_rows() {
        let previous = vec![
            p("w", json!({"name": "w", "type": "wireguard"})),
            p("1", json!({"name": "a", "type": "ss", "udp": true})),
            p("2", json!({"name": "b", "type": "ss"})),
            p("x", json!({"name": "x", "type": "wireguard"})),
        ];
        let links = vec!["".to_string(), "ss://a".into(), "ss://b".into(), "".into()];
        let edit = ShareLinkEdit::read("ss://b\nss://a2\nss://n", &previous, &links);
        let mut decoded = HashMap::new();
        decoded.insert(2, vec![json!({"name": "a", "type": "ss", "server": "s"})]);
        decoded.insert(3, vec![json!({"name": "", "type": "ss", "server": "w"})]);
        let mut linked = HashMap::new();
        linked.insert("1".to_string(), obj(json!({"name": "a", "type": "ss"})));
        let groups: BTreeSet<String> = ["G".to_string()].into();
        let mut n = 0;
        let out = edit.apply(&decoded, &groups, &linked, || {
            n += 1;
            format!("new{n}")
        });
        let names: Vec<String> = out.iter().map(CustomProxy::name).collect();
        assert_eq!(names, ["w", "b", "x", "a", "w-01"]);
        assert_eq!(out[3].id, "1");
        assert_eq!(out[3].definition["udp"], true);
        assert_eq!(out[4].id, "new1");
        assert_eq!(
            replaced_message(&previous, &out),
            None,
            "a took over its id, nothing removed"
        );
    }

    #[test]
    fn replaced_message_lists_removed_and_added() {
        let previous = vec![p("1", json!({"name": "a"})), p("2", json!({"name": "b"}))];
        let next = vec![p("2", json!({"name": "b"})), p("3", json!({"name": "c"}))];
        let m = replaced_message(&previous, &next).unwrap();
        assert!(m.starts_with("以下代理将被删除：a。以下代理将新增：c。"));
        let m = replaced_message(&previous, &previous[1..]).unwrap();
        assert!(m.starts_with("以下代理将被删除：a。改了名字"));
    }

    #[test]
    fn decoding_checks_every_line_and_orders_entries() {
        let a = Line {
            line: 4,
            link: "x".into(),
            kept: None,
        };
        let b = Line {
            line: 1,
            link: "y".into(),
            kept: None,
        };
        let r = parse_decoded(&json!({"results": [[{"name": "p"}], []]}));
        assert_eq!(
            decoded_lines(&[&a, &b], &r),
            Err("第 1 行：无法从链接中读取代理".into())
        );
        let r =
            parse_decoded(&json!({"results": [[{"name": "p"}], [{"name": "q"}, {"name": "r"}]]}));
        let d = decoded_lines(&[&a, &b], &r).unwrap();
        let e: Vec<usize> = entries(&d).iter().map(|(l, _)| *l).collect();
        assert_eq!(e, [1, 1, 4]);
        assert_eq!(parse_encoded(&json!({"links": ["a"]}), 2), ["a", ""]);
    }

    #[test]
    fn a_single_link_fills_the_form() {
        let previous = obj(json!({"name": "mine", "type": "ss", "udp": true}));
        let r = vec![
            vec![json!({"name": "", "type": "ss"})],
            vec![json!({"name": "mine", "type": "ss"})],
        ];
        let d = single_definition(&r, &previous).unwrap();
        assert_eq!(d["udp"], true);
        assert_eq!(single_definition(&[vec![]], &previous), Err(INVALID.into()));
        let two = vec![vec![json!({}), json!({})]];
        assert_eq!(single_definition(&two, &previous), Err(SINGLE_ONLY.into()));
        let current = p("1", json!({"name": "mine", "type": "ss"}));
        let named = named_from_link(d, &current, &[current.clone()], &BTreeSet::new());
        assert_eq!(named["name"], "mine");
        let others = vec![current.clone(), p("2", json!({"name": "hk"}))];
        let link = obj(json!({"name": "hk", "type": "ss"}));
        let named = named_from_link(link, &current, &others, &BTreeSet::new());
        assert_eq!(named["name"], "hk-01");
    }
}
