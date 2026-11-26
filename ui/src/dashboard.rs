//! The dashboard's figures, worked out the way lib/views/dashboard shows them.

use serde_json::Value;
use std::collections::VecDeque;

/// The cards a dashboard can hold, by the names settings keep (`DashboardWidget`).
pub const WIDGETS: &[&str] = &[
    "status", "speed", "mode", "vpn", "service", "profile", "groups", "stats", "ip", "memory",
];

/// `DashboardWidgetView.label`.
pub fn widget_label(w: &str) -> &'static str {
    match w {
        "status" => "状态",
        "speed" => "网络速度",
        "mode" => "出站模式",
        "vpn" => "VPN",
        "service" => "服务状态",
        "profile" => "配置",
        "groups" => "策略组",
        "stats" => "连接统计",
        "ip" => "内网 IP",
        "memory" => "内存信息",
        _ => "",
    }
}

/// `DashboardWidgetView.description`.
pub fn widget_description(w: &str) -> &'static str {
    match w {
        "status" => "含启动键，不可移除",
        "stats" => "实时连接、最近请求、DNS查询",
        "memory" => "应用与内核占用的内存",
        _ => "",
    }
}

/// `removable`: the status card holds the start button.
pub fn removable(w: &str) -> bool {
    w != "status"
}

/// The saved cards as the dashboard shows them: known ones once each, the status card
/// always there (first when it was missing).
pub fn normalize(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in list {
        if WIDGETS.contains(&w.as_str()) && !out.contains(w) {
            out.push(w.clone());
        }
    }
    if !out.iter().any(|w| w == "status") {
        out.insert(0, "status".into());
    }
    out
}

/// `copyAndReorder`: the card at `from` lands at `to`.
pub fn reorder(list: &mut Vec<String>, from: usize, to: usize) {
    if from >= list.len() {
        return;
    }
    let item = list.remove(from);
    list.insert(to.min(list.len()), item);
}

/// `ServiceTarget`: the Core's checker name and its label, in the default order.
pub const SERVICES: &[(&str, &str)] = &[
    ("google", "Google"),
    ("github", "GitHub"),
    ("youtube", "YouTube"),
    ("chatgpt", "ChatGPT"),
    ("claude", "Claude"),
    ("gemini", "Gemini"),
    ("netflix", "Netflix"),
    ("disney-plus", "Disney+"),
    ("prime-video", "Prime Video"),
    ("spotify", "Spotify"),
    ("tiktok", "TikTok"),
    ("discord", "Discord"),
    ("bilibili", "Bilibili"),
];

/// `orderServiceTargets`: the ids in `order` that name a service, then every other one in
/// the default order.
pub fn ordered_services(order: &[String]) -> Vec<(&'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str)> = Vec::new();
    for id in order {
        if let Some(t) = SERVICES.iter().find(|(s, _)| s == id) {
            if !out.contains(t) {
                out.push(*t);
            }
        }
    }
    for t in SERVICES {
        if !out.contains(t) {
            out.push(*t);
        }
    }
    out
}

/// `enabledServiceTargets`: the ordered services less the disabled ones, all of them when
/// none would be left.
pub fn enabled_services(
    order: &[String],
    disabled: &[String],
) -> Vec<(&'static str, &'static str)> {
    let all = ordered_services(order);
    let enabled: Vec<_> = all
        .iter()
        .filter(|(id, _)| !disabled.iter().any(|d| d == id))
        .copied()
        .collect();
    if enabled.is_empty() {
        all
    } else {
        enabled
    }
}

/// The line under a service in the sheet (`_OutboundIpLine`): the region and the address
/// the service saw, the node it left through when there is no address, else "—".
pub fn service_where(region: &str, ip: &str, node: &str) -> String {
    match (region.is_empty(), ip.is_empty()) {
        (false, false) => format!("{region} · {ip}"),
        (true, false) => ip.to_string(),
        _ if !node.is_empty() => node.to_string(),
        (false, true) => region.to_string(),
        _ => "—".into(),
    }
}

/// `shownCount`: a row of the service card's columns on a phone, two rows on a wide
/// dashboard (at most four, what the Core checks at once).
pub fn service_shown(wide: bool, columns: i32) -> usize {
    (if wide { 2 } else { 1 }) * columns.clamp(1, 2) as usize
}

/// How a service check reads on the card: the text and its tone (0 muted, 1 ok,
/// 2 warning, 3 error). `None` is a check not answered yet.
pub fn service_status(
    status: Option<&str>,
    delay: i64,
    probing: bool,
    failed: bool,
) -> (String, i32) {
    let Some(status) = status else {
        return if probing {
            ("加载中…".into(), 0)
        } else if failed {
            ("检测失败".into(), 3)
        } else {
            ("待检测".into(), 0)
        };
    };
    match status {
        "available" if delay > 0 => (format!("{delay} ms"), 0),
        "available" => ("可用".into(), 1),
        "restricted" => ("访问受限".into(), 2),
        "disallowed-isp" => ("不允许的 ISP".into(), 2),
        "blocked" => ("已被封禁".into(), 2),
        "unsupported-region" => ("地区不支持".into(), 2),
        "originals-only" => ("仅限自制内容".into(), 2),
        "coming-soon" => ("即将上线".into(), 2),
        "unavailable" => ("不可用".into(), 3),
        "timeout" => ("超时".into(), 3),
        _ => ("检测失败".into(), 3),
    }
}

/// The dot beside a service: the status tone, none while unanswered.
pub fn service_dot(status: Option<&str>, failed: bool) -> i32 {
    match status {
        Some("available") => 1,
        Some(
            "restricted" | "disallowed-isp" | "blocked" | "unsupported-region" | "originals-only"
            | "coming-soon",
        ) => 2,
        Some(_) => 3,
        None if failed => 3,
        None => 0,
    }
}

/// The card's verdict chip: "N 项异常", "全部可用" once every service answered, else none.
pub fn service_verdict(checks: &[(Option<String>, bool)]) -> String {
    let problems = checks
        .iter()
        .filter(|(s, failed)| match s {
            Some(s) => s != "available",
            None => *failed,
        })
        .count();
    if problems > 0 {
        format!("{problems} 项异常")
    } else if !checks.is_empty() && checks.iter().all(|(s, _)| s.is_some()) {
        "全部可用".into()
    } else {
        String::new()
    }
}

/// `dashboardCount`: grouped digits under 100000, compact above.
pub fn count(n: usize) -> String {
    if n >= 100_000 {
        let (v, unit) = if n >= 1_000_000 {
            (n as f64 / 1e6, "M")
        } else {
            (n as f64 / 1e3, "K")
        };
        let text = format!("{v:.1}");
        return format!("{}{unit}", text.trim_end_matches(".0"));
    }
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `platformDefaultDashboardWidgets` on Android.
pub fn default_widgets() -> Vec<String> {
    ["status", "speed", "mode", "profile", "groups"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// `ipInfoSources`: the sites `probe.outboundIp` asks, in order.
pub const IP_SOURCES: &[&str] = &[
    "https://www.cloudflare.com/cdn-cgi/trace",
    "https://api.ip.sb/geoip",
    "https://ipinfo.io/json",
    "https://ipwho.is",
    "https://get.geojs.io/v1/ip/geo.json",
    "https://api.country.is",
    "https://api.ipquery.io/?format=json",
    "https://ident.me/json",
];

/// `parseOutboundIp`: the address and its country from whichever site answered.
pub fn parse_ip(url: &str, body: &str) -> Option<(String, String)> {
    if url.contains("cloudflare.com") {
        let mut ip = None;
        let mut cc = None;
        for line in body.lines() {
            match line.split_once('=') {
                Some(("ip", v)) => ip = Some(v.to_string()),
                Some(("loc", v)) => cc = Some(v.to_string()),
                _ => {}
            }
        }
        return Some((ip?, cc?));
    }
    let json: Value = serde_json::from_str(body).ok()?;
    let s = |p: &str| json.pointer(p).and_then(Value::as_str).map(String::from);
    let ip = s("/ip")?;
    let cc = if url.contains("ipinfo.io") || url.contains("country.is") {
        s("/country")
    } else if url.contains("ipquery") {
        s("/location/country_code")
    } else if url.contains("ident.me") {
        s("/cc")
    } else {
        s("/country_code")
    }?;
    Some((ip, cc))
}

/// `IpType`'s label (`ipTypeResidential` ...).
fn type_label(kind: &str) -> &'static str {
    match kind {
        "residential" => "住宅",
        "mobile" => "移动网络",
        "business" => "商业",
        "hosting" => "机房",
        _ => "类型未知",
    }
}

/// `_declaredType`.
fn declared_type(value: Option<&Value>) -> Option<&'static str> {
    match value?.as_str()? {
        "isp" => Some("residential"),
        "business" | "education" | "government" | "banking" => Some("business"),
        "hosting" => Some("hosting"),
        _ => None,
    }
}

/// `_pickType`: hosting first, then mobile, then what the source declared.
fn pick_type(hosting: bool, mobile: bool, declared: Option<&'static str>) -> Option<&'static str> {
    if hosting || declared == Some("hosting") {
        Some("hosting")
    } else if mobile {
        Some("mobile")
    } else {
        declared
    }
}

/// One source's answer: organization, type label, level (1 good, 2 normal, 3 risky),
/// and whether the type was only inferred (silence taken as residential).
#[derive(Debug, Clone, PartialEq)]
pub struct Quality {
    pub org: String,
    pub kind: String,
    pub level: i32,
    pub inferred: bool,
}

/// `IpQualityExt.level`: Tor or an abuser is risky, hosting is normal, the rest good.
fn quality(
    kind: Option<&'static str>,
    inferred: bool,
    org: Option<String>,
    tor: bool,
    abuser: bool,
) -> Option<Quality> {
    let kind = kind?;
    let level = if tor || abuser {
        3
    } else if kind == "hosting" {
        2
    } else {
        1
    };
    Some(Quality {
        org: org.unwrap_or_default(),
        kind: type_label(kind).into(),
        level,
        inferred,
    })
}

fn obj<'a>(json: &'a Value, key: &str) -> &'a Value {
    json.get(key)
        .filter(|v| v.is_object())
        .unwrap_or(&Value::Null)
}
fn flag(json: &Value, key: &str) -> bool {
    json.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn text(json: &Value, key: &str) -> Option<String> {
    json.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// `lookupIpQuality`'s sources, in `_waves` order.
pub const QUALITY_WAVES: &[&[&str]] = &[
    &["identMe", "ipApiCom", "ipQuery"],
    &["ipLocate"],
    &["ipApiIs"],
    &["proxyCheck"],
];

fn quality_url(source: &str, ip: &str) -> String {
    match source {
        "identMe" => "https://ident.me/json".into(),
        // ip-api.com serves HTTPS only on its paid plan.
        "ipApiCom" => {
            format!("http://ip-api.com/json/{ip}?fields=status,isp,org,as,mobile,proxy,hosting")
        }
        "ipQuery" => format!("https://api.ipquery.io/{ip}?format=json"),
        "ipLocate" => format!("https://iplocate.io/api/lookup/{ip}"),
        "proxyCheck" => format!("https://proxycheck.io/v3/{ip}"),
        _ => format!("https://api.ipapi.is/?q={ip}"),
    }
}

/// `_parse`: what each source says about `ip`; None when it states no type or fails.
pub fn parse_quality(source: &str, ip: &str, json: &Value) -> Option<Quality> {
    match source {
        "identMe" => {
            if json.get("ip").and_then(Value::as_str) != Some(ip) {
                return None;
            }
            quality(
                declared_type(json.get("type")),
                false,
                text(json, "aso"),
                false,
                false,
            )
        }
        "ipApiCom" => {
            if json.get("status").and_then(Value::as_str) != Some("success") {
                return None;
            }
            let t = pick_type(flag(json, "hosting"), flag(json, "mobile"), None);
            quality(
                Some(t.unwrap_or("residential")),
                t.is_none(),
                text(json, "org").or_else(|| text(json, "isp")),
                false,
                false,
            )
        }
        "ipQuery" => {
            let (risk, isp) = (obj(json, "risk"), obj(json, "isp"));
            let t = pick_type(flag(risk, "is_datacenter"), flag(risk, "is_mobile"), None);
            quality(
                Some(t.unwrap_or("residential")),
                t.is_none(),
                text(isp, "org").or_else(|| text(isp, "isp")),
                flag(risk, "is_tor"),
                false,
            )
        }
        "ipLocate" => {
            let (privacy, company, asn) =
                (obj(json, "privacy"), obj(json, "company"), obj(json, "asn"));
            let declared =
                declared_type(company.get("type")).or_else(|| declared_type(asn.get("type")));
            quality(
                pick_type(flag(privacy, "is_hosting"), false, declared),
                false,
                text(company, "name").or_else(|| text(asn, "name")),
                flag(privacy, "is_tor"),
                flag(privacy, "is_abuser"),
            )
        }
        "proxyCheck" => {
            let status = json.get("status").and_then(Value::as_str);
            if status != Some("ok") && status != Some("warning") {
                return None;
            }
            let result = obj(json, ip);
            let (network, detections) = (obj(result, "network"), obj(result, "detections"));
            let nt = network.get("type").and_then(Value::as_str);
            let declared = match nt {
                Some("Residential") => Some("residential"),
                Some("Business") => Some("business"),
                _ => None,
            };
            quality(
                pick_type(
                    flag(detections, "hosting") || nt == Some("Hosting"),
                    nt == Some("Wireless"),
                    declared,
                ),
                false,
                text(network, "organisation"),
                flag(detections, "tor"),
                false,
            )
        }
        _ => {
            let (company, asn) = (obj(json, "company"), obj(json, "asn"));
            let declared =
                declared_type(company.get("type")).or_else(|| declared_type(asn.get("type")));
            quality(
                pick_type(
                    flag(json, "is_datacenter"),
                    flag(json, "is_mobile"),
                    declared,
                ),
                false,
                text(company, "name").or_else(|| text(asn, "org")),
                flag(json, "is_tor"),
                flag(json, "is_abuser"),
            )
        }
    }
}

/// `lookupIpQuality`: asks the sources wave by wave (a later wave starts after a
/// one-second hedge or once the earlier ones have all answered). The first stated type
/// wins; an inferred one only stands in when no source states a type.
pub fn lookup_quality(ip: &str) -> Option<Quality> {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .build()
        .into();
    let (tx, rx) = mpsc::channel::<Option<Quality>>();
    let mut fallback: Option<Quality> = None;
    let mut in_flight = 0usize;
    let mut wave = 0usize;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if wave < QUALITY_WAVES.len() {
            for source in QUALITY_WAVES[wave] {
                let (tx, agent, ip) = (tx.clone(), agent.clone(), ip.to_string());
                in_flight += 1;
                std::thread::spawn(move || {
                    let answer = (|| {
                        let mut res = agent.get(&quality_url(source, &ip)).call().ok()?;
                        if res.status() != 200 {
                            return None;
                        }
                        let body = res.body_mut().read_to_string().ok()?;
                        parse_quality(source, &ip, &serde_json::from_str(&body).ok()?)
                    })();
                    let _ = tx.send(answer);
                });
            }
            wave += 1;
        }
        let hedge = Instant::now() + Duration::from_secs(1);
        // Wait for answers until the hedge passes (with waves left) or none are out.
        loop {
            if in_flight == 0 {
                break;
            }
            let until = if wave < QUALITY_WAVES.len() {
                hedge
            } else {
                deadline
            };
            let now = Instant::now();
            if now >= until {
                break;
            }
            match rx.recv_timeout(until - now) {
                Ok(answer) => {
                    in_flight -= 1;
                    match answer {
                        Some(q) if !q.inferred => return Some(q),
                        Some(q) => {
                            fallback.get_or_insert(q);
                        }
                        None => {}
                    }
                }
                Err(_) => break,
            }
        }
        if wave >= QUALITY_WAVES.len() && (in_flight == 0 || Instant::now() >= deadline) {
            return fallback;
        }
    }
}

/// "50.6 KB/s" as its figure and its unit.
pub fn split_unit(text: &str) -> (String, String) {
    match text.split_once(' ') {
        Some((v, u)) => (v.to_string(), u.to_string()),
        None => (text.to_string(), String::new()),
    }
}

/// `runTime` as FlClash writes it: HH:MM:SS.
pub fn run_time(secs: u64) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// The speed history a card's chart draws, newest last.
pub const HISTORY: usize = 30;
const MIN_SCALE: f64 = 8.0 * 1024.0;

/// The chart's line and the area under it, in a 100 by 20 box (`_minSpeedScale` keeps a
/// quiet line low).
pub fn sparkline(samples: &VecDeque<i64>) -> (String, String) {
    if samples.len() < 2 {
        return (
            "M0 19 L100 19".into(),
            "M0 20 L0 19 L100 19 L100 20 Z".into(),
        );
    }
    let max = samples.iter().copied().max().unwrap_or(0).max(0) as f64;
    let scale = max.max(MIN_SCALE);
    let step = 100.0 / (HISTORY - 1) as f64;
    let start = (HISTORY - samples.len()) as f64 * step;
    let points: Vec<(f64, f64)> = samples
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                start + i as f64 * step,
                19.0 - (*v).max(0) as f64 / scale * 16.0,
            )
        })
        .collect();
    let mut line = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        line.push_str(&format!("{}{x:.1} {y:.1} ", if i == 0 { "M" } else { "L" }));
    }
    let (first, last) = (points[0].0, points[points.len() - 1].0);
    let fill = format!(
        "M{first:.1} 20 L{} L{last:.1} 20 Z",
        line.trim_start_matches('M').trim_end()
    );
    (line.trim_end().to_string(), fill)
}

/// The address this device has on its own network (`localIp`): the source a UDP socket
/// would use toward the internet, which sends nothing.
pub fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then(|| ip.to_string())
}

/// This process's resident memory in bytes, from /proc (the Core runs in it too).
pub fn resident_memory() -> Option<i64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: i64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// The profile card's usage line: "42%" and "42.0 GB / 100.0 GB" with the share used.
pub fn usage(used: i64, total: i64) -> Option<(String, String, f32)> {
    if total <= 0 {
        return None;
    }
    let ratio = (used as f64 / total as f64).clamp(0.0, 1.0);
    Some((
        format!("{}%", (ratio * 100.0).round() as i64),
        format!("{} / {}", crate::size(used), crate::size(total)),
        ratio as f32,
    ))
}

/// `ServiceFaultL10n`: "混合端口 127.0.0.1:7890" over the reason, for each fault in an
/// `engine.state` answer.
pub fn service_faults(engine: &serde_json::Value) -> Vec<String> {
    let Some(faults) = engine.get("faults").and_then(|f| f.as_array()) else {
        return Vec::new();
    };
    faults
        .iter()
        .map(|f| {
            let get = |k: &str| f.get(k).and_then(|v| v.as_str()).unwrap_or("");
            let name = match get("service") {
                "mixed" => "混合端口",
                "http" => "HTTP端口",
                "socks" => "Socks端口",
                "redir" => "Redir端口",
                "tproxy" => "Tproxy端口",
                "tun" => "虚拟网卡",
                "dns" => "DNS 服务器",
                _ => "其他",
            };
            let label = match get("address") {
                "" => name.to_string(),
                address => format!("{name} {address}"),
            };
            let reason = match get("reason") {
                "addressInUse" => "地址已被占用。",
                "addressNotAvailable" => "本机没有这个地址。",
                "permissionDenied" => "没有权限。",
                _ => get("message"),
            };
            format!("{label}\n{reason}")
        })
        .collect()
}

/// `profileStatsOf`: groups, proxies (with those the proxy providers bring), rules and
/// providers, from `profile.inspect` and `provider.list`.
pub fn profile_stats(inspect: &serde_json::Value, providers: &[serde_json::Value]) -> [i64; 4] {
    let n = |k: &str| inspect.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    let provided: i64 = providers
        .iter()
        .filter(|p| p.get("type").and_then(|t| t.as_str()) != Some("rule"))
        .map(|p| p.get("count").and_then(|c| c.as_i64()).unwrap_or(0))
        .sum();
    [
        n("proxyGroups"),
        n("proxies") + provided,
        n("rules"),
        providers.len() as i64,
    ]
}

/// A provider's row under 外部资源: its name over "代理 · http · 12" (kind, vehicle, count).
pub fn provider_row(p: &serde_json::Value) -> (String, String) {
    let get = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("");
    let kind = if get("type") == "rule" {
        "规则"
    } else {
        "代理"
    };
    let count = p.get("count").and_then(|c| c.as_i64()).unwrap_or(0);
    let mut parts = vec![kind.to_string()];
    if !get("vehicleType").is_empty() {
        parts.push(get("vehicleType").to_string());
    }
    parts.push(count.to_string());
    (get("name").to_string(), parts.join(" · "))
}

/// `_handleRelease`'s banner: what was freed, else that memory was released.
pub fn released_text(before: i64, after: i64) -> String {
    let freed = (before - after).max(0);
    if freed > 0 {
        format!("已释放 {}", crate::size(freed))
    } else {
        "内存已释放".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ip_sources_read() {
        assert_eq!(
            parse_ip(IP_SOURCES[0], "fl=1\nip=203.0.113.42\nloc=US\n"),
            Some(("203.0.113.42".into(), "US".into()))
        );
        assert_eq!(
            parse_ip(IP_SOURCES[2], r#"{"ip":"1.2.3.4","country":"JP"}"#),
            Some(("1.2.3.4".into(), "JP".into()))
        );
        assert_eq!(
            parse_ip(
                IP_SOURCES[6],
                r#"{"ip":"1.2.3.4","location":{"country_code":"HK"}}"#
            ),
            Some(("1.2.3.4".into(), "HK".into()))
        );
        assert_eq!(parse_ip(IP_SOURCES[1], "nope"), None);
    }

    #[test]
    fn quality_reads_sources() {
        let q = parse_quality("ipApiCom", "1.2.3.4", &json!({"status": "success", "org": "Example Broadband", "hosting": false, "mobile": false, "proxy": true})).unwrap();
        assert_eq!((q.kind.as_str(), q.level, q.inferred), ("住宅", 1, true));
        let q = parse_quality(
            "ipApiCom",
            "1.2.3.4",
            &json!({"status": "success", "org": "", "isp": "Cloud", "hosting": true}),
        )
        .unwrap();
        assert_eq!(
            (q.org.as_str(), q.kind.as_str(), q.level, q.inferred),
            ("Cloud", "机房", 2, false)
        );
        assert_eq!(
            parse_quality("ipApiCom", "1.2.3.4", &json!({"status": "fail"})),
            None
        );
        let q = parse_quality(
            "identMe",
            "1.2.3.4",
            &json!({"ip": "1.2.3.4", "type": "hosting", "aso": "CLOUDFLARENET"}),
        )
        .unwrap();
        assert_eq!((q.kind.as_str(), q.level), ("机房", 2));
        assert_eq!(
            parse_quality(
                "identMe",
                "1.2.3.4",
                &json!({"ip": "9.9.9.9", "type": "hosting"})
            ),
            None
        );
        let q = parse_quality("ipApiIs", "1.2.3.4", &json!({"is_datacenter": false, "is_tor": true, "company": {"type": "isp", "name": "Home"}})).unwrap();
        assert_eq!((q.kind.as_str(), q.level), ("住宅", 3));
        let q = parse_quality(
            "proxyCheck",
            "1.2.3.4",
            &json!({"status": "ok", "1.2.3.4": {"network": {"type": "Wireless"}}}),
        )
        .unwrap();
        assert_eq!(q.kind, "移动网络");
        assert_eq!(
            parse_quality(
                "ipLocate",
                "1.2.3.4",
                &json!({"privacy": {}, "company": {}})
            ),
            None
        );
    }

    #[test]
    fn figures_read_like_flclash() {
        assert_eq!(split_unit("50.6 KB/s"), ("50.6".into(), "KB/s".into()));
        assert_eq!(run_time(7_977), "02:12:57");
        assert_eq!(usage(42, 100).unwrap().0, "42%");
        assert!(usage(1, 0).is_none());
        let mut s = VecDeque::new();
        s.push_back(0);
        s.push_back(16 * 1024);
        let (line, fill) = sparkline(&s);
        assert!(line.starts_with("M96.6 19.0 L100.0 3.0"));
        assert!(fill.ends_with("Z"));
    }

    #[test]
    fn widgets_keep_the_status_card() {
        let l = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            normalize(&l(&["speed", "x", "speed"])),
            l(&["status", "speed"])
        );
        assert_eq!(normalize(&l(&["ip", "status"])), l(&["ip", "status"]));
        assert!(!removable("status") && removable("ip"));
        let mut v = l(&["a", "b", "c", "d"]);
        reorder(&mut v, 0, 2);
        assert_eq!(v, l(&["b", "c", "a", "d"]));
        reorder(&mut v, 3, 0);
        assert_eq!(v, l(&["d", "b", "c", "a"]));
        reorder(&mut v, 9, 0);
        assert_eq!(v.len(), 4);
        assert_eq!(widget_label("stats"), "连接统计");
        assert_eq!(widget_description("status"), "含启动键，不可移除");
        assert_eq!(widget_description("speed"), "");
    }

    #[test]
    fn services_follow_the_saved_order_and_switches() {
        let ids = |v: Vec<(&str, &str)>| v.iter().map(|(i, _)| i.to_string()).collect::<Vec<_>>();
        let order = vec!["netflix".to_string(), "nope".into(), "google".into()];
        let o = ids(ordered_services(&order));
        assert_eq!(o.len(), SERVICES.len());
        assert_eq!(&o[..3], ["netflix", "google", "github"]);
        let disabled = vec!["netflix".to_string(), "github".into()];
        let e = ids(enabled_services(&order, &disabled));
        assert_eq!(&e[..2], ["google", "youtube"]);
        assert_eq!(e.len(), SERVICES.len() - 2);
        let all: Vec<String> = SERVICES.iter().map(|(i, _)| i.to_string()).collect();
        assert_eq!(enabled_services(&[], &all).len(), SERVICES.len());
        assert_eq!(service_where("JP", "1.2.3.4", "n"), "JP · 1.2.3.4");
        assert_eq!(service_where("", "", "日本 03"), "日本 03");
        assert_eq!(service_where("", "", ""), "—");
    }

    #[test]
    fn service_checks_read_as_the_card_shows_them() {
        assert_eq!(
            service_status(Some("available"), 120, false, false),
            ("120 ms".into(), 0)
        );
        assert_eq!(
            service_status(Some("available"), 0, false, false),
            ("可用".into(), 1)
        );
        assert_eq!(service_status(Some("blocked"), 0, false, false).1, 2);
        assert_eq!(
            service_status(Some("timeout"), 0, false, false),
            ("超时".into(), 3)
        );
        assert_eq!(service_status(None, 0, true, false).0, "加载中…");
        assert_eq!(service_status(None, 0, false, true).0, "检测失败");
        assert_eq!(service_status(None, 0, false, false).0, "待检测");
        assert_eq!(service_dot(Some("coming-soon"), false), 2);
        assert_eq!(service_dot(None, false), 0);
        assert_eq!(service_shown(false, 2), 2);
        assert_eq!(service_shown(true, 1), 2);
        assert_eq!(service_shown(true, 2), 4);
        let ok = |s: &str| (Some(s.to_string()), false);
        assert_eq!(
            service_verdict(&[ok("available"), ok("blocked"), (None, true)]),
            "2 项异常"
        );
        assert_eq!(
            service_verdict(&[ok("available"), ok("available")]),
            "全部可用"
        );
        assert_eq!(service_verdict(&[ok("available"), (None, false)]), "");
    }

    #[test]
    fn counts_group_then_compact() {
        assert_eq!(count(0), "0");
        assert_eq!(count(1234), "1,234");
        assert_eq!(count(99_999), "99,999");
        assert_eq!(count(123_400), "123.4K");
        assert_eq!(count(2_000_000), "2M");
    }

    #[test]
    fn faults_name_the_service_and_reason() {
        let v = serde_json::json!({"state": "running", "faults": [
            {"service": "mixed", "address": "127.0.0.1:7890", "reason": "addressInUse", "message": "x"},
            {"service": "tun", "reason": "other", "message": "no fd"},
        ]});
        assert_eq!(
            service_faults(&v),
            vec!["混合端口 127.0.0.1:7890\n地址已被占用。", "虚拟网卡\nno fd"]
        );
        assert!(service_faults(&serde_json::json!({"state": "running"})).is_empty());
    }

    #[test]
    fn profile_stats_count_provided_proxies() {
        let inspect = serde_json::json!({"proxyGroups": 3, "proxies": 10, "rules": 120});
        let providers = vec![
            serde_json::json!({"name": "a", "type": "proxy", "vehicleType": "HTTP", "count": 5}),
            serde_json::json!({"name": "r", "type": "rule", "vehicleType": "File", "count": 900}),
        ];
        assert_eq!(profile_stats(&inspect, &providers), [3, 15, 120, 2]);
        assert_eq!(
            provider_row(&providers[0]),
            ("a".to_string(), "代理 · HTTP · 5".to_string())
        );
    }

    #[test]
    fn release_reports_what_was_freed() {
        assert_eq!(released_text(100, 200), "内存已释放");
        assert!(released_text(3 << 20, 1 << 20).starts_with("已释放 2"));
    }
}
