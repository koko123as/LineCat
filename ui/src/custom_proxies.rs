//! The app's own proxies of 进阶配置 → 自定义 → 代理 (lib/views/profiles/custom/custom_proxies.dart,
//! `CustomProxy` in lib/models/clash_config.dart, `ProxyField` in lib/enum/enum.dart): each one
//! kept as the raw mapping the Core parses, since every type reads its own keys.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeSet, HashSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CustomProxy {
    pub id: String,
    pub definition: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Secret,
    Number,
    Toggle,
    Choice,
    List,
    Map,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Authentication,
    Transport,
    Tls,
    Other,
    Connection,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Authentication,
        Section::Transport,
        Section::Tls,
        Section::Other,
        Section::Connection,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Authentication => "认证",
            Section::Transport => "传输协议",
            Section::Tls => "TLS",
            Section::Other => "其他",
            Section::Connection => "连接",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Field {
    pub name: &'static str,
    pub path: &'static [&'static str],
    pub kind: Kind,
    pub label: &'static str,
}

const fn f(
    name: &'static str,
    path: &'static [&'static str],
    kind: Kind,
    label: &'static str,
) -> Field {
    Field {
        name,
        path,
        kind,
        label,
    }
}

use Kind::{Choice, List, Map as MapKind, Number, Secret, Text, Toggle};

/// `ProxyField.values` in order, with `label`.
pub static FIELDS: &[Field] = &[
    f("username", &["username"], Text, "用户名"),
    f("password", &["password"], Secret, "密码"),
    f("uuid", &["uuid"], Text, "UUID"),
    f("psk", &["psk"], Secret, "预共享密钥"),
    f("key", &["key"], Secret, "密钥"),
    f("privateKey", &["private-key"], Secret, "私钥"),
    f("publicKey", &["public-key"], Text, "公钥"),
    f(
        "privateKeyPassphrase",
        &["private-key-passphrase"],
        Secret,
        "私钥密码",
    ),
    f("preSharedKey", &["pre-shared-key"], Secret, "预共享密钥"),
    f("authStr", &["auth-str"], Secret, "密码"),
    f("cipher", &["cipher"], Choice, "加密方式"),
    f("alterId", &["alterId"], Number, "Alter ID"),
    f("protocol", &["protocol"], Choice, "协议"),
    f("obfs", &["obfs"], Choice, "混淆"),
    f("transport", &["transport"], Choice, "传输协议"),
    f("up", &["up"], Text, "上行带宽"),
    f("down", &["down"], Text, "下行带宽"),
    f("network", &["network"], Choice, "网络"),
    f("flow", &["flow"], Choice, "Flow"),
    f("wsPath", &["ws-opts", "path"], Text, "WebSocket Path"),
    f(
        "wsHost",
        &["ws-opts", "headers", "Host"],
        Text,
        "WebSocket Host",
    ),
    f(
        "wsHeaders",
        &["ws-opts", "headers"],
        MapKind,
        "WebSocket 请求头",
    ),
    f(
        "wsMaxEarlyData",
        &["ws-opts", "max-early-data"],
        Number,
        "Max Early Data",
    ),
    f(
        "wsEarlyDataHeaderName",
        &["ws-opts", "early-data-header-name"],
        Text,
        "Early Data Header",
    ),
    f(
        "wsHttpUpgrade",
        &["ws-opts", "v2ray-http-upgrade"],
        Toggle,
        "HTTPUpgrade",
    ),
    f(
        "wsHttpUpgradeFastOpen",
        &["ws-opts", "v2ray-http-upgrade-fast-open"],
        Toggle,
        "HTTPUpgrade Fast Open",
    ),
    f("httpMethod", &["http-opts", "method"], Text, "HTTP Method"),
    f("httpPath", &["http-opts", "path"], List, "HTTP Path"),
    f(
        "httpHost",
        &["http-opts", "headers", "Host"],
        List,
        "HTTP Host",
    ),
    f(
        "httpHeaders",
        &["http-opts", "headers"],
        MapKind,
        "HTTP 请求头",
    ),
    f("h2Host", &["h2-opts", "host"], List, "HTTP/2 Host"),
    f("h2Path", &["h2-opts", "path"], Text, "HTTP/2 Path"),
    f(
        "grpcServiceName",
        &["grpc-opts", "grpc-service-name"],
        Text,
        "gRPC Service Name",
    ),
    f("xhttpPath", &["xhttp-opts", "path"], Text, "XHTTP Path"),
    f("xhttpHost", &["xhttp-opts", "host"], Text, "XHTTP Host"),
    f("mkcpSeed", &["mkcp-opts", "seed"], Secret, "mKCP Seed"),
    f(
        "mkcpHeader",
        &["mkcp-opts", "header"],
        Choice,
        "mKCP Header",
    ),
    f("mekyaUrl", &["mekya-opts", "url"], Text, "Mekya URL"),
    f("headers", &["headers"], MapKind, "请求头"),
    f("tls", &["tls"], Toggle, "TLS"),
    f("sni", &["sni"], Text, "SNI"),
    f("servername", &["servername"], Text, "SNI"),
    f("alpn", &["alpn"], List, "ALPN"),
    f(
        "skipCertVerify",
        &["skip-cert-verify"],
        Toggle,
        "跳过证书验证",
    ),
    f(
        "clientFingerprint",
        &["client-fingerprint"],
        Choice,
        "客户端指纹",
    ),
    f(
        "realityPublicKey",
        &["reality-opts", "public-key"],
        Text,
        "Reality Public Key",
    ),
    f(
        "realityShortId",
        &["reality-opts", "short-id"],
        Text,
        "Reality Short ID",
    ),
    f("plugin", &["plugin"], Choice, "插件"),
    f("pluginMode", &["plugin-opts", "mode"], Text, "插件模式"),
    f("pluginHost", &["plugin-opts", "host"], Text, "插件主机"),
    f("pluginPath", &["plugin-opts", "path"], Text, "插件路径"),
    f(
        "pluginUsername",
        &["plugin-opts", "username"],
        Text,
        "插件用户名",
    ),
    f(
        "pluginPassword",
        &["plugin-opts", "password"],
        Secret,
        "插件密码",
    ),
    f(
        "pluginVersion",
        &["plugin-opts", "version"],
        Number,
        "协议版本",
    ),
    f(
        "pluginVersionHint",
        &["plugin-opts", "version-hint"],
        Choice,
        "TLS Version",
    ),
    f(
        "pluginRestlsScript",
        &["plugin-opts", "restls-script"],
        Text,
        "Restls Script",
    ),
    f("pluginKey", &["plugin-opts", "key"], Secret, "密钥"),
    f(
        "pluginCrypt",
        &["plugin-opts", "crypt"],
        Choice,
        "插件加密方式",
    ),
    f("pluginTls", &["plugin-opts", "tls"], Toggle, "TLS"),
    f("pluginMux", &["plugin-opts", "mux"], Toggle, "Mux"),
    f(
        "pluginSkipCertVerify",
        &["plugin-opts", "skip-cert-verify"],
        Toggle,
        "跳过证书验证",
    ),
    f("udpOverTcp", &["udp-over-tcp"], Toggle, "UDP over TCP"),
    f("protocolParam", &["protocol-param"], Text, "协议参数"),
    f("obfsParam", &["obfs-param"], Text, "混淆参数"),
    f("obfsPassword", &["obfs-password"], Secret, "混淆密码"),
    f("obfsSecret", &["obfs"], Secret, "混淆密码"),
    f("obfsMode", &["obfs-opts", "mode"], Choice, "混淆模式"),
    f("obfsHost", &["obfs-opts", "host"], Text, "混淆主机"),
    f("version", &["version"], Number, "协议版本"),
    f("ports", &["ports"], Text, "端口跳跃"),
    f("hopInterval", &["hop-interval"], Text, "跳跃间隔"),
    f(
        "congestionController",
        &["congestion-controller"],
        Choice,
        "拥塞控制",
    ),
    f("udpRelayMode", &["udp-relay-mode"], Choice, "UDP 中继模式"),
    f("reduceRtt", &["reduce-rtt"], Toggle, "0-RTT"),
    f("ip", &["ip"], Text, "IPv4"),
    f("ipv6", &["ipv6"], Text, "IPv6"),
    f("allowedIps", &["allowed-ips"], List, "允许的 IP"),
    f("dns", &["dns"], List, "DNS"),
    f(
        "remoteDnsResolve",
        &["remote-dns-resolve"],
        Toggle,
        "远程 DNS 解析",
    ),
    f("mtu", &["mtu"], Number, "MTU"),
    f("tfo", &["tfo"], Toggle, "TCP Fast Open"),
    f("mptcp", &["mptcp"], Toggle, "MPTCP"),
    f("interfaceName", &["interface-name"], Text, "网卡名称"),
    f("routingMark", &["routing-mark"], Number, "路由标记"),
    f("ipVersion", &["ip-version"], Choice, "IP 版本"),
];

pub fn field(name: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.name == name)
}

fn fields_named(names: &[&str]) -> Vec<&'static Field> {
    names.iter().filter_map(|n| field(n)).collect()
}

/// `customProxyTypes`, in `adapter.ParseProxy`'s order.
pub const TYPES: &[&str] = &[
    "ss",
    "ssr",
    "socks5",
    "http",
    "vmess",
    "vless",
    "snell",
    "trojan",
    "hysteria",
    "hysteria2",
    "wireguard",
    "tuic",
    "shadowquic",
    "gost-relay",
    "direct",
    "dns",
    "reject",
    "rematch",
    "ssh",
    "mieru",
    "anytls",
    "sudoku",
    "masque",
    "trusttunnel",
    "openvpn",
    "tailscale",
    "zerotier",
    "easytier",
];

const SS_CIPHERS: &[&str] = &[
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
    "chacha20-ietf-poly1305",
    "xchacha20-ietf-poly1305",
    "2022-blake3-aes-128-gcm",
    "2022-blake3-aes-256-gcm",
    "2022-blake3-chacha20-poly1305",
    "aes-128-ctr",
    "aes-192-ctr",
    "aes-256-ctr",
    "aes-128-cfb",
    "aes-192-cfb",
    "aes-256-cfb",
    "rc4-md5",
    "chacha20-ietf",
    "xchacha20",
    "chacha20",
    "none",
];

const SSR_CIPHERS: &[&str] = &[
    "aes-128-ctr",
    "aes-192-ctr",
    "aes-256-ctr",
    "aes-128-cfb",
    "aes-192-cfb",
    "aes-256-cfb",
    "rc4-md5",
    "chacha20-ietf",
    "xchacha20",
    "chacha20",
    "none",
];

/// `reservedCustomProxyNames`.
pub const RESERVED_NAMES: &[&str] = &[
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "COMPATIBLE",
    "PASS",
    "PASS-RULE",
    "GLOBAL",
];

pub fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// mihomo matches struct keys in any case (`_keyIn`).
fn key_in<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a String> {
    if let Some((k, _)) = map.get_key_value(key) {
        return Some(k);
    }
    let lower = key.to_lowercase();
    map.keys().find(|k| k.to_lowercase() == lower)
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

/// `_withPath`: an empty value drops the key, and any map it leaves empty.
pub fn with_path(
    map: &Map<String, Value>,
    path: &[&str],
    value: Option<Value>,
) -> Map<String, Value> {
    let Some((key, rest)) = path.split_first() else {
        return map.clone();
    };
    let mut next = map.clone();
    let existing = key_in(&next, key)
        .cloned()
        .unwrap_or_else(|| key.to_string());
    let child = if rest.is_empty() {
        value
    } else {
        let inner = next
            .get(&existing)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        Some(Value::Object(with_path(&inner, rest, value)))
    };
    match child {
        Some(v) if !is_empty_value(&v) => {
            next.insert(existing, v);
        }
        _ => {
            next.remove(&existing);
        }
    }
    next
}

pub fn value_at<'a>(map: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let (key, rest) = path.split_first()?;
    let v = map.get(key_in(map, key)?)?;
    if rest.is_empty() {
        return (!v.is_null()).then_some(v);
    }
    value_at(v.as_object()?, rest)
}

impl CustomProxy {
    pub fn new(id: String, definition: Map<String, Value>) -> Self {
        CustomProxy {
            id,
            definition: Value::Object(definition),
        }
    }

    pub fn map(&self) -> Map<String, Value> {
        self.definition.as_object().cloned().unwrap_or_default()
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.definition.get(key).filter(|v| !v.is_null())
    }

    pub fn name(&self) -> String {
        self.get("name").map(text_of).unwrap_or_default()
    }

    pub fn kind(&self) -> String {
        self.get("type").map(text_of).unwrap_or_default()
    }

    pub fn server(&self) -> Option<String> {
        self.get("server").map(text_of)
    }

    pub fn port(&self) -> Option<i64> {
        match self.get("port")? {
            Value::Number(n) => n.as_i64(),
            other => text_of(other).trim().parse().ok(),
        }
    }

    pub fn search_text(&self) -> String {
        let mut parts = vec![self.name(), self.kind()];
        parts.extend(self.server());
        parts.join("\n")
    }

    pub fn value_of(&self, field: &Field) -> Option<&Value> {
        value_at(self.definition.as_object()?, field.path)
    }

    fn value_text(&self, field: &Field) -> Option<String> {
        self.value_of(field).map(text_of)
    }

    pub fn required_keys(&self) -> &'static [&'static str] {
        match self.kind().as_str() {
            "ss" => &["server", "port", "password", "cipher"],
            "ssr" => &["server", "port", "password", "cipher", "obfs", "protocol"],
            "vmess" => &["server", "port", "uuid", "alterId", "cipher"],
            "vless" => &["server", "port", "uuid"],
            "snell" => &["server", "port", "psk"],
            "trojan" | "anytls" => &["server", "port", "password"],
            "hysteria" => &["server", "up", "down"],
            "hysteria2" => &["server"],
            "wireguard" => &["private-key"],
            "ssh" => &["server", "port", "username"],
            "mieru" => &["server", "transport", "username", "password"],
            "sudoku" => &["server", "port", "key"],
            "masque" => &["server", "port", "private-key", "public-key"],
            "openvpn" => &["server", "port", "ca"],
            "zerotier" => &["network"],
            "socks5" | "http" | "tuic" | "shadowquic" | "gost-relay" | "trusttunnel" => {
                &["server", "port"]
            }
            _ => &[],
        }
    }

    pub fn is_required(&self, key: &str) -> bool {
        self.required_keys().contains(&key)
    }

    /// The rows the form always shows: the credentials and the required keys.
    pub fn fields(&self) -> Vec<&'static Field> {
        fields_named(match self.kind().as_str() {
            "ss" => &["cipher", "password"],
            "ssr" => &["cipher", "protocol", "obfs", "password"],
            "vmess" => &["cipher", "alterId", "uuid"],
            "vless" => &["uuid"],
            "tuic" => &["uuid", "password"],
            "trojan" | "anytls" | "hysteria2" => &["password"],
            "hysteria" => &["up", "down", "authStr"],
            "snell" => &["psk"],
            "sudoku" => &["key"],
            "wireguard" | "masque" => &["privateKey", "publicKey"],
            "mieru" => &["transport", "username", "password"],
            "socks5" | "http" | "ssh" | "shadowquic" | "trusttunnel" | "gost-relay" | "openvpn" => {
                &["username", "password"]
            }
            _ => &[],
        })
    }

    /// The keys the form adds one at a time.
    pub fn options(&self) -> Vec<&'static Field> {
        let fields = self.fields();
        FIELDS
            .iter()
            .filter(|f| !fields.contains(f) && self.offers(f))
            .collect()
    }

    fn uses_network(&self, network: &str) -> bool {
        let net = field("network").unwrap();
        self.value_text(net).as_deref() == Some(network) && self.choices(net).contains(&network)
    }

    fn plugin_reads(&self, key: &str) -> bool {
        let plugin = self
            .value_text(field("plugin").unwrap())
            .unwrap_or_default();
        let keys: &[&str] = match plugin.as_str() {
            "obfs" => &["mode", "host"],
            "v2ray-plugin" | "gost-plugin" => {
                &["mode", "host", "path", "tls", "mux", "skip-cert-verify"]
            }
            "kcptun" => &["mode", "key", "crypt"],
            "shadow-tls" => &["host", "password", "version", "skip-cert-verify"],
            "restls" => &[
                "host",
                "password",
                "version-hint",
                "restls-script",
                "skip-cert-verify",
            ],
            "jls" => &["host", "username", "password"],
            _ => &[],
        };
        keys.contains(&key)
    }

    fn offers(&self, field: &Field) -> bool {
        let t = self.kind();
        let t = t.as_str();
        let any = |types: &[&str]| types.contains(&t);
        match field.name {
            "username" | "password" | "uuid" | "psk" | "key" | "publicKey" | "authStr"
            | "cipher" | "alterId" | "transport" => false,
            "privateKey" | "privateKeyPassphrase" => t == "ssh",
            "preSharedKey" | "allowedIps" => t == "wireguard",
            "obfs" | "up" | "down" => t == "hysteria2",
            "obfsPassword" => t == "hysteria2" && self.value_of(field_ref("obfs")).is_some(),
            "protocol" | "obfsSecret" => t == "hysteria",
            "network" => !self.choices(field).is_empty(),
            "flow" => t == "vless",
            "wsPath"
            | "wsHost"
            | "wsHeaders"
            | "wsMaxEarlyData"
            | "wsEarlyDataHeaderName"
            | "wsHttpUpgrade" => self.uses_network("ws"),
            "wsHttpUpgradeFastOpen" => {
                self.uses_network("ws")
                    && self.value_of(field_ref("wsHttpUpgrade")) == Some(&Value::Bool(true))
            }
            "httpMethod" | "httpPath" | "httpHost" | "httpHeaders" => self.uses_network("http"),
            "h2Host" | "h2Path" => self.uses_network("h2"),
            "grpcServiceName" => self.uses_network("grpc"),
            "xhttpPath" | "xhttpHost" => self.uses_network("xhttp"),
            "mkcpSeed" | "mkcpHeader" => self.uses_network("mkcp"),
            "mekyaUrl" => self.uses_network("mekya"),
            "headers" => t == "http",
            "tls" => any(&["socks5", "http", "vmess", "vless", "gost-relay"]),
            "sni" => any(&[
                "http",
                "trojan",
                "hysteria",
                "hysteria2",
                "tuic",
                "shadowquic",
                "gost-relay",
                "anytls",
                "masque",
                "trusttunnel",
            ]),
            "servername" => t == "vmess" || t == "vless",
            "alpn" => any(&[
                "vmess",
                "vless",
                "trojan",
                "hysteria",
                "hysteria2",
                "tuic",
                "shadowquic",
                "anytls",
                "trusttunnel",
            ]),
            "skipCertVerify" => any(&[
                "socks5",
                "http",
                "vmess",
                "vless",
                "trojan",
                "hysteria",
                "hysteria2",
                "tuic",
                "gost-relay",
                "anytls",
                "masque",
                "trusttunnel",
            ]),
            "clientFingerprint" => any(&[
                "ss",
                "vmess",
                "vless",
                "snell",
                "trojan",
                "gost-relay",
                "anytls",
                "trusttunnel",
            ]),
            "realityPublicKey" | "realityShortId" => any(&["vmess", "vless", "trojan"]),
            "plugin" | "udpOverTcp" => t == "ss",
            "pluginMode"
            | "pluginHost"
            | "pluginPath"
            | "pluginUsername"
            | "pluginPassword"
            | "pluginVersion"
            | "pluginVersionHint"
            | "pluginRestlsScript"
            | "pluginKey"
            | "pluginCrypt"
            | "pluginTls"
            | "pluginMux"
            | "pluginSkipCertVerify" => {
                t == "ss" && self.plugin_reads(field.path[field.path.len() - 1])
            }
            "protocolParam" | "obfsParam" => t == "ssr",
            "obfsMode" | "obfsHost" | "version" => t == "snell",
            "ports" | "hopInterval" => t == "hysteria" || t == "hysteria2",
            "congestionController" => any(&["tuic", "shadowquic", "masque", "trusttunnel"]),
            "udpRelayMode" | "reduceRtt" => t == "tuic",
            "ip" | "ipv6" | "dns" | "remoteDnsResolve" | "mtu" => t == "wireguard" || t == "masque",
            "tfo" | "mptcp" => !any(&[
                "dns",
                "reject",
                "rematch",
                "hysteria",
                "hysteria2",
                "tuic",
                "wireguard",
                "shadowquic",
                "masque",
                "tailscale",
                "zerotier",
                "easytier",
            ]),
            "interfaceName" | "routingMark" | "ipVersion" => !any(&["dns", "reject", "rematch"]),
            _ => false,
        }
    }

    /// `choicesOf`: the values the Core accepts, in the form's order.
    pub fn choices(&self, field: &Field) -> Vec<&'static str> {
        let t = self.kind();
        let list: &[&str] = match (field.name, t.as_str()) {
            ("cipher", "ss") => SS_CIPHERS,
            ("cipher", "ssr") => SSR_CIPHERS,
            ("cipher", "vmess") => &[
                "auto",
                "aes-128-gcm",
                "chacha20-poly1305",
                "aes-128-cfb",
                "none",
                "zero",
            ],
            ("protocol", "ssr") => &[
                "origin",
                "auth_sha1_v4",
                "auth_aes128_md5",
                "auth_aes128_sha1",
                "auth_chain_a",
                "auth_chain_b",
            ],
            ("protocol", "hysteria") => &["udp", "wechat-video", "faketcp"],
            ("obfs", "ssr") => &[
                "plain",
                "http_simple",
                "http_post",
                "random_head",
                "tls1.2_ticket_auth",
                "tls1.2_ticket_fastauth",
            ],
            ("obfs", "hysteria2") => &["salamander", "gecko"],
            ("transport", _) => &["TCP", "UDP"],
            ("network", "vmess") => &["tcp", "ws", "http", "h2", "grpc", "mekya", "mkcp"],
            ("network", "vless") => &["tcp", "ws", "http", "h2", "grpc", "xhttp"],
            ("network", "trojan") => &["tcp", "ws", "grpc"],
            ("flow", _) => &["xtls-rprx-vision"],
            ("clientFingerprint", _) => &[
                "chrome",
                "firefox",
                "safari",
                "ios",
                "android",
                "edge",
                "360",
                "qq",
                "random",
                "chrome120",
                "firefox120",
                "safari16",
                "none",
            ],
            ("plugin", _) => &[
                "obfs",
                "v2ray-plugin",
                "gost-plugin",
                "shadow-tls",
                "restls",
                "jls",
                "kcptun",
            ],
            ("pluginVersionHint", _) => &["tls13", "tls12"],
            ("pluginCrypt", _) => &[
                "aes",
                "aes-128",
                "aes-192",
                "aes-128-gcm",
                "salsa20",
                "blowfish",
                "twofish",
                "cast5",
                "3des",
                "tea",
                "xtea",
                "xor",
                "none",
                "null",
            ],
            ("mkcpHeader", _) => &["none", "srtp", "utp", "wechat-video", "dtls", "wireguard"],
            ("obfsMode", _) => &["http", "tls", "shadow-tls", "restls", "jls"],
            ("congestionController", _) => {
                &["cubic", "new_reno", "bbr", "bbr_meta_v1", "bbr_meta_v2"]
            }
            ("udpRelayMode", _) => &["native", "quic"],
            ("ipVersion", _) => &["dual", "ipv4", "ipv6", "ipv4-prefer", "ipv6-prefer"],
            _ => &[],
        };
        list.to_vec()
    }

    pub fn with_value_at(&self, path: &[&str], value: Option<Value>) -> CustomProxy {
        CustomProxy {
            id: self.id.clone(),
            definition: Value::Object(with_path(&self.map(), path, value)),
        }
    }

    pub fn with_value(&self, key: &str, value: Option<Value>) -> CustomProxy {
        self.with_value_at(&[key], value)
    }

    /// `withField`: also drops the options only the old value made available.
    pub fn with_field(&self, field: &Field, value: Option<Value>) -> CustomProxy {
        let offered = self.options();
        let mut next = self.with_value_at(field.path, value);
        let still = next.options();
        for option in offered {
            if !still.contains(&option) {
                next = next.with_value_at(option.path, None);
            }
        }
        next
    }

    /// `withType`: keeps the values the new type reads alike, fills the usual defaults.
    pub fn with_type(&self, kind: &str) -> CustomProxy {
        if kind == self.kind() {
            return self.clone();
        }
        let mut next = self.with_value("type", Some(kind.into()));
        for field in FIELDS {
            let Some(value) = next.value_of(field).cloned() else {
                continue;
            };
            let choices = next.choices(field);
            let read = next.fields().contains(&field) || next.options().contains(&field);
            let alike = self.choices(field) == choices
                || value.as_str().is_some_and(|v| choices.contains(&v));
            if !(read && alike) {
                next = next.with_field(field, None);
            }
        }
        let defaults: &[(&str, Value)] = &match kind {
            "vmess" => vec![("cipher", Value::from("auto")), ("alterId", Value::from(0))],
            "mieru" => vec![("transport", Value::from("TCP"))],
            _ => vec![],
        };
        for (name, value) in defaults {
            let f = field_ref(name);
            if next.value_of(f).is_none() {
                next = next.with_field(f, Some(value.clone()));
            }
        }
        next
    }

    pub fn strings_of(&self, path: &[&str]) -> Vec<String> {
        match self.definition.as_object().and_then(|m| value_at(m, path)) {
            Some(Value::Array(items)) => items.iter().map(text_of).collect(),
            Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
            _ => Vec::new(),
        }
    }

    /// `stringMapOf`: the map less the keys other fields edit, value lists as one line.
    pub fn string_map_of(&self, field: &Field) -> Vec<(String, String)> {
        let owned = owned_keys_of(field);
        let Some(Value::Object(map)) = self.value_of(field) else {
            return Vec::new();
        };
        map.iter()
            .filter(|(k, v)| !v.is_null() && !owned.contains(&k.to_lowercase()))
            .map(|(k, v)| {
                let text = match v {
                    Value::Array(items) => items.iter().map(text_of).collect::<Vec<_>>().join(", "),
                    other => text_of(other),
                };
                (k.clone(), text)
            })
            .collect()
    }

    /// `withEntries`: an entry left as shown keeps its old value (a list stays a list).
    pub fn with_entries(&self, field: &Field, entries: &[(String, String)]) -> CustomProxy {
        let previous = self
            .value_of(field)
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let shown = self.string_map_of(field);
        let owned = owned_keys_of(field);
        let lists = field.name == "httpHeaders";
        let mut next = Map::new();
        for (k, v) in &previous {
            if owned.contains(&k.to_lowercase()) {
                next.insert(k.clone(), v.clone());
            }
        }
        for (k, v) in entries {
            let same = shown.iter().any(|(sk, sv)| sk == k && sv == v);
            let value = match previous.get(k) {
                Some(old) if same => old.clone(),
                _ if lists => Value::Array(vec![v.clone().into()]),
                _ => v.clone().into(),
            };
            next.insert(k.clone(), value);
        }
        self.with_field(field, Some(Value::Object(next)))
    }
}

fn field_ref(name: &str) -> &'static Field {
    field(name).expect("known field")
}

/// `ownedKeysOf`, lowercased: the keys of the field's map that fields of their own edit.
pub fn owned_keys_of(field: &Field) -> HashSet<String> {
    let n = field.path.len();
    FIELDS
        .iter()
        .filter(|o| o.path.len() == n + 1 && o.path[..n] == *field.path)
        .map(|o| o.path[n].to_lowercase())
        .collect()
}

pub fn is_credential(field: &Field) -> bool {
    matches!(
        field.name,
        "username"
            | "password"
            | "uuid"
            | "psk"
            | "key"
            | "privateKey"
            | "publicKey"
            | "privateKeyPassphrase"
            | "preSharedKey"
            | "authStr"
    )
}

pub fn section_of(field: &Field) -> Section {
    if is_credential(field) {
        return Section::Authentication;
    }
    match field.name {
        "network"
        | "flow"
        | "wsPath"
        | "wsHost"
        | "wsHeaders"
        | "wsMaxEarlyData"
        | "wsEarlyDataHeaderName"
        | "wsHttpUpgrade"
        | "wsHttpUpgradeFastOpen"
        | "httpMethod"
        | "httpPath"
        | "httpHost"
        | "httpHeaders"
        | "h2Host"
        | "h2Path"
        | "grpcServiceName"
        | "xhttpPath"
        | "xhttpHost"
        | "mkcpSeed"
        | "mkcpHeader"
        | "mekyaUrl" => Section::Transport,
        "tls" | "sni" | "servername" | "alpn" | "skipCertVerify" | "clientFingerprint"
        | "realityPublicKey" | "realityShortId" => Section::Tls,
        "tfo" | "mptcp" | "interfaceName" | "routingMark" | "ipVersion" => Section::Connection,
        _ => Section::Other,
    }
}

pub fn hint_of(field: &Field) -> Option<&'static str> {
    match field.name {
        "up" | "down" => Some("100 Mbps"),
        "ports" => Some("443,8000-9000"),
        "httpMethod" => Some("GET"),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Issue {
    EmptyName,
    ReservedName(String),
    DuplicateName(String),
    CoreRejected(String),
}

impl Issue {
    pub fn message(&self) -> String {
        match self {
            Issue::EmptyName => "名称为空".into(),
            Issue::ReservedName(n) => format!("{n} 是内置策略名，不能在此使用"),
            Issue::DuplicateName(n) => format!("名称 {n} 已被其他代理或策略组使用"),
            Issue::CoreRejected(m) => format!("内核无法解析该代理：{m}"),
        }
    }

    /// A new form holds these back until the first save.
    pub fn is_incomplete(&self) -> bool {
        matches!(self, Issue::EmptyName | Issue::CoreRejected(_))
    }

    pub fn is_name(&self) -> bool {
        !matches!(self, Issue::CoreRejected(_))
    }
}

/// `issues.getMessage`: one alone, else each on its own bulleted line.
pub fn issues_message(issues: &[Issue]) -> String {
    match issues {
        [one] => one.message(),
        _ => issues
            .iter()
            .map(|i| format!("• {}", i.message()))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// `customProxyIssues`.
pub fn issues(
    proxy: &CustomProxy,
    proxies: &[CustomProxy],
    group_names: &BTreeSet<String>,
    core_error: Option<&str>,
) -> Vec<Issue> {
    let name = proxy.name();
    let mut out = Vec::new();
    if name.is_empty() {
        out.push(Issue::EmptyName);
    } else if RESERVED_NAMES.contains(&name.as_str()) {
        out.push(Issue::ReservedName(name));
    } else if group_names.contains(&name)
        || proxies.iter().any(|p| p.id != proxy.id && p.name() == name)
    {
        out.push(Issue::DuplicateName(name));
    }
    if let Some(e) = core_error {
        out.push(Issue::CoreRejected(e.to_string()));
    }
    out
}

/// `importedCustomProxies`: each definition named as its link would be, apart from the
/// names already taken.
pub fn imported(
    definitions: &[Value],
    existing: &[CustomProxy],
    group_names: &BTreeSet<String>,
    mut new_id: impl FnMut() -> String,
) -> Vec<CustomProxy> {
    let mut taken: HashSet<String> = RESERVED_NAMES.iter().map(|s| s.to_string()).collect();
    taken.extend(group_names.iter().cloned());
    taken.extend(existing.iter().map(CustomProxy::name));
    definitions
        .iter()
        .filter_map(Value::as_object)
        .map(|d| {
            let name = crate::profile::free_proxy_name(
                &crate::profile::link_proxy_name(&Value::Object(d.clone())),
                &mut taken,
            );
            let mut d = d.clone();
            d.insert("name".into(), name.into());
            CustomProxy::new(new_id(), d)
        })
        .collect()
}

pub fn search<'a>(proxies: &'a [CustomProxy], query: &str) -> Vec<&'a CustomProxy> {
    proxies
        .iter()
        .filter(|p| crate::proxies::matches(query, &p.search_text()))
        .collect()
}

/// A list field's text: one item per line, commas also split.
pub fn parse_list(text: &str) -> Vec<String> {
    text.split(['\n', ','])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// A map field's text: `key: value` per line; Err names the first line without a key.
pub fn parse_map(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once(':').unwrap_or((line, ""));
        let (k, v) = (k.trim(), v.trim());
        if k.is_empty() {
            return Err(format!("第 {} 行：键不能为空", i + 1));
        }
        out.retain(|(ok, _)| ok != k);
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

pub fn map_text(entries: &[(String, String)]) -> String {
    entries
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The open form: the proxy as edited, which options it shows, and what the user revealed.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub proxy: CustomProxy,
    pub is_new: bool,
    pub shown: Vec<&'static str>,
    pub revealed: HashSet<String>,
    pub submitted: bool,
    pub core_error: Option<String>,
}

impl Draft {
    pub fn open(proxy: CustomProxy, is_new: bool) -> Self {
        let shown = proxy
            .options()
            .into_iter()
            .filter(|f| proxy.value_of(f).is_some())
            .map(|f| f.name)
            .collect();
        Draft {
            proxy,
            is_new,
            shown,
            ..Default::default()
        }
    }

    /// `_syncShown`: an option stays listed while offered and shown, set, or just unlocked.
    fn sync(&mut self, next: CustomProxy, unlocked: &[&'static str]) {
        self.shown = next
            .options()
            .into_iter()
            .filter(|f| {
                self.shown.contains(&f.name)
                    || unlocked.contains(&f.name)
                    || next.value_of(f).is_some()
            })
            .map(|f| f.name)
            .collect();
        self.proxy = next;
    }

    pub fn set_option(&mut self, field: &Field, value: Option<Value>) {
        let offered: Vec<_> = self.proxy.options().iter().map(|f| f.name).collect();
        let next = self.proxy.with_field(field, value);
        let unlocked: Vec<_> = next
            .options()
            .iter()
            .map(|f| f.name)
            .filter(|n| !offered.contains(n))
            .collect();
        self.sync(next, &unlocked);
    }

    pub fn set_type(&mut self, kind: &str) {
        let next = self.proxy.with_type(kind);
        self.sync(next, &[]);
    }

    /// `_handleAddedOption`: a toggle starts on, a choice at its first value.
    pub fn add_option(&mut self, field: &'static Field) {
        let initial = match field.kind {
            Kind::Toggle => Some(Value::Bool(true)),
            Kind::Choice => self.proxy.choices(field).first().map(|c| (*c).into()),
            _ => None,
        };
        let value = self.proxy.value_of(field).cloned().or(initial);
        if !self.shown.contains(&field.name) {
            self.shown.push(field.name);
        }
        self.set_option(field, value);
    }

    pub fn remove_option(&mut self, field: &Field) {
        self.shown.retain(|n| *n != field.name);
        self.set_option(field, None);
    }

    /// The options not listed yet, by section.
    pub fn addable(&self) -> Vec<(Section, &'static Field)> {
        let mut out: Vec<_> = self
            .proxy
            .options()
            .into_iter()
            .filter(|f| !self.shown.contains(&f.name))
            .map(|f| (section_of(f), f))
            .collect();
        out.sort_by_key(|(s, _)| *s);
        out
    }

    pub fn listed(&self) -> Vec<&'static Field> {
        self.proxy
            .options()
            .into_iter()
            .filter(|f| self.shown.contains(&f.name))
            .collect()
    }

    pub fn issues(&self, proxies: &[CustomProxy]) -> Vec<Issue> {
        issues(
            &self.proxy,
            proxies,
            &BTreeSet::new(),
            self.core_error.as_deref(),
        )
        .into_iter()
        .filter(|i| !self.is_new || self.submitted || !i.is_incomplete())
        .collect()
    }
}

/// What a row shows for a field: its value, masked when secret, or the hint.
pub fn shown_value(proxy: &CustomProxy, field: &Field, revealed: bool) -> String {
    let required = || {
        if proxy.is_required(&field.path.join(".")) {
            "必填"
        } else {
            "可选"
        }
    };
    let count = |n: usize| crate::overrides::count(n);
    match field.kind {
        Kind::List => count(proxy.strings_of(field.path).len()),
        Kind::Map => count(proxy.string_map_of(field).len()),
        Kind::Toggle => String::new(),
        _ => match proxy.value_of(field).map(text_of) {
            Some(v) if field.kind == Kind::Secret && !revealed => {
                "•".repeat(v.chars().count().clamp(6, 12))
            }
            Some(v) => v,
            None => hint_of(field).unwrap_or_else(required).to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proxy(v: Value) -> CustomProxy {
        CustomProxy {
            id: "1".into(),
            definition: v,
        }
    }

    fn names(fields: &[&Field]) -> Vec<&'static str> {
        fields.iter().map(|f| f.name).collect()
    }

    #[test]
    fn the_table_names_each_field_once() {
        let mut seen = HashSet::new();
        assert!(FIELDS.iter().all(|f| seen.insert(f.name)));
        assert_eq!(FIELDS.len(), 85);
        assert_eq!(TYPES.len(), 28);
        assert_eq!(section_of(field_ref("wsHost")), Section::Transport);
        assert_eq!(section_of(field_ref("privateKey")), Section::Authentication);
        assert_eq!(section_of(field_ref("alpn")), Section::Tls);
        assert_eq!(section_of(field_ref("tfo")), Section::Connection);
        assert_eq!(section_of(field_ref("plugin")), Section::Other);
    }

    #[test]
    fn fields_and_required_keys_follow_the_type() {
        let p = proxy(json!({"name": "a", "type": "ss"}));
        assert_eq!(names(&p.fields()), ["cipher", "password"]);
        assert!(p.is_required("cipher"));
        assert!(!p.is_required("udp"));
        assert_eq!(p.choices(field_ref("cipher"))[0], "aes-128-gcm");
        let v = proxy(json!({"type": "vmess"}));
        assert_eq!(names(&v.fields()), ["cipher", "alterId", "uuid"]);
        assert_eq!(v.choices(field_ref("cipher"))[0], "auto");
        assert!(proxy(json!({"type": "direct"})).fields().is_empty());
    }

    #[test]
    fn options_open_with_the_values_they_hang_on() {
        let p = proxy(json!({"type": "vless"}));
        let opts = names(&p.options());
        assert!(opts.contains(&"network") && opts.contains(&"flow") && opts.contains(&"tls"));
        assert!(!opts.contains(&"wsPath"));
        let ws = p.with_field(field_ref("network"), Some("ws".into()));
        assert!(names(&ws.options()).contains(&"wsPath"));
        assert!(!names(&ws.options()).contains(&"wsHttpUpgradeFastOpen"));
        let up = ws.with_field(field_ref("wsHttpUpgrade"), Some(true.into()));
        assert!(names(&up.options()).contains(&"wsHttpUpgradeFastOpen"));
        let ss = proxy(json!({"type": "ss", "plugin": "obfs"}));
        let opts = names(&ss.options());
        assert!(opts.contains(&"pluginMode") && !opts.contains(&"pluginPath"));
        assert!(!names(&proxy(json!({"type": "dns"})).options()).contains(&"tfo"));
    }

    #[test]
    fn nested_values_drop_the_maps_they_empty() {
        let p = proxy(json!({"type": "vmess", "network": "ws"}));
        let host = field_ref("wsHost");
        let p = p.with_field(host, Some("h.example".into()));
        assert_eq!(p.definition["ws-opts"]["headers"]["Host"], "h.example");
        assert_eq!(p.value_of(host).unwrap(), "h.example");
        let p = p.with_field(host, Some("".into()));
        assert!(p.definition.get("ws-opts").is_none());
        let p = proxy(json!({"ws-opts": {"headers": {"host": "x"}}}));
        assert_eq!(p.value_of(host).unwrap(), "x");
        let p = p.with_value_at(host.path, Some("y".into()));
        assert_eq!(p.definition["ws-opts"]["headers"], json!({"host": "y"}));
    }

    #[test]
    fn leaving_a_network_drops_its_options() {
        let p = proxy(json!({"type": "vmess", "network": "ws", "ws-opts": {"path": "/a"}}));
        let p = p.with_field(field_ref("network"), Some("grpc".into()));
        assert!(p.definition.get("ws-opts").is_none());
        assert_eq!(p.definition["network"], "grpc");
    }

    #[test]
    fn a_new_type_keeps_what_it_reads_alike() {
        let p = proxy(json!({"name": "a", "type": "ss", "cipher": "aes-128-gcm",
            "password": "pw", "plugin": "obfs", "tfo": true}));
        let v = p.with_type("vmess");
        assert_eq!(v.definition["cipher"], "aes-128-gcm");
        assert_eq!(v.definition["alterId"], 0);
        assert!(v.definition.get("password").is_none());
        assert!(v.definition.get("plugin").is_none());
        assert_eq!(v.definition["tfo"], true);
        let t = p.with_type("trojan");
        assert_eq!(t.definition["password"], "pw");
        assert!(t.definition.get("cipher").is_none());
        let m = p.with_type("mieru");
        assert_eq!(m.definition["transport"], "TCP");
    }

    #[test]
    fn maps_and_lists_read_and_write_as_shown() {
        let p = proxy(json!({"type": "vmess", "network": "http",
            "http-opts": {"headers": {"Host": ["a"], "X": ["1", "2"]}}}));
        let headers = field_ref("httpHeaders");
        assert_eq!(p.string_map_of(headers), [("X".into(), "1, 2".into())]);
        let q = p.with_entries(
            headers,
            &[("X".into(), "1, 2".into()), ("Y".into(), "3".into())],
        );
        let h = &q.definition["http-opts"]["headers"];
        assert_eq!(h["X"], json!(["1", "2"]));
        assert_eq!(h["Y"], json!(["3"]));
        assert_eq!(h["Host"], json!(["a"]));
        assert_eq!(q.strings_of(field_ref("httpHost").path), ["a"]);
        assert_eq!(parse_list("a, b\n\nc"), ["a", "b", "c"]);
        assert_eq!(
            parse_map("Host: x.example\nX: 1:2\n").unwrap(),
            [
                ("Host".into(), "x.example".into()),
                ("X".into(), "1:2".into())
            ]
        );
        assert!(parse_map(": v").is_err());
        assert_eq!(map_text(&[("a".into(), "b".into())]), "a: b");
    }

    #[test]
    fn names_must_be_set_free_and_unique() {
        let a = proxy(json!({"name": "a"}));
        let mut b = proxy(json!({"name": "a"}));
        b.id = "2".into();
        let none = BTreeSet::new();
        assert_eq!(
            issues(&a, &[a.clone(), b.clone()], &none, None),
            [Issue::DuplicateName("a".into())]
        );
        assert!(issues(&a, &[a.clone()], &none, None).is_empty());
        let g = proxy(json!({"name": "GLOBAL"}));
        assert_eq!(
            issues(&g, &[], &none, None),
            [Issue::ReservedName("GLOBAL".into())]
        );
        let e = proxy(json!({}));
        let all = issues(&e, &[], &none, Some("bad"));
        assert_eq!(all, [Issue::EmptyName, Issue::CoreRejected("bad".into())]);
        assert_eq!(
            issues_message(&all),
            "• 名称为空\n• 内核无法解析该代理：bad"
        );
        assert_eq!(issues_message(&all[..1]), "名称为空");
    }

    #[test]
    fn imports_take_free_names() {
        let existing = [proxy(json!({"name": "hk"}))];
        let mut n = 0;
        let got = imported(
            &[
                json!({"name": "hk", "type": "ss"}),
                json!({"server": "s.example", "type": "ss"}),
                json!({"name": "DIRECT", "type": "ss"}),
            ],
            &existing,
            &BTreeSet::new(),
            || {
                n += 1;
                n.to_string()
            },
        );
        let names: Vec<_> = got.iter().map(CustomProxy::name).collect();
        assert_eq!(names, ["hk-01", "s.example", "DIRECT-01"]);
        assert_eq!(got[2].id, "3");
    }

    #[test]
    fn the_draft_lists_added_options_until_removed() {
        let mut d = Draft::open(
            proxy(json!({"name": "a", "type": "vless", "tls": true})),
            false,
        );
        assert_eq!(d.shown, ["tls"]);
        d.add_option(field_ref("network"));
        assert_eq!(d.proxy.definition["network"], "tcp");
        d.set_option(field_ref("network"), Some("ws".into()));
        d.add_option(field_ref("wsPath"));
        assert!(d.listed().iter().any(|f| f.name == "wsPath"));
        assert!(d.addable().iter().all(|(_, f)| f.name != "wsPath"));
        d.remove_option(field_ref("network"));
        assert!(d.proxy.definition.get("network").is_none());
        assert!(!d.shown.contains(&"wsPath"));
        d.set_type("ss");
        assert!(d.shown.is_empty());
        let mut n = Draft::open(proxy(json!({"type": "ss"})), true);
        n.core_error = Some("x".into());
        assert!(n.issues(&[]).is_empty());
        n.submitted = true;
        assert_eq!(n.issues(&[]).len(), 2);
    }

    #[test]
    fn rows_show_values_masks_and_hints() {
        let p = proxy(json!({"type": "ss", "password": "secret", "alpn": ["h2"]}));
        assert_eq!(shown_value(&p, field_ref("password"), false), "••••••");
        assert_eq!(shown_value(&p, field_ref("password"), true), "secret");
        assert_eq!(shown_value(&p, field_ref("cipher"), false), "必填");
        assert_eq!(shown_value(&p, field_ref("up"), false), "100 Mbps");
        assert_eq!(shown_value(&p, field_ref("alpn"), false), "1 项");
        assert_eq!(shown_value(&p, field_ref("dns"), false), "无");
        assert_eq!(search(&[p.clone()], "SS").len(), 1);
        assert_eq!(search(&[p], "vmess").len(), 0);
    }
}
