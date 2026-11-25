//! The proxies page: what `proxy.groups` holds, shaped as FlClash's proxies views show it
//! (lib/views/proxies and lib/providers/state/proxies.dart).

use crate::store::ProxiesStyle;
use crate::{NodeItem, ProxyGroup};
use serde_json::Value;
use slint::{ModelRc, VecModel};
use std::collections::{HashMap, HashSet};

/// Delay states kept beside the measured values (`DelayTestPhase`).
pub const DELAY_RUNNING: i32 = -3;
pub const DELAY_QUEUED: i32 = -4;

/// Built-in adapters whose delay probe always fails, so a timeout says nothing.
const UNPROBEABLE: &[&str] = &[
    "Reject",
    "RejectDrop",
    "Pass",
    "PassRule",
    "Rematch",
    "Compatible",
    "Dns",
];

/// One group of `proxy.groups`.
#[derive(Clone, Debug, Default)]
pub struct Group {
    pub name: String,
    pub kind: String,
    pub now: String,
    pub fixed: String,
    pub all: Vec<String>,
    pub test_url: String,
    pub hidden: bool,
}

impl Group {
    /// `GroupType.isComputedSelected`: the Core picks, a pin only steers it.
    pub fn computed(&self) -> bool {
        matches!(self.kind.as_str(), "URLTest" | "Fallback" | "Smart")
    }

    /// Whether a tap may change the pick.
    pub fn selectable(&self) -> bool {
        self.computed() || self.kind == "Selector"
    }
}

pub struct Groups {
    pub order: Vec<String>,
    pub groups: HashMap<String, Group>,
    pub kinds: HashMap<String, String>,
}

impl Groups {
    pub fn parse(v: &Value) -> Groups {
        let str_of =
            |g: &Value, k: &str| g.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let mut groups = HashMap::new();
        let mut kinds = HashMap::new();
        for g in v
            .get("groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let group = Group {
                name: str_of(g, "name"),
                kind: str_of(g, "type"),
                now: str_of(g, "now"),
                fixed: str_of(g, "fixed"),
                all: g
                    .get("all")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                test_url: str_of(g, "testUrl"),
                hidden: g.get("hidden").and_then(Value::as_bool).unwrap_or(false),
            };
            kinds.insert(group.name.clone(), group.kind.clone());
            groups.insert(group.name.clone(), group);
        }
        for n in v
            .get("nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let (Some(name), Some(kind)) = (
                n.get("name").and_then(Value::as_str),
                n.get("type").and_then(Value::as_str),
            ) {
                kinds.entry(name.into()).or_insert_with(|| kind.into());
            }
        }
        let order = v
            .get("order")
            .and_then(Value::as_array)
            .map(|o| {
                o.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Groups {
            order,
            groups,
            kinds,
        }
    }

    /// The groups the page lists in a mode: Global only GLOBAL, Rule every visible group
    /// but GLOBAL, Direct none.
    pub fn visible(&self, mode: i32) -> Vec<&Group> {
        if mode == 2 {
            return vec![];
        }
        self.order
            .iter()
            .filter_map(|n| self.groups.get(n))
            .filter(|g| {
                if mode == 1 {
                    g.name == "GLOBAL"
                } else {
                    g.name != "GLOBAL" && !g.hidden
                }
            })
            .collect()
    }

    /// `computeRealSelectedProxyState`: a group stands for the proxy it picks, through
    /// any nesting.
    pub fn real(&self, name: &str) -> String {
        let mut current = name.to_string();
        for _ in 0..16 {
            match self.groups.get(&current) {
                Some(g) if !g.now.is_empty() && g.now != current => current = g.now.clone(),
                Some(_) => return String::new(),
                None => return current,
            }
        }
        current
    }

    pub fn test_url(&self, group: &str) -> String {
        self.groups
            .get(group)
            .map(|g| g.test_url.clone())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| crate::profile::DEFAULT_TEST_URL.to_string())
    }
}

/// `SearchQuery.matches`: every word, case-insensitive.
pub fn matches(query: &str, name: &str) -> bool {
    let name = name.to_lowercase();
    query
        .split_whitespace()
        .all(|w| name.contains(&w.to_lowercase()))
}

fn priority(delay: i32) -> i32 {
    match delay {
        d if d > 0 => 0,
        0 => 1,
        -1 => 2,
        _ => 3,
    }
}

/// What the page shows: the groups with their nodes, searched, sorted and with timeouts
/// hidden as the style asks.
pub struct View<'a> {
    pub groups: &'a Groups,
    pub mode: i32,
    pub delays: &'a HashMap<String, i32>,
    /// The delays as the last finished test left them, which sorting reads so the
    /// cards do not jump while a test runs (`delaysAtLastTestBatch`).
    pub sorted_by: &'a HashMap<String, i32>,
    pub style: &'a ProxiesStyle,
    pub query: &'a str,
    pub testing: &'a HashSet<String>,
}

impl View<'_> {
    fn delay_of(&self, map: &HashMap<String, i32>, name: &str) -> i32 {
        let real = self.groups.real(name);
        if real.is_empty() {
            return 0;
        }
        map.get(&real).copied().unwrap_or(0)
    }

    pub fn build(&self) -> Vec<ProxyGroup> {
        let searching = !self.query.trim().is_empty();
        let mut out = Vec::new();
        for g in self.groups.visible(self.mode) {
            let mut names: Vec<&String> = g
                .all
                .iter()
                .filter(|n| !searching || matches(self.query, n))
                .collect();
            if searching && names.is_empty() {
                continue;
            }
            if self.style.hide_timeout {
                names.retain(|n| {
                    if **n == g.now || self.groups.groups.contains_key(*n) {
                        return true;
                    }
                    let kind = self.groups.kinds.get(*n).map(String::as_str).unwrap_or("");
                    UNPROBEABLE.contains(&kind) || self.delay_of(self.sorted_by, n) >= 0
                });
            }
            match self.style.sort {
                1 => names.sort_by_key(|n| {
                    let d = self.delay_of(self.sorted_by, n);
                    (priority(d), d, !self.groups.groups.contains_key(*n))
                }),
                2 => names.sort(),
                _ => {}
            }
            let nodes: Vec<NodeItem> = names
                .iter()
                .map(|n| {
                    let kind = self.groups.kinds.get(*n).cloned().unwrap_or_default();
                    let is_group = self.groups.groups.contains_key(*n);
                    let real = self.groups.real(n);
                    // `_ProxyDesc`: a group names the proxy it stands for.
                    let desc = if is_group && !real.is_empty() && &real != *n {
                        format!("{kind}({real})")
                    } else {
                        kind
                    };
                    NodeItem {
                        name: n.as_str().into(),
                        kind: desc.into(),
                        delay: self.delay_of(self.delays, n),
                        selected: **n == g.now,
                        pinned: g.computed() && **n == g.fixed,
                    }
                })
                .collect();
            let first = g.name.chars().next().map(String::from).unwrap_or_default();
            out.push(ProxyGroup {
                name: g.name.as_str().into(),
                kind: g.kind.as_str().into(),
                now: g.now.as_str().into(),
                count: g.all.len() as i32,
                monogram: first.into(),
                expanded: self.style.unfold.iter().any(|u| u == &g.name),
                testing: self.testing.contains(&g.name),
                nodes: ModelRc::new(VecModel::from(nodes)),
            });
        }
        out
    }
}

/// The proxies a group's delay test measures: its nodes (only the ones the search shows),
/// each as the proxy it stands for, once.
pub fn targets(groups: &Groups, group: &str, query: &str) -> Vec<String> {
    let Some(g) = groups.groups.get(group) else {
        return vec![];
    };
    let mut seen = HashSet::new();
    g.all
        .iter()
        .filter(|n| query.trim().is_empty() || matches(query, n))
        .map(|n| groups.real(n))
        .filter(|r| !r.is_empty() && seen.insert(r.clone()))
        .collect()
}

/// `getProxiesColumns` for a view this wide, in logical pixels.
pub fn columns(width: f32, layout: i32) -> i32 {
    let base = ((width.max(0.0) / 250.0).ceil() as i32).max(2);
    match layout {
        0 => base + 1,
        2 => base - 1,
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Groups {
        Groups::parse(&json!({
            "order": ["GLOBAL", "节点选择", "自动", "隐藏"],
            "groups": [
                {"name": "GLOBAL", "type": "Selector", "now": "节点选择", "all": ["节点选择", "DIRECT"], "testUrl": "", "hidden": false},
                {"name": "节点选择", "type": "Selector", "now": "自动", "all": ["自动", "香港", "日本", "美国"], "testUrl": "", "hidden": false},
                {"name": "自动", "type": "URLTest", "now": "日本", "fixed": "日本", "all": ["香港", "日本"], "testUrl": "https://t.example/", "hidden": false},
                {"name": "隐藏", "type": "Selector", "now": "香港", "all": ["香港"], "testUrl": "", "hidden": true}
            ],
            "nodes": [
                {"name": "香港", "type": "Shadowsocks"}, {"name": "日本", "type": "Trojan"},
                {"name": "美国", "type": "VMess"}, {"name": "DIRECT", "type": "Direct"}
            ]
        }))
    }

    fn view<'a>(
        g: &'a Groups,
        d: &'a HashMap<String, i32>,
        s: &'a ProxiesStyle,
        q: &'a str,
        t: &'a HashSet<String>,
    ) -> Vec<ProxyGroup> {
        View {
            groups: g,
            mode: 0,
            delays: d,
            sorted_by: d,
            style: s,
            query: q,
            testing: t,
        }
        .build()
    }

    fn names(g: &ProxyGroup) -> Vec<String> {
        use slint::Model;
        g.nodes.iter().map(|n| n.name.to_string()).collect()
    }

    #[test]
    fn groups_follow_the_mode() {
        let g = sample();
        let names = |m| {
            g.visible(m)
                .iter()
                .map(|x| x.name.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(0), ["节点选择", "自动"]);
        assert_eq!(names(1), ["GLOBAL"]);
        assert!(names(2).is_empty());
    }

    #[test]
    fn a_group_stands_for_its_pick() {
        let g = sample();
        assert_eq!(g.real("节点选择"), "日本");
        assert_eq!(g.real("香港"), "香港");
        let d = HashMap::from([("日本".to_string(), 120)]);
        let s = ProxiesStyle::default();
        let t = HashSet::new();
        let v = view(&g, &d, &s, "", &t);
        use slint::Model;
        let first = v[0].nodes.row_data(0).unwrap();
        assert_eq!(first.name, "自动");
        assert_eq!(first.kind, "URLTest(日本)");
        assert_eq!(first.delay, 120);
        assert!(first.selected);
        let auto = v[1].nodes.row_data(1).unwrap();
        assert!(auto.pinned && auto.selected);
        assert_eq!(targets(&g, "节点选择", ""), ["日本", "香港", "美国"]);
    }

    #[test]
    fn search_sort_and_hide() {
        let g = sample();
        let d = HashMap::from([
            ("香港".to_string(), 300),
            ("日本".to_string(), 80),
            ("美国".to_string(), -1),
        ]);
        let t = HashSet::new();
        let mut s = ProxiesStyle {
            sort: 1,
            ..Default::default()
        };
        let v = view(&g, &d, &s, "", &t);
        assert_eq!(names(&v[0]), ["自动", "日本", "香港", "美国"]);
        s.sort = 2;
        s.hide_timeout = true;
        let v = view(&g, &d, &s, "", &t);
        assert_eq!(names(&v[0]), ["日本", "自动", "香港"]);
        let v = view(&g, &d, &ProxiesStyle::default(), "香", &t);
        assert_eq!(v.len(), 2);
        assert_eq!(names(&v[0]), ["香港"]);
        assert_eq!(targets(&g, "节点选择", "日"), ["日本"]);
    }

    #[test]
    fn columns_follow_the_layout() {
        assert_eq!(columns(380.0, 1), 2);
        assert_eq!(columns(380.0, 0), 3);
        assert_eq!(columns(380.0, 2), 1);
        assert_eq!(columns(900.0, 1), 4);
    }
}
