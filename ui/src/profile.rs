//! Subscription download and the `config.apply` arguments, the minimum of what
//! lib/models/profile.dart and lib/common/task.dart (`_makeRealProfileTask`) do on Android.

use serde_json::{json, Map, Value};
use std::io::Read;
use std::time::Duration;

/// `PackageInfoExtension.ua` in lib/common/package.dart: `productName` stays `FlClash`
/// so subscription servers answer as they answer FlClash. The version is pubspec's.
pub const USER_AGENT: &str = "FlClash/v0.9.4 clash-verge Platform/android";
pub const DEFAULT_TEST_URL: &str = "https://www.gstatic.com/generate_204";
pub const DELAY_TEST_TIMEOUT_MS: u64 = 8000;
const MAX_PROFILE_BYTES: u64 = 32 << 20;

pub struct Download {
    pub bytes: Vec<u8>,
    /// The file name from `content-disposition`, without a yaml extension.
    pub name: Option<String>,
}

pub fn download(url: &str) -> Result<Download, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| format!("下载失败: {e}"))?;
    let name = response
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .and_then(disposition_name);
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_PROFILE_BYTES)
        .read_to_vec()
        .map_err(|e| format!("下载失败: {e}"))?;
    Ok(Download {
        bytes: decompress_unlabeled(bytes),
        name,
    })
}

/// `decompressUnlabeled` in lib/common/request.dart: a gzip or zlib body the server did not label.
pub fn decompress_unlabeled(bytes: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    let ok = match bytes.as_slice() {
        [0x1f, 0x8b, ..] => flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut out)
            .is_ok(),
        [0x78, flags, ..] if ((0x78u16 << 8) | *flags as u16) % 31 == 0 => {
            flate2::read::ZlibDecoder::new(bytes.as_slice())
                .read_to_end(&mut out)
                .is_ok()
        }
        _ => return bytes,
    };
    if ok {
        out
    } else {
        bytes
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            if let Some(Ok(v)) = s.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16)) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn disposition_name(header: &str) -> Option<String> {
    let mut plain = None;
    for part in header.split(';').map(str::trim) {
        if let Some(v) = part.strip_prefix("filename*=") {
            let v = v.trim_matches('"');
            let v = v.splitn(3, '\'').last().unwrap_or(v);
            return clean_name(&percent_decode(v));
        }
        if let Some(v) = part.strip_prefix("filename=") {
            plain = clean_name(&percent_decode(v.trim_matches('"')));
        }
    }
    plain
}

fn clean_name(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let name = if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        &name[..name.rfind('.').unwrap()]
    } else {
        name
    };
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The URL's host, as a label for a profile whose server names none.
pub fn url_label(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.rsplit('@').next().unwrap_or(host);
    if host.is_empty() {
        "配置".into()
    } else {
        host.to_string()
    }
}

const PRIVATE_ROUTES: [&str; 11] = [
    "0.0.0.0/8",
    "10.0.0.0/8",
    "127.0.0.0/8",
    "169.254.0.0/16",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "224.0.0.0/4",
    "255.255.255.255/32",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
];

/// The overlay `config.apply` takes beside the profile: every key but the profile's proxies,
/// groups and rules. [base] is `profile.inspect`'s `config`. Defaults are PatchClashConfig's,
/// with the Android branches of `_makeRealProfileTask`; the log level is `info` so the logs
/// page has something to show (FlClash defaults to `error`).
pub fn overlay(base: &Value, mode: &str) -> Value {
    let mut c: Map<String, Value> = base.as_object().cloned().unwrap_or_default();
    c.remove("rule");
    for key in [
        "external-controller",
        "secret",
        "external-controller-tls",
        "external-controller-unix",
        "external-controller-pipe",
        "external-ui",
        "interface-name",
        "external-ui-url",
    ] {
        c.insert(key.into(), "".into());
    }
    let set = |c: &mut Map<String, Value>, k: &str, v: Value| {
        c.insert(k.into(), v);
    };
    set(&mut c, "log-level", "info".into());
    set(&mut c, "keep-alive-interval", 30.into());
    set(&mut c, "keep-alive-idle", 15.into());
    set(&mut c, "disable-keep-alive", false.into());
    set(&mut c, "routing-mark", 0.into());
    set(&mut c, "mixed-port", 7890.into());
    set(&mut c, "port", 0.into());
    set(&mut c, "socks-port", 0.into());
    set(&mut c, "redir-port", 0.into());
    set(&mut c, "tproxy-port", 0.into());
    set(&mut c, "allow-lan", false.into());
    set(&mut c, "authentication", json!([]));
    set(&mut c, "skip-auth-prefixes", json!([]));
    set(&mut c, "find-process-mode", "strict".into());
    set(&mut c, "mode", mode.into());
    let mut tun = c
        .get("tun")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    tun.insert("dns-hijack".into(), json!(["any:53"]));
    tun.insert("enable".into(), false.into());
    tun.insert("device".into(), "FlClash".into());
    tun.insert("auto-route".into(), true.into());
    tun.insert("auto-detect-interface".into(), true.into());
    let mut excluded: Vec<Value> = tun
        .get("route-exclude-address")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    excluded.extend(PRIVATE_ROUTES.iter().map(|r| Value::from(*r)));
    tun.insert("route-exclude-address".into(), Value::Array(excluded));
    set(&mut c, "tun", Value::Object(tun));
    set(&mut c, "geo-auto-update", true.into());
    set(&mut c, "geo-update-interval", 24.into());
    let mut profile = c
        .get("profile")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    profile.insert("store-selected".into(), false.into());
    set(&mut c, "profile", Value::Object(profile));
    set(
        &mut c,
        "geox-url",
        json!({
            "mmdb": "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geoip.metadb",
            "asn": "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/GeoLite2-ASN.mmdb",
            "geosite": "https://github.com/MetaCubeX/meta-rules-dat/releases/download/latest/geosite.dat",
        }),
    );
    set(&mut c, "global-ua", USER_AGENT.into());
    if !c.get("hosts").is_some_and(Value::is_object) {
        set(&mut c, "hosts", json!({}));
    }
    let mut dns = c
        .get("dns")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if dns.get("enable").and_then(Value::as_bool) != Some(true) {
        // baselineDns: a profile that brings no DNS of its own still resolves.
        dns.insert("enable".into(), true.into());
        dns.insert("enhanced-mode".into(), "fake-ip".into());
        dns.insert(
            "nameserver".into(),
            json!([
                "https://doh.pub/dns-query",
                "https://dns.alidns.com/dns-query"
            ]),
        );
    }
    set(&mut c, "dns", Value::Object(dns));
    set(&mut c, "rules", json!([]));
    Value::Object(c)
}

pub fn options() -> Value {
    json!({
        "testUrl": DEFAULT_TEST_URL,
        "skipCertVerify": false,
        "delayTestTimeout": DELAY_TEST_TIMEOUT_MS,
        "mitmCapture": false,
    })
}

/// `vpnOptionsProvider` with FlClash's defaults (VpnProps, NetworkProps, Tun). An empty
/// route list routes everything; the private ranges stay out through the overlay's
/// `route-exclude-address`, which the Core applies to the TUN.
pub fn vpn_options() -> Value {
    json!({
        "enable": true,
        "port": 7890,
        "ipv6": false,
        "dnsHijacking": false,
        "accessControlProps": {"enable": false, "mode": "rejectSelected", "acceptList": [], "rejectList": []},
        "allowBypass": true,
        "systemProxy": true,
        "bypassDomain": BYPASS_DOMAIN,
        "routeAddress": [],
        "mtu": 9000,
        "congestionController": "cubic",
    })
}

/// `defaultBypassDomain` in lib/models/config.dart.
const BYPASS_DOMAIN: [&str; 26] = [
    "*zhihu.com",
    "*zhimg.com",
    "*jd.com",
    "100ime-iat-api.xfyun.cn",
    "*360buyimg.com",
    "localhost",
    "*.local",
    "127.*",
    "10.*",
    "172.16.*",
    "172.17.*",
    "172.18.*",
    "172.19.*",
    "172.20.*",
    "172.21.*",
    "172.22.*",
    "172.23.*",
    "172.24.*",
    "172.25.*",
    "172.26.*",
    "172.27.*",
    "172.28.*",
    "172.29.*",
    "172.30.*",
    "172.31.*",
    "192.168.*",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_names() {
        assert_eq!(
            disposition_name("attachment; filename=\"My Sub.yaml\"").as_deref(),
            Some("My Sub")
        );
        assert_eq!(
            disposition_name(
                "attachment; filename=x.yaml; filename*=UTF-8''%E6%9C%BA%E5%9C%BA.yml"
            )
            .as_deref(),
            Some("机场")
        );
        assert_eq!(disposition_name("inline"), None);
    }

    #[test]
    fn labels_from_urls() {
        assert_eq!(
            url_label("https://user:pw@sub.example.com/api?token=1"),
            "sub.example.com"
        );
    }

    #[test]
    fn gzip_bodies_are_inflated() {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(b"proxies: []").unwrap();
        assert_eq!(decompress_unlabeled(gz.finish().unwrap()), b"proxies: []");
        assert_eq!(decompress_unlabeled(b"plain".to_vec()), b"plain");
    }

    #[test]
    fn overlay_keeps_profile_dns_and_sets_the_mode() {
        let base = json!({"dns": {"enable": true, "nameserver": ["1.1.1.1"]}, "tun": {"stack": "mixed"}, "rule": 1});
        let o = overlay(&base, "global");
        assert_eq!(o["mode"], "global");
        assert_eq!(o["dns"]["nameserver"], json!(["1.1.1.1"]));
        assert_eq!(o["tun"]["stack"], "mixed");
        assert_eq!(o["tun"]["auto-route"], true);
        assert!(o.get("rule").is_none());
        assert_eq!(o["external-controller"], "");
        let o = overlay(&json!({}), "rule");
        assert_eq!(o["dns"]["enhanced-mode"], "fake-ip");
    }
}
