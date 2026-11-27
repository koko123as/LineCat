//! The part of the Clash configuration the editor completes against, ported from FlClash rs
//! 56a9cf9 (FlClash: lib/features/editor/clash_schema.dart) with Concord's t0 and t1 keys
//! already picked. Each node names its children by index into `NODES`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Scalar,
    Map,
    List,
}

/// Where a scalar's completions come from besides `Node::values`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar {
    Plain,
    Rule,
    RulePayload,
    Policy,
    /// A policy the Core looks up once the config is loaded, when GLOBAL exists.
    Outbound,
    ProxyProvider,
    RuleProvider,
    SubRule,
}

pub type Fields = &'static [(&'static str, u16)];

pub struct Node {
    pub kind: Kind,
    pub scalar: Scalar,
    pub fields: Fields,
    /// More fields by the value of the map's `type`.
    pub variants: &'static [(&'static str, Fields)],
    /// What a list's items or a free-keyed map's values are.
    pub entry: Option<u16>,
    pub values: &'static [&'static str],
}

pub use data::*;

#[rustfmt::skip]
mod data {
use super::*;
pub static NODES: &[Node] = &[
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("mixed-port", 1), ("port", 1), ("socks-port", 1), ("redir-port", 1), ("tproxy-port", 1), ("allow-lan", 2), ("bind-address", 3), ("lan-allowed-ips", 4), ("lan-disallowed-ips", 4), ("authentication", 4), ("skip-auth-prefixes", 4), ("mode", 5), ("log-level", 6), ("ipv6", 2), ("unified-delay", 2), ("tcp-concurrent", 2), ("find-process-mode", 7), ("global-ua", 1), ("keep-alive-interval", 1), ("keep-alive-idle", 1), ("disable-keep-alive", 2), ("interface-name", 1), ("routing-mark", 1), ("external-controller", 8), ("external-controller-cors", 9), ("secret", 1), ("profile", 10), ("geo-auto-update", 2), ("geo-update-interval", 1), ("geox-url", 11), ("hosts", 12), ("tls", 13), ("experimental", 14), ("dns", 15), ("tun", 26), ("sniffer", 31), ("ntp", 34), ("proxies", 37), ("proxy-groups", 105), ("proxy-providers", 115), ("rule-providers", 122), ("rules", 128), ("sub-rules", 130)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["true", "false"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["*"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(1), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["rule", "global", "direct"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["debug", "info", "warning", "error", "silent"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["always", "strict", "off"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["127.0.0.1:9090"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("allow-origins", 4), ("allow-private-network", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("store-fake-ip", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("geosite", 1), ("mmdb", 1), ("asn", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(1), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("custom-certifactes", 4)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("dialer-ip4p-convert", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("listen", 16), ("listen-routing-mark", 1), ("ipv6", 2), ("ipv6-timeout", 1), ("prefer-h3", 2), ("cache-algorithm", 17), ("cache-max-size", 1), ("use-hosts", 2), ("use-system-hosts", 2), ("respect-rules", 2), ("enhanced-mode", 18), ("fake-ip-range", 19), ("fake-ip-range6", 1), ("fake-ip-filter", 4), ("fake-ip-filter-mode", 20), ("fake-ip-ttl", 1), ("default-nameserver", 21), ("nameserver", 21), ("fallback", 21), ("fallback-lazy-query", 2), ("proxy-server-nameserver", 21), ("direct-nameserver", 21), ("direct-nameserver-follow-policy", 2), ("nameserver-policy", 23), ("proxy-server-nameserver-policy", 23), ("fallback-filter", 24)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["0.0.0.0:1053"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["lru", "arc"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["normal", "fake-ip", "redir-host"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["198.18.0.1/16"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["blacklist", "whitelist", "rule"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(22), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["https://dns.alidns.com/dns-query", "https://doh.pub/dns-query", "https://dns.google/dns-query", "https://cloudflare-dns.com/dns-query", "tls://dns.alidns.com", "tls://1.1.1.1", "quic://dns.adguard-dns.com", "223.5.5.5", "119.29.29.29", "8.8.8.8", "1.1.1.1", "system", "dhcp://en0"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(21), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("geoip", 2), ("geoip-code", 25), ("geosite", 4), ("ipcidr", 4), ("domain", 4)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["CN"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("stack", 27), ("device", 1), ("dns-hijack", 28), ("auto-route", 2), ("auto-detect-interface", 2), ("strict-route", 2), ("mtu", 1), ("gso", 2), ("udp-timeout", 1), ("disable-icmp-forwarding", 2), ("route-address", 4), ("route-exclude-address", 4), ("include-interface", 4), ("exclude-interface", 4), ("include-uid", 4), ("exclude-uid", 4), ("inet4-route-address", 4), ("inet6-route-address", 4), ("inet4-route-exclude-address", 4), ("inet6-route-exclude-address", 4), ("exclude-src-port", 4), ("exclude-dst-port", 4), ("congestion-controller", 30)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["gvisor", "mips"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(29), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["any:53", "tcp://any:53"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["cubic", "reno", "bbr"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("force-dns-mapping", 2), ("parse-pure-ip", 2), ("override-destination", 2), ("sniff", 32), ("force-domain", 4), ("skip-domain", 4), ("skip-src-address", 4), ("skip-dst-address", 4)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("HTTP", 33), ("TLS", 33), ("QUIC", 33)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("ports", 4), ("override-destination", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("write-to-system", 2), ("server", 35), ("port", 1), ("interval", 1), ("dialer-proxy", 36)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["time.apple.com", "ntp.aliyun.com"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Outbound, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(38), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("name", 1), ("type", 39)], variants: &[("ss", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("cipher", 41), ("password", 1), ("udp-over-tcp", 2), ("udp-over-tcp-version", 42), ("client-fingerprint", 43), ("plugin", 44), ("plugin-opts", 45), ("smux", 49)]), ("ssr", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("cipher", 52), ("password", 1), ("obfs", 53), ("obfs-param", 1), ("protocol", 54), ("protocol-param", 1)]), ("vmess", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("uuid", 1), ("alterId", 1), ("cipher", 55), ("packet-encoding", 56), ("global-padding", 2), ("authenticated-length", 2), ("tls", 2), ("servername", 1), ("client-fingerprint", 43), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47), ("shadow-tls-opts", 59), ("jls-opts", 60), ("reality-opts", 61), ("network", 62), ("ws-opts", 63), ("http-opts", 64), ("h2-opts", 67), ("grpc-opts", 68), ("smux", 49)]), ("vless", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("uuid", 1), ("flow", 69), ("encryption", 1), ("packet-encoding", 56), ("tls", 2), ("servername", 1), ("client-fingerprint", 43), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47), ("shadow-tls-opts", 59), ("jls-opts", 60), ("reality-opts", 61), ("network", 70), ("ws-opts", 63), ("http-opts", 64), ("h2-opts", 67), ("grpc-opts", 68), ("xhttp-opts", 71), ("smux", 49)]), ("trojan", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("password", 1), ("ss-opts", 80), ("client-fingerprint", 43), ("sni", 1), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47), ("shadow-tls-opts", 59), ("jls-opts", 60), ("reality-opts", 61), ("network", 82), ("ws-opts", 63), ("grpc-opts", 68), ("smux", 49)]), ("anytls", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("password", 1), ("idle-session-check-interval", 1), ("idle-session-timeout", 1), ("min-idle-session", 1), ("client-metadata", 1), ("disable-reuse", 2), ("client-fingerprint", 43), ("sni", 1), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47), ("shadow-tls-opts", 59), ("jls-opts", 60)]), ("hysteria2", &[("server", 1), ("port", 1), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("ports", 1), ("password", 1), ("up", 1), ("down", 1), ("obfs", 83), ("obfs-password", 1), ("obfs-min-packet-size", 1), ("obfs-max-packet-size", 1), ("hop-interval", 1), ("bbr-profile", 84), ("cwnd", 1), ("udp-mtu", 1), ("handshake-timeout", 1), ("initial-stream-receive-window", 1), ("max-stream-receive-window", 1), ("initial-connection-receive-window", 1), ("max-connection-receive-window", 1), ("realm-opts", 85), ("sni", 1), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47)]), ("hysteria", &[("server", 1), ("port", 1), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("ports", 1), ("auth-str", 1), ("auth", 1), ("protocol", 86), ("obfs", 1), ("up", 1), ("down", 1), ("recv-window-conn", 1), ("recv-window", 1), ("disable-mtu-discovery", 2), ("fast-open", 2), ("hop-interval", 1), ("sni", 1), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47)]), ("tuic", &[("server", 1), ("port", 1), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("uuid", 1), ("password", 1), ("token", 1), ("ip", 1), ("heartbeat-interval", 1), ("disable-sni", 2), ("reduce-rtt", 2), ("request-timeout", 1), ("udp-relay-mode", 87), ("congestion-controller", 88), ("bbr-profile", 84), ("max-udp-relay-packet-size", 1), ("fast-open", 2), ("max-open-streams", 1), ("cwnd", 1), ("recv-window-conn", 1), ("recv-window", 1), ("disable-mtu-discovery", 2), ("max-datagram-frame-size", 1), ("udp-over-stream", 2), ("udp-over-stream-version", 42), ("sni", 1), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47)]), ("wireguard", &[("server", 1), ("port", 1), ("udp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("ip", 1), ("ipv6", 1), ("private-key", 1), ("public-key", 1), ("pre-shared-key", 1), ("reserved", 4), ("mtu", 1), ("workers", 1), ("refresh-server-ip-interval", 1), ("ip-stack", 89), ("allowed-ips", 4), ("persistent-keepalive", 1), ("remote-dns-resolve", 2), ("dns", 4), ("peers", 91), ("amnezia-wg-option", 93)]), ("socks5", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("username", 1), ("password", 1), ("tls", 2), ("fingerprint", 1), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("certificate", 1), ("private-key", 1)]), ("http", &[("server", 1), ("port", 1), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("username", 1), ("password", 1), ("tls", 2), ("sni", 1), ("fingerprint", 1), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("certificate", 1), ("private-key", 1), ("headers", 12)]), ("snell", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("psk", 1), ("version", 94), ("mode", 95), ("userkey", 1), ("reuse", 2), ("client-fingerprint", 43), ("obfs-opts", 96)]), ("ssh", &[("server", 1), ("port", 1), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("username", 1), ("password", 1), ("private-key", 1), ("private-key-passphrase", 1), ("host-key", 4), ("host-key-algorithms", 4)]), ("mieru", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("port-range", 1), ("transport", 98), ("username", 1), ("password", 1), ("multiplexing", 99), ("handshake-mode", 100), ("traffic-pattern", 1)]), ("shadowquic", &[("server", 1), ("port", 1), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("username", 1), ("password", 1), ("sni", 1), ("alpn", 4), ("quic-versions", 101), ("udp-over-stream", 2), ("zero-rtt", 2), ("disable-mtu-discovery", 2), ("congestion-controller", 88), ("bbr-profile", 84), ("cwnd", 1), ("up", 1), ("down", 1), ("keep-alive-interval", 1), ("recv-window-conn", 1), ("recv-window", 1), ("max-datagram-frame-size", 1), ("max-open-streams", 1)]), ("gost-relay", &[("server", 1), ("port", 1), ("udp", 2), ("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("username", 1), ("password", 1), ("forward", 2), ("mux", 2), ("tls", 2), ("sni", 1), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("client-fingerprint", 43)]), ("masque", &[("server", 1), ("port", 1), ("udp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1), ("dialer-proxy", 36), ("private-key", 1), ("public-key", 1), ("ip", 1), ("ipv6", 1), ("uri", 1), ("sni", 1), ("skip-cert-verify", 2), ("network", 103), ("congestion-controller", 88), ("bbr-profile", 84), ("cwnd", 1), ("mtu", 1), ("handshake-timeout", 1), ("ip-stack", 89), ("remote-dns-resolve", 2), ("dns", 4)]), ("direct", &[("tfo", 2), ("mptcp", 2), ("ip-version", 40), ("interface-name", 1), ("routing-mark", 1)]), ("dns", &[]), ("reject", &[]), ("rematch", &[("target-rematch-name", 1), ("target-sub-rule", 104)])], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["ss", "ssr", "vmess", "vless", "trojan", "anytls", "hysteria2", "hysteria", "tuic", "wireguard", "socks5", "http", "snell", "ssh", "mieru", "shadowquic", "gost-relay", "masque", "direct", "dns", "reject", "rematch"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["dual", "ipv4", "ipv6", "ipv4-prefer", "ipv6-prefer"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["aes-128-gcm", "aes-192-gcm", "aes-256-gcm", "chacha20-ietf-poly1305", "xchacha20-ietf-poly1305", "2022-blake3-aes-128-gcm", "2022-blake3-aes-256-gcm", "2022-blake3-chacha20-poly1305", "aes-128-ctr", "aes-192-ctr", "aes-256-ctr", "aes-128-cfb", "aes-192-cfb", "aes-256-cfb", "rc4-md5", "chacha20-ietf", "xchacha20", "chacha20", "none"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["1", "2"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "chrome120", "firefox120", "safari16", "none"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["obfs", "v2ray-plugin", "gost-plugin", "shadow-tls", "jls"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("mode", 46), ("host", 1), ("path", 1), ("tls", 2), ("headers", 12), ("mux", 2), ("v2ray-http-upgrade", 2), ("v2ray-http-upgrade-fast-open", 2), ("alpn", 4), ("ech-opts", 47), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("certificate", 1), ("private-key", 1), ("username", 1), ("password", 1), ("version", 48), ("fingerprint", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["http", "tls", "websocket"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("config", 1), ("query-server-name", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["1", "2", "3"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enabled", 2), ("protocol", 50), ("max-connections", 1), ("min-streams", 1), ("max-streams", 1), ("padding", 2), ("statistic", 2), ("only-tcp", 2), ("brutal-opts", 51)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["smux", "yamux", "h2mux"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enabled", 2), ("up", 1), ("down", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["aes-128-ctr", "aes-192-ctr", "aes-256-ctr", "aes-128-cfb", "aes-192-cfb", "aes-256-cfb", "rc4-md5", "chacha20-ietf", "xchacha20", "chacha20", "none"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["plain", "http_simple", "http_post", "random_head", "tls1.2_ticket_auth", "tls1.2_ticket_fastauth"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["origin", "auth_sha1_v4", "auth_aes128_md5", "auth_aes128_sha1", "auth_chain_a", "auth_chain_b"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["auto", "none", "zero", "aes-128-gcm", "chacha20-poly1305", "aes-128-cfb"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["packetaddr", "xudp"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(58), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["h2", "http/1.1", "h3"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("host", 1), ("password", 1), ("version", 48), ("fingerprint", 1), ("skip-cert-verify", 2), ("alpn", 57), ("certificate", 1), ("private-key", 1), ("name-cert-verify", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("username", 1), ("password", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("public-key", 1), ("short-id", 1), ("support-x25519mlkem768", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["tcp", "ws", "http", "h2", "grpc"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("path", 1), ("headers", 12), ("max-early-data", 1), ("early-data-header-name", 1), ("v2ray-http-upgrade", 2), ("v2ray-http-upgrade-fast-open", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("method", 65), ("path", 4), ("headers", 66)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["GET", "POST"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(4), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("host", 4), ("path", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("grpc-service-name", 1), ("grpc-user-agent", 1), ("ping-interval", 1), ("max-connections", 1), ("min-streams", 1), ("max-streams", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["xtls-rprx-vision"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["tcp", "ws", "http", "h2", "grpc", "xhttp"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("path", 1), ("host", 1), ("mode", 72), ("headers", 12), ("no-grpc-header", 2), ("x-padding-bytes", 1), ("x-padding-obfs-mode", 2), ("x-padding-key", 1), ("x-padding-header", 1), ("x-padding-placement", 73), ("x-padding-method", 74), ("uplink-http-method", 1), ("session-placement", 75), ("session-key", 1), ("session-table", 76), ("session-length", 1), ("seq-placement", 75), ("seq-key", 1), ("uplink-data-placement", 77), ("uplink-data-key", 1), ("uplink-chunk-size", 1), ("sc-max-each-post-bytes", 1), ("sc-min-posts-interval-ms", 1), ("reuse-settings", 78), ("download-settings", 79)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["auto", "packet-up", "stream-up", "stream-one"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["queryInHeader", "header", "cookie", "query"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["repeat-x", "tokenish"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["path", "query", "header", "cookie"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["uuid", "number", "hex", "HEX", "alphabet", "Alphabet", "ALPHABET", "base36", "BASE36", "Base62"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["body", "header", "cookie"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("max-concurrency", 1), ("max-connections", 1), ("c-max-reuse-times", 1), ("h-max-request-times", 1), ("h-max-reusable-secs", 1), ("h-keep-alive-period", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("server", 1), ("port", 1), ("path", 1), ("host", 1), ("headers", 12), ("reuse-settings", 78), ("tls", 2), ("servername", 1), ("client-fingerprint", 43), ("alpn", 57), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("fingerprint", 1), ("certificate", 1), ("private-key", 1), ("ech-opts", 47), ("shadow-tls-opts", 59), ("jls-opts", 60), ("reality-opts", 61)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enabled", 2), ("method", 81), ("password", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["aes-128-gcm", "aes-192-gcm", "aes-256-gcm", "chacha20-ietf-poly1305", "xchacha20-ietf-poly1305"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["tcp", "ws", "grpc"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["salamander", "gecko"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["conservative", "standard", "aggressive"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("server-url", 1), ("token", 1), ("realm-id", 1), ("stun-servers", 4), ("sni", 1), ("alpn", 4), ("fingerprint", 1), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("certificate", 1), ("private-key", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["udp", "wechat-video", "faketcp"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["native", "quic"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["bbr", "cubic", "new_reno", "bbr_meta_v1", "bbr_meta_v2"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("mode", 90), ("congestion-controller", 30)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["auto", "gvisor", "mips"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(92), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("server", 1), ("port", 1), ("public-key", 1), ("pre-shared-key", 1), ("reserved", 4), ("allowed-ips", 4)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("jc", 1), ("jmin", 1), ("jmax", 1), ("s1", 1), ("s2", 1), ("h1", 1), ("h2", 1), ("h3", 1), ("h4", 1), ("version", 1), ("s3", 1), ("s4", 1), ("i1", 1), ("i2", 1), ("i3", 1), ("i4", 1), ("i5", 1), ("header-protection-key", 1), ("content-padding-addition", 1), ("rekey-after-time", 1), ("rekey-timeout", 1), ("reject-after-time", 1), ("keepalive-timeout", 1), ("max-handshake-attempts", 1), ("random-trailers", 2), ("disable-cookies", 2)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["1", "2", "3", "4", "5", "6"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["default", "unshaped", "unsafe-raw"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("mode", 97), ("host", 1), ("alpn", 4), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("certificate", 1), ("private-key", 1), ("username", 1), ("password", 1), ("version", 48), ("fingerprint", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["http", "tls", "shadow-tls", "jls"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["TCP", "UDP"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["MULTIPLEXING_DEFAULT", "MULTIPLEXING_OFF", "MULTIPLEXING_LOW", "MULTIPLEXING_MIDDLE", "MULTIPLEXING_HIGH"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["HANDSHAKE_DEFAULT", "HANDSHAKE_STANDARD", "HANDSHAKE_NO_WAIT"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(102), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["v1", "v2"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["h2", "h3-l4proxy"] },
    Node { kind: Kind::Scalar, scalar: Scalar::SubRule, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(106), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("name", 1), ("type", 107), ("proxies", 108), ("use", 110), ("use-position", 12), ("url", 112), ("interval", 1), ("lazy", 2), ("timeout", 1), ("max-failed-times", 1), ("empty-fallback", 109), ("expected-status", 1), ("filter", 1), ("exclude-filter", 1), ("exclude-type", 1), ("disable-udp", 2), ("hidden", 2), ("icon", 1), ("include-all", 2), ("include-all-proxies", 2), ("include-all-providers", 2)], variants: &[("select", &[("default-selected", 109)]), ("url-test", &[("tolerance", 1)]), ("smart", &[("tolerance", 1), ("policy-priority", 1)]), ("load-balance", &[("strategy", 113), ("hash-key", 114)])], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["select", "url-test", "fallback", "load-balance", "smart"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(109), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Policy, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(111), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::ProxyProvider, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["https://www.gstatic.com/generate_204"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["consistent-hashing", "round-robin", "sticky-sessions"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["in-user"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(116), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("type", 117), ("url", 1), ("path", 1), ("interval", 1), ("proxy", 36), ("dialer-proxy", 36), ("size-limit", 1), ("header", 66), ("age-secret-key", 1), ("health-check", 118), ("override", 119), ("filter", 1), ("exclude-filter", 1), ("exclude-type", 1), ("payload", 37)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["http", "file", "inline"] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("enable", 2), ("url", 112), ("interval", 1), ("timeout", 1), ("lazy", 2), ("expected-status", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("tfo", 2), ("mptcp", 2), ("udp", 2), ("udp-over-tcp", 2), ("up", 1), ("down", 1), ("skip-cert-verify", 2), ("name-cert-verify", 1), ("dialer-proxy", 36), ("interface-name", 1), ("routing-mark", 1), ("ip-version", 40), ("additional-prefix", 1), ("additional-suffix", 1), ("proxy-name", 120), ("override-expr", 4)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(121), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("pattern", 1), ("target", 1)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(123), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("type", 117), ("behavior", 124), ("format", 125), ("url", 1), ("path", 1), ("interval", 1), ("proxy", 36), ("size-limit", 1), ("header", 66), ("path-in-bundle", 1), ("payload", 126)], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["domain", "ipcidr", "classical"] },
    Node { kind: Kind::Scalar, scalar: Scalar::Plain, fields: &[], variants: &[], entry: None, values: &["yaml", "text", "mrs"] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(127), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::RulePayload, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::List, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(129), values: &[] },
    Node { kind: Kind::Scalar, scalar: Scalar::Rule, fields: &[], variants: &[], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[], variants: &[], entry: Some(128), values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("name", 1), ("type", 107), ("proxies", 108), ("use", 110), ("use-position", 12), ("url", 112), ("interval", 1), ("lazy", 2), ("timeout", 1), ("max-failed-times", 1), ("empty-fallback", 109), ("expected-status", 1), ("filter", 1), ("exclude-filter", 1), ("exclude-type", 1), ("disable-udp", 2), ("hidden", 2), ("icon", 1)], variants: &[("select", &[("default-selected", 109)]), ("url-test", &[("tolerance", 1)]), ("smart", &[("tolerance", 1), ("policy-priority", 1)]), ("load-balance", &[("strategy", 113), ("hash-key", 114)])], entry: None, values: &[] },
    Node { kind: Kind::Map, scalar: Scalar::Plain, fields: &[("proxies", 37), ("payload", 126)], variants: &[], entry: None, values: &[] },
];
pub const CONFIG: u16 = 0;
pub const DNS: u16 = 15;
pub const NTP: u16 = 34;
pub const SNIFFER: u16 = 31;
pub const PROXY: u16 = 38;
pub const PROXY_GROUP: u16 = 131;
pub const RULES: u16 = 128;
pub const SHARE_LINKS: u16 = 1;
pub const PROVIDER: u16 = 132;
pub const RULE_TYPES: &[(&str, &str)] = &[("DOMAIN", "example.com"), ("DOMAIN-SUFFIX", "example.com"), ("DOMAIN-KEYWORD", "example"), ("DOMAIN-WILDCARD", "*.example.com"), ("DOMAIN-REGEX", "^.*\\.example\\.com$"), ("GEOSITE", "cn"), ("IP-CIDR", "192.168.0.0/16"), ("IP-CIDR6", "fc00::/7"), ("IP-SUFFIX", "8.8.8.8/24"), ("IP-ASN", "13335"), ("GEOIP", "CN"), ("SRC-GEOIP", "CN"), ("SRC-IP-ASN", "13335"), ("SRC-IP-CIDR", "192.168.1.0/24"), ("SRC-IP-SUFFIX", "192.168.1.1/24"), ("DST-PORT", "443"), ("SRC-PORT", "7777"), ("IN-PORT", "7890"), ("IN-TYPE", "SOCKS/HTTP"), ("IN-USER", "user"), ("IN-NAME", "listener"), ("REMATCH-NAME", "rematch"), ("PROCESS-NAME", "curl"), ("PROCESS-NAME-REGEX", ".*telegram.*"), ("PROCESS-NAME-WILDCARD", "*telegram*"), ("PROCESS-PATH", "/usr/bin/curl"), ("PROCESS-PATH-REGEX", ".*bin/curl"), ("PROCESS-PATH-WILDCARD", "/usr/bin/*"), ("UID", "1001"), ("NETWORK", "udp"), ("DSCP", "4"), ("RULE-SET", "provider"), ("SUB-RULE", "(NETWORK,tcp)"), ("AND", "((DOMAIN,…),(NETWORK,udp))"), ("OR", "((DOMAIN,…),(NETWORK,udp))"), ("NOT", "((DOMAIN,…))"), ("MATCH", "")];
pub const NO_RESOLVE_RULE_TYPES: &[&str] = &["IP-CIDR", "IP-CIDR6", "IP-SUFFIX", "IP-ASN", "GEOIP", "RULE-SET"];
pub const RULE_PAYLOAD_VALUES: &[(&str, &[&str])] = &[("GEOSITE", &["cn", "geolocation-!cn", "geolocation-cn", "private", "category-ads-all", "gfw", "google", "youtube", "github", "microsoft", "apple", "openai", "anthropic", "telegram", "twitter", "facebook", "netflix", "spotify", "steam", "tiktok", "bilibili"]), ("GEOIP", &["CN", "private", "LAN", "telegram", "google", "netflix", "facebook", "twitter", "cloudflare"]), ("SRC-GEOIP", &["CN", "private", "LAN"]), ("NETWORK", &["tcp", "udp"]), ("IN-TYPE", &["HTTP", "HTTPS", "SOCKS4", "SOCKS5", "REDIR", "TPROXY", "TUN", "INNER", "SOCKS", "SHADOWSOCKS", "SNELL", "VMESS", "VLESS", "TROJAN", "TUNNEL", "TUIC", "HYSTERIA2", "ANYTLS", "MIERU", "SUDOKU", "TRUSTTUNNEL", "SHADOWQUIC"])];
pub const RULE_OPTIONS: &[&str] = &["no-resolve", "src"];
}
