//! The DNS queries `dns.watch` reports (lib/features/connection/dns_queries.dart and
//! dns_query_detail.dart).

use crate::requests::{conditions_match, matches_terms, terms, Filters, Record};
use serde_json::Value;
use std::collections::VecDeque;

/// `maxDnsQueriesLength`.
pub const MAX_QUERIES: usize = 3000;

/// `DnsField`.
pub const FIELDS: &[(&str, &str)] = &[
    ("domain", "域名"),
    ("type", "记录类型"),
    ("initiator", "发起方"),
    ("upstream", "上游"),
];

/// `DnsQuery`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    /// The log's own number for it, which the page selects by.
    pub seq: i64,
    pub id: Option<i64>,
    pub domain: String,
    pub kind: String,
    pub initiator: String,
    pub upstream: String,
    pub cached: bool,
    pub answers: Vec<String>,
    pub rcode: String,
    pub error: String,
    pub delay: i64,
    /// Unix milliseconds.
    pub time: i64,
}

/// `DnsQueryInitiator.label`; empty for one this build does not know.
pub fn initiator_label(initiator: &str) -> &'static str {
    match initiator {
        "app" => "应用",
        "rule" => "规则",
        "direct" => "直连",
        "proxy" => "代理",
        "other" => "其他",
        _ => "",
    }
}

impl Query {
    pub fn parse(v: &Value) -> Option<Query> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Some(Query {
            seq: 0,
            id: v.get("id").and_then(Value::as_i64),
            domain: v.get("domain")?.as_str()?.to_string(),
            kind: s("type"),
            initiator: s("initiator"),
            upstream: s("upstream"),
            cached: v.get("cached").and_then(Value::as_bool).unwrap_or(false),
            answers: v
                .get("answers")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            rcode: s("rcode"),
            error: s("error"),
            delay: v.get("delay").and_then(Value::as_i64).unwrap_or(0),
            time: v.get("time").and_then(Value::as_i64).unwrap_or(0),
        })
    }

    pub fn has_failure_rcode(&self) -> bool {
        !self.rcode.is_empty() && self.rcode != "NOERROR"
    }

    pub fn is_failed(&self) -> bool {
        !self.error.is_empty() || self.has_failure_rcode()
    }

    pub fn summary(&self) -> String {
        if !self.error.is_empty() {
            self.error.clone()
        } else if self.has_failure_rcode() {
            self.rcode.clone()
        } else {
            self.answers.join(", ")
        }
    }

    pub fn search_fields(&self) -> Vec<String> {
        let mut f = vec![self.domain.clone(), self.kind.clone()];
        f.push(initiator_label(&self.initiator).to_string());
        if self.cached {
            f.push("缓存".into());
        }
        if self.has_failure_rcode() {
            f.push(self.rcode.clone());
        }
        f.extend([
            self.rcode.clone(),
            self.upstream.clone(),
            self.error.clone(),
        ]);
        f.extend(self.answers.iter().cloned());
        f
    }

    /// `DnsStatus`: 0 all, 1 failed, 2 cached.
    pub fn status_holds(&self, status: i32) -> bool {
        match status {
            1 => self.is_failed(),
            2 => self.cached,
            _ => true,
        }
    }

    /// `_failureTitle`: a rule answers through mihomo's rcode:// upstream; the rest is
    /// Go's wording.
    pub fn failure_title(&self) -> &'static str {
        if self.upstream.starts_with("rcode://") {
            return "被规则拦截";
        }
        if self.error.contains("i/o timeout") || self.error.contains("deadline exceeded") {
            return "查询超时";
        }
        match self.rcode.as_str() {
            "NXDOMAIN" => "域名不存在",
            "SERVFAIL" => "上游出错",
            "REFUSED" => "上游拒绝了查询",
            _ => "DNS 解析失败",
        }
    }

    /// The row's subtitle: who asked, then the answer.
    pub fn about(&self) -> String {
        [initiator_label(&self.initiator).to_string(), self.summary()]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

impl Record for Query {
    fn values(&self, field: &str) -> Vec<String> {
        match field {
            "domain" => vec![self.domain.clone()],
            "type" => vec![self.kind.clone()],
            "initiator" => vec![initiator_label(&self.initiator).to_string()],
            "upstream" => vec![self.upstream.clone()],
            _ => Vec::new(),
        }
    }
}

/// The queries of this run, oldest first, the newest [MAX_QUERIES].
#[derive(Default)]
pub struct Log {
    pub queries: VecDeque<Query>,
    next: i64,
}

impl Log {
    pub fn push(&mut self, v: &Value) {
        if let Some(mut q) = Query::parse(v) {
            self.next += 1;
            q.seq = self.next;
            self.queries.push_back(q);
            while self.queries.len() > MAX_QUERIES {
                self.queries.pop_front();
            }
        }
    }

    pub fn by_ids(&self, ids: &[i64]) -> Vec<&Query> {
        self.queries
            .iter()
            .rev()
            .filter(|q| q.id.is_some_and(|id| ids.contains(&id)))
            .collect()
    }

    pub fn status_counts(&self) -> [usize; 3] {
        let mut n = [0; 3];
        for q in &self.queries {
            for (s, count) in n.iter_mut().enumerate() {
                if q.status_holds(s as i32) {
                    *count += 1;
                }
            }
        }
        n
    }

    /// The queries the search and filters let through, oldest first, with their index.
    pub fn shown(&self, query: &str, filters: &Filters) -> Vec<(usize, &Query)> {
        let terms = terms(query);
        self.queries
            .iter()
            .enumerate()
            .filter(|(_, q)| {
                q.status_holds(filters.status)
                    && conditions_match(&filters.conditions, *q)
                    && matches_terms(&terms, &q.search_fields())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::Condition;
    use serde_json::json;

    fn q(domain: &str, rcode: &str, cached: bool) -> Value {
        json!({"id": 1, "domain": domain, "type": "A", "initiator": "app", "upstream": "https://doh.pub/dns-query",
               "cached": cached, "answers": ["1.2.3.4", "5.6.7.8"], "rcode": rcode, "delay": 12, "time": 1000})
    }

    #[test]
    fn queries_read_like_flclash() {
        let ok = Query::parse(&q("a.com", "NOERROR", false)).unwrap();
        assert!(!ok.is_failed());
        assert_eq!(ok.summary(), "1.2.3.4, 5.6.7.8");
        assert_eq!(ok.about(), "应用 · 1.2.3.4, 5.6.7.8");
        let bad = Query::parse(&q("b.com", "NXDOMAIN", true)).unwrap();
        assert!(bad.is_failed());
        assert_eq!(bad.summary(), "NXDOMAIN");
        assert_eq!(bad.failure_title(), "域名不存在");
        let mut blocked = bad.clone();
        blocked.upstream = "rcode://success".into();
        assert_eq!(blocked.failure_title(), "被规则拦截");
    }

    #[test]
    fn the_log_is_capped_and_filtered() {
        let mut log = Log::default();
        for i in 0..(MAX_QUERIES + 3) {
            log.push(&q(&format!("{i}.com"), "NOERROR", i % 2 == 0));
        }
        assert_eq!(log.queries.len(), MAX_QUERIES);
        assert_eq!(log.queries[0].domain, "3.com");
        log.push(&q("x.org", "SERVFAIL", false));
        let all = Filters::default();
        assert_eq!(log.shown("x.org", &all).len(), 1);
        let failed = Filters {
            status: 1,
            conditions: vec![],
        };
        assert_eq!(log.shown("", &failed).len(), 1);
        let only = Filters {
            status: 0,
            conditions: vec![Condition {
                field: "domain".into(),
                value: "x.org".into(),
                excludes: false,
                equals: true,
            }],
        };
        assert_eq!(log.shown("", &only).len(), 1);
        assert_eq!(log.status_counts()[2], MAX_QUERIES / 2);
    }
}
