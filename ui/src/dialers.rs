//! The custom profile's dialers (lib/views/profiles/custom/dialers.dart and the dialer part
//! of lib/providers/state/custom.dart): a custom proxy a profile names may dial through
//! another of its proxies or groups, as long as that dial does not lead back to it.

use crate::custom_profile::ProxyDialer;
use crate::proxy_groups::{self as pg, Data, Issue};
use crate::store::State;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// `_takesInEveryProxy`: what a filter picks is left to the core.
fn takes_in_every_proxy(def: &Value) -> bool {
    pg::includes_all(def, "include-all-proxies")
        && ["filter", "exclude-filter", "exclude-type"]
            .iter()
            .all(|k| pg::text(def, k).is_empty())
}

/// `_DialerGraph`: the groups' members, each declared proxy's dialer as its one member, and
/// an include-all group taking in every proxy that dials.
struct Graph {
    declared: HashSet<String>,
    chained: HashMap<String, String>,
    members: HashMap<String, Vec<String>>,
    group_names: HashSet<String>,
    taking_every: HashSet<String>,
    parents: HashMap<String, Vec<String>>,
}

impl Graph {
    fn new(groups: &[Value], dialers: &BTreeMap<String, String>) -> Graph {
        let declared: HashSet<String> = groups
            .iter()
            .flat_map(|g| pg::names(g, "proxies"))
            .collect();
        let mut chained = HashMap::new();
        let mut members: HashMap<String, Vec<String>> = HashMap::new();
        for (name, dialer) in dialers {
            if declared.contains(name) {
                chained.insert(name.clone(), dialer.clone());
                members.insert(name.clone(), vec![dialer.clone()]);
            }
        }
        let mut group_names = HashSet::new();
        let mut taking_every = HashSet::new();
        for g in groups {
            let n = pg::name(g);
            let takes_all = takes_in_every_proxy(g);
            let mut items = pg::names(g, "proxies");
            if takes_all {
                items.extend(chained.keys().cloned());
            }
            members.insert(n.clone(), items);
            group_names.insert(n.clone());
            if takes_all {
                taking_every.insert(n);
            } else {
                taking_every.remove(&n);
            }
        }
        let mut parents: HashMap<String, Vec<String>> = HashMap::new();
        for (name, items) in &members {
            for item in items {
                parents.entry(item.clone()).or_default().push(name.clone());
            }
        }
        Graph {
            declared,
            chained,
            members,
            group_names,
            taking_every,
            parents,
        }
    }

    /// `loopsOf`: the proxy's own dialer is the dial being checked, never a way back.
    fn loop_of(&self, proxy: &str, target: &str) -> BTreeSet<String> {
        if !self.declared.contains(proxy) {
            return BTreeSet::new();
        }
        if target == proxy {
            return [proxy.to_string()].into();
        }
        let passes_through = self.group_names.contains(proxy);
        let taken_in = !self.chained.contains_key(proxy);
        let mut reaching: HashSet<String> = HashSet::new();
        let mut pending: Vec<String> = self.parents.get(proxy).cloned().unwrap_or_default();
        if taken_in {
            pending.extend(self.taking_every.iter().cloned());
        }
        while let Some(n) = pending.pop() {
            if (passes_through || n != proxy) && reaching.insert(n.clone()) {
                pending.extend(self.parents.get(&n).into_iter().flatten().cloned());
            }
        }
        let mut found = BTreeSet::new();
        let mut stack = vec![target.to_string()];
        while let Some(n) = stack.pop() {
            if !reaching.contains(&n) || !found.insert(n.clone()) {
                continue;
            }
            stack.extend(self.members.get(&n).into_iter().flatten().cloned());
            if taken_in && self.taking_every.contains(&n) {
                stack.push(proxy.to_string());
            }
        }
        found
    }

    /// Everything `from` reaches through members, itself included.
    fn reach(&self, from: &str) -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut stack = vec![from.to_string()];
        while let Some(n) = stack.pop() {
            if seen.insert(n.clone()) {
                stack.extend(self.members.get(&n).into_iter().flatten().cloned());
            }
        }
        seen
    }

    fn known(&self, n: &str) -> bool {
        self.members.contains_key(n) || self.parents.contains_key(n)
    }
}

/// `dialerLoop`: `target` and what under it leads back to `proxy`; empty when the dial does
/// not loop.
pub fn dialer_loop(
    target: &str,
    groups: &[Value],
    proxy: &str,
    dialers: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    Graph::new(groups, dialers).loop_of(proxy, target)
}

/// `_DialLoops`: the dials that loop, and for each group the dials whose loop passes it.
#[derive(Default, Debug)]
pub struct DialLoops {
    pub looping: BTreeSet<String>,
    pub through: BTreeMap<String, Vec<(String, String)>>,
}

pub fn dial_loops(groups: &[Value], dialers: &BTreeMap<String, String>) -> DialLoops {
    let mut out = DialLoops::default();
    if dialers.is_empty() {
        return out;
    }
    let graph = Graph::new(groups, dialers);
    let reaches: HashMap<&str, HashSet<String>> = graph
        .members
        .keys()
        .map(|n| (n.as_str(), graph.reach(n)))
        .collect();
    for (proxy, target) in dialers.iter().filter(|(p, _)| graph.declared.contains(*p)) {
        let from_target = if graph.known(target) {
            graph.reach(target)
        } else {
            HashSet::new()
        };
        if target == proxy || from_target.contains(proxy) {
            out.looping.insert(proxy.clone());
        }
        for g in &graph.group_names {
            let reaches_proxy = reaches.get(g.as_str()).is_some_and(|r| r.contains(proxy));
            let on_loop = || {
                graph.members[g]
                    .iter()
                    .any(|m| reaches.get(m.as_str()).is_some_and(|r| r.contains(g)))
            };
            let through = if target == proxy {
                g == proxy
            } else {
                (if g == proxy { on_loop() } else { reaches_proxy }) && from_target.contains(g)
            };
            if through {
                out.through
                    .entry(g.clone())
                    .or_default()
                    .push((proxy.clone(), target.clone()));
            }
        }
    }
    out
}

/// `proxyDialerIssues`: a dialer must exist and must not lead back.
pub fn issues(proxy: &str, target: &str, data: &Data) -> Vec<Issue> {
    if !data.is_rule_target(target) {
        return vec![Issue::MissingDialer(target.into())];
    }
    let groups: Vec<Value> = data.groups.iter().map(|(_, g)| g.clone()).collect();
    if !dialer_loop(target, &groups, proxy, &data.dialers).is_empty() {
        return vec![Issue::DialerLoop(proxy.into(), target.into())];
    }
    Vec::new()
}

/// The profile's dialers by custom proxy name, for the proxies that still exist.
pub fn by_name(state: &State, profile_id: &str) -> BTreeMap<String, String> {
    state
        .proxy_dialers
        .iter()
        .filter(|d| d.profile_id == profile_id)
        .filter_map(|d| {
            let p = state
                .settings
                .custom_proxies
                .iter()
                .find(|p| p.id == d.proxy_id)?;
            Some((p.name(), d.target.clone()))
        })
        .collect()
}

/// The custom proxies the profile names (`namedProxies`), each with its dialer.
pub fn rows(state: &State, profile_id: &str) -> Vec<(String, String, Option<String>)> {
    let data = pg::data(state, profile_id);
    let named = data.named_proxies();
    state
        .settings
        .custom_proxies
        .iter()
        .filter(|p| named.contains(&p.name()))
        .map(|p| {
            let target = state
                .proxy_dialers
                .iter()
                .find(|d| d.profile_id == profile_id && d.proxy_id == p.id)
                .map(|d| d.target.clone());
            (p.id.clone(), p.name(), target)
        })
        .collect()
}

/// `customProfileIssues.dialers`: each named, dialing proxy's issues, by proxy id.
pub fn profile_issues(state: &State, profile_id: &str) -> BTreeMap<String, Vec<Issue>> {
    let data = pg::data(state, profile_id);
    rows(state, profile_id)
        .into_iter()
        .filter_map(|(id, name, target)| {
            let found = issues(&name, &target?, &data);
            (!found.is_empty()).then_some((id, found))
        })
        .collect()
}

/// `_dialerCount`: the dialers whose proxy the profile still names.
pub fn count(state: &State, profile_id: &str) -> usize {
    let named = pg::data(state, profile_id).named_proxies();
    by_name(state, profile_id)
        .keys()
        .filter(|n| named.contains(*n))
        .count()
}

/// `ProxyDialers.set`: None takes the proxy's dialer off.
pub fn set(state: &mut State, profile_id: &str, proxy_id: &str, target: Option<String>) {
    state
        .proxy_dialers
        .retain(|d| d.profile_id != profile_id || d.proxy_id != proxy_id);
    if let Some(target) = target {
        state.proxy_dialers.push(ProxyDialer {
            profile_id: profile_id.into(),
            proxy_id: proxy_id.into(),
            target,
        });
    }
}

/// The dialer sheet's choices (`_handleSelect`): proxies and groups whose dial would not
/// come back to the proxy, as (name, type).
pub fn choices(
    state: &State,
    profile_id: &str,
    proxy: &str,
) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let data = pg::data(state, profile_id);
    let groups: Vec<Value> = data.groups.iter().map(|(_, g)| g.clone()).collect();
    let graph = Graph::new(&groups, &data.dialers);
    let loops = |t: &str| !graph.loop_of(proxy, t).is_empty();
    let named = data.named_proxies();
    let nodes = state
        .settings
        .custom_proxies
        .iter()
        .filter(|p| named.contains(&p.name()) && !loops(&p.name()))
        .map(|p| (p.name(), p.kind()))
        .collect();
    let group_types = data
        .groups
        .iter()
        .filter(|(_, g)| !loops(&pg::name(g)))
        .map(|(_, g)| (pg::name(g), pg::type_name(&pg::kind(g))))
        .collect();
    (nodes, group_types)
}

/// `appProxiesPayload`'s dialers: the named proxies' dialers, less those that loop.
pub fn payload_dialers(
    groups: &[Value],
    dialers: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let named: HashSet<String> = groups
        .iter()
        .flat_map(|g| pg::names(g, "proxies"))
        .collect();
    let chain: BTreeMap<String, String> = dialers
        .iter()
        .filter(|(n, _)| named.contains(*n))
        .map(|(n, t)| (n.clone(), t.clone()))
        .collect();
    let looping = dial_loops(groups, &chain).looping;
    chain
        .into_iter()
        .filter(|(n, _)| !looping.contains(n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn group(name: &str, proxies: &[&str]) -> Value {
        json!({"name": name, "type": "select", "proxies": proxies})
    }

    fn dialers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn set_of(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn landing() -> Value {
        group("Landing", &["Home"])
    }

    fn relay() -> Value {
        group("Relay", &["Node"])
    }

    #[test]
    fn a_dial_loops_only_through_groups_that_reach_the_proxy() {
        let groups = [landing(), relay(), group("Outer", &["Relay", "Landing"])];
        let none = BTreeMap::new();
        let lp = |t: &str| dialer_loop(t, &groups, "Home", &none);
        assert!(lp("Relay").is_empty() && lp("Node").is_empty());
        assert_eq!(lp("Landing"), set_of(&["Landing"]));
        assert_eq!(lp("Outer"), set_of(&["Outer", "Landing"]));
        assert!(dialer_loop("Landing", &groups, "Work", &none).is_empty());
    }

    #[test]
    fn a_dial_loops_through_the_dialers_of_the_proxies_on_its_way() {
        let groups = [group("Exit", &["Home", "Work", "Edge"])];
        let lp = |t: &str, d: &[(&str, &str)]| dialer_loop(t, &groups, "Home", &dialers(d));
        assert_eq!(lp("Home", &[]), set_of(&["Home"]));
        assert!(lp("Work", &[("Work", "Edge")]).is_empty());
        assert_eq!(lp("Work", &[("Work", "Home")]), set_of(&["Work"]));
        assert_eq!(
            lp("Work", &[("Work", "Edge"), ("Edge", "Exit")]),
            set_of(&["Work", "Edge", "Exit"])
        );
        assert_eq!(
            lp(
                "Work",
                &[("Work", "Home"), ("Home", "Edge"), ("Gone", "Home")]
            ),
            set_of(&["Work"])
        );
        assert!(lp("Work", &[("Work", "Away"), ("Away", "Home")]).is_empty());
    }

    #[test]
    fn include_all_proxies_takes_in_only_declared_proxies_and_leaves_filters_to_the_core() {
        let all = json!({"name": "All", "type": "select", "include-all-proxies": true});
        let none = BTreeMap::new();
        assert!(dialer_loop("All", &[all.clone()], "Home", &none).is_empty());
        assert_eq!(
            dialer_loop("All", &[all.clone(), landing()], "Home", &none),
            set_of(&["All"])
        );
        assert!(dialer_loop("Relay", &[all.clone(), landing(), relay()], "Home", &none).is_empty());
        assert_eq!(
            dialer_loop(
                "Exit",
                &[group("Exit", &["Work"]), all.clone(), landing()],
                "Home",
                &dialers(&[("Work", "All")])
            ),
            set_of(&["Exit", "Work", "All"])
        );
        for (k, v) in [
            ("filter", "(?i)hk"),
            ("exclude-filter", "Home"),
            ("exclude-type", "ss"),
        ] {
            let mut g = all.clone();
            g[k] = v.into();
            assert!(dialer_loop("All", &[g, landing()], "Home", &none).is_empty());
        }
    }

    fn data() -> Data {
        Data {
            groups: vec![(3, landing()), (4, relay())],
            proxies: ["Home", "Work", "Loop"].map(String::from).into(),
            dialers: dialers(&[("Home", "Relay")]),
            ..Default::default()
        }
    }

    #[test]
    fn a_dialer_must_exist_and_must_not_lead_back() {
        let d = data();
        assert!(issues("Home", "Relay", &d).is_empty());
        assert_eq!(
            issues("Home", "Work", &d),
            [Issue::MissingDialer("Work".into())]
        );
        assert_eq!(
            issues("Home", "Gone", &d),
            [Issue::MissingDialer("Gone".into())]
        );
        assert_eq!(
            issues("Home", "Landing", &d),
            [Issue::DialerLoop("Home".into(), "Landing".into())]
        );
        let mut named = d.clone();
        named.groups[0].1 = group("Landing", &["Home", "Work"]);
        assert!(issues("Home", "Work", &named).is_empty());
    }

    #[test]
    fn a_group_edit_that_closes_a_loop_is_flagged_on_the_group() {
        let mut d = data();
        let loops = |id: u64, def: &Value, d: &Data| -> Vec<Issue> {
            pg::issues(id, def, d)
                .into_iter()
                .filter(|i| matches!(i, Issue::DialerLoop(..)))
                .collect()
        };
        assert!(loops(4, &relay(), &d).is_empty());
        let closing = group("Relay", &["Node", "Landing"]);
        assert_eq!(
            loops(4, &closing, &d),
            [Issue::DialerLoop("Home".into(), "Relay".into())]
        );
        d.groups = vec![
            (6, group("Twin", &["Home"])),
            (7, group("Twin", &["DIRECT"])),
        ];
        d.dialers = dialers(&[("Home", "Twin")]);
        let issue = Issue::DialerLoop("Home".into(), "Twin".into());
        assert!(pg::issues(6, &d.groups[0].1.clone(), &d).contains(&issue));
        assert!(!pg::issues(7, &d.groups[1].1.clone(), &d).contains(&issue));
        let found = pg::profile_issues(&d);
        assert!(found[&6].contains(&issue) && !found[&7].contains(&issue));
    }

    #[test]
    fn the_payload_keeps_only_dialers_that_do_not_loop() {
        let named = [group("Landing", &["Home", "Work"]), relay()];
        assert_eq!(
            payload_dialers(
                &named,
                &dialers(&[("Home", "Landing"), ("Unnamed", "Relay")])
            ),
            BTreeMap::new()
        );
        assert!(
            payload_dialers(&named[..1], &dialers(&[("Home", "Work"), ("Work", "Home")]))
                .is_empty()
        );
        assert_eq!(
            payload_dialers(&[landing(), relay()], &dialers(&[("Home", "Relay")])),
            dialers(&[("Home", "Relay")])
        );
        let all = json!({"name": "All", "type": "select", "include-all-proxies": true});
        assert!(payload_dialers(&[all, landing()], &dialers(&[("Home", "All")])).is_empty());
    }

    #[test]
    fn dialers_follow_their_profile_and_count_only_named_proxies() {
        let mut s = State::default();
        s.profiles
            .push(crate::custom_profile::new_profile("p".into(), "c", 1));
        for (id, n) in [("1", "Home"), ("2", "Work"), ("3", "Loop")] {
            s.settings
                .custom_proxies
                .push(crate::custom_proxies::CustomProxy {
                    id: id.into(),
                    definition: json!({"name": n, "type": "ss"}),
                });
        }
        s.proxy_groups.push(crate::custom_profile::ProxyGroupRow {
            id: 7,
            profile_id: "p".into(),
            definition: group("Landing", &["Home", "Work", "Loop"]),
            order: None,
        });
        s.proxy_groups.push(crate::custom_profile::ProxyGroupRow {
            id: 8,
            profile_id: "p".into(),
            definition: group("Relay", &["DIRECT"]),
            order: None,
        });
        set(&mut s, "p", "1", Some("Gone".into()));
        set(&mut s, "p", "2", Some("Relay".into()));
        set(&mut s, "p", "3", Some("Landing".into()));
        set(&mut s, "q", "3", Some("Relay".into()));
        assert_eq!(
            by_name(&s, "p"),
            dialers(&[("Home", "Gone"), ("Work", "Relay"), ("Loop", "Landing")])
        );
        let found = profile_issues(&s, "p");
        assert_eq!(found["1"], [Issue::MissingDialer("Gone".into())]);
        assert_eq!(
            found["3"],
            [Issue::DialerLoop("Loop".into(), "Landing".into())]
        );
        assert_eq!(found.len(), 2);
        assert_eq!(count(&s, "p"), 3);
        let (nodes, groups) = choices(&s, "p", "Home");
        assert_eq!(
            nodes.iter().map(|n| n.0.as_str()).collect::<Vec<_>>(),
            ["Work"]
        );
        assert_eq!(groups, [("Relay".to_string(), "Selector".to_string())]);
        s.proxy_groups[0].definition = group("Landing", &["DIRECT"]);
        assert!(profile_issues(&s, "p").is_empty());
        assert_eq!(count(&s, "p"), 0);
        set(&mut s, "p", "1", None);
        assert_eq!(s.proxy_dialers.len(), 3);
    }
}
