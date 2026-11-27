//! A captured request copied as cURL or HTTP text, exported as HAR, or written into a module as
//! a reject or map-local rule, and the menu that offers them. FlClash: lib/common/mitm_export.dart,
//! lib/common/mitm_module_file.dart, lib/features/connection/mitm_exchange_menu.dart.

use crate::http_body::{self, header_value, Raw};
use crate::{http_capture, requests, ActionItem};
use serde_json::{json, Map, Value};

pub const URL_REWRITE: &str = "URL Rewrite";
pub const MAP_LOCAL: &str = "Map Local";
pub const CLEARED: &str = "这条请求的记录已清除";
pub const PRIVATE_TITLE: &str = "包含隐私信息";
pub const PRIVATE: &str = "请求里可能有 Cookie、令牌、密码和表单内容，分享前请确认内容和对象。";
pub const STASH_COPIED: &str = "这个模块是 Stash 覆写，规则已复制，请手动添加。";

type Header = Vec<(String, Vec<String>)>;

/// `MitmExchange`: what the menu's actions read of `mitm.exchange`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exchange {
    pub id: i64,
    /// Unix milliseconds.
    pub start: i64,
    pub pending: bool,
    pub duration: i64,
    pub ttfb: i64,
    pub method: String,
    pub url: String,
    pub original_url: String,
    pub proto: String,
    pub status: i64,
    pub request_size: i64,
    pub response_size: i64,
    pub connection: String,
    pub request_header: Header,
    pub response_header: Header,
}

impl Exchange {
    pub fn parse(v: &Value) -> Option<Exchange> {
        let id = v.get("id").and_then(Value::as_i64)?;
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let int = |k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
        Some(Exchange {
            id,
            start: http_capture::rfc3339_ms(&text("start")),
            pending: v.get("pending").and_then(Value::as_bool).unwrap_or(false),
            duration: int("duration"),
            ttfb: int("ttfb"),
            method: text("method"),
            url: text("url"),
            original_url: text("original-url"),
            proto: text("proto"),
            status: int("status"),
            request_size: int("request-size"),
            response_size: int("response-size"),
            connection: text("connection"),
            request_header: http_capture::header_of(v, "request-header"),
            response_header: http_capture::header_of(v, "response-header"),
        })
    }

    /// The url a reject rule matches: the one the app asked for, before any rewrite.
    pub fn asked_url(&self) -> &str {
        if self.original_url.is_empty() {
            &self.url
        } else {
            &self.original_url
        }
    }
}

/// A url's parts as `Uri` reads them: authority, path, and the query when it has one.
fn url_parts(url: &str) -> Option<(&str, &str, Option<&str>)> {
    let (_, rest) = url.split_once("://")?;
    let rest = rest.split('#').next().unwrap_or("");
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    let (path, query) = match tail.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (tail, None),
    };
    Some((authority, path, query))
}

fn shell_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `mitmCurl`: a POSIX shell command that sends the request again; a body that is not text is
/// piped in from Base64.
pub fn curl(e: &Exchange, body: Option<&Raw>) -> String {
    let bytes = body.map(Raw::bytes).unwrap_or_default();
    let authority = url_parts(&e.url).map(|(a, _, _)| a);
    let has_body = !bytes.is_empty();
    let undecoded = body.is_some_and(|b| !b.encoding.is_empty());
    let mut arguments = vec![shell_quoted(&e.url)];
    if e.method == "HEAD" {
        arguments.push("--head".into());
    } else if e.method != (if has_body { "POST" } else { "GET" }) {
        arguments.push(format!("-X {}", shell_quoted(&e.method)));
    }
    let mut compressed = false;
    for (name, values) in &e.request_header {
        let lower = name.to_lowercase();
        for value in values {
            if lower == "content-length"
                || (lower == "host" && Some(value.as_str()) == authority)
                || (lower == "content-encoding" && has_body && !undecoded)
            {
                continue;
            }
            compressed |= lower == "accept-encoding";
            arguments.push(format!("-H {}", shell_quoted(&format!("{name}: {value}"))));
        }
    }
    if compressed {
        arguments.push("--compressed".into());
    }
    let mut input = String::new();
    if has_body {
        match http_body::body_text(&bytes, false) {
            Some(text) => arguments.push(format!("--data-raw {}", shell_quoted(&text))),
            None => {
                input = format!(
                    "printf '%s' '{}' | base64 --decode | ",
                    http_body::base64_encode(&bytes)
                );
                arguments.push("--data-binary @-".into());
            }
        }
    }
    format!("{input}curl {}", arguments.join(" \\\n  "))
}

fn raw_message(start_line: String, header: &Header, body: Option<&Raw>) -> String {
    let mut out = start_line;
    for (name, values) in header {
        for value in values {
            out.push_str(&format!("\n{name}: {value}"));
        }
    }
    if let Some(body) = body.filter(|b| !b.data.is_empty()) {
        out.push_str("\n\n");
        out.push_str(&http_body::copy_text(body));
    }
    out
}

/// `mitmRawRequest`: the request as HTTP/1 text; a body that is not text is Base64.
pub fn raw_request(e: &Exchange, body: Option<&Raw>) -> String {
    let target = match url_parts(&e.url) {
        Some((_, path, query)) => {
            let path = if path.is_empty() { "/" } else { path };
            match query {
                Some(q) => format!("{path}?{q}"),
                None => path.to_string(),
            }
        }
        None => e.url.clone(),
    };
    raw_message(
        format!("{} {target} {}", e.method, e.proto),
        &e.request_header,
        body,
    )
}

/// `mitmRawResponse`.
pub fn raw_response(e: &Exchange, body: Option<&Raw>) -> String {
    raw_message(
        format!("{} {}", e.proto, e.status),
        &e.response_header,
        body,
    )
}

fn percent_decoded(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(h), Some(l)) => {
                        out.push((h * 16 + l) as u8);
                        i += 3;
                        continue;
                    }
                    _ => out.push(b'%'),
                }
            }
            b'+' if plus_is_space => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `Uri.queryParametersAll`: each name once, in the order it first came, with all its values.
fn query_parameters(url: &str) -> Header {
    let mut out: Header = Vec::new();
    let Some((_, _, Some(query))) = url_parts(url) else {
        return out;
    };
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let (name, value) = (percent_decoded(name, true), percent_decoded(value, true));
        match out.iter_mut().find(|(n, _)| *n == name) {
            Some((_, values)) => values.push(value),
            None => out.push((name, vec![value])),
        }
    }
    out
}

fn har_pairs(pairs: &Header) -> Value {
    Value::Array(
        pairs
            .iter()
            .flat_map(|(name, values)| {
                values
                    .iter()
                    .map(move |value| json!({"name": name, "value": value}))
            })
            .collect(),
    )
}

/// `_harText`: (text, whether it is Base64, whether it was cut).
fn har_text(body: &Raw) -> (String, bool, bool) {
    let text = if body.encoding.is_empty() {
        http_body::body_text(&body.bytes(), false)
    } else {
        None
    };
    match text {
        Some(text) => (text, false, body.truncated),
        None => (body.data.clone(), true, body.truncated),
    }
}

/// `toUtc().toIso8601String()` of a Unix millisecond.
pub fn iso8601(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = requests::civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        s / 3600,
        s / 60 % 60,
        s % 60,
        ms.rem_euclid(1000)
    )
}

/// `mitmHar`: the request with its bodies as an HTTP Archive 1.2 log, which browser developer
/// tools import.
pub fn har(
    e: &Exchange,
    request: Option<&Raw>,
    response: Option<&Raw>,
    app: &str,
    version: &str,
) -> String {
    let request_type = header_value(&e.request_header, "Content-Type");
    let response_type = header_value(&e.response_header, "Content-Type");
    let ttfb = if e.ttfb > 0 { e.ttfb } else { e.duration };
    let receive = (e.duration - ttfb).max(0);
    let mut req = json!({
        "method": e.method,
        "url": e.url,
        "httpVersion": e.proto,
        "cookies": [],
        "headers": har_pairs(&e.request_header),
        "queryString": har_pairs(&query_parameters(&e.url)),
        "headersSize": -1,
        "bodySize": e.request_size,
    });
    if let Some(body) = request.filter(|b| !b.data.is_empty()) {
        let (text, base64, _) = har_text(body);
        let mut post = json!({"mimeType": request_type, "text": text});
        if base64 {
            post["_encoding"] = json!("base64");
        }
        req["postData"] = post;
    }
    let mut content = Map::new();
    content.insert(
        "size".into(),
        json!(response.map_or(e.response_size, |b| b.bytes().len() as i64)),
    );
    let mime = if response_type.is_empty() {
        "x-unknown".to_string()
    } else {
        response_type
    };
    content.insert("mimeType".into(), json!(mime));
    if let Some(body) = response {
        let (text, base64, truncated) = har_text(body);
        content.insert("text".into(), json!(text));
        if base64 {
            content.insert("encoding".into(), json!("base64"));
        }
        if truncated {
            content.insert("comment".into(), json!("truncated"));
        }
    }
    let mut entry = json!({
        "startedDateTime": iso8601(e.start),
        "time": e.duration,
        "request": req,
        "response": {
            "status": e.status,
            "statusText": "",
            "httpVersion": e.proto,
            "cookies": [],
            "headers": har_pairs(&e.response_header),
            "content": Value::Object(content),
            "redirectURL": header_value(&e.response_header, "Location"),
            "headersSize": -1,
            "bodySize": e.response_size,
        },
        "cache": {},
        "timings": {
            "send": 0,
            "wait": ttfb,
            "receive": receive,
        },
    });
    if !e.connection.is_empty() {
        entry["connection"] = json!(e.connection);
    }
    let log = json!({"log": {
        "version": "1.2",
        "creator": {"name": app, "version": version},
        "entries": [entry],
    }});
    serde_json::to_string_pretty(&log).unwrap_or_default()
}

/// The file name a HAR export offers: the host, else the app, then the request's id.
pub fn har_file_name(e: &Exchange, app: &str) -> String {
    let host = url_parts(&e.url)
        .map(|(a, _, _)| a.rsplit('@').next().unwrap_or(a))
        .map(|a| match a.strip_prefix('[') {
            Some(v6) => v6.split(']').next().unwrap_or(v6),
            None => a.split(':').next().unwrap_or(a),
        })
        .unwrap_or("");
    format!("{}-{}.har", if host.is_empty() { app } else { host }, e.id)
}

fn field(text: &str) -> String {
    if !text.is_empty() && !text.chars().any(|c| c.is_whitespace() || c == '"') {
        return text.to_string();
    }
    format!("\"{}\"", text.replace('"', "\\\""))
}

/// `mitmUrlPattern`: the url up to its query, as a regex that also takes it with one.
pub fn url_pattern(url: &str) -> String {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    let mut out = String::from("^");
    for c in url[..end].chars() {
        if ".*+?^${}()|[]\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push_str("(\\?|$)");
    out
}

pub fn reject_line(url: &str) -> String {
    format!("{} reject", field(&url_pattern(url)))
}

/// `mitmAttachmentName`: the url's last path segment made a file name, numbered past `taken`.
pub fn attachment_name<'a>(url: &str, taken: impl IntoIterator<Item = &'a String>) -> String {
    let path = url_parts(url).map(|(_, p, _)| p).unwrap_or("");
    let segment = path
        .split('/')
        .filter(|s| !s.is_empty())
        .next_back()
        .map(|s| percent_decoded(s, false))
        .unwrap_or_default();
    let base: String = segment
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = if base.is_empty() || base.starts_with('.') {
        format!("response{base}")
    } else {
        base
    };
    let used: Vec<&String> = taken.into_iter().collect();
    let (stem, extension) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    let mut candidate = name.clone();
    let mut i = 2;
    while used.iter().any(|u| **u == candidate) {
        candidate = format!("{stem}-{i}{extension}");
        i += 1;
    }
    candidate
}

/// `mitmMapLocalLine`: the response kept as `file`, else an empty text, with its status and
/// type.
pub fn map_local_line(url: &str, file: Option<&str>, status: i64, content_type: &str) -> String {
    let mut params = vec![
        ("data-type", (if file.is_some() { "file" } else { "text" }).to_string()),
        ("data", file.unwrap_or("").to_string()),
    ];
    if status > 200 && status <= 999 {
        params.push(("status-code", status.to_string()));
    }
    if !content_type.is_empty() {
        params.push(("header", format!("Content-Type:{content_type}")));
    }
    let mut out = field(&url_pattern(url));
    for (key, value) in params {
        out.push_str(&format!(" {key}={}", field(&value)));
    }
    out
}

fn section_title(line: &str) -> Option<&str> {
    let line = line.trim();
    let title = line.strip_prefix('[')?.strip_suffix(']')?;
    (!title.is_empty() && !title.contains(']')).then_some(title)
}

const STASH_KEYS: &[&str] = &[
    "http",
    "script-providers",
    "cron",
    "rules",
    "tiles",
    "dns",
    "hosts",
];

/// A Stash override is YAML with one of its own top-level keys and no `[Section]` line.
fn is_stash_override(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines.iter().any(|l| section_title(l).is_some()) {
        return false;
    }
    lines.iter().any(|l| {
        !l.starts_with([' ', '\t', '#', '-'])
            && l.split_once(':')
                .is_some_and(|(k, _)| STASH_KEYS.contains(&k.trim().trim_matches(['"', '\''])))
    })
}

/// `mitmAddLine`: `line` at the end of the module's `section`, which is added after the others
/// when missing; None for a Stash override, which has no sections.
pub fn add_line(text: &str, section: &str, line: &str) -> Option<String> {
    if is_stash_override(text) {
        return None;
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    let is_header = |l: &str, name: Option<&str>| {
        section_title(l)
            .is_some_and(|t| name.is_none_or(|n| t.trim().to_lowercase() == n.to_lowercase()))
    };
    let Some(header) = lines.iter().position(|l| is_header(l, Some(section))) else {
        let head = text.trim_end();
        let gap = if head.is_empty() { "" } else { "\n\n" };
        return Some(format!("{head}{gap}[{section}]\n{line}\n"));
    };
    let mut end = header + 1;
    for (i, l) in lines.iter().enumerate().skip(header + 1) {
        if is_header(l, None) {
            break;
        }
        if !l.trim().is_empty() {
            end = i + 1;
        }
    }
    lines.insert(end, line);
    Some(lines.join("\n"))
}

/// What the menu needs of a request: a list row knows only its summary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuTarget {
    pub id: i64,
    pub url: String,
    pub pending: bool,
    pub status: i64,
    pub response_size: i64,
}

impl MenuTarget {
    pub fn of_exchange(v: &Value) -> Option<MenuTarget> {
        let e = Exchange::parse(v)?;
        Some(MenuTarget {
            id: e.id,
            url: e.url,
            pending: e.pending,
            status: e.status,
            response_size: e.response_size,
        })
    }

    pub fn of_summary(s: &http_capture::Summary) -> MenuTarget {
        MenuTarget {
            id: s.id,
            url: s.url.clone(),
            pending: s.pending,
            status: s.status,
            response_size: s.response_size,
        }
    }
}

/// The two items whose modules unfold under them rather than closing the menu.
pub const REJECT: &str = "http-reject";
pub const MAP_LOCAL_ITEM: &str = "http-map-local";

/// `mitmExchangeMenuItems`; `modules` are the local ones a rule can be written into, by id and
/// name, and `open` the item whose modules show.
pub fn menu_items(t: &MenuTarget, modules: &[(String, String)], open: Option<&str>) -> Vec<ActionItem> {
    let item = |id: String, glyph: &str, label: &str| ActionItem {
        id: id.into(),
        glyph: glyph.into(),
        label: label.into(),
        enabled: true,
        ..Default::default()
    };
    let id = t.id;
    let mut items = vec![
        item(format!("http-copy-url:{id}"), "link", "复制链接"),
        item(format!("http-curl:{id}"), "code", "复制为 cURL"),
        item(format!("http-copy-request:{id}"), "copy", "复制请求"),
    ];
    if t.status != 0 {
        items.push(item(format!("http-copy-response:{id}"), "copy", "复制响应"));
    }
    if !t.pending && t.response_size > 0 {
        items.push(item(format!("http-copy-body:{id}"), "copy", "复制响应体"));
    }
    items.push(item(format!("http-har:{id}"), "export", "导出 HAR"));
    let mut parents = vec![(REJECT, "block", "添加拒绝规则")];
    if !t.pending && t.status != 0 {
        parents.push((MAP_LOCAL_ITEM, "swap", "添加本地映射规则"));
    }
    for (i, (key, glyph, label)) in parents.into_iter().enumerate() {
        let mut parent = item(key.to_string(), glyph, label);
        parent.heading = "more".into();
        parent.section = i == 0;
        items.push(parent);
        if open == Some(key) {
            let mut new = item(format!("{key}-to:{id}:"), "plus", "新建模块");
            new.sub = true;
            items.push(new);
            for (mid, name) in modules {
                let mut m = item(format!("{key}-to:{id}:{mid}"), "", name);
                m.sub = true;
                items.push(m);
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exchange() -> Exchange {
        Exchange::parse(&json!({
            "id": 7, "start": "2026-11-25T03:04:05.250Z", "duration": 300, "ttfb": 120,
            "method": "POST", "url": "https://a.test:8443/api/v1?q=a+b&q=c&x=%E4%B8%AD#frag",
            "proto": "HTTP/1.1", "status": 201, "request-size": 7, "response-size": 2,
            "connection": "c1",
            "request-header": {"Host": ["a.test:8443"], "Content-Length": ["7"],
                               "Accept-Encoding": ["gzip"], "X-Say": ["it's"]},
            "response-header": {"Content-Type": ["application/json"], "Location": ["/next"]}
        }))
        .unwrap()
    }

    fn body(bytes: &[u8]) -> Raw {
        Raw {
            data: http_body::base64_encode(bytes),
            ..Default::default()
        }
    }

    #[test]
    fn curl_sends_the_request_again() {
        let e = exchange();
        assert_eq!(
            curl(&e, Some(&body(b"{\"a\":1}"))),
            "curl 'https://a.test:8443/api/v1?q=a+b&q=c&x=%E4%B8%AD#frag' \\\n  \
             -H 'Accept-Encoding: gzip' \\\n  -H 'X-Say: it'\\''s' \\\n  --compressed \\\n  \
             --data-raw '{\"a\":1}'"
        );
        let binary = curl(&e, Some(&body(&[0, 1])));
        assert!(binary.starts_with("printf '%s' 'AAE=' | base64 --decode | curl "));
        assert!(binary.ends_with("--data-binary @-"));
        let mut get = e.clone();
        get.method = "PUT".into();
        assert!(curl(&get, None).contains("\\\n  -X 'PUT'"));
        get.method = "HEAD".into();
        assert!(curl(&get, None).contains("--head"));
    }

    #[test]
    fn raw_messages_read_as_http1() {
        let e = exchange();
        let headers: Vec<String> = e
            .request_header
            .iter()
            .map(|(n, v)| format!("{n}: {}", v[0]))
            .collect();
        assert_eq!(
            raw_request(&e, Some(&body(b"payload"))),
            format!(
                "POST /api/v1?q=a+b&q=c&x=%E4%B8%AD HTTP/1.1\n{}\n\npayload",
                headers.join("\n")
            )
        );
        assert!(raw_response(&e, None).starts_with("HTTP/1.1 201\n"));
        assert_eq!(
            raw_response(&e, Some(&body(&[0xff]))).rsplit("\n\n").next(),
            Some("/w==")
        );
        let mut bare = e.clone();
        bare.url = "https://a.test".into();
        assert!(raw_request(&bare, None).starts_with("POST / HTTP/1.1"));
    }

    #[test]
    fn har_holds_one_entry() {
        let e = exchange();
        let text = har(&e, Some(&body(b"payload")), Some(&body(b"{}")), "SeasonCat", "1.0");
        let v: Value = serde_json::from_str(&text).unwrap();
        let entry = &v["log"]["entries"][0];
        assert_eq!(v["log"]["version"], "1.2");
        assert_eq!(v["log"]["creator"]["name"], "SeasonCat");
        assert_eq!(entry["startedDateTime"], "2026-11-25T03:04:05.250Z");
        assert_eq!(entry["request"]["postData"]["text"], "payload");
        assert_eq!(
            entry["request"]["queryString"],
            json!([{"name": "q", "value": "a b"}, {"name": "q", "value": "c"},
                   {"name": "x", "value": "中"}])
        );
        assert_eq!(entry["response"]["content"]["mimeType"], "application/json");
        assert_eq!(entry["response"]["content"]["text"], "{}");
        assert_eq!(entry["response"]["content"]["size"], 2);
        assert_eq!(entry["response"]["redirectURL"], "/next");
        assert_eq!(entry["timings"], json!({"send": 0, "wait": 120, "receive": 180}));
        assert_eq!(entry["connection"], "c1");
        let bin = har(&e, None, Some(&body(&[0, 1])), "SeasonCat", "1.0");
        let v: Value = serde_json::from_str(&bin).unwrap();
        let content = &v["log"]["entries"][0]["response"]["content"];
        assert_eq!((content["text"].as_str(), content["encoding"].as_str()), (Some("AAE="), Some("base64")));
        assert!(v["log"]["entries"][0]["request"].get("postData").is_none());
        assert_eq!(har_file_name(&e, "SeasonCat"), "a.test-7.har");
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn rule_lines_match_the_url() {
        assert_eq!(url_pattern("https://a.test/x.js?v=1"), "^https://a\\.test/x\\.js(\\?|$)");
        assert_eq!(reject_line("https://a.test/a b"), "\"^https://a\\.test/a b(\\?|$)\" reject");
        assert_eq!(
            map_local_line("https://a.test/x", Some("x.json"), 404, "application/json"),
            "^https://a\\.test/x(\\?|$) data-type=file data=x.json status-code=404 \
             header=Content-Type:application/json"
        );
        assert_eq!(
            map_local_line("https://a.test/x", None, 200, ""),
            "^https://a\\.test/x(\\?|$) data-type=text data=\"\""
        );
    }

    #[test]
    fn attachments_take_free_names() {
        let taken = ["a.json".to_string(), "a-2.json".to_string()];
        assert_eq!(attachment_name("https://a.test/v/a.json?x", &taken), "a-3.json");
        assert_eq!(attachment_name("https://a.test/", &[]), "response");
        assert_eq!(attachment_name("https://a.test/.env", &[]), "response.env");
        assert_eq!(attachment_name("https://a.test/a%20b", &[]), "a_b");
    }

    #[test]
    fn lines_join_their_section() {
        let text = "#!name=m\n\n[URL Rewrite]\na reject\n\n[Map Local]\nb data=x\n";
        assert_eq!(
            add_line(text, URL_REWRITE, "c reject").unwrap(),
            "#!name=m\n\n[URL Rewrite]\na reject\nc reject\n\n[Map Local]\nb data=x\n"
        );
        assert_eq!(
            add_line("#!name=m\n", MAP_LOCAL, "x").unwrap(),
            "#!name=m\n\n[Map Local]\nx\n"
        );
        assert_eq!(add_line("", URL_REWRITE, "x").unwrap(), "[URL Rewrite]\nx\n");
        assert_eq!(add_line("http:\n  rewrite: []\n", URL_REWRITE, "x"), None);
    }

    #[test]
    fn the_menu_follows_the_request() {
        let mut t = MenuTarget {
            id: 3,
            url: "https://a.test/".into(),
            pending: false,
            status: 200,
            response_size: 10,
        };
        let labels = |items: Vec<ActionItem>| -> Vec<String> {
            items.iter().map(|i| i.label.to_string()).collect()
        };
        let modules = vec![("m1".to_string(), "Mine".to_string())];
        assert_eq!(
            labels(menu_items(&t, &modules, None)),
            ["复制链接", "复制为 cURL", "复制请求", "复制响应", "复制响应体", "导出 HAR", "添加拒绝规则", "添加本地映射规则"]
        );
        let open = menu_items(&t, &modules, Some(REJECT));
        assert_eq!(labels(open.clone())[7..9], ["新建模块", "Mine"]);
        assert_eq!(open[8].id.as_str(), "http-reject-to:3:m1");
        assert!(open[6].section && open[7].sub);
        t.pending = true;
        t.status = 0;
        assert_eq!(
            labels(menu_items(&t, &modules, None)),
            ["复制链接", "复制为 cURL", "复制请求", "导出 HAR", "添加拒绝规则"]
        );
    }
}
