//! Subscription download and the `config.apply` arguments, the minimum of what
//! lib/models/profile.dart and lib/common/task.dart (`_makeRealProfileTask`) do on Android.

use serde_json::{json, Map, Value};
use std::io::Read;
use std::time::Duration;

/// `PackageInfoExtension.ua` in lib/common/package.dart: `productName` stays `FlClash`
/// so subscription servers answer as they answer FlClash. The version is pubspec's.
pub const USER_AGENT: &str = "FlClash/v0.9.4 clash-verge Platform/android";
pub const DEFAULT_TEST_URL: &str = "https://www.gstatic.com/generate_204";
const MAX_PROFILE_BYTES: u64 = 32 << 20;

pub struct Download {
    pub bytes: Vec<u8>,
    /// The file name from `content-disposition`, without a yaml extension.
    pub name: Option<String>,
    /// The `subscription-userinfo` header.
    pub userinfo: Option<String>,
}

/// `_ageArmorHeader` in lib/common/file.dart.
pub const AGE_ARMOR_HEADER: &[u8] = b"-----BEGIN AGE ENCRYPTED FILE-----";

/// `shareLinkSchemes` in lib/common/constant.dart.
const SHARE_LINK_SCHEMES: &[&str] = &[
    "hysteria",
    "hysteria2",
    "hy2",
    "hysteria2+realm",
    "hy2+realm",
    "tuic",
    "trojan",
    "vless",
    "vmess",
    "ss",
    "ssr",
    "socks",
    "socks5",
    "socks5h",
    "anytls",
    "mierus",
];

/// `String.isShareLink` in lib/common/string.dart.
pub fn is_share_link(s: &str) -> bool {
    match s.find("://") {
        Some(i) if i > 0 && s.len() > i + 3 && !s.contains('\n') => {
            SHARE_LINK_SCHEMES.contains(&s[..i].to_ascii_lowercase().as_str())
        }
        _ => false,
    }
}

/// `String.isUrl`: http, https or ftp with a host.
pub fn is_url(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("ftp://"))
        && crate::store::host_of(s).is_some()
}

/// `reservedCustomProxyNames` in lib/providers/state/custom.dart.
const RESERVED_NAMES: &[&str] = &[
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "COMPATIBLE",
    "PASS",
    "PASS-RULE",
    "GLOBAL",
];

/// `linkProxyName`: a link's own name, else its server, else its type.
pub(crate) fn link_proxy_name(definition: &Value) -> String {
    let name = definition
        .get("name")
        .map(|v| match v {
            Value::String(s) => s.trim().to_string(),
            other => other.to_string(),
        })
        .unwrap_or_default();
    let unnamed = name.is_empty()
        || (name.starts_with('-')
            && name.len() > 1
            && name[1..].chars().all(|c| c.is_ascii_digit()));
    if !unnamed {
        return name;
    }
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    definition
        .get("server")
        .map(text)
        .or_else(|| definition.get("type").map(text))
        .unwrap_or_default()
}

/// `freeProxyName`: `base`, else `base-01`, `base-02`, ...
pub(crate) fn free_proxy_name(base: &str, taken: &mut std::collections::HashSet<String>) -> String {
    let mut name = base.to_string();
    let mut index = 1;
    while !taken.insert(name.clone()) {
        name = format!("{base}-{index:02}");
        index += 1;
    }
    name
}

/// `_fromShareLinks` in lib/providers/actions/profiles.dart: the decoded proxies, renamed
/// apart, and the profile's label. `decoded` is `convert.decodeLinks`' results, one list per
/// link.
pub fn share_link_proxies(
    links: &[String],
    decoded: &[Value],
) -> Result<(Vec<Value>, String), String> {
    let mut taken: std::collections::HashSet<String> =
        RESERVED_NAMES.iter().map(|s| s.to_string()).collect();
    let mut proxies = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let definitions = decoded
            .get(index)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if definitions.is_empty() {
            return Err(if links.len() == 1 {
                "无法从链接中读取代理".into()
            } else {
                format!("无法从 {link} 中读取代理")
            });
        }
        for mut definition in definitions {
            let name = free_proxy_name(&link_proxy_name(&definition), &mut taken);
            definition["name"] = name.into();
            proxies.push(definition);
        }
    }
    let first = proxies
        .first()
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let label = if proxies.len() == 1 {
        first
    } else {
        format!("{first} 等 {} 个代理", proxies.len())
    };
    Ok((proxies, label))
}

/// `check` off is `checkCertificate` off: the certificate goes unverified (lib/common/http.dart).
pub fn download(url: &str, check: bool) -> Result<Download, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .disable_verification(!check)
                .build(),
        )
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
    let userinfo = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_PROFILE_BYTES)
        .read_to_vec()
        .map_err(|e| format!("下载失败: {e}"))?;
    Ok(Download {
        bytes: decompress_unlabeled(bytes),
        name,
        userinfo,
    })
}

/// `decompressUnlabeled` in lib/common/request.dart: a gzip or zlib body the server did not label.
pub fn decompress_unlabeled(bytes: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    let ok = match bytes.as_slice() {
        [0x1f, 0x8b, ..] => flate2::read::GzDecoder::new(bytes.as_slice())
            .read_to_end(&mut out)
            .is_ok(),
        [0x78, flags, ..] if ((0x78u16 << 8) | *flags as u16).is_multiple_of(31) => {
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

pub(crate) fn percent_decode(s: &str) -> String {
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
pub fn overlay(base: &Value, mode: &str, settings: &crate::store::Settings) -> Value {
    overlay_for(
        base,
        mode,
        settings,
        &crate::custom_profile::Own::default(),
        Vec::new(),
        &crate::mitm_network::Network::default(),
    )
}

/// [overlay] with what the profile brings of its own ([crate::custom_profile::apply_own]) and
/// the added rules' lines.
pub fn overlay_for(
    base: &Value,
    mode: &str,
    settings: &crate::store::Settings,
    own: &crate::custom_profile::Own,
    added: Vec<String>,
    modules: &crate::mitm_network::Network,
) -> Value {
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
    set(
        &mut c,
        "log-level",
        settings.general.log_level.as_str().into(),
    );
    set(&mut c, "keep-alive-interval", 30.into());
    set(&mut c, "keep-alive-idle", 15.into());
    set(&mut c, "disable-keep-alive", false.into());
    set(&mut c, "routing-mark", 0.into());
    let general = &settings.general;
    set(&mut c, "mixed-port", general.mixed_port.into());
    set(&mut c, "port", general.http_port.into());
    set(&mut c, "socks-port", general.socks_port.into());
    set(&mut c, "redir-port", 0.into());
    set(&mut c, "tproxy-port", 0.into());
    set(&mut c, "allow-lan", general.allow_lan.into());
    set(&mut c, "authentication", json!(authentication(general)));
    set(&mut c, "skip-auth-prefixes", json!([]));
    set(
        &mut c,
        "find-process-mode",
        if general.find_process_always {
            "always"
        } else {
            "strict"
        }
        .into(),
    );
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
    let network = &settings.network;
    excluded.extend(
        network
            .route_exclude_address
            .iter()
            .map(|r| Value::from(r.as_str())),
    );
    if network.bypass_private_route {
        excluded.extend(PRIVATE_ROUTES.iter().map(|r| Value::from(*r)));
    }
    tun.insert("route-exclude-address".into(), Value::Array(excluded));
    set(&mut c, "tun", Value::Object(tun));
    set(&mut c, "geo-auto-update", settings.geo_auto_update.into());
    set(
        &mut c,
        "geo-update-interval",
        settings.geo_update_interval.into(),
    );
    let mut profile = c
        .get("profile")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    profile.insert("store-selected".into(), false.into());
    set(&mut c, "profile", Value::Object(profile));
    set(&mut c, "geox-url", json!(settings.geox_url));
    set(
        &mut c,
        "mitm",
        crate::mitm::core_json(
            &settings.mitm,
            &settings.mitm_modules,
            &crate::mitm::language(settings.locale.as_deref()),
        ),
    );
    set(
        &mut c,
        "global-ua",
        general.global_ua.as_deref().unwrap_or(USER_AGENT).into(),
    );
    if !c.get("hosts").is_some_and(Value::is_object) {
        set(&mut c, "hosts", json!({}));
    }
    if let Some(hosts) = c.get_mut("hosts").and_then(Value::as_object_mut) {
        for (host, value) in &general.hosts {
            hosts.insert(host.clone(), host_value(value));
        }
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
    if general.append_system_dns {
        dns.insert("append-system".into(), true.into());
    }
    crate::custom_profile::apply_dns(own, &settings.dns_override, &mut dns);
    set(&mut c, "dns", Value::Object(dns));
    let mut general_patch = Map::new();
    crate::overrides::GENERAL.apply(&settings.general_override, &mut general_patch);
    general_patch.remove("test-url");
    for (k, v) in general_patch {
        set(&mut c, &k, v);
    }
    crate::custom_profile::apply_own(&mut c, own, added);
    if let Some(tun) = c.get_mut("tun").and_then(Value::as_object_mut) {
        modules.apply_to_tun(tun);
    }
    Value::Object(c)
}

/// The test URL the general overrides give, else the default.
pub fn test_url(settings: &crate::store::Settings) -> String {
    crate::overrides::GENERAL
        .picked_value(&settings.general_override, "test-url")
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| DEFAULT_TEST_URL.to_string())
}

pub fn options(settings: &crate::store::Settings, mitm_capture: bool) -> Value {
    json!({
        "testUrl": test_url(settings),
        "skipCertVerify": !settings.general.check_certificate,
        "delayTestTimeout": settings.general.delay_test_timeout,
        "mitmCapture": mitm_capture,
    })
}

pub struct Release {
    pub tag: String,
    pub notes: String,
    pub url: String,
}

/// `request.checkForUpdate` on the stable channel: `releases/latest`, None when there is none.
pub fn latest_release(repository: &str) -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{repository}/releases/latest");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("检查更新失败：{e}"))?;
    match response.status().as_u16() {
        404 => return Ok(None),
        200 => {}
        code => return Err(format!("检查更新失败：HTTP {code}")),
    }
    let body: Value = response
        .body_mut()
        .read_to_string()
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .ok_or_else(|| "检查更新失败".to_string())?;
    let text = |k: &str| {
        body.get(k)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    Ok(Some(Release {
        tag: text("tag_name"),
        notes: text("body"),
        url: text("html_url"),
    }))
}

/// `authentication` as `user:pass` entries, from `NetworkProps.authentication`.
pub fn authentication(general: &crate::store::General) -> Vec<String> {
    let a = &general.authentication;
    if a.enable && !a.username.is_empty() {
        vec![format!("{}:{}", a.username, a.password)]
    } else {
        Vec::new()
    }
}

/// `String.splitByMultipleSeparators`: a value naming several addresses becomes a list.
pub fn host_value(value: &str) -> Value {
    let parts: Vec<&str> = value
        .split([',', ' ', ';'])
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() > 1 {
        json!(parts)
    } else {
        value.into()
    }
}

/// `hostSettingsOf`: the external controller only the host sets, never a profile. None,
/// while it was never turned on, keeps the Core's own.
pub fn host_settings(general: &crate::store::General) -> Option<Value> {
    let open = general.external_controller && !general.secret.is_empty();
    (open || !general.secret.is_empty()).then(|| {
        json!({
            "externalController": if open { "127.0.0.1:9090" } else { "" },
            "secret": general.secret,
            "externalControllerCors": {
                "allowOrigins": if general.external_controller_cors { json!(["*"]) } else { json!([]) },
                "allowPrivateNetwork": general.external_controller_cors,
            },
            "hostCredential": "",
        })
    })
}

/// `vpnOptionsProvider` with FlClash's defaults (VpnProps, NetworkProps, Tun). An empty
/// route list routes everything; the private ranges stay out through the overlay's
/// `route-exclude-address`, which the Core applies to the TUN.
pub fn vpn_options(
    settings: &crate::store::Settings,
    modules: &crate::mitm_network::Network,
) -> Value {
    let n = &settings.network;
    let authenticated = !authentication(&settings.general).is_empty();
    json!({
        "enable": n.vpn,
        "port": settings.general.mixed_port,
        "ipv6": n.ipv6,
        "dnsHijacking": n.dns_hijacking,
        "accessControlProps": settings.access_control.sorted(),
        "allowBypass": n.allow_bypass,
        // VpnService.setHttpProxy cannot carry credentials.
        "systemProxy": n.system_proxy && !authenticated,
        "bypassDomain": n.bypass_domain,
        "routeAddress": modules.vpn_route_address(&n.route_address),
        "mtu": n.mtu,
        "congestionController": n.congestion_controller,
    })
}

/// `defaultBypassDomain` in lib/models/config.dart.
pub const BYPASS_DOMAIN: [&str; 26] = [
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
    fn general_settings_reach_the_overlay() {
        let mut settings = crate::store::Settings::default();
        let g = &mut settings.general;
        g.mixed_port = 7899;
        g.allow_lan = true;
        g.find_process_always = true;
        g.global_ua = Some("sample/1.0".into());
        g.authentication = crate::store::Authentication {
            enable: true,
            username: "user".into(),
            password: "pass".into(),
        };
        g.hosts.insert("a.test".into(), "1.1.1.1, 2.2.2.2".into());
        g.hosts.insert("b.test".into(), "3.3.3.3".into());
        g.append_system_dns = true;
        g.check_certificate = false;
        g.delay_test_timeout = 2000;
        let c = overlay(&json!({"hosts": {"c.test": "4.4.4.4"}}), "rule", &settings);
        assert_eq!(c["mixed-port"], 7899);
        assert_eq!(c["allow-lan"], true);
        assert_eq!(c["find-process-mode"], "always");
        assert_eq!(c["global-ua"], "sample/1.0");
        assert_eq!(c["authentication"], json!(["user:pass"]));
        assert_eq!(c["hosts"]["a.test"], json!(["1.1.1.1", "2.2.2.2"]));
        assert_eq!(c["hosts"]["b.test"], "3.3.3.3");
        assert_eq!(c["hosts"]["c.test"], "4.4.4.4");
        assert_eq!(c["dns"]["append-system"], true);
        let o = options(&settings, false);
        assert_eq!(
            (o["skipCertVerify"].clone(), o["delayTestTimeout"].clone()),
            (json!(true), json!(2000))
        );
        assert_eq!(vpn_options(&settings, &Default::default())["port"], 7899);
    }

    #[test]
    fn the_controller_goes_to_the_host_once_set() {
        let mut g = crate::store::General::default();
        assert!(host_settings(&g).is_none());
        g.external_controller = true;
        g.secret = "s".into();
        g.external_controller_cors = true;
        let h = host_settings(&g).unwrap();
        assert_eq!(h["externalController"], "127.0.0.1:9090");
        assert_eq!(h["externalControllerCors"]["allowOrigins"], json!(["*"]));
        g.external_controller = false;
        assert_eq!(host_settings(&g).unwrap()["externalController"], "");
    }

    #[test]
    fn share_links_are_named_apart() {
        assert!(is_share_link("vmess://abc"));
        assert!(!is_share_link("https://a.com"));
        assert!(is_url("https://a.com/x"));
        let links = vec!["ss://a".to_string(), "ss://b".to_string()];
        let decoded = vec![
            json!([{"name": "", "server": "1.1.1.1", "type": "ss"}]),
            json!([{"name": "1.1.1.1", "type": "ss"}, {"name": "DIRECT", "type": "ss"}]),
        ];
        let (proxies, label) = share_link_proxies(&links, &decoded).unwrap();
        let names: Vec<&str> = proxies
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["1.1.1.1", "1.1.1.1-01", "DIRECT-01"]);
        assert_eq!(label, "1.1.1.1 等 3 个代理");
        assert!(share_link_proxies(&links[..1], &[json!([])]).is_err());
    }

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
        let o = overlay(&base, "global", &crate::store::Settings::default());
        assert_eq!(o["mode"], "global");
        assert_eq!(o["dns"]["nameserver"], json!(["1.1.1.1"]));
        assert_eq!(o["tun"]["stack"], "mixed");
        assert_eq!(o["tun"]["auto-route"], true);
        assert!(o.get("rule").is_none());
        assert_eq!(o["external-controller"], "");
        let o = overlay(&json!({}), "rule", &crate::store::Settings::default());
        assert_eq!(o["dns"]["enhanced-mode"], "fake-ip");
    }

    #[test]
    fn a_custom_profile_overlay_carries_its_groups_rules_and_settings() {
        use crate::custom_profile::{new_profile, own, ProfileRule, ProxyGroupRow};
        let mut state = crate::store::State::default();
        let mut p = new_profile("7".into(), "c", 1);
        p.overrides.sniffer.keys.push("enable".into());
        p.overrides
            .sniffer
            .model
            .insert("enable".into(), json!(true));
        state.profiles.push(p.clone());
        state.proxy_groups.push(ProxyGroupRow {
            id: 1,
            profile_id: "7".into(),
            definition: json!({"name": "G", "type": "select", "proxies": ["DIRECT"]}),
            order: None,
        });
        state.profile_rules.push(ProfileRule {
            id: 2,
            profile_id: "7".into(),
            action: "MATCH".into(),
            target: "G".into(),
            ..Default::default()
        });
        state.settings.dns_override.keys.push("ipv6".into());
        state
            .settings
            .dns_override
            .model
            .insert("ipv6".into(), json!(true));
        let own = own(&state, &p);
        let o = overlay_for(
            &json!({}),
            "rule",
            &state.settings,
            &own,
            vec!["DOMAIN,x,DIRECT".into()],
            &crate::mitm_network::Network::from_core_json(&json!({
                "routeAddress": ["10.0.0.0/8"],
                "routeExcludeAddress": {"after": ["1.1.1.1/32"]},
            })),
        );
        assert!(o["tun"].get("route-address").is_none());
        let excluded = o["tun"]["route-exclude-address"].as_array().unwrap();
        assert_eq!(excluded.last().unwrap(), "1.1.1.1/32");
        assert_eq!(o["proxy-groups"][0]["name"], "G");
        assert_eq!(o["rules"], json!(["MATCH,G"]));
        assert_eq!(o["sniffer"]["enable"], true);
        assert_eq!(o["dns"]["enhanced-mode"], "fake-ip");
        assert!(o["dns"].get("ipv6").is_none());
        let file = overlay(&json!({}), "rule", &state.settings);
        assert_eq!(file["dns"]["ipv6"], true);
        assert_eq!(file["rules"], json!([]));
        assert!(file.get("proxy-groups").is_none());
    }

    #[test]
    fn a_profile_keeps_its_own_sniffer_and_ntp_and_old_app_pages_are_dropped() {
        let base = json!({"sniffer": {"enable": false, "skip-domain": ["a.com"]}});
        let s: crate::store::Settings = serde_json::from_value(json!({
            "sniffer_override": {"keys": ["enable"], "model": {"enable": true}},
            "ntp_override": {"keys": ["enable"]},
            "tun_override": {"keys": ["dns-hijack"], "model": {"dns-hijack": ["any:5353"]}},
            "mode": 1
        }))
        .unwrap();
        assert_eq!(s.mode, 1);
        let o = overlay(&base, "rule", &s);
        assert!(o.get("ntp").is_none());
        assert_eq!(o["sniffer"], base["sniffer"]);
        assert_eq!(o["tun"]["dns-hijack"], json!(["any:53"]));
        assert_eq!(o["tun"]["auto-route"], true);
        let back = serde_json::to_value(&s).unwrap();
        assert!(back.get("sniffer_override").is_none() && back.get("tun_override").is_none());
    }

    #[test]
    fn general_overrides_patch_the_config_and_the_test_url() {
        use crate::overrides::GENERAL;
        let base = json!({"unified-delay": false, "ipv6": true});
        let mut s = crate::store::Settings::default();
        let o = overlay(&base, "rule", &s);
        assert_eq!(
            (o["unified-delay"].clone(), o["ipv6"].clone()),
            (json!(false), json!(true))
        );
        assert!(o.get("tcp-concurrent").is_none());
        assert_eq!(test_url(&s), DEFAULT_TEST_URL);
        for k in ["test-url", "unified-delay", "ipv6"] {
            GENERAL.add(&mut s.general_override, k);
        }
        GENERAL.set(&mut s.general_override, "ipv6", true.into());
        GENERAL.set(
            &mut s.general_override,
            "test-url",
            "https://cp.example/204".into(),
        );
        let o = overlay(&base, "rule", &s);
        assert_eq!(
            (o["unified-delay"].clone(), o["ipv6"].clone()),
            (json!(true), json!(true))
        );
        assert!(o.get("test-url").is_none());
        assert_eq!(options(&s, false)["testUrl"], "https://cp.example/204");
        GENERAL.remove(&mut s.general_override, "test-url");
        assert_eq!(test_url(&s), DEFAULT_TEST_URL);
    }
}
