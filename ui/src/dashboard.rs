//! The dashboard's figures, worked out the way lib/views/dashboard shows them.

use serde_json::Value;
use std::collections::VecDeque;

/// The cards a dashboard can hold, by the names settings keep (`DashboardWidget`).
pub const WIDGETS: &[&str] = &["status", "speed", "mode", "vpn", "profile", "groups", "ip", "memory"];

/// `platformDefaultDashboardWidgets` on Android.
pub fn default_widgets() -> Vec<String> {
    ["status", "speed", "mode", "profile", "groups"].iter().map(|s| s.to_string()).collect()
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

/// What `_fromIpApiCom` reads from ip-api.com: the organization, the type and
/// the quality level (0 unknown, 1 good, 2 normal, 3 risky).
pub fn parse_quality(json: &Value) -> Option<(String, String, i32)> {
    if json.get("status").and_then(Value::as_str) != Some("success") {
        return None;
    }
    let flag = |k: &str| json.get(k).and_then(Value::as_bool).unwrap_or(false);
    let org = json
        .get("org")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .or_else(|| json.get("isp").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    // `_pickType`: hosting before mobile, else a home line.
    let (kind, level) = if flag("hosting") {
        ("机房", 2)
    } else if flag("mobile") {
        ("移动网络", 1)
    } else {
        ("住宅", 1)
    };
    let level = if flag("proxy") && level == 1 { 2 } else { level };
    Some((org, kind.to_string(), level))
}

/// `lookupIpQuality`'s ip-api.com wave, asked directly.
pub fn lookup_quality(ip: &str) -> Option<(String, String, i32)> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(8)))
        .build()
        .into();
    // ip-api.com serves HTTPS only on its paid plan.
    let url = format!("http://ip-api.com/json/{ip}?fields=status,isp,org,as,mobile,proxy,hosting");
    let body = agent.get(&url).call().ok()?.body_mut().read_to_string().ok()?;
    parse_quality(&serde_json::from_str(&body).ok()?)
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
        return ("M0 19 L100 19".into(), "M0 20 L0 19 L100 19 L100 20 Z".into());
    }
    let max = samples.iter().copied().max().unwrap_or(0).max(0) as f64;
    let scale = max.max(MIN_SCALE);
    let step = 100.0 / (HISTORY - 1) as f64;
    let start = (HISTORY - samples.len()) as f64 * step;
    let points: Vec<(f64, f64)> = samples
        .iter()
        .enumerate()
        .map(|(i, v)| (start + i as f64 * step, 19.0 - (*v).max(0) as f64 / scale * 16.0))
        .collect();
    let mut line = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        line.push_str(&format!("{}{x:.1} {y:.1} ", if i == 0 { "M" } else { "L" }));
    }
    let (first, last) = (points[0].0, points[points.len() - 1].0);
    let fill = format!("M{first:.1} 20 L{} L{last:.1} 20 Z", line.trim_start_matches('M').trim_end());
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ip_sources_read() {
        assert_eq!(parse_ip(IP_SOURCES[0], "fl=1\nip=203.0.113.42\nloc=US\n"), Some(("203.0.113.42".into(), "US".into())));
        assert_eq!(parse_ip(IP_SOURCES[2], r#"{"ip":"1.2.3.4","country":"JP"}"#), Some(("1.2.3.4".into(), "JP".into())));
        assert_eq!(parse_ip(IP_SOURCES[6], r#"{"ip":"1.2.3.4","location":{"country_code":"HK"}}"#), Some(("1.2.3.4".into(), "HK".into())));
        assert_eq!(parse_ip(IP_SOURCES[1], "nope"), None);
    }

    #[test]
    fn quality_reads_ip_api() {
        let q = parse_quality(&json!({"status": "success", "org": "Example Broadband", "hosting": false, "mobile": false, "proxy": false}));
        assert_eq!(q, Some(("Example Broadband".into(), "住宅".into(), 1)));
        let q = parse_quality(&json!({"status": "success", "org": "", "isp": "Cloud", "hosting": true}));
        assert_eq!(q, Some(("Cloud".into(), "机房".into(), 2)));
        assert_eq!(parse_quality(&json!({"status": "fail"})), None);
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
}
