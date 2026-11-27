//! The logs page's logic: the payload's routing line, search, the level filters
//! (`logFacets`) and the export text. FlClash: lib/views/logs.dart, lib/common/log_payload.dart.

/// In order. FlClash: lib/enum/enum.dart (`LogLevel`).
pub const LEVELS: [&str; 5] = ["debug", "info", "warning", "error", "silent"];

/// `maxLogsLength`.
pub const MAX_LOGS: usize = 5000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub level: String,
    /// `DateTime.showFull`: "2026-11-25 17:30:00".
    pub date_time: String,
    pub payload: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Route {
    pub source: String,
    pub source_detail: String,
    pub destination: String,
    pub rule: String,
    pub proxy: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Payload {
    pub tag: String,
    pub message: String,
    pub route: Option<Route>,
}

impl Payload {
    /// `LogPayload.parse`: "[tag] message", the message read as a routing or dial line.
    pub fn parse(payload: &str) -> Payload {
        let Some((tag, message)) = split_tag(payload) else {
            return Payload {
                message: payload.into(),
                ..Default::default()
            };
        };
        Payload {
            tag: tag.into(),
            message: message.into(),
            route: parse_route(message).or_else(|| parse_dial(message)),
        }
    }

    /// `_LogBody._sourceText`.
    pub fn source_text(&self) -> String {
        let mut parts = vec![self.tag.as_str()];
        if let Some(r) = &self.route {
            parts.push(&r.source);
            parts.push(&r.source_detail);
        }
        parts
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("  ·  ")
    }
}

/// `^\[([^\]]+)\] (.+)$`
fn split_tag(payload: &str) -> Option<(&str, &str)> {
    let rest = payload.strip_prefix('[')?;
    let end = rest.find(']')?;
    let tag = &rest[..end];
    let message = rest[end + 1..].strip_prefix(' ')?;
    (!tag.is_empty() && !message.is_empty()).then_some((tag, message))
}

/// `((?:\[[^\]]+\]|[^\s\[\]()]+):\d+|concord)(?:\((.*)\))?` matching all of `s`.
fn endpoint(s: &str) -> Option<(String, String)> {
    let (address, detail) = match s.find('(') {
        Some(i) => {
            let detail = s[i + 1..].strip_suffix(')')?;
            (&s[..i], detail)
        }
        None => (s, ""),
    };
    let ok = address == "concord"
        || address.rsplit_once(':').is_some_and(|(host, port)| {
            let port_ok = !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit());
            let host_ok = if let Some(inner) = host.strip_prefix('[') {
                inner
                    .strip_suffix(']')
                    .is_some_and(|i| !i.is_empty() && !i.contains(']'))
            } else {
                !host.is_empty()
                    && !host
                        .chars()
                        .any(|c| c.is_whitespace() || "[]()".contains(c))
            };
            port_ok && host_ok
        });
    ok.then(|| (address.to_string(), detail.to_string()))
}

/// `$_endpoint --> (\S+) (?:(?:match (.+?)|(doesn't match any rule)) )?using (.+)$`
fn parse_route(message: &str) -> Option<Route> {
    if message.contains('\n') {
        return None;
    }
    for (i, _) in message
        .match_indices(" --> ")
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let Some((source, source_detail)) = endpoint(&message[..i]) else {
            continue;
        };
        let right = &message[i + 5..];
        let Some((destination, rest)) = right.split_once(' ') else {
            continue;
        };
        if destination.is_empty() {
            continue;
        }
        let (rule, proxy) = if let Some(m) = rest.strip_prefix("match ") {
            match m.find(" using ") {
                Some(j) if j > 0 => (m[..j].to_string(), &m[j + 7..]),
                _ => continue,
            }
        } else if let Some(p) = rest.strip_prefix("doesn't match any rule using ") {
            ("doesn't match any rule".to_string(), p)
        } else if let Some(p) = rest.strip_prefix("using ") {
            (String::new(), p)
        } else {
            continue;
        };
        if proxy.is_empty() {
            continue;
        }
        return Some(Route {
            source,
            source_detail,
            destination: destination.into(),
            rule,
            proxy: proxy.into(),
            error: String::new(),
        });
    }
    None
}

/// `^dial (.+?) (?:\(match (.+?)\) )?$_endpoint --> (\S+) error: (.+)$` (dotAll).
fn parse_dial(message: &str) -> Option<Route> {
    let rest = message.strip_prefix("dial ")?;
    for (i, _) in rest.match_indices(' ') {
        let proxy = &rest[..i];
        if proxy.is_empty() {
            continue;
        }
        let tail = &rest[i + 1..];
        if let Some(m) = tail.strip_prefix("(match ") {
            for (j, _) in m.match_indices(") ") {
                if j == 0 {
                    continue;
                }
                if let Some(r) = dial_tail(&m[j + 2..], proxy, &m[..j]) {
                    return Some(r);
                }
            }
        }
        if let Some(r) = dial_tail(tail, proxy, "") {
            return Some(r);
        }
    }
    None
}

fn dial_tail(s: &str, proxy: &str, rule: &str) -> Option<Route> {
    for (i, _) in s
        .match_indices(" --> ")
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let Some((source, source_detail)) = endpoint(&s[..i]) else {
            continue;
        };
        let right = &s[i + 5..];
        let Some(sp) = right.find(char::is_whitespace) else {
            continue;
        };
        let destination = &right[..sp];
        let Some(error) = right[sp..].strip_prefix(" error: ") else {
            continue;
        };
        if destination.is_empty() || error.is_empty() {
            continue;
        }
        return Some(Route {
            source,
            source_detail,
            destination: destination.into(),
            rule: rule.into(),
            proxy: proxy.into(),
            error: error.into(),
        });
    }
    None
}

/// `LogsStateExt.list` and `RecordFilterController.apply`: the indexes shown, oldest first.
/// The search matches the payload or the level, ignoring case; levels picked as filters
/// keep a record when it has one of them.
pub fn visible(records: &[Record], query: &str, levels: &[String]) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    records
        .iter()
        .enumerate()
        .filter(|(_, r)| levels.is_empty() || levels.iter().any(|l| *l == r.level))
        .filter(|(_, r)| {
            if words.is_empty() {
                return true;
            }
            let payload = r.payload.to_lowercase();
            let level = r.level.to_lowercase();
            words
                .iter()
                .all(|w| payload.contains(w.as_str()) || level.contains(w.as_str()))
        })
        .map(|(i, _)| i)
        .collect()
}

/// `RecordFilterController.offer` for the level facet: the levels the records have, plus
/// the ones picked, in `LogLevel` order.
pub fn offered(records: &[Record], picked: &[String]) -> Vec<&'static str> {
    LEVELS
        .iter()
        .copied()
        .filter(|l| picked.iter().any(|p| p == l) || records.iter().any(|r| r.level == *l))
        .collect()
}

/// `RecordFilterController.toggle`.
pub fn toggle(picked: &mut Vec<String>, level: &str) {
    if let Some(i) = picked.iter().position(|p| p == level) {
        picked.remove(i);
    } else {
        picked.push(level.into());
    }
}

/// A record as FlClash's `Log.toString()` prints it, one record per line (`_encodeLogsTask`).
pub fn line(r: &Record) -> String {
    format!(
        "Log(logLevel: LogLevel.{}, payload: {}, dateTime: {})",
        r.level, r.payload, r.date_time
    )
}

pub fn export_text(records: &[Record]) -> String {
    records.iter().map(line).collect::<Vec<_>>().join("\n")
}

/// `_copyLog`.
pub fn copy_text(r: &Record) -> String {
    format!("{} [{}] {}", r.date_time, r.level, r.payload)
}

/// `logFileName`: "SeasonCat_2026-11-25 17:30:00.log" with `DateTime.show`.
pub fn file_name(app: &str, date_time: &str) -> String {
    format!("{app}_{date_time}.log")
}

/// Keeps the newest `MAX_LOGS`.
pub fn trim(records: &mut Vec<Record>) {
    if records.len() > MAX_LOGS {
        let extra = records.len() - MAX_LOGS;
        records.drain(..extra);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(level: &str, payload: &str) -> Record {
        Record {
            level: level.into(),
            date_time: "2026-11-25 17:30:00".into(),
            payload: payload.into(),
        }
    }

    #[test]
    fn plain_payload_has_no_tag() {
        let p = Payload::parse("Start initial configuration in progress");
        assert_eq!(p.tag, "");
        assert_eq!(p.message, "Start initial configuration in progress");
        assert!(p.route.is_none());
    }

    #[test]
    fn route_line_with_rule() {
        let p = Payload::parse(
            "[TCP] 192.168.1.2:51234(com.android.chrome) --> www.google.com:443 match DomainSuffix(google.com) using Proxy",
        );
        assert_eq!(p.tag, "TCP");
        let r = p.route.unwrap();
        assert_eq!(r.source, "192.168.1.2:51234");
        assert_eq!(r.source_detail, "com.android.chrome");
        assert_eq!(r.destination, "www.google.com:443");
        assert_eq!(r.rule, "DomainSuffix(google.com)");
        assert_eq!(r.proxy, "Proxy");
        assert_eq!(
            Payload::parse("[TCP] 1.1.1.1:1(a) --> b:2 using DIRECT").source_text(),
            "TCP  ·  1.1.1.1:1  ·  a"
        );
    }

    #[test]
    fn route_line_without_rule_and_ipv6() {
        let r =
            Payload::parse("[UDP] [fe80::1]:53 --> 8.8.8.8:53 doesn't match any rule using DIRECT")
                .route
                .unwrap();
        assert_eq!(r.source, "[fe80::1]:53");
        assert_eq!(r.rule, "doesn't match any rule");
        assert_eq!(r.proxy, "DIRECT");
        let r = Payload::parse("[TCP] concord --> a.com:80 using 香港 01")
            .route
            .unwrap();
        assert_eq!(r.source, "concord");
        assert_eq!(r.rule, "");
        assert_eq!(r.proxy, "香港 01");
    }

    #[test]
    fn dial_line_with_error() {
        let r = Payload::parse(
            "[TCP] dial Proxy (match Match()) 10.0.0.2:4000 --> x.com:443 error: connect failed\nmore",
        )
        .route
        .unwrap();
        assert_eq!(r.proxy, "Proxy");
        assert_eq!(r.rule, "Match()");
        assert_eq!(r.source, "10.0.0.2:4000");
        assert_eq!(r.destination, "x.com:443");
        assert_eq!(r.error, "connect failed\nmore");
        let r = Payload::parse("[TCP] dial 日本 A 10.0.0.2:4000 --> x.com:443 error: timeout")
            .route
            .unwrap();
        assert_eq!(r.proxy, "日本 A");
        assert_eq!(r.rule, "");
    }

    #[test]
    fn tagged_text_that_is_not_a_route() {
        let p = Payload::parse("[Sniffer] something else happened");
        assert_eq!(p.tag, "Sniffer");
        assert_eq!(p.message, "something else happened");
        assert!(p.route.is_none());
        assert_eq!(p.source_text(), "Sniffer");
        assert!(Payload::parse("[] x").route.is_none());
        assert!(Payload::parse("[TCP] bad host:port --> a:1 using X")
            .route
            .is_none());
    }

    #[test]
    fn search_and_level_filters() {
        let rs = vec![
            rec("info", "hello World"),
            rec("warning", "disk"),
            rec("error", "world ends"),
        ];
        assert_eq!(visible(&rs, "", &[]), vec![0, 1, 2]);
        assert_eq!(visible(&rs, "WORLD", &[]), vec![0, 2]);
        assert_eq!(visible(&rs, "warn", &[]), vec![1]);
        assert_eq!(visible(&rs, "world", &["error".into()]), vec![2]);
        assert_eq!(
            visible(&rs, "", &["info".into(), "warning".into()]),
            vec![0, 1]
        );
    }

    #[test]
    fn offered_levels_follow_the_enum_order() {
        let rs = vec![rec("error", "a"), rec("info", "b")];
        assert_eq!(offered(&rs, &[]), vec!["info", "error"]);
        assert_eq!(
            offered(&rs, &["debug".into()]),
            vec!["debug", "info", "error"]
        );
        assert!(offered(&[], &[]).is_empty());
        let mut picked = vec![];
        toggle(&mut picked, "info");
        toggle(&mut picked, "error");
        toggle(&mut picked, "info");
        assert_eq!(picked, vec!["error".to_string()]);
    }

    #[test]
    fn export_copy_and_trim() {
        let r = rec("info", "x");
        assert_eq!(
            export_text(&[r.clone(), rec("error", "y")]),
            "Log(logLevel: LogLevel.info, payload: x, dateTime: 2026-11-25 17:30:00)\nLog(logLevel: LogLevel.error, payload: y, dateTime: 2026-11-25 17:30:00)"
        );
        assert_eq!(copy_text(&r), "2026-11-25 17:30:00 [info] x");
        assert_eq!(
            file_name("SeasonCat", "2026-11-25 17:30:00"),
            "SeasonCat_2026-11-25 17:30:00.log"
        );
        let mut many = vec![r; MAX_LOGS + 3];
        many[3].payload = "keep".into();
        trim(&mut many);
        assert_eq!(many.len(), MAX_LOGS);
        assert_eq!(many[0].payload, "keep");
    }
}
