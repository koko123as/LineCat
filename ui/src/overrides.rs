//! The override pages (lib/views/config/override.dart): a spec lists the keys a page may
//! write over a profile's config; the user picks some of them and gives each a value.
//! The DNS page (lib/views/config/dns.dart) is built on it, and so is the core section of
//! the general page (lib/views/config/general/overrides.dart).

use crate::{SetRow, SetSection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use slint::{ModelRc, VecModel};
use std::collections::BTreeMap;

/// How a value is checked before it is saved.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Check {
    None,
    Cidr,
    Listen,
    Url,
}

/// The kind of field a key is edited with (`OverrideField`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Toggle,
    Number {
        min: i64,
        max: i64,
    },
    Text(Check),
    Options(&'static [&'static str]),
    /// A list on its own page, with the page's description.
    List(Check, &'static str),
    /// A map of domain to server on its own page, with the page's description.
    Map(&'static str),
}

pub struct Key {
    pub path: &'static str,
    pub label: &'static str,
    pub section: &'static str,
    /// Under the section, for the keys that have one (`description`).
    pub desc: &'static str,
    pub kind: Kind,
}

/// `OverrideSpec` with its `PatchOverrideTarget`.
pub struct Spec {
    pub id: &'static str,
    pub title: &'static str,
    pub keys: &'static [Key],
    /// The keys this target may hold; empty means all of them.
    pub allowed: &'static [&'static str],
    pub defaults: fn() -> Value,
    pub description: &'static str,
    pub entries: &'static str,
    pub add: &'static str,
}

/// `OverrideState`: the picked keys and their values, by path.
#[derive(Serialize, Deserialize, Default, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct OverrideState {
    pub keys: Vec<String>,
    pub model: BTreeMap<String, Value>,
}

const NUM_MAX: i64 = 0x7fff_ffff;

const fn key(path: &'static str, label: &'static str, section: &'static str, kind: Kind) -> Key {
    Key {
        path,
        label,
        section,
        desc: "",
        kind,
    }
}

const OPTIONS: &str = "选项";
const FAKE_IP: &str = "Fake-IP";
const NAMESERVER: &str = "Nameserver";
const FALLBACK_FILTER: &str = "Fallback过滤";

/// `DnsOverrideKey`, in its order.
pub static DNS_KEYS: [Key; 31] = [
    Key {
        desc: "关闭 DNS 后将使用系统 DNS。",
        ..key("enable", "状态", OPTIONS, Kind::Toggle)
    },
    key("listen", "监听", OPTIONS, Kind::Text(Check::Listen)),
    Key {
        desc: "监听路由标记仅在 Linux 生效。",
        ..key(
            "listen-routing-mark",
            "监听路由标记",
            OPTIONS,
            Kind::Number {
                min: 0,
                max: NUM_MAX,
            },
        )
    },
    key("use-hosts", "使用Hosts", OPTIONS, Kind::Toggle),
    key("use-system-hosts", "使用系统Hosts", OPTIONS, Kind::Toggle),
    key("ipv6", "IPv6", OPTIONS, Kind::Toggle),
    key(
        "ipv6-timeout",
        "IPv6超时（毫秒）",
        OPTIONS,
        Kind::Number {
            min: 0,
            max: NUM_MAX,
        },
    ),
    Key {
        desc: "遵守规则需配置 Proxy Server Nameserver。",
        ..key("respect-rules", "遵守规则", OPTIONS, Kind::Toggle)
    },
    key("prefer-h3", "Prefer H3", OPTIONS, Kind::Toggle),
    key(
        "cache-algorithm",
        "缓存算法",
        OPTIONS,
        Kind::Options(&["lru", "arc"]),
    ),
    key(
        "cache-max-size",
        "缓存大小",
        OPTIONS,
        Kind::Number {
            min: 0,
            max: NUM_MAX,
        },
    ),
    key(
        "enhanced-mode",
        "DNS模式",
        OPTIONS,
        Kind::Options(&["normal", "fake-ip", "redir-host"]),
    ),
    key(
        "fake-ip-range",
        "Fake-IP范围",
        FAKE_IP,
        Kind::Text(Check::Cidr),
    ),
    key(
        "fake-ip-range6",
        "Fake-IP范围（IPv6）",
        FAKE_IP,
        Kind::Text(Check::Cidr),
    ),
    key(
        "fake-ip-filter",
        "Fake-IP过滤",
        FAKE_IP,
        Kind::List(
            Check::None,
            "这些域名不使用 Fake-IP，返回真实 IP；过滤模式为 whitelist 时则只有它们使用 Fake-IP。",
        ),
    ),
    key(
        "fake-ip-filter-mode",
        "Fake-IP过滤模式",
        FAKE_IP,
        Kind::Options(&["blacklist", "whitelist", "rule"]),
    ),
    key(
        "fake-ip-ttl",
        "Fake-IP TTL",
        FAKE_IP,
        Kind::Number {
            min: 0,
            max: NUM_MAX,
        },
    ),
    key(
        "default-nameserver",
        "Default Nameserver",
        NAMESERVER,
        Kind::List(
            Check::None,
            "用于解析其他 DNS 服务器的域名，只能填写 IP 地址。",
        ),
    ),
    key(
        "nameserver-policy",
        "Nameserver Policy",
        NAMESERVER,
        Kind::Map("匹配的域名改用此处指定的 DNS 服务器，优先于 Nameserver 和 Fallback。"),
    ),
    key(
        "nameserver",
        "Nameserver",
        NAMESERVER,
        Kind::List(
            Check::None,
            "默认使用的 DNS 服务器，如 223.5.5.5、tls://1.1.1.1、https://dns.google/dns-query。",
        ),
    ),
    key(
        "fallback",
        "Fallback",
        NAMESERVER,
        Kind::List(
            Check::None,
            "Nameserver 的结果被 Fallback 过滤判定为污染时，改用这些服务器的结果。",
        ),
    ),
    key(
        "fallback-lazy-query",
        "Fallback Lazy Query",
        NAMESERVER,
        Kind::Toggle,
    ),
    key(
        "proxy-server-nameserver",
        "Proxy Server Nameserver",
        NAMESERVER,
        Kind::List(Check::None, "仅用于解析代理服务器的域名。"),
    ),
    key(
        "proxy-server-nameserver-policy",
        "Proxy Server Nameserver Policy",
        NAMESERVER,
        Kind::Map(
            "匹配的代理服务器域名改用此处指定的 DNS 服务器，优先于 Proxy Server Nameserver。",
        ),
    ),
    key(
        "direct-nameserver",
        "Direct Nameserver",
        NAMESERVER,
        Kind::List(Check::None, "用于解析直连连接的域名。"),
    ),
    key(
        "direct-nameserver-follow-policy",
        "Direct Nameserver Follow Policy",
        NAMESERVER,
        Kind::Toggle,
    ),
    key(
        "fallback-filter.geoip",
        "GeoIP",
        FALLBACK_FILTER,
        Kind::Toggle,
    ),
    key(
        "fallback-filter.geoip-code",
        "GeoIP Code",
        FALLBACK_FILTER,
        Kind::Text(Check::None),
    ),
    key(
        "fallback-filter.geosite",
        "GeoSite",
        FALLBACK_FILTER,
        Kind::List(
            Check::None,
            "属于这些 GeoSite 分类的域名直接使用 Fallback 解析。",
        ),
    ),
    key(
        "fallback-filter.ipcidr",
        "IP-CIDR",
        FALLBACK_FILTER,
        Kind::List(
            Check::Cidr,
            "Nameserver 返回这些网段内的 IP 时，改用 Fallback 的结果。",
        ),
    ),
    key(
        "fallback-filter.domain",
        "域名",
        FALLBACK_FILTER,
        Kind::List(Check::None, "这些域名直接使用 Fallback 解析。"),
    ),
];

/// `defaultDns`.
fn dns_defaults() -> Value {
    json!({
        "enable": false,
        "listen": "",
        "listen-routing-mark": 0,
        "prefer-h3": false,
        "use-hosts": true,
        "use-system-hosts": true,
        "respect-rules": false,
        "ipv6": false,
        "ipv6-timeout": 100,
        "cache-algorithm": "lru",
        "cache-max-size": 4096,
        "default-nameserver": ["114.114.114.114", "223.5.5.5", "8.8.8.8", "1.0.0.1"],
        "enhanced-mode": "redir-host",
        "fake-ip-range": "198.18.0.1/16",
        "fake-ip-range6": "",
        "fake-ip-filter": ["dns.msftncsi.com", "www.msftncsi.com", "www.msftconnecttest.com"],
        "fake-ip-filter-mode": "blacklist",
        "fake-ip-ttl": 1,
        "nameserver-policy": {},
        "nameserver": ["https://doh.pub/dns-query", "tls://223.5.5.5:853"],
        "fallback": [],
        "fallback-lazy-query": false,
        "proxy-server-nameserver": [],
        "proxy-server-nameserver-policy": {},
        "direct-nameserver": [],
        "direct-nameserver-follow-policy": false,
        "fallback-filter": {
            "geoip": false,
            "geoip-code": "",
            "geosite": [],
            "ipcidr": [],
            "domain": [],
        },
    })
}

/// `DnsView()`: the keys written over the DNS of every subscription and file profile
/// (`DnsOverrideKey.normalProfileKeys`).
pub static DNS: Spec = Spec {
    id: "dns",
    title: "DNS",
    keys: &DNS_KEYS,
    allowed: &[
        "listen",
        "use-hosts",
        "use-system-hosts",
        "ipv6",
        "respect-rules",
        "prefer-h3",
        "cache-algorithm",
        "enhanced-mode",
        "fake-ip-filter",
        "fake-ip-filter-mode",
        "default-nameserver",
        "nameserver",
        "fallback",
        "proxy-server-nameserver",
        "direct-nameserver",
    ],
    defaults: dns_defaults,
    description: "添加的项覆写 URL 和文件配置的 DNS。",
    entries: "覆写项",
    add: "添加覆写项",
};

/// `GeneralOverrideKey`, in its order.
pub static GENERAL_KEYS: [Key; 4] = [
    key("test-url", "测速链接", "", Kind::Text(Check::Url)),
    key("unified-delay", "统一延迟", "", Kind::Toggle),
    key("tcp-concurrent", "TCP并发", "", Kind::Toggle),
    Key {
        desc: "开启 IPv6 后才会接收 IPv6 流量。",
        ..key("ipv6", "IPv6", "", Kind::Toggle)
    },
];

fn general_defaults() -> Value {
    json!({
        "test-url": crate::profile::DEFAULT_TEST_URL,
        "unified-delay": true,
        "tcp-concurrent": true,
        "ipv6": false,
    })
}

/// `_GeneralOverrideSpec`: the core section of the general page, written over every
/// profile. Its test URL goes to the Core's options instead of the config.
pub static GENERAL: Spec = Spec {
    id: "general",
    title: "内核",
    keys: &GENERAL_KEYS,
    allowed: &[],
    defaults: general_defaults,
    description: "添加的项覆写所有配置的对应设置。",
    entries: "覆写项",
    add: "添加覆写项",
};

/// The spec a page id names.
pub fn spec(id: &str) -> Option<&'static Spec> {
    match id {
        "dns" => Some(&DNS),
        "general" => Some(&GENERAL),
        _ => None,
    }
}

fn get_path<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |v, p| v.get(p))
}

fn set_path(target: &mut Map<String, Value>, path: &str, value: Value) {
    match path.split_once('.') {
        Some((parent, rest)) => {
            let child = target
                .entry(parent.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !child.is_object() {
                *child = Value::Object(Map::new());
            }
            if let Some(obj) = child.as_object_mut() {
                set_path(obj, rest, value);
            }
        }
        None => {
            target.insert(path.to_string(), value);
        }
    }
}

impl Spec {
    /// `values`: the keys this target may hold, in the spec's order.
    pub fn values(&self) -> impl Iterator<Item = &'static Key> + '_ {
        self.keys
            .iter()
            .filter(|k| self.allowed.is_empty() || self.allowed.contains(&k.path))
    }

    pub fn key(&self, path: &str) -> Option<&'static Key> {
        self.values().find(|k| k.path == path)
    }

    /// A key's value: the user's, else the default.
    pub fn value(&self, state: &OverrideState, path: &str) -> Value {
        state
            .model
            .get(path)
            .cloned()
            .or_else(|| get_path(&(self.defaults)(), path).cloned())
            .unwrap_or(Value::Null)
    }

    /// The picked keys, in the spec's order.
    pub fn picked<'a>(
        &'a self,
        state: &'a OverrideState,
    ) -> impl Iterator<Item = &'static Key> + 'a {
        self.values()
            .filter(|k| state.keys.iter().any(|p| p == k.path))
    }

    /// A picked key's value; None while the key is not picked.
    pub fn picked_value(&self, state: &OverrideState, path: &str) -> Option<Value> {
        self.picked(state)
            .any(|k| k.path == path)
            .then(|| self.value(state, path))
    }

    /// `OverrideSection`: the picked keys' rows after the section's own, the add row while
    /// a key is left, and the footer.
    pub fn inline(&self, state: &OverrideState, mut rows: Vec<SetRow>) -> (Vec<SetRow>, String) {
        let mut footer = vec![self.description];
        for k in self.picked(state) {
            rows.push(self.row(state, k));
            if !k.desc.is_empty() {
                footer.push(k.desc);
            }
        }
        if self.can_add(state) {
            rows.push(SetRow {
                id: "action".into(),
                kind: "action".into(),
                title: self.add.into(),
                enabled: true,
                lead: "add".into(),
                ..Default::default()
            });
        }
        (rows, footer.join("\n"))
    }

    /// Whether a key is left to add (`canAdd`).
    pub fn can_add(&self, state: &OverrideState) -> bool {
        self.values()
            .any(|k| !state.keys.iter().any(|p| p == k.path))
    }

    /// The add sheet: the keys not picked yet, by section, as (section, path, label, desc).
    pub fn addable(&self, state: &OverrideState) -> Vec<(&'static str, &'static Key)> {
        self.values()
            .filter(|k| !state.keys.iter().any(|p| p == k.path))
            .map(|k| (k.section, k))
            .collect()
    }

    /// Adds a key, starting from its default value.
    pub fn add(&self, state: &mut OverrideState, path: &str) {
        if self.key(path).is_none() || state.keys.iter().any(|p| p == path) {
            return;
        }
        state.model.remove(path);
        state.keys.push(path.to_string());
    }

    /// `withoutKeys`: the key goes, and so does its value.
    pub fn remove(&self, state: &mut OverrideState, path: &str) {
        state.keys.retain(|p| p != path);
        state.model.remove(path);
    }

    pub fn set(&self, state: &mut OverrideState, path: &str, value: Value) {
        if self.key(path).is_some() {
            state.model.insert(path.to_string(), value);
        }
    }

    /// Writes the picked keys over `target` (a profile's `dns` section, say).
    pub fn apply(&self, state: &OverrideState, target: &mut Map<String, Value>) {
        for k in self.picked(state) {
            set_path(target, k.path, self.value(state, k.path));
        }
    }

    /// A typed value as the input dialog's text.
    pub fn text(&self, state: &OverrideState, path: &str) -> String {
        match self.value(state, path) {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => String::new(),
        }
    }

    /// The input dialog's check: the value to save, or what is wrong with it.
    pub fn check(&self, path: &str, input: &str) -> Result<Value, String> {
        let input = input.trim();
        let Some(k) = self.key(path) else {
            return Err(String::new());
        };
        match k.kind {
            Kind::Number { min, max } => {
                let n: i64 = input
                    .parse()
                    .map_err(|_| format!("{}必须为数字", k.label))?;
                if !(min..=max).contains(&n) {
                    return Err(format!("{}必须在 {min} 到 {max} 之间", k.label));
                }
                Ok(n.into())
            }
            Kind::Text(check) | Kind::List(check, _) => {
                if matches!(k.kind, Kind::List(..)) && input.is_empty() {
                    return Err("值不能为空".into());
                }
                check_text(check, k.label, input)?;
                Ok(input.into())
            }
            _ => Ok(input.into()),
        }
    }

    /// The rows of the page (`_OverrideList`): one section per section the picked keys fall
    /// in, each row led by the button that takes it off.
    pub fn sections(&self, state: &OverrideState) -> Vec<SetSection> {
        let mut out: Vec<(&str, Vec<SetRow>, Vec<&str>)> = Vec::new();
        for k in self.picked(state) {
            let row = self.row(state, k);
            match out.iter_mut().find(|(s, _, _)| *s == k.section) {
                Some((_, rows, notes)) => {
                    rows.push(row);
                    if !k.desc.is_empty() {
                        notes.push(k.desc);
                    }
                }
                None => out.push((
                    k.section,
                    vec![row],
                    if k.desc.is_empty() {
                        vec![]
                    } else {
                        vec![k.desc]
                    },
                )),
            }
        }
        let mut sections: Vec<SetSection> = out
            .into_iter()
            .map(|(title, rows, notes)| SetSection {
                title: title.into(),
                footer: notes.join("\n").into(),
                rows: ModelRc::new(VecModel::from(rows)),
            })
            .collect();
        if !sections.is_empty() {
            sections.push(SetSection {
                title: "".into(),
                footer: self.description.into(),
                rows: ModelRc::default(),
            });
        }
        sections
    }

    fn row(&self, state: &OverrideState, k: &Key) -> SetRow {
        let v = self.value(state, k.path);
        let (kind, subtitle, on) = match k.kind {
            Kind::Toggle => ("toggle", String::new(), v.as_bool().unwrap_or(false)),
            Kind::List(..) => ("open", count(v.as_array().map_or(0, Vec::len)), false),
            Kind::Map(_) => ("open", count(v.as_object().map_or(0, Map::len)), false),
            _ => {
                let t = self.text(state, k.path);
                ("value", if t.is_empty() { "无".into() } else { t }, false)
            }
        };
        SetRow {
            id: k.path.into(),
            kind: kind.into(),
            glyph: "".into(),
            title: k.label.into(),
            subtitle: subtitle.into(),
            on,
            enabled: true,
            lead: "remove".into(),
            chip: "".into(),
            trailing: "".into(),
            image: "".into(),
            invalid: false,
        }
    }
}

/// `itemsCount`, or `none`.
pub fn count(n: usize) -> String {
    if n == 0 {
        "无".into()
    } else {
        format!("{n} 项")
    }
}

fn check_text(check: Check, label: &str, input: &str) -> Result<(), String> {
    match check {
        Check::Cidr if !input.is_empty() && !crate::settings_view::valid_cidr(input) => {
            Err("请输入有效的 CIDR".into())
        }
        Check::Listen if !input.is_empty() && !valid_listen(input) => {
            Err("请输入地址和端口，如 0.0.0.0:1053".into())
        }
        Check::Url if !crate::profile::is_url(input) => Err(format!("{label}必须为URL")),
        _ => Ok(()),
    }
}

/// `validateListenAddress`: `host:port`, the host may be empty or bracketed IPv6.
pub fn valid_listen(value: &str) -> bool {
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    let port_ok = port.parse::<u16>().is_ok_and(|p| p > 0);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let host_ok = host.is_empty()
        || host.parse::<std::net::IpAddr>().is_ok()
        || host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    port_ok && host_ok
}

/// A list kept in an override, as a list of strings.
pub fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A map kept in an override, as name to value.
pub fn pairs(v: &Value) -> BTreeMap<String, String> {
    v.as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn general_keys_sit_inline_with_an_add_row() {
        let mut s = OverrideState::default();
        let base = |n: usize| vec![SetRow::default(); n];
        let (rows, footer) = GENERAL.inline(&s, base(2));
        assert_eq!(rows.len(), 3);
        assert_eq!(
            (rows[2].id.as_str(), rows[2].lead.as_str()),
            ("action", "add")
        );
        assert_eq!(footer, GENERAL.description);
        for k in GENERAL_KEYS.iter().rev() {
            GENERAL.add(&mut s, k.path);
        }
        let (rows, footer) = GENERAL.inline(&s, base(2));
        let ids: Vec<&str> = rows[2..].iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["test-url", "unified-delay", "tcp-concurrent", "ipv6"]);
        assert!(rows[2..].iter().all(|r| r.lead == "remove"));
        assert_eq!(rows[2].subtitle, crate::profile::DEFAULT_TEST_URL);
        assert!(footer.ends_with("开启 IPv6 后才会接收 IPv6 流量。"));
        assert_eq!(
            GENERAL.check("test-url", "not a url"),
            Err("测速链接必须为URL".to_string())
        );
        assert!(GENERAL.check("test-url", "http://cp.example/204").is_ok());
    }

    #[test]
    fn picks_apply_over_the_profile() {
        let mut s = OverrideState::default();
        assert!(DNS.sections(&s).is_empty());
        DNS.add(&mut s, "enhanced-mode");
        DNS.add(&mut s, "nameserver");
        DNS.add(&mut s, "fake-ip-ttl"); // not for subscription profiles
        assert_eq!(s.keys, vec!["enhanced-mode", "nameserver"]);
        DNS.set(&mut s, "enhanced-mode", "fake-ip".into());
        let mut dns = Map::new();
        dns.insert("enable".into(), true.into());
        DNS.apply(&s, &mut dns);
        assert_eq!(dns["enhanced-mode"], "fake-ip");
        assert_eq!(dns["nameserver"][1], "tls://223.5.5.5:853");
        assert_eq!(dns["enable"], true);
        let secs = DNS.sections(&s);
        assert_eq!(secs.len(), 3);
        assert_eq!(secs[0].title, "选项");
        assert_eq!(secs[1].title, "Nameserver");
        assert_eq!(secs[1].rows.row_data(0).unwrap().subtitle, "2 项");
        assert_eq!(secs[2].footer, DNS.description);
        DNS.remove(&mut s, "enhanced-mode");
        assert!(!s.model.contains_key("enhanced-mode"));
        assert_eq!(DNS.addable(&s).len(), 14);
    }

    #[test]
    fn nested_paths_and_checks() {
        let mut m = Map::new();
        set_path(&mut m, "fallback-filter.geoip", true.into());
        assert_eq!(m["fallback-filter"]["geoip"], true);
        let all = Spec {
            allowed: &[],
            ..DNS_SPEC_ALL
        };
        assert_eq!(all.values().count(), 31);
        assert_eq!(
            all.check("ipv6-timeout", "x").unwrap_err(),
            "IPv6超时（毫秒）必须为数字"
        );
        assert_eq!(all.check("cache-max-size", " 12 ").unwrap(), json!(12));
        assert!(all.check("fake-ip-range", "1.2.3.4").is_err());
        assert!(all.check("fake-ip-range", "").is_ok());
        assert!(DNS.check("listen", "0.0.0.0:53").is_ok());
        assert!(DNS.check("listen", "53").is_err());
        assert!(valid_listen("[::]:1053"));
        assert_eq!(all.text(&OverrideState::default(), "fake-ip-ttl"), "1");
    }

    const DNS_SPEC_ALL: Spec = Spec {
        id: "dns",
        title: "DNS",
        keys: &DNS_KEYS,
        allowed: &[],
        defaults: dns_defaults,
        description: "",
        entries: "",
        add: "",
    };
}
