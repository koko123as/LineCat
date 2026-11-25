//! The connections of this run (lib/views/connection/requests.dart and
//! lib/features/connection): the log `connection.watch` keeps, the search
//! (`trackerInfoQueryMatcher`), the filters and the rows and details the page shows.

use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};

/// `maxRequestsLength`: closed connections kept, oldest dropped first.
pub const MAX_CLOSED: usize = 2000;
/// A connection the counters have not mentioned for this long moves no data.
const SPEED_STALE_MS: i64 = 2500;

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn int(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn opt(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(Value::as_i64)
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------- search and conditions

/// `SearchQuery`: lower-case terms, all of which must appear.
pub fn terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect()
}

pub fn matches_terms(terms: &[String], fields: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let text = fields.join("\n").to_lowercase();
    terms.iter().all(|t| text.contains(t.as_str()))
}

/// A record whose fields conditions are set on (`RecordField.valuesOf`).
pub trait Record {
    fn values(&self, field: &str) -> Vec<String>;
}

/// `RecordCondition`: the field's name as typed (`host`), the value, whether it drops
/// what it holds, and whether it must equal a value rather than appear in one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    pub field: String,
    pub value: String,
    pub excludes: bool,
    pub equals: bool,
}

impl Condition {
    pub fn holds<R: Record>(&self, r: &R) -> bool {
        let needle = self.value.to_lowercase();
        let values = r.values(&self.field);
        if self.equals {
            values.iter().any(|v| v.to_lowercase() == needle)
        } else {
            values.join("\n").to_lowercase().contains(&needle)
        }
    }

    /// `operatorLabel`.
    pub fn operator(&self) -> &'static str {
        match (self.excludes, self.equals) {
            (false, false) => "包含",
            (true, false) => "不包含",
            (false, true) => "等于",
            (true, true) => "不等于",
        }
    }
}

/// `conditionsMatcher`: one field's conditions widen each other, fields narrow, and an
/// excluding one drops what it holds.
pub fn conditions_match<R: Record>(conditions: &[Condition], r: &R) -> bool {
    if conditions.iter().any(|c| c.excludes && c.holds(r)) {
        return false;
    }
    let mut rows: BTreeMap<&str, bool> = BTreeMap::new();
    for c in conditions.iter().filter(|c| !c.excludes) {
        let held = rows.entry(c.field.as_str()).or_insert(false);
        *held = *held || c.holds(r);
    }
    rows.values().all(|h| *h)
}

/// `ConnectionQuery.parse`: `host:github.com -app:=slack "rule:a b"` conditions out of a
/// search, and the text left.
pub fn parse_query(query: &str, fields: &[&str]) -> (Vec<Condition>, String) {
    let mut conditions = Vec::new();
    let mut rest = Vec::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in query.chars() {
        if c == '"' {
            quoted = !quoted;
            current.push(c);
        } else if c.is_whitespace() && !quoted {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    for token in tokens {
        let (excludes, body) = match token.strip_prefix('-') {
            Some(b) => (true, b),
            None => (false, token.as_str()),
        };
        let parsed = body.split_once(':').and_then(|(name, value)| {
            if !fields.contains(&name) {
                return None;
            }
            let (equals, value) = match value.strip_prefix('=') {
                Some(v) => (true, v),
                None => (false, value),
            };
            let value = if let Some(v) = value.strip_prefix('"') {
                v.trim_end_matches('"').to_string()
            } else if value.is_empty() || value.starts_with('=') {
                return None;
            } else {
                value.to_string()
            };
            Some((name, value.trim().to_string(), equals))
        });
        match parsed {
            Some((_, value, _)) if value.is_empty() => {}
            Some((name, value, equals)) => conditions.push(Condition {
                field: name.to_string(),
                value,
                excludes,
                equals,
            }),
            None => rest.push(token),
        }
    }
    (conditions, rest.join(" "))
}

/// A list's filter state (`ConditionFilterController`): one status, and conditions.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    pub status: i32,
    pub conditions: Vec<Condition>,
}

impl Filters {
    pub fn add(&mut self, c: Condition) {
        if !self.conditions.contains(&c) {
            self.conditions.push(c);
        }
    }

    /// `onlyCondition`: [c] alone lets its field through, the excluding ones stay.
    pub fn only(&mut self, c: Condition) {
        self.conditions.retain(|k| k.excludes || k.field != c.field);
        self.conditions.push(c);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.conditions.len() {
            self.conditions.remove(index);
        }
    }

    pub fn is_filtering(&self) -> bool {
        self.status != 0 || !self.conditions.is_empty()
    }
}

/// One line of the condition block: a field's widening conditions together, each
/// excluding one on its own line.
pub fn condition_lines(
    filters: &Filters,
    fields: &[(&str, &str)],
) -> Vec<(String, Vec<(usize, String)>)> {
    let mut lines = Vec::new();
    for (name, label) in fields {
        let widening: Vec<(usize, String)> = filters
            .conditions
            .iter()
            .enumerate()
            .filter(|(_, c)| c.field == *name && !c.excludes)
            .map(|(i, c)| (i, format!("{} {}", c.operator(), c.value)))
            .collect();
        if !widening.is_empty() {
            lines.push((label.to_string(), widening));
        }
        for (i, c) in filters.conditions.iter().enumerate() {
            if c.field == *name && c.excludes {
                lines.push((
                    label.to_string(),
                    vec![(i, format!("{} {}", c.operator(), c.value))],
                ));
            }
        }
    }
    lines
}

/// `FieldDraft.suggestionsOf`: the values a field holds most, those holding [text].
pub fn suggestions(
    values: impl Iterator<Item = String>,
    text: &str,
    limit: usize,
) -> Vec<(String, usize)> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for v in values.filter(|v| !v.is_empty()) {
        let n = counts.entry(v.clone()).or_insert(0);
        if *n == 0 {
            order.push(v);
        }
        *n += 1;
    }
    let mut all: Vec<(String, usize)> = order
        .into_iter()
        .map(|v| {
            let n = counts[&v];
            (v, n)
        })
        .collect();
    all.sort_by_key(|a| std::cmp::Reverse(a.1));
    let needle = text.trim().to_lowercase();
    all.into_iter()
        .filter(|(v, _)| v.to_lowercase().contains(&needle))
        .take(limit)
        .collect()
}

// ---------------------------------------------------------------- texts

/// `trackerDurationText`.
pub fn duration_text(ms: i64) -> String {
    let seconds = ms / 1000;
    if seconds < 60 {
        return format!("{} s", seconds.max(0));
    }
    let minutes = seconds as f64 / 60.0;
    if minutes < 60.0 {
        return if minutes < 10.0 {
            format!("{minutes:.1} min")
        } else {
            format!("{} min", minutes.round() as i64)
        };
    }
    format!("{:.1} h", minutes / 60.0)
}

/// `trackerEndpointText`.
pub fn endpoint(ip: &str, port: &str) -> String {
    if ip.is_empty() {
        return String::new();
    }
    if port.is_empty() {
        return ip.to_string();
    }
    if ip.contains(':') {
        format!("[{ip}]:{port}")
    } else {
        format!("{ip}:{port}")
    }
}

pub fn failure_label(kind: &str) -> &'static str {
    match kind {
        "timeout" => "超时",
        "refused" => "连接被拒绝",
        "unreachable" => "不可达",
        "dns" => "DNS 解析失败",
        "handshake" => "握手失败",
        _ => "其他错误",
    }
}

/// `ConnectionEndReason.label`; `failed` is named by the failure itself.
pub fn end_label(reason: &str) -> &'static str {
    match reason {
        "local" => "本地关闭",
        "remote" => "对端关闭",
        "finished" => "已结束",
        "timeout" => "超时",
        "reset" => "被重置",
        "core" => "被核心关闭",
        "error" => "错误",
        _ => "",
    }
}

// ---------------------------------------------------------------- connections

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timing {
    /// Unix milliseconds; the steps are microseconds after it.
    pub accepted: i64,
    pub dns_start: Option<i64>,
    pub dns_end: Option<i64>,
    pub routed: Option<i64>,
    pub dial_start: Option<i64>,
    pub connected: Option<i64>,
    pub dial_end: Option<i64>,
    pub decrypted: Option<i64>,
}

impl Timing {
    /// The wall-clock millisecond of a step.
    pub fn at(&self, step: Option<i64>) -> Option<i64> {
        step.map(|us| self.accepted + us / 1000)
    }
}

/// `TrackerInfo`, from Concord's `ConnectionInfo`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conn {
    pub id: i64,
    pub start: i64,
    pub network: String,
    pub source_ip: String,
    pub source_port: String,
    pub dest_ip: String,
    pub dest_port: String,
    pub host: String,
    pub process: String,
    pub process_path: String,
    pub uid: i64,
    pub remote_destination: String,
    pub dest_geoip: Vec<String>,
    pub dest_asn: String,
    pub special_proxy: String,
    pub special_rules: String,
    pub dns_mode: String,
    pub mitm: bool,
    pub mitm_refused: bool,
    pub chains: Vec<String>,
    pub rule: String,
    pub rule_payload: String,
    pub rule_index: Option<i64>,
    pub upload: i64,
    pub download: i64,
    pub up_speed: i64,
    pub down_speed: i64,
    pub sampled_at: i64,
    pub closed_at: Option<i64>,
    /// (kind, message)
    pub failure: Option<(String, String)>,
    pub timing: Option<Timing>,
    pub dns_queries: Vec<i64>,
    /// (reason, ended at, message)
    pub end: Option<(String, i64, String)>,
}

impl Conn {
    pub fn parse(v: &Value) -> Option<Conn> {
        let id = v.get("id")?.as_i64()?;
        let port = |k: &str| {
            v.get(k)
                .and_then(Value::as_i64)
                .map(|p| p.to_string())
                .unwrap_or_default()
        };
        Some(Conn {
            id,
            start: int(v, "start"),
            network: text(v, "network"),
            source_ip: text(v, "sourceIp"),
            source_port: port("sourcePort"),
            dest_ip: text(v, "destinationIp"),
            dest_port: port("destinationPort"),
            host: text(v, "host"),
            process: text(v, "process"),
            process_path: text(v, "processPath"),
            uid: int(v, "uid"),
            remote_destination: text(v, "remoteDestination"),
            dest_geoip: strings(v, "destinationGeoIp"),
            dest_asn: text(v, "destinationAsn"),
            special_proxy: text(v, "specialProxy"),
            special_rules: text(v, "specialRules"),
            dns_mode: text(v, "dnsMode"),
            mitm: v.get("mitm").and_then(Value::as_bool).unwrap_or(false),
            mitm_refused: v
                .get("mitmRefused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            chains: strings(v, "chains"),
            rule: text(v, "rule"),
            rule_payload: text(v, "rulePayload"),
            rule_index: opt(v, "ruleIndex"),
            upload: int(v, "upload"),
            download: int(v, "download"),
            failure: v
                .get("failure")
                .filter(|f| f.is_object())
                .map(|f| (text(f, "kind"), text(f, "message"))),
            timing: v.get("timing").filter(|t| t.is_object()).map(|t| Timing {
                accepted: int(t, "accepted"),
                dns_start: opt(t, "dnsStart"),
                dns_end: opt(t, "dnsEnd"),
                routed: opt(t, "routed"),
                dial_start: opt(t, "dialStart"),
                connected: opt(t, "connected"),
                dial_end: opt(t, "dialEnd"),
                decrypted: opt(t, "decrypted"),
            }),
            dns_queries: v
                .get("dnsQueries")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default(),
            ..Default::default()
        })
    }

    pub fn is_active(&self) -> bool {
        self.closed_at.is_none()
    }

    pub fn ended_at(&self) -> Option<i64> {
        self.closed_at.map(|c| {
            self.end
                .as_ref()
                .map(|e| e.1)
                .filter(|e| *e > 0)
                .unwrap_or(c)
        })
    }

    /// `trackerSummaryText`: the title, the headline, then each non-empty field as
    /// "label: value".
    pub fn summary(&self, now: i64) -> String {
        let mut chains = self.chains.clone();
        chains.reverse();
        let mut fields = vec![
            (
                "来源".to_string(),
                endpoint(&self.source_ip, &self.source_port),
            ),
            (
                "目标地址".to_string(),
                endpoint(&self.dest_ip, &self.dest_port),
            ),
            ("进程路径".to_string(), self.process_path.clone()),
            ("规则".to_string(), self.rule_text()),
            ("链路".to_string(), chains.join(" → ")),
        ];
        if let Some((kind, message)) = &self.failure {
            fields.push((failure_label(kind).to_string(), message.clone()));
        }
        let mut lines = vec![self.title(), self.headline(now)];
        lines.extend(
            fields
                .into_iter()
                .filter(|(_, v)| !v.is_empty())
                .map(|(l, v)| format!("{l}: {v}")),
        );
        lines.join("\n")
    }

    pub fn title(&self) -> String {
        if self.host.is_empty() {
            self.dest_ip.clone()
        } else {
            self.host.clone()
        }
    }

    /// The row's name: the host, else the address.
    pub fn name(&self) -> String {
        if self.host.is_empty() {
            endpoint(&self.dest_ip, &self.dest_port)
        } else {
            self.host.clone()
        }
    }

    pub fn is_global_routed(&self) -> bool {
        self.rule.is_empty()
            && self.special_proxy.is_empty()
            && self.chains.last().map(String::as_str) == Some("GLOBAL")
    }

    /// `trackerRuleText`.
    pub fn rule_text(&self) -> String {
        if !self.rule.is_empty() {
            return if self.rule_payload.is_empty() {
                self.rule.clone()
            } else {
                format!("{}({})", self.rule, self.rule_payload)
            };
        }
        (if self.is_global_routed() {
            "全局模式"
        } else {
            "未经规则"
        })
        .into()
    }

    /// `trackerRuleLine`.
    pub fn rule_line(&self) -> String {
        if self.rule.is_empty() {
            return self.rule_text();
        }
        if self.rule_payload.is_empty() {
            self.rule.clone()
        } else {
            format!("{},{}", self.rule, self.rule_payload)
        }
    }

    pub fn search_fields(&self) -> Vec<String> {
        let mut f = vec![
            self.network.clone(),
            self.host.clone(),
            self.dest_ip.clone(),
            self.dest_port.clone(),
            self.source_ip.clone(),
            self.source_port.clone(),
            self.process.clone(),
            self.process_path.clone(),
            self.remote_destination.clone(),
            self.dest_asn.clone(),
        ];
        f.extend(self.dest_geoip.iter().cloned());
        f.extend([
            self.special_proxy.clone(),
            self.special_rules.clone(),
            self.rule.clone(),
            self.rule_payload.clone(),
        ]);
        if self.rule.is_empty() {
            f.push(self.rule_text());
        }
        f.extend(self.chains.iter().cloned());
        f
    }

    /// `trackerLifetimeText`.
    pub fn lifetime(&self, now: i64) -> String {
        if let Some(end) = self.ended_at() {
            return duration_text(end - self.start);
        }
        let elapsed = ((now - self.start) / 1000).max(0);
        if elapsed < 3600 {
            format!("{}:{:02}", elapsed / 60, elapsed % 60)
        } else {
            format!(
                "{}:{:02}:{:02}",
                elapsed / 3600,
                elapsed / 60 % 60,
                elapsed % 60
            )
        }
    }

    pub fn speeds(&self, now: i64) -> (i64, i64) {
        if !self.is_active() || now - self.sampled_at > SPEED_STALE_MS {
            (0, 0)
        } else {
            (self.up_speed, self.down_speed)
        }
    }

    /// `trackerHeadlineParts`, joined.
    pub fn headline(&self, now: i64) -> String {
        let mut parts: Vec<String> = [
            self.process.trim().to_string(),
            self.chains.first().cloned().unwrap_or_default(),
        ]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
        if self.failure.is_some() {
            parts.push("失败".into());
            parts.push(self.lifetime(now));
        } else if self.is_active() {
            parts.push(format!("活动 {}", self.lifetime(now)));
        } else {
            parts.push("已关闭".into());
        }
        parts.join(" · ")
    }

    /// `trackerRepeatKeyOf`.
    pub fn repeat_key(&self) -> String {
        let kind = self.failure.as_ref().map(|f| f.0.as_str()).unwrap_or("");
        format!(
            "{kind}\u{0}{}\u{0}{}\u{0}{}\u{0}{}",
            self.network,
            self.title(),
            self.dest_port,
            self.process
        )
    }

    pub fn status_holds(&self, status: i32) -> bool {
        match status {
            1 => self.is_active(),
            2 => self.failure.is_some(),
            _ => true,
        }
    }
}

/// `ConnectionField`.
pub const FIELDS: &[(&str, &str)] = &[
    ("host", "主机"),
    ("app", "应用"),
    ("exit", "出口"),
    ("rule", "规则"),
    ("net", "协议"),
    ("ip", "IP"),
];

impl Record for Conn {
    fn values(&self, field: &str) -> Vec<String> {
        match field {
            "host" => vec![self.host.clone(), self.title()],
            "app" => vec![self.process.clone(), self.process_path.clone()],
            "exit" => self.chains.clone(),
            "rule" => vec![self.rule_text()],
            "net" => vec![self.network.clone()],
            "ip" => vec![self.dest_ip.clone(), self.source_ip.clone()],
            _ => Vec::new(),
        }
    }
}

/// `suggestedValuesOf` for the connection fields.
pub fn suggested(c: &Conn, field: &str) -> Vec<String> {
    match field {
        "host" => vec![if c.host.is_empty() {
            c.title()
        } else {
            c.host.clone()
        }],
        "app" => vec![c.process.clone()],
        "ip" => vec![c.dest_ip.clone()],
        _ => c.values(field),
    }
}

/// The connections of this run: every open one and the last [MAX_CLOSED] that closed.
#[derive(Default)]
pub struct Log {
    pub conns: HashMap<i64, Conn>,
    closed: VecDeque<i64>,
}

impl Log {
    /// `ConnectionSnapshot`: the open ones; any other held open has closed.
    pub fn snapshot(&mut self, list: &[Value], now: i64) {
        let mut seen = std::collections::HashSet::new();
        for v in list {
            if let Some(c) = Conn::parse(v) {
                seen.insert(c.id);
                self.open(c, now);
            }
        }
        let gone: Vec<i64> = self
            .conns
            .values()
            .filter(|c| c.is_active() && !seen.contains(&c.id))
            .map(|c| c.id)
            .collect();
        for id in gone {
            self.close(id, now);
        }
    }

    fn open(&mut self, mut c: Conn, now: i64) {
        c.sampled_at = now;
        match self.conns.get_mut(&c.id) {
            Some(held) => {
                let (closed_at, end, queries) =
                    (held.closed_at, held.end.clone(), held.dns_queries.clone());
                let (up, down, at) = (held.up_speed, held.down_speed, held.sampled_at);
                *held = c;
                held.closed_at = closed_at;
                held.end = end;
                if held.dns_queries.is_empty() {
                    held.dns_queries = queries;
                }
                held.up_speed = up;
                held.down_speed = down;
                held.sampled_at = at;
            }
            None => {
                self.conns.insert(c.id, c);
            }
        }
    }

    fn close(&mut self, id: i64, now: i64) {
        if let Some(c) = self.conns.get_mut(&id) {
            if c.closed_at.is_none() {
                c.closed_at = Some(now);
                self.closed.push_back(id);
            }
        }
        while self.closed.len() > MAX_CLOSED {
            if let Some(old) = self.closed.pop_front() {
                self.conns.remove(&old);
            }
        }
    }

    /// `ConnectionDelta`.
    pub fn delta(&mut self, d: &Value, now: i64) {
        for v in d
            .get("opened")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(c) = Conn::parse(v) {
                self.open(c, now);
            }
        }
        for e in d
            .get("ended")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(c) = self.conns.get_mut(&int(e, "id")) {
                c.end = Some((text(e, "reason"), int(e, "endedAt"), text(e, "message")));
            }
        }
        for k in d
            .get("counters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(c) = self.conns.get_mut(&int(k, "id")) {
                let dt = (now - c.sampled_at).max(1);
                let (up, down) = (int(k, "upload"), int(k, "download"));
                c.up_speed = ((up - c.upload).max(0) * 1000) / dt;
                c.down_speed = ((down - c.download).max(0) * 1000) / dt;
                c.upload = up;
                c.download = down;
                c.sampled_at = now;
            }
        }
        for id in d
            .get("closed")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
        {
            self.close(id, now);
        }
    }

    /// One event of `connection.watch`: the snapshot or a delta.
    pub fn take(&mut self, event: &Value, now: i64) {
        match event.get("connections").and_then(Value::as_array) {
            Some(list) => self.snapshot(list, now),
            None => self.delta(event, now),
        }
    }

    pub fn clear(&mut self) {
        self.conns.clear();
        self.closed.clear();
    }

    pub fn active_count(&self) -> usize {
        self.conns.values().filter(|c| c.is_active()).count()
    }

    /// Oldest first, as `ConnectionRows._compare`.
    pub fn ordered(&self) -> Vec<&Conn> {
        let mut all: Vec<&Conn> = self.conns.values().collect();
        all.sort_by_key(|c| (c.start, c.id));
        all
    }

    /// `resolvedBy`: the connections a DNS query gave the address of, newest first.
    pub fn resolved_by(&self, query: i64) -> Vec<&Conn> {
        let mut found: Vec<&Conn> = self
            .conns
            .values()
            .filter(|c| c.dns_queries.contains(&query))
            .collect();
        found.sort_by_key(|c| std::cmp::Reverse((c.start, c.id)));
        found
    }

    pub fn status_counts(&self) -> [usize; 3] {
        let mut n = [0; 3];
        for c in self.conns.values() {
            for (s, count) in n.iter_mut().enumerate() {
                if c.status_holds(s as i32) {
                    *count += 1;
                }
            }
        }
        n
    }
}

/// What the page shows: the connections the search and filters let through, oldest
/// first, and how many each merged failure row stands for.
pub fn shown<'a>(log: &'a Log, query: &str, filters: &Filters) -> Vec<(&'a Conn, usize)> {
    let (typed, rest) = parse_query(query, &FIELDS.iter().map(|f| f.0).collect::<Vec<_>>());
    let terms = terms(&rest);
    let passing = log.ordered().into_iter().filter(|c| {
        c.status_holds(filters.status)
            && conditions_match(&filters.conditions, *c)
            && conditions_match(&typed, *c)
            && matches_terms(&terms, &c.search_fields())
    });
    if filters.status != 2 {
        return passing.map(|c| (c, 1)).collect();
    }
    let mut runs: Vec<(&Conn, usize)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for c in passing {
        match at.get(&c.repeat_key()) {
            Some(&i) => {
                runs[i].0 = c;
                runs[i].1 += 1;
            }
            None => {
                at.insert(c.repeat_key(), runs.len());
                runs.push((c, 1));
            }
        }
    }
    runs.sort_by_key(|(c, _)| (c.start, c.id));
    runs
}

/// `recordMinuteGroups`: newest minute first, each minute's records newest first.
pub fn minute_groups<T>(items: Vec<T>, time_of: impl Fn(&T) -> i64) -> Vec<(i64, Vec<T>)> {
    let mut groups: Vec<(i64, Vec<T>)> = Vec::new();
    for item in items.into_iter().rev() {
        let minute = time_of(&item).div_euclid(60_000);
        match groups.iter_mut().find(|g| g.0 == minute) {
            Some(g) => g.1.push(item),
            None => groups.push((minute, vec![item])),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.0));
    groups
}

/// `recordMinuteLabel`: HH:mm today, MM-dd HH:mm before. `offset` is the local zone's
/// offset from UTC in seconds.
pub fn minute_label(minute: i64, now_ms: i64, offset: i64) -> String {
    let local = minute * 60 + offset;
    let day = local.div_euclid(86_400);
    let today = (now_ms / 1000 + offset).div_euclid(86_400);
    let (h, m) = (local.rem_euclid(86_400) / 3600, local.rem_euclid(3600) / 60);
    if day == today {
        format!("{h:02}:{m:02}")
    } else {
        let (_, mo, d) = civil(day);
        format!("{mo:02}-{d:02} {h:02}:{m:02}")
    }
}

/// Days since 1970-01-01 to (year, month, day).
pub fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `DateTime.showFull`: yyyy-MM-dd HH:mm:ss.
pub fn full_time(ms: i64, offset: i64) -> String {
    let local = ms.div_euclid(1000) + offset;
    let (y, mo, d) = civil(local.div_euclid(86_400));
    let s = local.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02} {:02}:{:02}:{:02}",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// A step of a connection's timeline (`TimelineEvent`): kind, title, note, when, and
/// its mark (0 step, 1 end, 2 error).
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub kind: String,
    pub title: String,
    pub note: String,
    pub at: Option<i64>,
    pub mark: i32,
}

fn dns_mode_label(mode: &str) -> Option<&'static str> {
    match mode {
        "fake-ip" => Some("fake-ip 映射"),
        "redir-host" => Some("redir-host"),
        "hosts" => Some("来自 hosts"),
        _ => None,
    }
}

fn millis(from: i64, to: i64) -> String {
    format!("{} ms", ((to - from) as f64 / 1000.0).round() as i64)
}

fn join(parts: &[String]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// `trackerTimelineEvents`, for the DNS queries the log still holds. `size` formats
/// a byte count.
pub fn timeline(
    c: &Conn,
    queries: &[&crate::dns_queries::Query],
    now: i64,
    size: fn(i64) -> String,
) -> Vec<Step> {
    let timing = c.timing.as_ref();
    let at = |step: Option<i64>| timing.and_then(|t| t.at(step));
    let dns_mode = dns_mode_label(&c.dns_mode).map(String::from);
    let dns_time = timing.and_then(|t| Some(millis(t.dns_start?, t.dns_end?)));
    let dial_time =
        timing.and_then(|t| Some(format!("握手 {}", millis(t.dial_start?, t.dial_end?))));
    let address = if c.dest_ip.is_empty() {
        &c.host
    } else {
        &c.dest_ip
    };
    let target = endpoint(address, &c.dest_port);
    let traffic = join(&[
        format!("下载 {}", size(c.download)),
        format!("上传 {}", size(c.upload)),
    ]);
    let failed = c.failure.is_some()
        || matches!(
            c.end.as_ref().map(|e| e.0.as_str()),
            Some("error" | "failed")
        );
    let mut steps = Vec::new();
    for q in queries {
        steps.push(Step {
            kind: "DNS".into(),
            title: format!("{} → {}", q.kind, q.summary()),
            note: join(&[
                dns_mode.clone().unwrap_or_default(),
                if q.cached {
                    "缓存".into()
                } else {
                    String::new()
                },
                format!("{} ms", q.delay),
            ]),
            at: at(timing.and_then(|t| t.dns_start)),
            mark: if q.is_failed() { 2 } else { 0 },
        });
    }
    if queries.is_empty() && !c.host.is_empty() && (dns_mode.is_some() || dns_time.is_some()) {
        steps.push(Step {
            kind: "DNS".into(),
            title: c.host.clone(),
            note: join(&[
                dns_mode.clone().unwrap_or_default(),
                dns_time.unwrap_or_default(),
            ]),
            at: at(timing.and_then(|t| t.dns_start)),
            mark: 0,
        });
    }
    let mut note = Vec::new();
    if let Some(i) = c.rule_index {
        note.push(format!("第 {} 条", i + 1));
    }
    if let Some(last) = c.chains.last() {
        note.push(format!("→ {last}"));
    }
    steps.push(Step {
        kind: "规则".into(),
        title: c.rule_line(),
        note: note.join(" "),
        at: at(timing.and_then(|t| t.routed)),
        mark: 0,
    });
    if c.chains.len() > 1 {
        steps.push(Step {
            kind: "节点".into(),
            title: c.chains[0].clone(),
            note: String::new(),
            at: at(timing.and_then(|t| t.dial_start)),
            mark: 0,
        });
    }
    steps.push(Step {
        kind: "连接".into(),
        title: format!("{} {target}", c.network.to_uppercase()),
        note: match &c.failure {
            Some((kind, message)) => join(&[failure_label(kind).into(), message.clone()]),
            None => dial_time.unwrap_or_default(),
        },
        at: at(timing.and_then(|t| t.dial_end.or(t.connected))),
        mark: if c.failure.is_some() { 2 } else { 0 },
    });
    if c.mitm {
        steps.push(Step {
            kind: "解密".into(),
            title: "已解密".into(),
            note: String::new(),
            at: at(timing.and_then(|t| t.decrypted)),
            mark: 0,
        });
    }
    if c.mitm_refused {
        steps.push(Step {
            kind: "解密".into(),
            title: "客户端拒绝了证书".into(),
            note: String::new(),
            at: None,
            mark: 2,
        });
    }
    if c.is_active() {
        steps.push(Step {
            kind: "状态".into(),
            title: format!("活动 {}", c.lifetime(now)),
            note: traffic,
            at: Some(now),
            mark: 1,
        });
    } else {
        let reason = c
            .end
            .as_ref()
            .map(|e| end_label(&e.0).to_string())
            .unwrap_or_default();
        steps.push(Step {
            kind: "状态".into(),
            title: format!("已关闭 {}", c.lifetime(now)),
            note: join(&[reason, traffic]),
            at: timing.and(c.ended_at()),
            mark: if failed { 2 } else { 1 },
        });
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn conn(id: i64, host: &str, process: &str) -> Value {
        json!({"id": id, "start": 1_000 * id, "network": "tcp", "sourceIp": "10.0.0.2", "sourcePort": 5000,
               "destinationIp": "1.2.3.4", "destinationPort": 443, "host": host, "inboundType": "tun",
               "process": process, "chains": ["香港 01", "节点选择"], "rule": "DOMAIN-SUFFIX", "rulePayload": "google.com",
               "upload": 0, "download": 0})
    }

    #[test]
    fn snapshot_and_deltas_keep_the_log() {
        let mut log = Log::default();
        log.take(
            &json!({"connections": [conn(1, "a.com", "chrome"), conn(2, "b.com", "slack")]}),
            10_000,
        );
        assert_eq!(log.active_count(), 2);
        log.take(
            &json!({"counters": [{"id": 1, "upload": 2048, "download": 4096}]}),
            11_000,
        );
        let c = &log.conns[&1];
        assert_eq!((c.up_speed, c.down_speed), (2048, 4096));
        assert_eq!(c.speeds(13_000), (2048, 4096));
        assert_eq!(c.speeds(14_000), (0, 0));
        log.take(
            &json!({"closed": [2], "ended": [{"id": 2, "reason": "remote", "endedAt": 11_500}]}),
            12_000,
        );
        assert!(!log.conns[&2].is_active());
        assert_eq!(log.conns[&2].ended_at(), Some(11_500));
        log.take(&json!({"connections": [conn(2, "b.com", "slack")]}), 13_000);
        assert!(!log.conns[&1].is_active() && !log.conns[&2].is_active());
        assert_eq!(log.active_count(), 0);
    }

    #[test]
    fn closed_connections_are_capped() {
        let mut log = Log::default();
        for id in 0..(MAX_CLOSED as i64 + 5) {
            log.take(&json!({"opened": [conn(id, "x", "")], "closed": [id]}), id);
        }
        assert_eq!(log.conns.len(), MAX_CLOSED);
        assert!(!log.conns.contains_key(&0));
    }

    #[test]
    fn query_conditions_read_like_flclash() {
        let fields = ["host", "app", "exit", "rule", "net", "ip"];
        let (c, rest) = parse_query(r#"github host:api -app:=slack rule:"a b" exit:"#, &fields);
        assert_eq!(rest, "github exit:");
        assert_eq!(c.len(), 3);
        assert_eq!(
            c[0],
            Condition {
                field: "host".into(),
                value: "api".into(),
                excludes: false,
                equals: false
            }
        );
        assert!(c[1].excludes && c[1].equals && c[1].value == "slack");
        assert_eq!(c[2].value, "a b");
        let (_, rest) = parse_query("foo:bar", &fields);
        assert_eq!(rest, "foo:bar");
    }

    #[test]
    fn conditions_widen_within_a_field_and_narrow_across() {
        let a = Conn::parse(&conn(1, "a.github.com", "chrome")).unwrap();
        let cond = |f: &str, v: &str, ex: bool| Condition {
            field: f.into(),
            value: v.into(),
            excludes: ex,
            equals: false,
        };
        assert!(conditions_match(
            &[cond("host", "github", false), cond("host", "gitlab", false)],
            &a
        ));
        assert!(!conditions_match(
            &[cond("host", "github", false), cond("app", "slack", false)],
            &a
        ));
        assert!(!conditions_match(
            &[cond("host", "github", false), cond("app", "chrome", true)],
            &a
        ));
        let eq = Condition {
            field: "app".into(),
            value: "CHROME".into(),
            excludes: false,
            equals: true,
        };
        assert!(eq.holds(&a));
    }

    #[test]
    fn search_and_status_and_repeats() {
        let mut log = Log::default();
        let mut failed = conn(3, "bad.com", "app");
        failed["failure"] = json!({"kind": "timeout", "message": "i/o timeout"});
        let mut again = failed.clone();
        again["id"] = json!(4);
        again["start"] = json!(4000);
        log.take(&json!({"opened": [conn(1, "a.com", "chrome"), conn(2, "b.com", "slack"), failed, again], "closed": [3, 4]}), 5000);
        let f = Filters::default();
        assert_eq!(shown(&log, "", &f).len(), 4);
        assert_eq!(shown(&log, "slack", &f).len(), 1);
        assert_eq!(shown(&log, "app:chrome b.com", &f).len(), 0);
        let failed_only = Filters {
            status: 2,
            conditions: vec![],
        };
        let rows = shown(&log, "", &failed_only);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].0.id, rows[0].1), (4, 2));
        assert_eq!(log.status_counts(), [4, 2, 2]);
    }

    #[test]
    fn texts_read_like_flclash() {
        assert_eq!(duration_text(59_000), "59 s");
        assert_eq!(duration_text(90_000), "1.5 min");
        assert_eq!(duration_text(30 * 60_000), "30 min");
        assert_eq!(duration_text(90 * 60_000), "1.5 h");
        assert_eq!(endpoint("::1", "53"), "[::1]:53");
        let c = Conn::parse(&conn(1, "", "")).unwrap();
        assert_eq!(c.name(), "1.2.3.4:443");
        assert_eq!(c.lifetime(1000 + 65_000), "1:05");
        assert_eq!(c.rule_text(), "DOMAIN-SUFFIX(google.com)");
        assert_eq!(c.rule_line(), "DOMAIN-SUFFIX,google.com");
    }

    #[test]
    fn minutes_group_newest_first() {
        let groups = minute_groups(vec![10_000i64, 50_000, 70_000, 130_000], |t| *t);
        assert_eq!(
            groups
                .iter()
                .map(|g| (g.0, g.1.clone()))
                .collect::<Vec<_>>(),
            vec![
                (2, vec![130_000]),
                (1, vec![70_000]),
                (0, vec![50_000, 10_000])
            ]
        );
        assert_eq!(minute_label(1, 120_000, 0), "00:01");
        assert_eq!(minute_label(1, 2 * 86_400_000, 0), "01-01 00:01");
        assert_eq!(
            full_time(1_700_000_000_000, 8 * 3600),
            "2023-11-15 06:13:20"
        );
    }

    #[test]
    fn suggestions_by_count() {
        let s = suggestions(
            ["a", "b", "b", "ab", ""].iter().map(|s| s.to_string()),
            "b",
            2,
        );
        assert_eq!(s, vec![("b".to_string(), 2), ("ab".to_string(), 1)]);
    }

    #[test]
    fn summary_lists_the_filled_fields() {
        let c = Conn::parse(&conn(1, "a.com", "chrome")).unwrap();
        let text = c.summary(5_000);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], c.title());
        assert!(lines.iter().any(|l| l.starts_with("来源: 10.0.0.2")));
        assert!(lines.iter().all(|l| !l.ends_with(": ")));
    }
}
