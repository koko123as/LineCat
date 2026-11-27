//! HTTP capture and the requests it records, read through `mitm.query`, `mitm.exchange` and
//! `mitm.body`. FlClash: lib/views/connection/http_capture.dart,
//! lib/features/connection/http_requests.dart, lib/features/connection/mitm_exchange_detail.dart.

use crate::requests::{conditions_match, matches_terms, terms, Filters, Record};
use crate::store::MitmSettings;
use serde_json::Value;
use std::collections::BTreeMap;

/// The most `mitm.query` gives in one page.
pub const PAGE_LIMIT: i64 = 64;
/// `_androidMitmCaptureOptions`; past the top one the byte limit drops records first.
pub const CAPTURE_OPTIONS: &[i64] = &[200, 500, 1000, 2000, 5000];
/// `_androidMitmLimits['capture-entries']`.
pub const DEFAULT_CAPTURE_ENTRIES: i64 = 500;

/// `httpCapturing`.
pub fn capturing(m: &MitmSettings) -> bool {
    m.enable && m.capture
}

/// `captureLimit`.
pub fn limit(m: &MitmSettings) -> i64 {
    m.capture_entries.unwrap_or(DEFAULT_CAPTURE_ENTRIES)
}

/// The limit's choices, the current one among them.
pub fn options(m: &MitmSettings) -> Vec<i64> {
    let mut o = CAPTURE_OPTIONS.to_vec();
    let l = limit(m);
    if !o.contains(&l) {
        o.push(l);
        o.sort();
    }
    o
}

/// `httpCaptureHasNoHost` without modules: no listed host, capture records nothing.
pub fn has_no_host(m: &MitmSettings) -> bool {
    m.hostname.iter().all(|line| line.starts_with('-'))
}

/// `MitmExchangeSummary`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub id: i64,
    /// Unix milliseconds.
    pub start: i64,
    pub pending: bool,
    pub duration: i64,
    pub method: String,
    pub url: String,
    pub status: i64,
    pub response_size: i64,
    pub error: String,
    pub host: String,
    pub path: String,
    pub status_class: String,
    pub kind: String,
    pub rewritten: bool,
    pub answer: String,
    pub action_kinds: Vec<String>,
    pub connection: Option<i64>,
    pub process: String,
    pub process_path: String,
}

/// RFC 3339 to Unix milliseconds.
pub fn rfc3339_ms(s: &str) -> i64 {
    let b = s.as_bytes();
    let num = |from: usize, len: usize| -> i64 {
        s.get(from..from + len)
            .and_then(|t| t.parse().ok())
            .unwrap_or(0)
    };
    if b.len() < 19 {
        return 0;
    }
    let (y, mo, d, h, mi, sec) = (
        num(0, 4),
        num(5, 2),
        num(8, 2),
        num(11, 2),
        num(14, 2),
        num(17, 2),
    );
    let mut i = 19;
    let mut frac_ms = 0;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let digits = &s[start..i];
        let padded = format!("{:0<3}", &digits[..digits.len().min(3)]);
        frac_ms = padded.parse().unwrap_or(0);
    }
    let offset = match b.get(i) {
        Some(b'+') | Some(b'-') => {
            let sign = if b[i] == b'-' { -1 } else { 1 };
            sign * (num(i + 1, 2) * 3600 + num(i + 4, 2) * 60)
        }
        _ => 0,
    };
    let (yy, mm) = if mo <= 2 {
        (y - 1, mo + 9)
    } else {
        (y, mo - 3)
    };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * mm + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days * 86_400 + h * 3600 + mi * 60 + sec - offset) * 1000 + frac_ms
}

impl Summary {
    pub fn parse(v: &Value) -> Option<Summary> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let i = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        let app = v.get("app").filter(|a| a.is_object());
        Some(Summary {
            id: v.get("id")?.as_i64()?,
            start: rfc3339_ms(v.get("start").and_then(Value::as_str).unwrap_or("")),
            pending: v.get("pending").and_then(Value::as_bool).unwrap_or(false),
            duration: i("duration"),
            method: s("method"),
            url: s("url"),
            status: i("status"),
            response_size: i("response-size"),
            error: s("error"),
            host: s("host"),
            path: s("path"),
            status_class: s("status-class"),
            kind: s("kind"),
            rewritten: v.get("rewritten").and_then(Value::as_bool).unwrap_or(false),
            answer: s("answer"),
            action_kinds: v
                .get("action-kinds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|k| k.as_str().map(String::from))
                .collect(),
            connection: v.get("connection-id").and_then(Value::as_i64),
            process: app
                .and_then(|a| a.get("process"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            process_path: app
                .and_then(|a| a.get("process-path"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }

    /// Answered 4xx or 5xx, or not answered at all.
    pub fn is_failed(&self) -> bool {
        match self.status_class.as_str() {
            "4xx" | "5xx" | "failed" => true,
            "" => self.status >= 400 || (!self.pending && self.status == 0),
            _ => false,
        }
    }

    pub fn search_fields(&self) -> Vec<String> {
        vec![
            self.method.clone(),
            self.host.clone(),
            self.path.clone(),
            if self.status > 0 {
                self.status.to_string()
            } else {
                String::new()
            },
            self.error.clone(),
            self.process.clone(),
        ]
    }

    /// `HttpRequestStatus`: 0 all, 1 failed, 2 rewritten.
    pub fn status_holds(&self, status: i32) -> bool {
        match status {
            1 => self.is_failed(),
            2 => self.rewritten,
            _ => true,
        }
    }

    /// The row's status text and its tone (0 plain, 1 error, 2 success, 3 warning, 4 muted).
    pub fn status_text(&self) -> (String, i32) {
        if self.status > 0 {
            (self.status.to_string(), status_tone(self.status))
        } else if self.pending {
            ("进行中".into(), 4)
        } else {
            ("失败".into(), 1)
        }
    }

    /// `exchangeMark`: how the request was answered when not by the server, else the
    /// first rule that touched it, and how many more did.
    pub fn mark(&self) -> String {
        let (answer, answer_kind) = match self.answer.as_str() {
            "" => (None, None),
            "reject" => (Some("拒绝".to_string()), Some("url-rewrite")),
            "redirect" => (Some("重定向".to_string()), Some("url-rewrite")),
            "map-local" => (Some("本地映射".to_string()), Some("map-local")),
            "script" => (Some("脚本响应".to_string()), Some("script")),
            other => (Some(other.to_string()), None),
        };
        let Some(label) = answer
            .clone()
            .or_else(|| self.action_kinds.first().map(|k| action_label(k)))
        else {
            return String::new();
        };
        let others = match answer {
            None => self.action_kinds.len().saturating_sub(1),
            Some(_) => self
                .action_kinds
                .iter()
                .filter(|k| Some(k.as_str()) != answer_kind)
                .count(),
        };
        if others > 0 {
            format!("{label} +{others}")
        } else {
            label
        }
    }
}

/// `mitmActionLabel`.
pub fn action_label(kind: &str) -> String {
    match kind {
        "url-rewrite" => "URL 重写",
        "header-rewrite" => "Header 重写",
        "body-rewrite" => "Body 重写",
        "request-body-mock" => "请求体模拟",
        "map-local" => "本地映射",
        "script" => "脚本",
        other => other,
    }
    .to_string()
}

/// `httpStatusColor` as a tone.
pub fn status_tone(status: i64) -> i32 {
    match status {
        400.. => 1,
        200..=299 => 2,
        _ => 0,
    }
}

/// `httpMillisText`.
pub fn millis_text(ms: i64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.1} s", ms as f64 / 1000.0)
    }
}

/// `HttpField`.
pub const FIELDS: &[(&str, &str)] = &[
    ("host", "主机"),
    ("path", "路径"),
    ("method", "方法"),
    ("status", "状态码"),
    ("kind", "类型"),
    ("app", "应用"),
];

impl Record for Summary {
    fn values(&self, field: &str) -> Vec<String> {
        match field {
            "host" => vec![self.host.clone()],
            "path" => vec![self.path.clone()],
            "method" => vec![self.method.clone()],
            "status" => {
                if self.status > 0 {
                    vec![self.status.to_string()]
                } else {
                    Vec::new()
                }
            }
            "kind" => vec![self.kind.clone()],
            "app" => vec![self.process.clone(), self.process_path.clone()],
            _ => Vec::new(),
        }
    }
}

/// `HttpRequestFeed`: the first pass pages through every request, each later one
/// through what changed since the pass before.
#[derive(Default)]
pub struct Feed {
    pub by_id: BTreeMap<i64, Summary>,
    version: Option<i64>,
    /// Grows each time the Core is found restarted, which starts its ids over.
    pub runs: u64,
}

/// One pass's pages, as `mitm.query` gave them.
pub struct Page {
    pub version: i64,
    pub oldest: i64,
    pub next: Option<i64>,
    pub exchanges: Vec<Summary>,
    pub unmatched: Vec<i64>,
}

impl Page {
    pub fn parse(v: &Value) -> Page {
        Page {
            version: v.get("version").and_then(Value::as_i64).unwrap_or(0),
            oldest: v.get("oldest").and_then(Value::as_i64).unwrap_or(0),
            next: v.get("next").and_then(Value::as_i64),
            exchanges: v
                .get("exchanges")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Summary::parse)
                .collect(),
            unmatched: v
                .get("unmatched")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect(),
        }
    }
}

impl Feed {
    /// The `since` of the next pass.
    pub fn since(&self) -> Option<i64> {
        self.version
    }

    /// Takes a pass read with `since()`; false when the Core started over and the pass
    /// must be read again from the start.
    pub fn take(&mut self, pages: Vec<Page>) -> bool {
        let Some(first) = pages.first() else {
            return true;
        };
        if let Some(since) = self.version {
            if first.version < since {
                self.forget();
                return false;
            }
        }
        let version = first.version;
        let mut oldest = 0;
        for page in pages {
            oldest = page.oldest;
            for e in page.exchanges {
                self.by_id.insert(e.id, e);
            }
            for id in page.unmatched {
                self.by_id.remove(&id);
            }
        }
        self.by_id.retain(|id, _| *id >= oldest);
        self.version = Some(version);
        true
    }

    pub fn forget(&mut self) {
        self.by_id.clear();
        self.version = None;
        self.runs += 1;
    }

    pub fn status_counts(&self) -> [usize; 3] {
        let mut n = [0; 3];
        for e in self.by_id.values() {
            for (s, count) in n.iter_mut().enumerate() {
                if e.status_holds(s as i32) {
                    *count += 1;
                }
            }
        }
        n
    }

    /// Oldest first.
    pub fn shown(&self, query: &str, filters: &Filters) -> Vec<&Summary> {
        let terms = terms(query);
        self.by_id
            .values()
            .filter(|e| {
                e.status_holds(filters.status)
                    && conditions_match(&filters.conditions, *e)
                    && matches_terms(&terms, &e.search_fields())
            })
            .collect()
    }
}

/// `exchangePathOf`.
pub fn path_of(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let rest = rest.split('#').next().unwrap_or("");
    match rest.find(['/', '?']) {
        Some(i) if rest[i..].starts_with('?') => format!("/{}", &rest[i..]),
        Some(i) => rest[i..].to_string(),
        None => "/".into(),
    }
}

/// `httpStatusText`.
pub fn status_text(status: i64, reason: &str) -> String {
    if reason.is_empty() {
        status.to_string()
    } else {
        format!("{status} {reason}")
    }
}

pub fn header_of(exchange: &Value, key: &str) -> Vec<(String, Vec<String>)> {
    exchange
        .get(key)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(name, values)| {
            let values = values
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect();
            (name.clone(), values)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn summary(id: i64, status: i64) -> Value {
        json!({"id": id, "start": "2026-11-25T03:04:05.250+08:00", "duration": 1500, "method": "GET",
               "url": "https://a.com/x?y", "status": status, "request-size": 0, "response-size": 10,
               "host": "a.com", "path": "/x", "status-class": "", "kind": "json", "app": {"process": "chrome"}})
    }

    #[test]
    fn capture_settings_read_like_flclash() {
        let mut m = MitmSettings::default();
        assert!(!capturing(&m));
        m.enable = true;
        m.capture = true;
        assert!(capturing(&m));
        assert_eq!(limit(&m), 500);
        m.capture_entries = Some(300);
        assert_eq!(options(&m), vec![200, 300, 500, 1000, 2000, 5000]);
        assert!(has_no_host(&m));
        m.hostname = vec!["-a.com".into(), "b.com".into()];
        assert!(!has_no_host(&m));
    }

    #[test]
    fn summaries_parse_and_classify() {
        let s = Summary::parse(&summary(1, 404)).unwrap();
        assert_eq!(s.start, rfc3339_ms("2026-11-24T19:04:05.250Z"));
        assert_eq!(s.start % 1000, 250);
        assert!(s.is_failed());
        assert_eq!(s.status_text(), ("404".to_string(), 1));
        assert_eq!(s.process, "chrome");
        let ok = Summary::parse(&summary(2, 200)).unwrap();
        assert!(!ok.is_failed());
        assert_eq!(millis_text(1500), "1.5 s");
        assert_eq!(path_of("https://a.com/x?y"), "/x?y");
        assert_eq!(status_text(404, "Not Found"), "404 Not Found");
    }

    #[test]
    fn passes_merge_and_restart() {
        let mut feed = Feed::default();
        let page = |version, ids: &[i64], unmatched: &[i64], oldest| Page {
            version,
            oldest,
            next: None,
            exchanges: ids
                .iter()
                .map(|i| Summary::parse(&summary(*i, 200)).unwrap())
                .collect(),
            unmatched: unmatched.to_vec(),
        };
        assert!(feed.take(vec![page(5, &[1, 2, 3], &[], 1)]));
        assert_eq!(feed.since(), Some(5));
        assert!(feed.take(vec![page(7, &[4], &[2], 3)]));
        assert_eq!(feed.by_id.keys().copied().collect::<Vec<_>>(), vec![3, 4]);
        assert!(!feed.take(vec![page(1, &[1], &[], 1)]));
        assert!(feed.by_id.is_empty() && feed.runs == 1 && feed.since().is_none());
    }

    #[test]
    fn headers_read_as_pairs() {
        let e = json!({"request-header": {"Accept": ["*/*", "text/html"]}});
        assert_eq!(
            header_of(&e, "request-header"),
            vec![(
                "Accept".to_string(),
                vec!["*/*".to_string(), "text/html".to_string()]
            )]
        );
        assert!(header_of(&e, "response-header").is_empty());
    }
}
