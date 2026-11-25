//! `ConnectionsTableCard`: the wide dashboard's tables of live connections, recent requests
//! and DNS queries, their columns by width and the header sort.

use crate::dns_queries::Query;
use crate::requests::Conn;
use std::collections::BTreeMap;

pub const MID_WIDTH: f32 = 620.0;
pub const WIDE_WIDTH: f32 = 880.0;
pub const MAX_ROWS: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Feed {
    Connections,
    Requests,
    Dns,
}

impl Feed {
    pub fn from_index(i: i32) -> Feed {
        match i {
            1 => Feed::Requests,
            2 => Feed::Dns,
            _ => Feed::Connections,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Col {
    pub id: &'static str,
    pub label: &'static str,
    pub width: f32,
    pub flex: f32,
    pub end: bool,
    pub descending_first: bool,
}

fn fixed(id: &'static str, label: &'static str, width: f32) -> Col {
    Col {
        id,
        label,
        width,
        flex: 0.0,
        end: false,
        descending_first: false,
    }
}

fn flex(id: &'static str, label: &'static str, flex: f32) -> Col {
    Col {
        flex,
        ..fixed(id, label, 0.0)
    }
}

fn number(id: &'static str, label: &'static str, width: f32) -> Col {
    Col {
        end: true,
        descending_first: true,
        ..fixed(id, label, width)
    }
}

pub fn columns(feed: Feed, width: f32) -> Vec<Col> {
    let mid = width >= MID_WIDTH;
    let wide = width >= WIDE_WIDTH;
    let w = |wide_w: f32, w: f32| if wide { wide_w } else { w };
    match feed {
        Feed::Connections => {
            let mut cols = vec![
                fixed("source", "来源", w(160.0, 140.0)),
                flex("target", "目标", w(1.3, 1.0)),
            ];
            if mid {
                cols.push(fixed("protocol", "协议", w(52.0, 46.0)));
            }
            if wide {
                cols.push(flex("rule", "规则", 1.0));
            }
            cols.push(fixed("exit", "出口", w(120.0, 100.0)));
            cols.push(number("download", "下载", w(80.0, 70.0)));
            cols.push(number("upload", "上传", w(80.0, 70.0)));
            if mid {
                cols.push(number("duration", "时长", w(72.0, 64.0)));
            }
            cols
        }
        Feed::Requests => vec![
            Col {
                descending_first: true,
                ..fixed("time", "时间", 76.0)
            },
            fixed("source", "来源", 140.0),
            flex("target", "目标", 1.2),
            flex("rule", "规则", 1.0),
            fixed("exit", "出口", 100.0),
            number("traffic", "流量", 64.0),
        ],
        Feed::Dns => vec![
            Col {
                descending_first: true,
                ..fixed("time", "时间", 76.0)
            },
            flex("domain", "域名", 1.3),
            fixed("type", "类型", 60.0),
            fixed("initiator", "发起", 52.0),
            flex("result", "结果", 1.0),
            number("elapsed", "耗时", 72.0),
        ],
    }
}

/// `tableColumnMinWidth`: no drag narrows a column below this.
pub const MIN_WIDTH: f32 = 48.0;

/// `tableColumnWidths`: each column's width in `total`, a dragged width over the column's
/// own; flex columns share what is left. When the dragged widths leave too little for
/// the flex ones, or none flexes, everything scales to fit.
pub fn widths(cols: &[Col], total: f32, overrides: &BTreeMap<String, f32>) -> Vec<f32> {
    let sized: Vec<Option<f32>> = cols
        .iter()
        .map(|c| {
            overrides
                .get(c.id)
                .copied()
                .or((c.flex == 0.0).then_some(c.width))
        })
        .collect();
    let fixed: f32 = sized.iter().flatten().sum();
    let flex: f32 = cols
        .iter()
        .zip(&sized)
        .filter(|(_, s)| s.is_none())
        .map(|(c, _)| c.flex)
        .sum();
    let flex_count = sized.iter().filter(|s| s.is_none()).count();
    let free = total - fixed;
    if overrides.is_empty() || (flex_count > 0 && free >= flex_count as f32 * MIN_WIDTH) {
        let share = free.max(0.0);
        return cols
            .iter()
            .zip(&sized)
            .map(|(c, s)| s.unwrap_or_else(|| share * c.flex / flex))
            .collect();
    }
    let natural: Vec<f32> = sized.iter().map(|s| s.unwrap_or(MIN_WIDTH)).collect();
    let sum: f32 = natural.iter().sum();
    natural.iter().map(|n| n * total / sum).collect()
}

/// `resizedTableColumns`: the boundary after column `boundary` moved by `delta`. Widening
/// takes from the columns after it, each down to [MIN_WIDTH]; narrowing gives to the next.
pub fn resized(
    cols: &[Col],
    widths: &[f32],
    overrides: &BTreeMap<String, f32>,
    boundary: usize,
    delta: f32,
) -> BTreeMap<String, f32> {
    let mut out = widths.to_vec();
    if boundary + 1 < cols.len() {
        if delta > 0.0 {
            let mut rest = delta;
            for w in out.iter_mut().skip(boundary + 1) {
                if rest <= 0.0 {
                    break;
                }
                let taken = rest.clamp(0.0, (*w - MIN_WIDTH).max(0.0));
                *w -= taken;
                rest -= taken;
            }
            out[boundary] += delta - rest;
        } else {
            let given = (-delta).clamp(0.0, (out[boundary] - MIN_WIDTH).max(0.0));
            out[boundary] -= given;
            out[boundary + 1] += given;
        }
    }
    let mut map = overrides.clone();
    for ((c, w), old) in cols.iter().zip(out).zip(widths) {
        if w != *old {
            map.insert(c.id.to_string(), w);
        }
    }
    map
}

/// The key a feed's dragged widths are kept under in `tableColumnWidths`.
pub fn table_id(feed: Feed) -> &'static str {
    match feed {
        Feed::Connections => "dashboard.connections",
        Feed::Requests => "dashboard.requests",
        Feed::Dns => "dashboard.dns",
    }
}

/// `nextRecordSort`: the column's first direction, then the other, then unsorted.
pub fn next_sort(sort: Option<(String, bool)>, col: &Col) -> Option<(String, bool)> {
    match sort {
        Some((id, descending)) if id == col.id => {
            (descending == col.descending_first).then(|| (id, !descending))
        }
        _ => Some((col.id.to_string(), col.descending_first)),
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub enum Key {
    Num(i64),
    Text(String),
}

/// `RecordSorter`: stable, so equal keys keep the feed's order.
pub fn sort_by_key<T>(items: &mut Vec<T>, descending: bool, key: impl Fn(&T) -> Key) {
    let keys: Vec<Key> = items.iter().map(&key).collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| {
        let c = keys[a]
            .partial_cmp(&keys[b])
            .unwrap_or(std::cmp::Ordering::Equal);
        let c = if descending { c.reverse() } else { c };
        c.then(a.cmp(&b))
    });
    let mut taken: Vec<Option<T>> = items.drain(..).map(Some).collect();
    items.extend(order.into_iter().filter_map(|i| taken[i].take()));
}

pub fn is_builtin_outbound(name: &str) -> bool {
    matches!(name, "DIRECT" | "REJECT" | "REJECT-DROP" | "PASS")
}

pub fn conn_key(c: &Conn, col: &str, now: i64) -> Key {
    let lower = |s: &str| Key::Text(s.to_lowercase());
    match col {
        "source" => lower(&c.process),
        "target" => lower(&c.title()),
        "protocol" => lower(&c.network),
        "rule" => lower(&format!("{} {}", c.rule, c.rule_payload)),
        "exit" => lower(c.chains.first().map(String::as_str).unwrap_or("")),
        "download" => Key::Num(c.download),
        "upload" => Key::Num(c.upload),
        "traffic" => Key::Num(c.upload + c.download),
        "duration" => Key::Num(now - c.start),
        _ => Key::Num(c.start),
    }
}

pub fn dns_key(q: &Query, col: &str) -> Key {
    match col {
        "domain" => Key::Text(q.domain.to_lowercase()),
        "type" => Key::Text(q.kind.clone()),
        "initiator" => Key::Text(crate::dns_queries::initiator_label(&q.initiator).into()),
        "result" => Key::Text(q.summary().to_lowercase()),
        "elapsed" => Key::Num(q.delay),
        _ => Key::Num(q.time),
    }
}

/// A cell's text and how it reads: 0 plain, 1 muted, 2 emphasized, 3 error, 4 warning;
/// `badge` is the source's letter tile or the 缓存 chip before a DNS answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    pub text: String,
    pub tone: i32,
    pub badge: String,
}

fn cell(text: impl Into<String>, tone: i32) -> Cell {
    Cell {
        text: text.into(),
        tone,
        badge: String::new(),
    }
}

fn or_dash(s: &str) -> String {
    if s.is_empty() {
        "—".into()
    } else {
        s.into()
    }
}

pub fn conn_cells(c: &Conn, cols: &[Col], now: i64, offset: i64) -> Vec<Cell> {
    cols.iter()
        .map(|col| match col.id {
            "time" => cell(crate::requests_view::clock(c.start, offset), 1),
            "source" => Cell {
                badge: c
                    .process
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or("?".into()),
                ..cell(or_dash(&c.process), 2)
            },
            "target" => cell(c.title(), 0),
            "protocol" => cell(c.network.to_uppercase(), 1),
            "rule" => cell(c.rule_text(), 0),
            "exit" => {
                let exit = c.chains.first().map(String::as_str).unwrap_or("");
                cell(or_dash(exit), if is_builtin_outbound(exit) { 1 } else { 0 })
            }
            "download" => cell(crate::size(c.download), 0),
            "upload" => cell(crate::size(c.upload), 0),
            "traffic" => cell(crate::size(c.upload + c.download), 0),
            "duration" => cell(crate::requests::duration_text(now - c.start), 1),
            _ => Cell::default(),
        })
        .collect()
}

pub fn dns_cells(q: &Query, cols: &[Col], offset: i64) -> Vec<Cell> {
    cols.iter()
        .map(|col| match col.id {
            "time" => cell(crate::requests_view::clock(q.time, offset), 1),
            "domain" => cell(q.domain.clone(), 2),
            "type" => cell(q.kind.clone(), 1),
            "initiator" => cell(
                or_dash(crate::dns_queries::initiator_label(&q.initiator)),
                1,
            ),
            "result" if !q.error.is_empty() => cell(q.error.clone(), 3),
            "result" if q.has_failure_rcode() => cell(q.rcode.clone(), 4),
            "result" => Cell {
                badge: if q.cached {
                    "缓存".into()
                } else {
                    String::new()
                },
                ..cell(
                    or_dash(q.answers.first().map(String::as_str).unwrap_or("")),
                    0,
                )
            },
            "elapsed" => cell(format!("{} ms", q.delay), 1),
            _ => Cell::default(),
        })
        .collect()
}

/// The feed's rows, newest first unless a column sorts them, at most [MAX_ROWS].
pub fn rows(
    feed: Feed,
    conns: &crate::requests::Log,
    dns: &crate::dns_queries::Log,
    cols: &[Col],
    sort: Option<&(String, bool)>,
    now: i64,
    offset: i64,
) -> Vec<Vec<Cell>> {
    if feed == Feed::Dns {
        let mut list: Vec<&Query> = dns.queries.iter().rev().collect();
        if let Some((id, desc)) = sort {
            sort_by_key(&mut list, *desc, |q| dns_key(q, id));
        }
        return list
            .into_iter()
            .take(MAX_ROWS)
            .map(|q| dns_cells(q, cols, offset))
            .collect();
    }
    let mut list: Vec<&Conn> = conns
        .ordered()
        .into_iter()
        .rev()
        .filter(|c| feed == Feed::Requests || c.is_active())
        .collect();
    if let Some((id, desc)) = sort {
        sort_by_key(&mut list, *desc, |c| conn_key(c, id, now));
    }
    list.into_iter()
        .take(MAX_ROWS)
        .map(|c| conn_cells(c, cols, now, offset))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(cols: &[Col]) -> Vec<&str> {
        cols.iter().map(|c| c.id).collect()
    }

    #[test]
    fn connection_columns_follow_the_width() {
        assert_eq!(
            ids(&columns(Feed::Connections, 500.0)),
            ["source", "target", "exit", "download", "upload"]
        );
        assert_eq!(
            ids(&columns(Feed::Connections, 700.0)),
            ["source", "target", "protocol", "exit", "download", "upload", "duration"]
        );
        let wide = columns(Feed::Connections, 900.0);
        assert_eq!(wide.len(), 8);
        assert_eq!((wide[0].width, wide[1].flex), (160.0, 1.3));
        assert_eq!(ids(&columns(Feed::Dns, 300.0))[5], "elapsed");
    }

    #[test]
    fn flex_columns_share_what_is_left() {
        let none = BTreeMap::new();
        let cols = columns(Feed::Requests, 600.0);
        let w = widths(&cols, 600.0, &none);
        assert_eq!(w[0], 76.0);
        let left = 600.0 - 76.0 - 140.0 - 100.0 - 64.0;
        assert!((w[2] - left * 1.2 / 2.2).abs() < 0.01);
        assert!((w.iter().sum::<f32>() - 600.0).abs() < 0.01);
        let narrow = widths(&cols, 300.0, &none);
        assert_eq!((narrow[1], narrow[2]), (140.0, 0.0));
    }

    #[test]
    fn dragged_widths_win_and_scale_when_cramped() {
        let cols = columns(Feed::Requests, 600.0);
        let mut over = BTreeMap::new();
        over.insert("source".to_string(), 200.0);
        let w = widths(&cols, 600.0, &over);
        assert_eq!(w[1], 200.0);
        assert!((w.iter().sum::<f32>() - 600.0).abs() < 0.01);
        over.insert("target".to_string(), 300.0);
        let w = widths(&cols, 600.0, &over);
        assert!((w.iter().sum::<f32>() - 600.0).abs() < 0.01);
        assert!((w[3] / w[0] - 48.0 / 76.0).abs() < 0.01);
    }

    #[test]
    fn resizing_takes_from_the_next_columns() {
        let cols = columns(Feed::Requests, 600.0);
        let none = BTreeMap::new();
        let w = widths(&cols, 600.0, &none);
        let m = resized(&cols, &w, &none, 0, 30.0);
        assert_eq!(m["time"], 106.0);
        assert_eq!(m["source"], 110.0);
        assert_eq!(m.len(), 2);
        let m = resized(&cols, &w, &none, 0, 120.0);
        assert_eq!((m["source"], m["time"]), (MIN_WIDTH, 76.0 + 120.0));
        let m = resized(&cols, &w, &none, 0, -50.0);
        assert_eq!((m["time"], m["source"]), (MIN_WIDTH, 168.0));
        assert_eq!(table_id(Feed::Dns), "dashboard.dns");
    }

    #[test]
    fn sort_cycles_through_both_directions() {
        let cols = columns(Feed::Connections, 900.0);
        let source = &cols[0];
        let download = &cols[5];
        let s = next_sort(None, source);
        assert_eq!(s, Some(("source".into(), false)));
        let s = next_sort(s, source);
        assert_eq!(s, Some(("source".into(), true)));
        assert_eq!(next_sort(s, source), None);
        assert_eq!(
            next_sort(Some(("source".into(), true)), download),
            Some(("download".into(), true))
        );
    }

    #[test]
    fn sorting_is_stable() {
        let mut v = vec![("b", 2), ("a", 1), ("c", 2), ("d", 1)];
        sort_by_key(&mut v, true, |x| Key::Num(x.1));
        assert_eq!(v, [("b", 2), ("c", 2), ("a", 1), ("d", 1)]);
        sort_by_key(&mut v, false, |x| Key::Text(x.0.into()));
        assert_eq!(v[0].0, "a");
    }

    #[test]
    fn rows_read_the_feed_newest_first() {
        let mut conns = crate::requests::Log::default();
        let c = |id: i64, start: i64, process: &str, chain: &str| {
            serde_json::json!({"id": id, "start": start, "network": "udp", "host": "a.example",
                "process": process, "chains": [chain], "upload": 10, "download": 2048})
        };
        conns.snapshot(
            &[c(1, 1000, "curl", "DIRECT"), c(2, 2000, "", "香港")],
            3000,
        );
        let cols = columns(Feed::Connections, 700.0);
        let r = rows(
            Feed::Connections,
            &conns,
            &Default::default(),
            &cols,
            None,
            5000,
            0,
        );
        assert_eq!(r.len(), 2);
        assert_eq!((r[0][0].text.as_str(), r[0][0].tone), ("—", 2));
        assert_eq!((r[1][0].badge.as_str(), r[1][3].tone), ("c", 1));
        assert_eq!(r[1][2].text, "UDP");
        let sort = ("source".to_string(), false);
        let r = rows(
            Feed::Requests,
            &conns,
            &Default::default(),
            &columns(Feed::Requests, 700.0),
            Some(&sort),
            5000,
            0,
        );
        assert_eq!(r[0][1].text, "—");
        assert_eq!(r[0][0].text, "00:00:02");
        assert!(is_builtin_outbound("REJECT-DROP") && !is_builtin_outbound("节点"));
    }

    #[test]
    fn dns_results_show_errors_and_the_cache() {
        let mut dns = crate::dns_queries::Log::default();
        dns.push(
            &serde_json::json!({"domain": "a.example", "type": "A", "initiator": "app",
            "cached": true, "answers": ["1.2.3.4"], "delay": 3, "time": 1000}),
        );
        dns.push(&serde_json::json!({"domain": "b.example", "type": "AAAA", "error": "timeout", "time": 2000}));
        let cols = columns(Feed::Dns, 700.0);
        let r = rows(Feed::Dns, &Default::default(), &dns, &cols, None, 0, 0);
        assert_eq!((r[0][4].text.as_str(), r[0][4].tone), ("timeout", 3));
        assert_eq!(
            (r[1][4].badge.as_str(), r[1][3].text.as_str()),
            ("缓存", "应用")
        );
        assert_eq!(r[1][5].text, "3 ms");
    }
}
