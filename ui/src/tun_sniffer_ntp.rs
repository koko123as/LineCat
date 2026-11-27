//! A custom profile's TUN, sniffer and NTP settings (`ProfileOverrideTarget`).

use crate::overrides::{key, Check, Key, Kind, OverrideState, Spec, OPTIONS};
use crate::{SetRow, SetSection};
use serde_json::{json, Map, Value};
use slint::{ModelRc, VecModel};

const NUM_MAX: i64 = 0x7fff_ffff;

pub const PROFILE_DESC: &str = "仅作用于当前配置，未添加的项使用默认值。";
const PROTOCOLS: &str = "协议";

/// `TunOverrideKey`, in its order.
pub static TUN_KEYS: [Key; 15] = [
    key(
        "mtu",
        "MTU",
        OPTIONS,
        Kind::Number {
            min: 1280,
            max: 65535,
        },
    ),
    Key {
        desc: "TCP 拥塞控制仅在 mips 栈生效。",
        ..key(
            "congestion-controller",
            "TCP 拥塞控制",
            OPTIONS,
            Kind::Options(&crate::store::CONGESTION_CONTROLLERS),
        )
    },
    Key {
        desc: "严格路由防止 DNS 等流量绕过虚拟网卡，关闭 IPv6 时会阻断本机的 IPv6 连接。",
        ..key("strict-route", "严格路由", OPTIONS, Kind::Toggle)
    },
    key(
        "route-address",
        "路由地址",
        OPTIONS,
        Kind::List(Check::Cidr, ""),
    ),
    key(
        "route-exclude-address",
        "排除路由地址",
        OPTIONS,
        Kind::List(Check::Cidr, ""),
    ),
    Key {
        desc: "未添加时劫持所有发往 53 端口的 DNS 请求（any:53）。",
        ..key(
            "dns-hijack",
            "DNS劫持",
            OPTIONS,
            Kind::List(Check::None, ""),
        )
    },
    Key {
        desc: "UDP 超时以秒为单位，仅在 gvisor、system 和 mixed 栈生效。",
        ..key(
            "udp-timeout",
            "UDP 超时",
            OPTIONS,
            Kind::Number {
                min: 1,
                max: NUM_MAX,
            },
        )
    },
    Key {
        desc: "禁用 ICMP 转发后由虚拟网卡直接应答 ping，ping 将不再显示真实延迟。",
        ..key(
            "disable-icmp-forwarding",
            "禁用ICMP转发",
            OPTIONS,
            Kind::Toggle,
        )
    },
    key("gso", "GSO", OPTIONS, Kind::Toggle),
    Key {
        desc: "只有从包含网卡进入的流量经过虚拟网卡，设置后排除网卡不再生效。",
        ..key(
            "include-interface",
            "包含网卡",
            OPTIONS,
            Kind::List(Check::None, ""),
        )
    },
    key(
        "exclude-interface",
        "排除网卡",
        OPTIONS,
        Kind::List(Check::None, ""),
    ),
    key(
        "include-uid",
        "包含用户 ID",
        OPTIONS,
        Kind::List(Check::None, ""),
    ),
    key(
        "exclude-uid",
        "排除用户 ID",
        OPTIONS,
        Kind::List(Check::None, ""),
    ),
    key(
        "exclude-src-port",
        "排除源端口",
        OPTIONS,
        Kind::List(Check::None, ""),
    ),
    key(
        "exclude-dst-port",
        "排除目标端口",
        OPTIONS,
        Kind::List(Check::None, ""),
    ),
];

/// `defaultProfileTun`.
fn tun_defaults() -> Value {
    json!({
        "mtu": crate::store::TUN_MTU.2,
        "congestion-controller": crate::store::CONGESTION_CONTROLLERS[0],
        "strict-route": false,
        "route-address": [],
        "route-exclude-address": [],
        "dns-hijack": ["any:53"],
        "udp-timeout": 300,
        "disable-icmp-forwarding": false,
        "gso": false,
        "include-interface": [],
        "exclude-interface": [],
        "include-uid": [],
        "exclude-uid": [],
        "exclude-src-port": [],
        "exclude-dst-port": [],
    })
}

pub static TUN: Spec = Spec {
    id: "tun",
    title: "虚拟网卡",
    keys: &TUN_KEYS,
    allowed: &[],
    defaults: tun_defaults,
    description: "仅作用于当前配置，优先于网络设置中的覆写项。",
    entries: "设置项",
    add: "添加设置项",
};

/// `SnifferOverrideKey`, in its order.
pub static SNIFFER_KEYS: [Key; 11] = [
    key("enable", "状态", OPTIONS, Kind::Toggle),
    key(
        "override-destination",
        "覆盖目标地址",
        OPTIONS,
        Kind::Toggle,
    ),
    Key {
        desc: "强制 DNS 映射对通过 DNS 映射得到域名的连接也进行嗅探。",
        ..key("force-dns-mapping", "强制DNS映射", OPTIONS, Kind::Toggle)
    },
    key("parse-pure-ip", "解析纯IP", OPTIONS, Kind::Toggle),
    Key {
        desc: "强制嗅探域名中的域名同样会被嗅探，其余已有域名的连接不嗅探。",
        ..key(
            "force-domain",
            "强制嗅探域名",
            OPTIONS,
            Kind::List(Check::None, ""),
        )
    },
    key(
        "skip-domain",
        "跳过域名",
        OPTIONS,
        Kind::List(Check::None, ""),
    ),
    key(
        "skip-src-address",
        "跳过来源地址",
        OPTIONS,
        Kind::List(Check::IpMatcher, ""),
    ),
    key(
        "skip-dst-address",
        "跳过目标地址",
        OPTIONS,
        Kind::List(Check::IpMatcher, ""),
    ),
    key("sniff.HTTP", "HTTP", PROTOCOLS, Kind::Sniff),
    key("sniff.TLS", "TLS", PROTOCOLS, Kind::Sniff),
    key("sniff.QUIC", "QUIC", PROTOCOLS, Kind::Sniff),
];

/// `defaultSniffer`, with each protocol as an empty `SnifferConfig`.
fn sniffer_defaults() -> Value {
    json!({
        "enable": false,
        "override-destination": true,
        "force-dns-mapping": true,
        "parse-pure-ip": true,
        "force-domain": [],
        "skip-domain": [],
        "skip-src-address": [],
        "skip-dst-address": [],
        "sniff": {"HTTP": {"ports": []}, "TLS": {"ports": []}, "QUIC": {"ports": []}},
    })
}

pub static SNIFFER: Spec = Spec {
    id: "sniffer",
    title: "域名嗅探",
    keys: &SNIFFER_KEYS,
    allowed: &[],
    defaults: sniffer_defaults,
    description: PROFILE_DESC,
    entries: "设置项",
    add: "添加设置项",
};

/// `NtpOverrideKey`, in its order.
pub static NTP_KEYS: [Key; 6] = [
    key("enable", "状态", OPTIONS, Kind::Toggle),
    key("server", "服务器", OPTIONS, Kind::Text(Check::Host)),
    key("port", "端口", OPTIONS, Kind::Number { min: 1, max: 65535 }),
    key(
        "interval",
        "同步间隔（分钟）",
        OPTIONS,
        Kind::Number {
            min: 1,
            max: NUM_MAX,
        },
    ),
    key("dialer-proxy", "拨号代理", OPTIONS, Kind::Text(Check::None)),
    Key {
        desc: "写入系统会同时设置系统时钟，Android 上不生效。",
        ..key("write-to-system", "写入系统", OPTIONS, Kind::Toggle)
    },
];

/// `defaultNtp`.
fn ntp_defaults() -> Value {
    json!({
        "enable": false,
        "server": "time.apple.com",
        "port": 123,
        "interval": 30,
        "dialer-proxy": "",
        "write-to-system": false,
    })
}

pub static NTP: Spec = Spec {
    id: "ntp",
    title: "NTP",
    keys: &NTP_KEYS,
    allowed: &[],
    defaults: ntp_defaults,
    description: PROFILE_DESC,
    entries: "设置项",
    add: "添加设置项",
};

/// Writes a spec's picked keys over a profile's section; None while nothing is picked.
pub fn apply_section(spec: &Spec, state: &OverrideState, raw: Option<&Value>) -> Option<Value> {
    if spec.picked(state).next().is_none() {
        return None;
    }
    let raw = raw.and_then(Value::as_object).cloned().unwrap_or_default();
    if spec.id != SNIFFER.id {
        let mut merged = raw;
        spec.apply(state, &mut merged);
        return Some(Value::Object(merged));
    }
    let mut over = Map::new();
    spec.apply(state, &mut over);
    Some(Value::Object(merge_sniffer(&raw, over)))
}

/// `mergeSnifferOverride`: mihomo matches sniff protocols case-insensitively, so an
/// overridden protocol replaces the profile's entry whatever its case.
fn merge_sniffer(raw: &Map<String, Value>, over: Map<String, Value>) -> Map<String, Value> {
    let mut merged = raw.clone();
    for (k, v) in over {
        let Some(sniff) = v.as_object().filter(|_| k == "sniff") else {
            merged.insert(k, v);
            continue;
        };
        let overridden: Vec<String> = sniff.keys().map(|n| n.to_uppercase()).collect();
        let mut out: Map<String, Value> = raw_sniff(raw)
            .into_iter()
            .filter(|(n, _)| !overridden.contains(&n.to_uppercase()))
            .collect();
        out.extend(sniff.clone());
        merged.insert(k, Value::Object(out));
    }
    merged
}

/// `_rawSniff`: mihomo reads the deprecated `sniffing` list only while `sniff` is empty, so
/// its protocols are carried into `sniff` before an override fills it.
fn raw_sniff(raw: &Map<String, Value>) -> Map<String, Value> {
    if let Some(sniff) = raw.get("sniff").and_then(Value::as_object) {
        if !sniff.is_empty() {
            return sniff.clone();
        }
    }
    let Some(sniffing) = raw.get("sniffing").and_then(Value::as_array) else {
        return Map::new();
    };
    let ports = raw.get("port-whitelist").filter(|p| p.is_array());
    sniffing
        .iter()
        .map(|name| {
            let name = name
                .as_str()
                .map_or_else(|| name.to_string(), str::to_string);
            let entry = match ports {
                Some(p) => json!({"ports": p}),
                None => json!({}),
            };
            (name.to_uppercase(), entry)
        })
        .collect()
}

/// A protocol's ports (`_formJsonPorts`: numbers read as text).
pub fn ports(v: &Value) -> Vec<String> {
    v.get("ports")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| p.as_str().map_or_else(|| p.to_string(), str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `_portsText`: mihomo sniffs a protocol listed without ports on its standard port.
pub fn ports_text(v: &Value) -> String {
    match ports(v).len() {
        0 => "默认".into(),
        n => format!("{n} 项"),
    }
}

/// `_SniffDestination`: a protocol without its own `override-destination` follows the
/// sniffer's.
pub const DESTINATIONS: [(&str, &str, Option<bool>); 3] = [
    ("default", "默认", None),
    ("on", "已开启", Some(true)),
    ("off", "已关闭", Some(false)),
];

pub fn destination(v: &Value) -> Option<bool> {
    v.get("override-destination").and_then(Value::as_bool)
}

/// The protocol with new ports, or a new destination (None drops it).
pub fn with_ports(v: &Value, items: Vec<String>) -> Value {
    let mut out = v.as_object().cloned().unwrap_or_default();
    out.insert("ports".into(), json!(items));
    Value::Object(out)
}

pub fn with_destination(v: &Value, dest: Option<bool>) -> Value {
    let mut out = v.as_object().cloned().unwrap_or_default();
    out.entry("ports").or_insert_with(|| json!([]));
    match dest {
        Some(d) => out.insert("override-destination".into(), d.into()),
        None => out.remove("override-destination"),
    };
    Value::Object(out)
}

/// `_SniffView`: the protocol's ports and destination.
pub fn sniff_sections(v: &Value) -> Vec<SetSection> {
    let dest = destination(v);
    let label = DESTINATIONS
        .iter()
        .find(|d| d.2 == dest)
        .map_or("默认", |d| d.1);
    let row = |id: &str, kind: &str, title: &str, subtitle: String| SetRow {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        enabled: true,
        ..Default::default()
    };
    vec![SetSection {
        title: "".into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(vec![
            row("ports", "open", "端口", ports_text(v)),
            row("destination", "value", "覆盖目标地址", label.into()),
        ])),
    }]
}

/// `validatePortRange`: a port or `from-to`, each at most 65535.
pub fn valid_port_range(value: &str) -> bool {
    let port = |p: &str| {
        (1..=5).contains(&p.len())
            && p.bytes().all(|b| b.is_ascii_digit())
            && p.parse::<u32>().is_ok_and(|n| n <= 65535)
    };
    match value.split_once('-') {
        Some((a, b)) => port(a) && port(b),
        None => port(value),
    }
}

/// `validateHost`: an IPv6 address, or a name of dot-separated labels.
pub fn valid_host(value: &str) -> bool {
    if value.contains(':') {
        return value.parse::<std::net::Ipv6Addr>().is_ok();
    }
    let label = |l: &str| {
        let b = l.as_bytes();
        let edge = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_';
        (1..=63).contains(&b.len())
            && b.first().is_some_and(edge)
            && b.last().is_some_and(edge)
            && b.iter().all(|c| edge(c) || *c == b'-')
    };
    value.len() <= 253 && value.split('.').all(label)
}

/// `validateIpMatcher`.
pub fn valid_ip_matcher(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("geoip:")
        || lower.starts_with("rule-set:")
        || crate::settings_view::valid_cidr(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn tun_settings_apply_over_the_tun() {
        assert_eq!(TUN.values().count(), 15);
        let mut s = OverrideState::default();
        let base = json!({"stack": "mixed", "dns-hijack": ["any:53"]});
        assert_eq!(apply_section(&TUN, &s, Some(&base)), None);
        TUN.add(&mut s, "udp-timeout");
        TUN.add(&mut s, "dns-hijack");
        TUN.set(&mut s, "dns-hijack", json!(["tcp://any:53"]));
        let tun = apply_section(&TUN, &s, Some(&base)).unwrap();
        assert_eq!(tun["udp-timeout"], 300);
        assert_eq!(tun["dns-hijack"], json!(["tcp://any:53"]));
        assert_eq!(tun["stack"], "mixed");
        let secs = TUN.sections(&s);
        assert_eq!(secs[0].rows.row_data(0).unwrap().subtitle, "1 项");
        assert_eq!(secs.last().unwrap().footer, TUN.description);
        assert!(TUN.check("udp-timeout", "0").is_err());
        assert!(TUN.check("mtu", "1000").is_err());
    }

    #[test]
    fn sniffer_override_replaces_protocols_whatever_their_case() {
        let mut s = OverrideState::default();
        SNIFFER.add(&mut s, "enable");
        SNIFFER.set(&mut s, "enable", true.into());
        SNIFFER.add(&mut s, "sniff.TLS");
        let tls = SNIFFER.value(&s, "sniff.TLS");
        assert_eq!(ports_text(&tls), "默认");
        let tls = with_destination(&with_ports(&tls, vec!["443".into()]), Some(false));
        SNIFFER.set(&mut s, "sniff.TLS", tls);
        let raw =
            json!({"enable": false, "sniff": {"tls": {"ports": [8443]}, "http": {"ports": [80]}}});
        let out = apply_section(&SNIFFER, &s, Some(&raw)).unwrap();
        assert_eq!(out["enable"], true);
        assert_eq!(
            out["sniff"],
            json!({"http": {"ports": [80]}, "TLS": {"ports": ["443"], "override-destination": false}})
        );
        let legacy = json!({"sniffing": ["http", "tls"], "port-whitelist": [80, 443]});
        let out = apply_section(&SNIFFER, &s, Some(&legacy)).unwrap();
        assert_eq!(out["sniff"]["HTTP"], json!({"ports": [80, 443]}));
        assert_eq!(out["sniff"]["TLS"]["ports"], json!(["443"]));
        let secs = SNIFFER.sections(&s);
        assert_eq!(secs[0].title, "选项");
        assert_eq!(secs[1].title, "协议");
        assert_eq!(secs[1].rows.row_data(0).unwrap().subtitle, "1 项");
        let page = sniff_sections(&SNIFFER.value(&s, "sniff.TLS"));
        assert_eq!(page[0].rows.row_data(1).unwrap().subtitle, "已关闭");
        assert!(SNIFFER.check("sniff.TLS", "8000-9000").is_ok());
        assert!(SNIFFER.check("sniff.TLS", "70000").is_err());
        assert!(SNIFFER.check("skip-dst-address", "geoip:cn").is_ok());
        assert!(SNIFFER.check("skip-dst-address", "1.2.3.4").is_err());
        assert_eq!(
            ports(&json!({"ports": [80, "8000-9000"]})),
            ["80", "8000-9000"]
        );
        assert_eq!(destination(&with_destination(&json!({}), None)), None);
    }

    #[test]
    fn ntp_checks_and_merges() {
        let mut s = OverrideState::default();
        for k in ["enable", "server", "port"] {
            NTP.add(&mut s, k);
        }
        NTP.set(&mut s, "port", 1234.into());
        let raw = json!({"enable": false, "server": "pool.ntp.org", "interval": 60});
        let ntp = apply_section(&NTP, &s, Some(&raw)).unwrap();
        assert_eq!(
            ntp,
            json!({"enable": false, "server": "time.apple.com", "port": 1234, "interval": 60})
        );
        assert!(apply_section(&NTP, &s, None).is_some());
        assert_eq!(
            NTP.check("server", "a b").unwrap_err(),
            "请输入域名或 IP 地址"
        );
        assert!(NTP.check("server", "ntp.aliyun.com").is_ok());
        assert!(NTP.check("server", "::1").is_ok());
        assert!(NTP.check("port", "65536").is_err());
        assert!(valid_port_range("1-65535"));
        assert!(!valid_port_range("80-"));
        assert!(!valid_host(""));
    }
}
