//! lib/common/mitm_network.dart: the DNS, TUN and system proxy settings a module changes, as
//! `mitm.readModule` reports them under `network`, merged over the enabled modules.

use serde_json::{Map, Value};

/// `MitmNetworkKey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Nameserver,
    DefaultNameserver,
    FakeIpFilter,
    NameserverPolicy,
    Hosts,
    RouteAddress,
    RouteExcludeAddress,
    BypassDomain,
}

impl Key {
    pub fn path(self) -> &'static str {
        match self {
            Key::Nameserver => "nameserver",
            Key::DefaultNameserver => "default-nameserver",
            Key::FakeIpFilter => "fake-ip-filter",
            Key::NameserverPolicy => "nameserver-policy",
            Key::Hosts => "hosts",
            Key::RouteAddress => "route-address",
            Key::RouteExcludeAddress => "route-exclude-address",
            Key::BypassDomain => "bypass-domain",
        }
    }

    /// `mitmNetworkChangeTitle`.
    pub fn title(self) -> &'static str {
        match self {
            Key::Nameserver => "Nameserver",
            Key::DefaultNameserver => "Default Nameserver",
            Key::FakeIpFilter => "Fake-IP过滤",
            Key::NameserverPolicy => "Nameserver Policy",
            Key::Hosts => "Hosts",
            Key::RouteAddress => "路由地址",
            Key::RouteExcludeAddress => "排除路由地址",
            Key::BypassDomain => "排除域名",
        }
    }
}

/// `MitmListEdit`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListEdit {
    pub replace: bool,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

fn push_unique(out: &mut Vec<String>, items: &[String]) {
    for i in items {
        if !out.contains(i) {
            out.push(i.clone());
        }
    }
}

impl ListEdit {
    pub fn values(&self) -> Vec<String> {
        [self.before.clone(), self.after.clone()].concat()
    }

    pub fn apply_to(&self, current: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        push_unique(&mut out, &self.before);
        if !self.replace {
            push_unique(&mut out, current);
        }
        push_unique(&mut out, &self.after);
        out
    }

    pub fn then(&self, next: &ListEdit) -> ListEdit {
        if next.replace {
            return next.clone();
        }
        ListEdit {
            replace: self.replace,
            before: [next.before.clone(), self.before.clone()].concat(),
            after: [self.after.clone(), next.after.clone()].concat(),
        }
    }
}

fn list_of(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .map(|i| match i {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Per-domain entries in the order they first appear; a later one takes the value.
type Entries = Vec<(String, Vec<String>)>;

fn put(entries: &mut Entries, key: &str, values: &[String]) {
    match entries.iter_mut().find(|(k, _)| k == key) {
        Some((_, v)) => *v = values.to_vec(),
        None => entries.push((key.to_string(), values.to_vec())),
    }
}

/// `MitmNetwork`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Network {
    pub nameserver: Option<ListEdit>,
    pub default_nameserver: Option<ListEdit>,
    pub fake_ip_filter: Option<ListEdit>,
    pub nameserver_policy: Entries,
    pub hosts: Entries,
    pub route_address: Vec<String>,
    pub route_exclude_address: Option<ListEdit>,
    pub bypass_domain: Option<ListEdit>,
}

/// `MitmNetworkChange`: one setting a module changes, a per-domain entry or a list edit.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub key: Key,
    pub entry: Option<String>,
    pub values: Vec<String>,
    pub edit: Option<ListEdit>,
}

const LIST_KEYS: [Key; 5] = [
    Key::Nameserver,
    Key::DefaultNameserver,
    Key::FakeIpFilter,
    Key::RouteExcludeAddress,
    Key::BypassDomain,
];

impl Network {
    /// `MitmNetwork.fromCoreJson`: the Core's `MitmModuleNetwork`.
    pub fn from_core_json(v: &Value) -> Network {
        let edit = |key: &str| {
            v.get(key).and_then(Value::as_object).map(|e| ListEdit {
                replace: e.get("replace").and_then(Value::as_bool) == Some(true),
                before: list_of(e.get("before")),
                after: list_of(e.get("after")),
            })
        };
        let entries = |key: &str| {
            let mut out = Entries::new();
            for e in v.get(key).and_then(Value::as_array).into_iter().flatten() {
                if e.is_object() {
                    let domain = match e.get("domain") {
                        Some(Value::String(s)) => s.clone(),
                        Some(other) => other.to_string(),
                        None => "null".into(),
                    };
                    put(&mut out, &domain, &list_of(e.get("values")));
                }
            }
            out
        };
        Network {
            nameserver: edit("nameserver"),
            default_nameserver: edit("defaultNameserver"),
            fake_ip_filter: edit("fakeIpFilter"),
            nameserver_policy: entries("nameserverPolicy"),
            hosts: entries("hosts"),
            route_address: list_of(v.get("routeAddress")),
            route_exclude_address: edit("routeExcludeAddress"),
            bypass_domain: edit("bypassDomain"),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes().is_empty()
    }

    pub fn edit_of(&self, key: Key) -> Option<&ListEdit> {
        match key {
            Key::Nameserver => self.nameserver.as_ref(),
            Key::DefaultNameserver => self.default_nameserver.as_ref(),
            Key::FakeIpFilter => self.fake_ip_filter.as_ref(),
            Key::RouteExcludeAddress => self.route_exclude_address.as_ref(),
            Key::BypassDomain => self.bypass_domain.as_ref(),
            _ => None,
        }
    }

    pub fn changes(&self) -> Vec<Change> {
        let edit = |key: Key| {
            self.edit_of(key).map(|e| Change {
                key,
                entry: None,
                values: Vec::new(),
                edit: Some(e.clone()),
            })
        };
        let entry = |key: Key, (k, v): &(String, Vec<String>)| Change {
            key,
            entry: Some(k.clone()),
            values: v.clone(),
            edit: None,
        };
        let mut out: Vec<Change> = LIST_KEYS[..3].iter().filter_map(|k| edit(*k)).collect();
        out.extend(
            self.nameserver_policy
                .iter()
                .map(|e| entry(Key::NameserverPolicy, e)),
        );
        out.extend(self.hosts.iter().map(|e| entry(Key::Hosts, e)));
        if !self.route_address.is_empty() {
            out.push(Change {
                key: Key::RouteAddress,
                entry: None,
                values: self.route_address.clone(),
                edit: None,
            });
        }
        out.extend(LIST_KEYS[3..].iter().filter_map(|k| edit(*k)));
        out
    }

    /// `MitmNetwork.merge`: later modules go over earlier ones.
    pub fn merge<'a>(modules: impl IntoIterator<Item = &'a Network>) -> Network {
        let modules: Vec<&Network> = modules.into_iter().collect();
        let fold = |of: fn(&Network) -> Option<&ListEdit>| {
            modules
                .iter()
                .fold(None::<ListEdit>, |merged, m| match (merged, of(m)) {
                    (Some(a), Some(b)) => Some(a.then(b)),
                    (None, Some(b)) => Some(b.clone()),
                    (a, None) => a,
                })
        };
        let mut merged = Network {
            nameserver: fold(|m| m.nameserver.as_ref()),
            default_nameserver: fold(|m| m.default_nameserver.as_ref()),
            fake_ip_filter: fold(|m| m.fake_ip_filter.as_ref()),
            route_exclude_address: fold(|m| m.route_exclude_address.as_ref()),
            bypass_domain: fold(|m| m.bypass_domain.as_ref()),
            ..Default::default()
        };
        for m in &modules {
            for (k, v) in &m.nameserver_policy {
                put(&mut merged.nameserver_policy, k, v);
            }
            for (k, v) in &m.hosts {
                put(&mut merged.hosts, k, v);
            }
            push_unique(&mut merged.route_address, &m.route_address);
        }
        merged
    }

    /// `applyToTun`: included routes only widen a list that names any, since an empty one
    /// already routes everything.
    pub fn apply_to_tun(&self, tun: &mut Map<String, Value>) {
        let route = Key::RouteAddress.path();
        let included = list_of(tun.get(route));
        if !self.route_address.is_empty() && !included.is_empty() {
            let mut all = Vec::new();
            push_unique(&mut all, &included);
            push_unique(&mut all, &self.route_address);
            tun.insert(route.into(), all.into());
        }
        if let Some(edit) = &self.route_exclude_address {
            let exclude = Key::RouteExcludeAddress.path();
            let current = list_of(tun.get(exclude));
            tun.insert(exclude.into(), edit.apply_to(&current).into());
        }
    }

    /// `vpnRouteAddress`'s share: Android's VPN adds a module's routes beside its own rows.
    pub fn vpn_route_address(&self, own: &[String]) -> Vec<String> {
        if own.is_empty() {
            return Vec::new();
        }
        let mut all = Vec::new();
        push_unique(&mut all, own);
        push_unique(&mut all, &self.route_address);
        all
    }
}

impl Change {
    /// Sets the whole of what it changes, so whatever stood before no longer counts.
    pub fn replaces(&self) -> bool {
        self.entry.is_some() || self.edit.as_ref().is_some_and(|e| e.replace)
    }

    pub fn replaced_by(&self, later: &Network) -> bool {
        let has = |entries: &Entries| {
            self.entry
                .as_ref()
                .is_some_and(|e| entries.iter().any(|(k, _)| k == e))
        };
        match self.key {
            Key::NameserverPolicy => has(&later.nameserver_policy),
            Key::Hosts => has(&later.hosts),
            key => later.edit_of(key).is_some_and(|e| e.replace),
        }
    }

    /// `mitmNetworkChangeValue`.
    pub fn value(&self) -> String {
        if let Some(entry) = &self.entry {
            return format!("{entry} → {}", self.values.join(", "));
        }
        let Some(edit) = &self.edit else {
            return format!("添加 {}", self.values.join(", "));
        };
        if edit.replace {
            return format!("替换为 {}", edit.values().join(", "));
        }
        let mut parts = Vec::new();
        if !edit.before.is_empty() {
            parts.push(format!("在前面添加 {}", edit.before.join(", ")));
        }
        if !edit.after.is_empty() {
            parts.push(format!("添加 {}", edit.after.join(", ")));
        }
        parts.join("\n")
    }

    /// `mitmChangeOverridesUser`: a list goes over the user's only when it replaces it whole,
    /// a per-domain entry only where the user wrote that domain.
    pub fn overrides_user(&self, user: &UserNetwork) -> bool {
        let dns = |k: &str| user.dns_keys.iter().any(|d| d == k);
        let entry = |names: &[String]| self.entry.as_ref().is_some_and(|e| names.contains(e));
        match self.key {
            Key::Nameserver | Key::DefaultNameserver | Key::FakeIpFilter => {
                self.replaces() && dns(self.key.path())
            }
            Key::NameserverPolicy => {
                dns(Key::NameserverPolicy.path()) && entry(&user.nameserver_policy)
            }
            Key::Hosts => entry(&user.hosts),
            Key::RouteExcludeAddress => self.replaces() && user.tun_route_exclude,
            Key::BypassDomain => self.replaces(),
            Key::RouteAddress => false,
        }
    }
}

/// What of the user's own settings a module can go over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserNetwork {
    /// The DNS override keys set for the current profile.
    pub dns_keys: Vec<String>,
    pub nameserver_policy: Vec<String>,
    pub hosts: Vec<String>,
    /// A desktop profile that overrides `route-exclude-address`; Android's VPN reads its own rows.
    pub tun_route_exclude: bool,
}

/// `MitmNetworkChangeState`: each change of a module with whether it goes over the user's
/// setting and the name of the last enabled module after it that replaces it.
pub fn change_states(
    network: &Network,
    later: &[(String, Network)],
    user: &UserNetwork,
) -> Vec<(Change, bool, Option<String>)> {
    network
        .changes()
        .into_iter()
        .map(|c| {
            let by = later
                .iter()
                .rev()
                .find(|(_, n)| c.replaced_by(n))
                .map(|(name, _)| name.clone());
            let yours = c.overrides_user(user);
            (c, yours, by)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn list_edits_chain_and_apply() {
        let a = ListEdit {
            replace: false,
            before: s(&["a"]),
            after: s(&["z"]),
        };
        let b = ListEdit {
            replace: false,
            before: s(&["b"]),
            after: s(&["y"]),
        };
        let ab = a.then(&b);
        assert_eq!(ab.before, s(&["b", "a"]));
        assert_eq!(ab.after, s(&["z", "y"]));
        assert_eq!(ab.apply_to(&s(&["m", "a"])), s(&["b", "a", "m", "z", "y"]));
        let r = ListEdit {
            replace: true,
            before: s(&["r"]),
            after: vec![],
        };
        assert_eq!(ab.then(&r), r);
        assert_eq!(r.apply_to(&s(&["m"])), s(&["r"]));
    }

    #[test]
    fn modules_merge_and_widen_the_tun() {
        let one = Network::from_core_json(&json!({
            "routeAddress": ["10.0.0.0/8"],
            "routeExcludeAddress": {"before": ["1.1.1.1/32"]},
            "hosts": [{"domain": "a.test", "values": ["1.2.3.4"]}],
        }));
        let two = Network::from_core_json(&json!({
            "routeAddress": ["10.0.0.0/8", "172.16.0.0/12"],
            "routeExcludeAddress": {"after": "2.2.2.2/32"},
            "hosts": [{"domain": "a.test", "values": ["5.6.7.8"]}],
            "nameserver": {"replace": true, "after": ["https://dns.test"]},
        }));
        let merged = Network::merge([&one, &two]);
        assert_eq!(merged.route_address, s(&["10.0.0.0/8", "172.16.0.0/12"]));
        assert_eq!(merged.hosts, vec![("a.test".into(), s(&["5.6.7.8"]))]);
        let mut tun = json!({"route-exclude-address": ["9.9.9.9/32"]})
            .as_object()
            .cloned()
            .unwrap();
        merged.apply_to_tun(&mut tun);
        assert!(tun.get("route-address").is_none());
        assert_eq!(
            tun["route-exclude-address"],
            json!(["1.1.1.1/32", "9.9.9.9/32", "2.2.2.2/32"])
        );
        tun.insert("route-address".into(), json!(["0.0.0.0/1"]));
        merged.apply_to_tun(&mut tun);
        assert_eq!(
            tun["route-address"],
            json!(["0.0.0.0/1", "10.0.0.0/8", "172.16.0.0/12"])
        );
        assert!(merged.vpn_route_address(&[]).is_empty());
        assert_eq!(merged.vpn_route_address(&s(&["10.0.0.0/8"])).len(), 2);
        assert!(Network::merge(std::iter::empty()).is_empty());
    }

    #[test]
    fn changes_name_what_goes_over_the_user() {
        let n = Network::from_core_json(&json!({
            "nameserver": {"replace": true, "after": ["1.1.1.1"]},
            "fakeIpFilter": {"before": ["*.lan"], "after": ["*.test"]},
            "hosts": [{"domain": "a.test", "values": ["1.2.3.4"]}],
            "routeAddress": ["10.0.0.0/8"],
            "bypassDomain": {"after": ["*.b"]},
        }));
        let changes = n.changes();
        let keys: Vec<Key> = changes.iter().map(|c| c.key).collect();
        assert_eq!(
            keys,
            [
                Key::Nameserver,
                Key::FakeIpFilter,
                Key::Hosts,
                Key::RouteAddress,
                Key::BypassDomain
            ]
        );
        assert_eq!(changes[0].value(), "替换为 1.1.1.1");
        assert_eq!(changes[1].value(), "在前面添加 *.lan\n添加 *.test");
        assert_eq!(changes[2].value(), "a.test → 1.2.3.4");
        assert_eq!(changes[3].value(), "添加 10.0.0.0/8");
        let user = UserNetwork {
            dns_keys: s(&["nameserver", "fake-ip-filter"]),
            hosts: s(&["a.test"]),
            ..Default::default()
        };
        let later = Network::from_core_json(&json!({
            "hosts": [{"domain": "a.test", "values": ["9.9.9.9"]}],
        }));
        let states = change_states(&n, &[("后来".into(), later)], &user);
        assert!(states[0].1 && states[0].2.is_none());
        assert!(!states[1].1);
        assert_eq!(states[2].2.as_deref(), Some("后来"));
        assert!(!states[3].1 && !states[4].1);
    }
}
