//! The requests page's state and what it shows (lib/views/connection/requests.dart,
//! requests_detail_pane.dart and the rows and details of lib/features/connection): the
//! three logs, the segment, search and filters, the selection and the condition dialog.

use crate::dns_queries::{self, Query};
use crate::http_capture::{self, Summary};
use crate::proxies::Groups;
use crate::requests::{self, Condition, Conn, Filters};
use crate::{
    ActionItem, ConditionItem, ConditionLine, ConditionRow, DetailRow, DetailSection, RecordLabel,
    RecordRow, ReqDetail, Suggestion,
};
use serde_json::Value;
use slint::{ModelRc, SharedString, VecModel};
use std::collections::HashMap;

pub const CONNECTIONS: i32 = 0;
pub const HTTP: i32 = 1;
pub const DNS: i32 = 2;

/// The condition operators in `_Operator` order: (excludes, equals).
pub const OPERATORS: [(bool, bool); 4] =
    [(false, false), (true, false), (false, true), (true, true)];

/// The open condition dialog.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub editing: Option<usize>,
    pub field: usize,
    pub operator: usize,
    pub value: String,
}

#[derive(Default)]
pub struct ReqState {
    pub conns: requests::Log,
    pub dns: dns_queries::Log,
    pub http: http_capture::Feed,
    pub segment: i32,
    pub query: String,
    pub filters: [Filters; 3],
    pub selected: [Option<i64>; 3],
    pub tab: [i32; 3],
    /// `mitm.exchange` and the bodies (request, response) of the selected request.
    pub exchange: Option<Value>,
    pub bodies: [Option<String>; 2],
    /// `mitm.exchanges` of the selected connection.
    pub conn_http: Option<(i64, Vec<Summary>)>,
    pub draft: Option<Draft>,
    pub limit_open: bool,
    /// The width a condition line's chips wrap to.
    pub cond_width: f32,
    /// The requests tab's view while the dashboard's sheet (`openDashboardFeed`) shows its own.
    pub tab_view: Option<ReqView>,
}

#[derive(Default)]
pub struct ReqView {
    segment: i32,
    query: String,
    filters: [Filters; 3],
    selected: [Option<i64>; 3],
    tab: [i32; 3],
    exchange: Option<Value>,
    bodies: [Option<String>; 2],
    conn_http: Option<(i64, Vec<Summary>)>,
    draft: Option<Draft>,
    limit_open: bool,
}

/// What a page build reads besides the state.
pub struct Ctx<'a> {
    pub now: i64,
    pub offset: i64,
    pub capturing: bool,
    pub groups: &'a Groups,
    pub delays: &'a HashMap<String, i32>,
    /// The MITM hostnames while decryption is on.
    pub mitm_hosts: Option<&'a [String]>,
}

fn s(v: impl AsRef<str>) -> SharedString {
    v.as_ref().into()
}

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(v))
}

/// `RecordLabel`s from their texts and tones (0 neutral, 1 error, 2 success, 3 warning);
/// an empty text gives no label.
fn labels<T: AsRef<str>>(parts: impl IntoIterator<Item = (T, i32)>) -> ModelRc<RecordLabel> {
    model(
        parts
            .into_iter()
            .filter(|(text, _)| !text.as_ref().is_empty())
            .map(|(text, tone)| RecordLabel {
                text: s(text),
                tone,
            })
            .collect(),
    )
}

/// `outcomeLabels`: the status, then the error, or that it is still in progress, or
/// `finished`.
fn outcome_labels(
    status: i64,
    reason: &str,
    error: &str,
    pending: bool,
    finished: Vec<String>,
) -> ModelRc<RecordLabel> {
    let mut out = Vec::new();
    if status != 0 {
        out.push((
            http_capture::status_text(status, reason),
            http_capture::status_tone(status),
        ));
    }
    if !error.is_empty() {
        out.push((error.to_string(), 1));
    } else if pending {
        out.push(("进行中".to_string(), 0));
    } else {
        out.extend(finished.into_iter().map(|p| (p, 0)));
    }
    labels(out)
}

/// `NumberFormat.decimalPattern('en')`.
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The local wall clock of a Unix millisecond, HH:mm:ss.
pub fn clock(ms: i64, offset: i64) -> String {
    let s = (ms.div_euclid(1000) + offset).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

pub fn fields(segment: i32) -> &'static [(&'static str, &'static str)] {
    match segment {
        HTTP => http_capture::FIELDS,
        DNS => dns_queries::FIELDS,
        _ => requests::FIELDS,
    }
}

pub fn status_names(segment: i32) -> [&'static str; 3] {
    match segment {
        HTTP => ["全部", "失败", "已改写"],
        DNS => ["全部", "失败", "缓存"],
        _ => ["全部", "活动", "失败"],
    }
}

/// A package name by its last part (com.android.chrome is Chrome).
fn monogram(name: &str) -> String {
    let name = name.trim();
    let name = if name.contains('/') || name.contains(' ') {
        name
    } else {
        name.rsplit('.').next().unwrap_or(name)
    };
    name.chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

impl ReqState {
    fn swap_view(&mut self, v: &mut ReqView) {
        use std::mem::swap;
        swap(&mut self.segment, &mut v.segment);
        swap(&mut self.query, &mut v.query);
        swap(&mut self.filters, &mut v.filters);
        swap(&mut self.selected, &mut v.selected);
        swap(&mut self.tab, &mut v.tab);
        swap(&mut self.exchange, &mut v.exchange);
        swap(&mut self.bodies, &mut v.bodies);
        swap(&mut self.conn_http, &mut v.conn_http);
        swap(&mut self.draft, &mut v.draft);
        swap(&mut self.limit_open, &mut v.limit_open);
    }

    /// The dashboard's sheet opens on `segment` with nothing searched, filtered or picked.
    pub fn open_sheet(&mut self, segment: i32) {
        self.close_sheet();
        let mut v = ReqView {
            segment: segment.clamp(0, 2),
            ..Default::default()
        };
        self.swap_view(&mut v);
        self.tab_view = Some(v);
    }

    /// `openDashboardFeed`: live connections open on the active ones.
    pub fn open_feed(&mut self, feed: i32) {
        self.open_sheet(if feed == 2 { DNS } else { CONNECTIONS });
        if feed == 0 {
            self.filters_mut().status = 1;
        }
    }

    pub fn close_sheet(&mut self) {
        if let Some(mut v) = self.tab_view.take() {
            self.swap_view(&mut v);
        }
    }

    pub fn filters(&self) -> &Filters {
        &self.filters[self.segment.clamp(0, 2) as usize]
    }

    pub fn filters_mut(&mut self) -> &mut Filters {
        &mut self.filters[self.segment.clamp(0, 2) as usize]
    }

    pub fn status_counts(&self) -> [String; 3] {
        let counts = match self.segment {
            HTTP => self.http.status_counts(),
            DNS => self.dns.status_counts(),
            _ => self.conns.status_counts(),
        };
        counts.map(grouped)
    }

    /// `_StatusMenuButton`'s menu.
    pub fn status_actions(&self) -> Vec<ActionItem> {
        let status = self.filters().status;
        status_names(self.segment)
            .iter()
            .zip(self.status_counts())
            .enumerate()
            .map(|(i, (name, n))| ActionItem {
                id: s(format!("status:{i}")),
                label: s(name),
                subtitle: s(n),
                check: true,
                checked: i as i32 == status,
                enabled: true,
                ..Default::default()
            })
            .collect()
    }

    pub fn condition_lines(&self) -> Vec<ConditionLine> {
        requests::condition_lines(self.filters(), fields(self.segment))
            .into_iter()
            .map(|(label, items)| {
                let texts: Vec<String> = items.iter().map(|(_, t)| t.clone()).collect();
                let rows = requests::wrap_chips(&texts, self.cond_width)
                    .into_iter()
                    .map(|row| ConditionRow {
                        items: model(
                            row.into_iter()
                                .map(|i| ConditionItem {
                                    index: items[i].0 as i32,
                                    text: s(&items[i].1),
                                    or: i > 0,
                                })
                                .collect(),
                        ),
                    })
                    .collect();
                ConditionLine {
                    label: s(label),
                    rows: model(rows),
                }
            })
            .collect()
    }

    pub fn is_searching(&self) -> bool {
        !self.query.trim().is_empty() || self.filters().is_filtering()
    }

    /// The list's null status: its label and description.
    pub fn empty(&self, no_host: bool) -> (String, String) {
        if !self.query.trim().is_empty() || !self.filters().conditions.is_empty() {
            return ("没有匹配的结果".into(), String::new());
        }
        let status = self.filters().status;
        let named = match (self.segment, status) {
            (HTTP, 1) => "暂无失败的 HTTP 请求",
            (HTTP, 2) => "暂无改写过的 HTTP 请求",
            (DNS, 1) => "暂无失败的DNS查询",
            (DNS, 2) => "暂无命中缓存的DNS查询",
            (CONNECTIONS, 1) => "暂无活动连接",
            (CONNECTIONS, 2) => "暂无失败的连接",
            _ => "",
        };
        if !named.is_empty() {
            return (named.into(), String::new());
        }
        match self.segment {
            HTTP => (
                "暂无HTTP 请求".into(),
                if no_host {
                    "在“主机名”中添加主机后才会记录。".into()
                } else {
                    String::new()
                },
            ),
            DNS => ("暂无DNS查询".into(), String::new()),
            _ => ("暂无连接".into(), String::new()),
        }
    }

    /// The rows the list shows: newest minute first, each a header then its records.
    pub fn rows(&self, cx: &Ctx) -> Vec<RecordRow> {
        let selected = self.selected[self.segment.clamp(0, 2) as usize];
        let mut out = Vec::new();
        let mut push_groups = |groups: Vec<(i64, Vec<RecordRow>)>| {
            for (minute, rows) in groups {
                out.push(RecordRow {
                    header: true,
                    label: s(requests::minute_label(minute, cx.now, cx.offset)),
                    count: s(rows.len().to_string()),
                    time: rows.first().map(|r| r.time.clone()).unwrap_or_default(),
                    ..Default::default()
                });
                let last = rows.len().saturating_sub(1);
                for (i, mut row) in rows.into_iter().enumerate() {
                    row.first = i == 0;
                    row.last = i == last;
                    row.selected = selected == Some(row.id as i64);
                    out.push(row);
                }
            }
        };
        match self.segment {
            HTTP => {
                let items: Vec<(i64, RecordRow)> = self
                    .http
                    .shown(&self.query, self.filters())
                    .into_iter()
                    .map(|e| (e.start, http_row(e)))
                    .collect();
                push_groups(group(items, cx.offset));
            }
            DNS => {
                let items: Vec<(i64, RecordRow)> = self
                    .dns
                    .shown(&self.query, self.filters())
                    .into_iter()
                    .map(|(_, q)| (q.time, dns_row(q)))
                    .collect();
                push_groups(group(items, cx.offset));
            }
            _ => {
                let items: Vec<(i64, RecordRow)> =
                    requests::shown(&self.conns, &self.query, self.filters())
                        .into_iter()
                        .map(|(c, n)| (c.start, conn_row(c, n, cx.now)))
                        .collect();
                push_groups(group(items, cx.offset));
            }
        }
        out
    }

    /// The menu of the page's bar (`_buildMenuItems`).
    pub fn page_actions(&self, mitm: &crate::store::MitmSettings) -> Vec<ActionItem> {
        let mut items = Vec::new();
        let offers = mitm.enable;
        if offers {
            items.push(ActionItem {
                id: s("capture-on"),
                label: s("开启"),
                subtitle: s("仅保存在内存中"),
                checked: mitm.capture,
                check: true,
                enabled: true,
                heading: s("抓包"),
                ..Default::default()
            });
            items.push(ActionItem {
                id: s("capture-off"),
                label: s("关闭"),
                checked: !mitm.capture,
                check: true,
                enabled: true,
                ..Default::default()
            });
            let limit = http_capture::limit(mitm);
            items.push(ActionItem {
                id: s("capture-limit"),
                label: s("最多记录条数"),
                subtitle: s(limit.to_string()),
                glyph: s(if self.limit_open { "fold" } else { "unfold" }),
                enabled: true,
                ..Default::default()
            });
            if self.limit_open {
                for o in http_capture::options(mitm) {
                    items.push(ActionItem {
                        id: s(format!("capture-entries:{o}")),
                        label: s(o.to_string()),
                        checked: o == limit,
                        check: true,
                        enabled: true,
                        sub: true,
                        ..Default::default()
                    });
                }
            }
        }
        match self.segment {
            CONNECTIONS => items.push(ActionItem {
                id: s("close-active"),
                label: s("关闭全部活动连接"),
                danger: true,
                enabled: self.conns.active_count() > 0,
                section: offers,
                ..Default::default()
            }),
            DNS => items.push(ActionItem {
                id: s("clear-dns"),
                label: s("清除DNS缓存"),
                enabled: true,
                section: offers,
                ..Default::default()
            }),
            _ => {}
        }
        items
    }

    /// A row's menu (`trackerInfoMenuItems`, `dnsQueryMenuItems`); the HTTP rows have none.
    pub fn row_actions(&self, segment: i32, id: i64) -> Vec<ActionItem> {
        let item = |id: String, glyph: &str, label: &str, section: bool| ActionItem {
            id: s(id),
            glyph: s(glyph),
            label: s(label),
            enabled: true,
            section,
            ..Default::default()
        };
        let mut items = Vec::new();
        match segment {
            CONNECTIONS => {
                let Some(c) = self.conns.conns.get(&id) else {
                    return items;
                };
                if c.is_active() {
                    let mut close = item(format!("close:{id}"), "block", "关闭连接", false);
                    close.danger = true;
                    items.push(close);
                }
                let destination = requests::endpoint(&c.dest_ip, &c.dest_port);
                if !c.host.is_empty() {
                    items.push(item(format!("copy-host:{id}"), "copy", "复制主机", true));
                } else if !destination.is_empty() {
                    items.push(item(
                        format!("copy-host:{id}"),
                        "copy",
                        "复制目标地址",
                        true,
                    ));
                }
                if !c.process.is_empty() {
                    items.push(item(format!("only-app:{id}"), "filter", "只看此应用", true));
                }
                if !c.title().is_empty() {
                    items.push(item(
                        format!("hide-host:{id}"),
                        "eye-off",
                        "隐藏此主机",
                        c.process.is_empty(),
                    ));
                }
                if c.mitm {
                    items.push(item(format!("view-http:{id}"), "eye", "查看 HTTP", true));
                }
            }
            DNS => {
                let Some(q) = self.dns.queries.iter().find(|q| q.seq == id) else {
                    return items;
                };
                items.push(item(format!("copy-domain:{id}"), "copy", "复制域名", false));
                if !q.answers.is_empty() {
                    items.push(item(
                        format!("copy-answers:{id}"),
                        "copy",
                        "复制应答",
                        false,
                    ));
                }
                items.push(item(
                    format!("only-domain:{id}"),
                    "filter",
                    "只看此域名",
                    true,
                ));
            }
            _ => {}
        }
        items
    }

    /// The text a copy action of a row puts on the clipboard (`trackerCopyText`).
    /// `trackerDetailMenuItems`: copy the host, the destination and the summary.
    pub fn detail_actions(&self, id: i64) -> Vec<(String, &'static str, &'static str)> {
        let Some(c) = self.conns.conns.get(&id) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        if !c.host.is_empty() {
            items.push((format!("copy-host:{id}"), "copy", "复制主机"));
        }
        if !requests::endpoint(&c.dest_ip, &c.dest_port).is_empty() {
            items.push((format!("copy-dest:{id}"), "copy", "复制目标地址"));
        }
        items.push((format!("copy-summary:{id}"), "copy", "复制摘要"));
        items
    }

    pub fn copy_text(&self, action: &str, id: i64) -> Option<String> {
        match action {
            "copy-dest" => self
                .conns
                .conns
                .get(&id)
                .map(|c| requests::endpoint(&c.dest_ip, &c.dest_port)),
            "copy-host" => self.conns.conns.get(&id).map(|c| {
                if c.host.is_empty() {
                    requests::endpoint(&c.dest_ip, &c.dest_port)
                } else {
                    c.host.clone()
                }
            }),
            "copy-domain" => self
                .dns
                .queries
                .iter()
                .find(|q| q.seq == id)
                .map(|q| q.domain.clone()),
            "copy-answers" => self
                .dns
                .queries
                .iter()
                .find(|q| q.seq == id)
                .map(|q| q.answers.join("\n")),
            _ => None,
        }
    }

    /// `_onlyApp`, `_hideHost` and `_onlyDomain`.
    pub fn filter_by(&mut self, action: &str, id: i64) {
        match action {
            "only-app" => {
                if let Some(c) = self.conns.conns.get(&id) {
                    let c = Condition {
                        field: "app".into(),
                        value: c.process.clone(),
                        excludes: false,
                        equals: true,
                    };
                    self.filters[CONNECTIONS as usize].only(c);
                }
            }
            "hide-host" => {
                if let Some(c) = self.conns.conns.get(&id) {
                    let value = if c.host.is_empty() {
                        c.title()
                    } else {
                        c.host.clone()
                    };
                    self.filters[CONNECTIONS as usize].add(Condition {
                        field: "host".into(),
                        value,
                        excludes: true,
                        equals: false,
                    });
                }
            }
            "only-domain" => {
                if let Some(q) = self.dns.queries.iter().find(|q| q.seq == id) {
                    let c = Condition {
                        field: "domain".into(),
                        value: q.domain.clone(),
                        excludes: false,
                        equals: true,
                    };
                    self.filters[DNS as usize].only(c);
                }
            }
            _ => {}
        }
    }

    /// Drops a selection whose record the logs no longer hold.
    pub fn drop_gone(&mut self) {
        if let Some(id) = self.selected[CONNECTIONS as usize] {
            if !self.conns.conns.contains_key(&id) {
                self.selected[CONNECTIONS as usize] = None;
            }
        }
        if let Some(id) = self.selected[DNS as usize] {
            if !self.dns.queries.iter().any(|q| q.seq == id) {
                self.selected[DNS as usize] = None;
            }
        }
        if let Some(id) = self.selected[HTTP as usize] {
            if !self.http.by_id.contains_key(&id) {
                self.selected[HTTP as usize] = None;
                self.exchange = None;
            }
        }
    }

    // ------------------------------------------------------------ conditions

    pub fn open_draft(&mut self, editing: Option<usize>) {
        let names: Vec<&str> = fields(self.segment).iter().map(|f| f.0).collect();
        let draft = match editing.and_then(|i| self.filters().conditions.get(i)) {
            Some(c) => Draft {
                editing,
                field: names.iter().position(|n| *n == c.field).unwrap_or(0),
                operator: OPERATORS
                    .iter()
                    .position(|o| *o == (c.excludes, c.equals))
                    .unwrap_or(0),
                value: c.value.clone(),
            },
            None => Draft::default(),
        };
        self.draft = Some(draft);
    }

    fn condition_of(&self, d: &Draft) -> Option<Condition> {
        let value = d.value.trim().replace('"', "");
        if value.is_empty() {
            return None;
        }
        let (excludes, equals) = OPERATORS[d.operator.min(3)];
        Some(Condition {
            field: fields(self.segment).get(d.field)?.0.to_string(),
            value,
            excludes,
            equals,
        })
    }

    /// The draft's suggestions and how many records it would show (`FieldDraft`).
    pub fn draft_outcome(&self) -> (Vec<Suggestion>, String, bool) {
        let Some(d) = &self.draft else {
            return (Vec::new(), String::new(), false);
        };
        let field = fields(self.segment).get(d.field).map(|f| f.0).unwrap_or("");
        let mut base = self.filters().clone();
        if let Some(i) = d.editing {
            base.remove(i);
        }
        let draft = self.condition_of(d);
        let mut with = base.clone();
        if let Some(c) = draft.clone() {
            with.add(c);
        }
        let (values, shown): (Vec<String>, usize) = match self.segment {
            HTTP => (
                self.http
                    .by_id
                    .values()
                    .flat_map(|e| match field {
                        "app" => vec![e.process.clone()],
                        _ => requests::Record::values(e, field),
                    })
                    .collect(),
                self.http.shown("", &with).len(),
            ),
            DNS => (
                self.dns
                    .queries
                    .iter()
                    .flat_map(|q| requests::Record::values(q, field))
                    .collect(),
                self.dns.shown("", &with).len(),
            ),
            _ => {
                let counted = Filters {
                    status: with.status,
                    conditions: with.conditions.clone(),
                };
                (
                    self.conns
                        .conns
                        .values()
                        .flat_map(|c| requests::suggested(c, field))
                        .collect(),
                    self.conns
                        .conns
                        .values()
                        .filter(|c| {
                            c.status_holds(counted.status)
                                && requests::conditions_match(&counted.conditions, *c)
                        })
                        .count(),
                )
            }
        };
        let suggestions = requests::suggestions(values.into_iter(), &d.value, 4)
            .into_iter()
            .map(|(value, n)| Suggestion {
                value: s(value),
                count: s(grouped(n)),
            })
            .collect();
        let noun = match self.segment {
            HTTP => format!("将筛出 {} 个请求", grouped(shown)),
            DNS => format!("将筛出 {} 次查询", grouped(shown)),
            _ => format!("将筛出 {} 条连接", grouped(shown)),
        };
        (suggestions, noun, draft.is_some())
    }

    pub fn submit_draft(&mut self) -> bool {
        let Some(d) = self.draft.clone() else {
            return false;
        };
        let Some(c) = self.condition_of(&d) else {
            return false;
        };
        let filters = self.filters_mut();
        match d.editing {
            Some(i) if i < filters.conditions.len() => {
                if filters.conditions.contains(&c) {
                    filters.remove(i);
                } else {
                    filters.conditions[i] = c;
                }
            }
            _ => filters.add(c),
        }
        self.draft = None;
        true
    }

    // ------------------------------------------------------------ details

    /// The selected record of the segment shown, or what to select.
    pub fn detail(&self, cx: &Ctx) -> ReqDetail {
        let segment = self.segment.clamp(0, 2);
        let selected = self.selected[segment as usize];
        let empty = |label: &str, hint: &str| ReqDetail {
            empty: s(label),
            hint: s(hint),
            ..Default::default()
        };
        match (segment, selected) {
            (CONNECTIONS, Some(id)) => match self.conns.conns.get(&id) {
                Some(c) => self.conn_detail(c, cx),
                None => empty("选择一条连接", "这里会显示它的时间线、HTTP 请求和连接信息"),
            },
            (DNS, Some(id)) => match self.dns.queries.iter().find(|q| q.seq == id) {
                Some(q) => self.dns_detail(q, cx),
                None => empty("选择一次查询", "这里会显示它的应答、上游，以及相关的连接"),
            },
            (HTTP, Some(id)) => self.http_detail(id, cx),
            (HTTP, None) => empty("选择一个请求", "这里会显示它的请求、响应和耗时"),
            (DNS, None) => empty("选择一次查询", "这里会显示它的应答、上游，以及相关的连接"),
            _ => empty("选择一条连接", "这里会显示它的时间线、HTTP 请求和连接信息"),
        }
    }

    fn conn_detail(&self, c: &Conn, cx: &Ctx) -> ReqDetail {
        let capturing = cx.capturing && c.mitm;
        let exchanges = match &self.conn_http {
            Some((id, list)) if *id == c.id => list.as_slice(),
            _ => &[],
        };
        let mut tabs = vec![s("时间线")];
        if capturing {
            tabs.push(s(if exchanges.is_empty() {
                "HTTP 请求".to_string()
            } else {
                format!("HTTP 请求 {}", exchanges.len())
            }));
        }
        tabs.push(s("信息"));
        let tab = self.tab[CONNECTIONS as usize].clamp(0, tabs.len() as i32 - 1);
        let sections = if tab == 0 {
            vec![self.timeline_section(c, cx)]
        } else if capturing && tab == 1 {
            vec![section("", exchanges.iter().map(exchange_link).collect())]
        } else {
            self.conn_info(c, cx)
        };
        ReqDetail {
            has: true,
            well: s(conn_well(c)),
            app: s(&c.process),
            well_label: false,
            title: s(c.title()),
            subtitle: s(c.process.trim()),
            labels: labels(c.headline_labels(cx.now)),
            closable: c.is_active(),
            menu: true,
            tabs: model(tabs),
            tab,
            sections: model(sections),
            sheet_title: s("连接详情"),
            ..Default::default()
        }
    }

    fn timeline_section(&self, c: &Conn, cx: &Ctx) -> DetailSection {
        let queries = self.dns.by_ids(&c.dns_queries);
        let steps = requests::timeline(c, &queries, cx.now, crate::size);
        let timed = c.timing.is_some();
        let mut shown_second = None;
        let last = steps.len().saturating_sub(1);
        let rows = steps
            .into_iter()
            .enumerate()
            .map(|(i, step)| {
                let trailing = match step.at {
                    Some(_) if step.kind == "状态" && c.is_active() => "现在".to_string(),
                    Some(at) if timed => {
                        let second = at.div_euclid(1000);
                        let label = if shown_second == Some(second) {
                            format!(".{:03}", at.rem_euclid(1000))
                        } else {
                            clock(at, cx.offset)
                        };
                        shown_second = Some(second);
                        label
                    }
                    _ => String::new(),
                };
                DetailRow {
                    kind: s("rail"),
                    title: s(&step.title),
                    note: s(&step.kind),
                    labels: labels(step.notes),
                    trailing: s(trailing),
                    mark: step.mark,
                    first: i == 0,
                    last: i == last,
                    ..Default::default()
                }
            })
            .collect();
        section("", rows)
    }

    fn conn_info(&self, c: &Conn, cx: &Ctx) -> Vec<DetailSection> {
        let mut out = Vec::new();
        if let Some((kind, message)) = &c.failure {
            out.push(section(
                "失败",
                vec![DetailRow {
                    kind: s("stacked"),
                    title: s(requests::failure_label(kind)),
                    value: s(message),
                    tone: 1,
                    arg: s(format!("copy:{message}")),
                    ..Default::default()
                }],
            ));
        }
        let mut path = Vec::new();
        if !c.process.is_empty() || c.uid != 0 {
            let uid = if c.uid != 0 {
                format!("UID {}", c.uid)
            } else {
                String::new()
            };
            let mut row = rail(
                if c.process.is_empty() {
                    c.uid.to_string()
                } else {
                    c.process.clone()
                },
                "进程".into(),
                String::new(),
                0,
            );
            row.labels = labels([(uid, 0)]);
            path.push(row);
        }
        let hops: Vec<&String> = c.chains.iter().rev().collect();
        path.push(rail(
            c.rule_line(),
            "规则".into(),
            String::new(),
            if hops.is_empty() { 1 } else { 0 },
        ));
        for (step, name) in hops.iter().enumerate() {
            let end = step + 1 == hops.len();
            let (note, kind, trailing) = match cx.groups.groups.get(*name) {
                Some(g) => ("策略组", g.kind.clone(), String::new()),
                None => {
                    let kind = cx.groups.kinds.get(*name).cloned().unwrap_or_default();
                    let delay = cx.delays.get(*name).copied().filter(|d| *d > 0);
                    (
                        "节点",
                        kind,
                        delay.map(|d| format!("{d} ms")).unwrap_or_default(),
                    )
                }
            };
            let mut row = rail(
                (*name).clone(),
                note.into(),
                trailing,
                if end { 1 } else { 0 },
            );
            row.labels = labels([(kind, 0)]);
            path.push(row);
        }
        let last = path.len().saturating_sub(1);
        for (i, row) in path.iter_mut().enumerate() {
            row.first = i == 0;
            row.last = i == last;
        }
        out.push(section("链路", path));
        let (up, down) = c.speeds(cx.now);
        let mut traffic = section(
            "流量",
            vec![DetailRow {
                kind: s("traffic"),
                title: s(crate::size(c.download)),
                value: s(crate::size(c.upload)),
                note: s(if c.is_active() {
                    crate::size(down)
                } else {
                    String::new()
                }),
                trailing: s(if c.is_active() {
                    crate::size(up)
                } else {
                    String::new()
                }),
                ..Default::default()
            }],
        );
        traffic.plain = true;
        out.push(traffic);
        if let Some(change) =
            crate::mitm::host_change(cx.mitm_hosts, &c.host, &c.network, c.mitm || c.mitm_refused)
        {
            let mut decrypt = section(
                "HTTPS 解密",
                vec![DetailRow {
                    kind: s("link"),
                    title: s(change.title),
                    arg: s(format!("mitm-host:{}:{}", change.exclude, c.host)),
                    ..Default::default()
                }],
            );
            if c.mitm_refused {
                decrypt.footer =
                    s("客户端拒绝了证书。请安装证书；如果应用固定了证书，请排除此主机。");
            }
            out.push(decrypt);
        }
        let copy = |title: &str, value: String| text_row(title, &value, true);
        let plain = |title: &str, value: String| text_row(title, &value, false);
        let rows: Vec<DetailRow> = [
            copy("来源", requests::endpoint(&c.source_ip, &c.source_port)),
            copy("目标地址", requests::endpoint(&c.dest_ip, &c.dest_port)),
            copy("远程目标", c.remote_destination.clone()),
            copy("主机", c.host.clone()),
            plain("目标地理定位", c.dest_geoip.join(" ")),
            plain("目标IP ASN", c.dest_asn.clone()),
            plain("网络类型", c.network.clone()),
            plain("DNS模式", c.dns_mode.clone()),
            plain("特殊代理", c.special_proxy.clone()),
            plain("特殊规则", c.special_rules.clone()),
            plain("创建时间", requests::full_time(c.start, cx.offset)),
            plain(
                "关闭时间",
                c.ended_at()
                    .map(|t| requests::full_time(t, cx.offset))
                    .unwrap_or_default(),
            ),
        ]
        .into_iter()
        .filter(|r| !r.value.is_empty())
        .collect();
        out.push(section("地址", rows));
        out
    }

    fn dns_detail(&self, q: &Query, cx: &Ctx) -> ReqDetail {
        let mut sections = Vec::new();
        if q.is_failed() {
            sections.push(section(
                "错误",
                vec![DetailRow {
                    kind: s("stacked"),
                    title: s(q.failure_title()),
                    value: s(if q.error.is_empty() {
                        &q.rcode
                    } else {
                        &q.error
                    }),
                    tone: 1,
                    arg: s(format!(
                        "copy:{}",
                        if q.error.is_empty() {
                            &q.rcode
                        } else {
                            &q.error
                        }
                    )),
                    ..Default::default()
                }],
            ));
        }
        sections.push(section(
            &format!("应答 {}", q.answers.len()),
            q.answers
                .iter()
                .map(|a| DetailRow {
                    kind: s("text"),
                    title: s(a),
                    arg: s(format!("copy:{a}")),
                    ..Default::default()
                })
                .collect(),
        ));
        if let Some(id) = q.id {
            let related = self.conns.resolved_by(id);
            if !related.is_empty() {
                sections.push(section(
                    &format!("相关的连接 {}", related.len()),
                    related.iter().map(|c| conn_link(c, cx.now)).collect(),
                ));
            }
        }
        let initiator = dns_queries::initiator_label(&q.initiator);
        let mut fields = vec![text_row("记录类型", &q.kind, false)];
        if !initiator.is_empty() {
            fields.push(text_row("发起方", initiator, false));
        }
        if !q.upstream.is_empty() {
            fields.push(text_row("上游", &q.upstream, true));
        }
        fields.push(text_row("缓存", if q.cached { "是" } else { "否" }, false));
        if !q.rcode.is_empty() {
            let mut code = text_row("响应码", &q.rcode, true);
            code.tone = if q.has_failure_rcode() { 1 } else { 0 };
            fields.push(code);
        }
        sections.push(section("查询", fields));
        let asked = if initiator.is_empty() {
            String::new()
        } else {
            format!("{initiator}发起")
        };
        ReqDetail {
            has: true,
            well: s(&q.kind),
            well_label: true,
            title: s(&q.domain),
            labels: labels([
                (asked, 0),
                (clock(q.time, cx.offset), 0),
                (format!("{} ms", q.delay), 0),
            ]),
            menu: true,
            tabs: model(Vec::new()),
            sections: model(sections),
            sheet_title: s("DNS查询详情"),
            ..Default::default()
        }
    }

    fn http_detail(&self, id: i64, cx: &Ctx) -> ReqDetail {
        let base = ReqDetail {
            has: true,
            sheet_title: s("请求详情"),
            ..Default::default()
        };
        let Some(e) = self
            .exchange
            .as_ref()
            .filter(|e| e.get("id").and_then(Value::as_i64) == Some(id))
        else {
            let summary = self.http.by_id.get(&id);
            return ReqDetail {
                loading: summary.is_some(),
                empty: s(if summary.is_some() {
                    ""
                } else {
                    "这条请求的记录已清除"
                }),
                well: s(summary.map(|e| e.method.as_str()).unwrap_or("")),
                well_label: true,
                title: s(summary
                    .map(|e| http_capture::path_of(&e.url))
                    .unwrap_or_default()),
                tabs: model(Vec::new()),
                sections: model(Vec::new()),
                ..base
            };
        };
        let text = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let int = |k: &str| e.get(k).and_then(Value::as_i64).unwrap_or(0);
        let pending = e.get("pending").and_then(Value::as_bool).unwrap_or(false);
        let (status, reason, error) = (int("status"), text("reason"), text("error"));
        let (duration, ttfb) = (int("duration"), int("ttfb"));
        let (request_size, response_size) = (int("request-size"), int("response-size"));
        let url = text("url");
        let head = outcome_labels(
            status,
            &reason,
            &error,
            pending,
            vec![
                http_capture::millis_text(duration),
                crate::size(response_size),
            ],
        );
        let tabs = vec![s("概览"), s("请求"), s("响应")];
        let tab = self.tab[HTTP as usize].clamp(0, 2);
        let headers = |key: &str| -> Vec<DetailRow> {
            let mut rows = Vec::new();
            if let Some(map) = e.get(key).and_then(Value::as_object) {
                let mut names: Vec<&String> = map.keys().collect();
                names.sort();
                for name in names {
                    for v in map[name]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        rows.push(DetailRow {
                            kind: s("stacked"),
                            title: s(name),
                            value: s(v),
                            arg: s(format!("copy:{v}")),
                            ..Default::default()
                        });
                    }
                }
            }
            rows
        };
        let sections = match tab {
            0 => {
                let mut info = vec![DetailRow {
                    kind: s("stacked"),
                    title: s("URL"),
                    value: s(&url),
                    arg: s(format!("copy:{url}")),
                    ..Default::default()
                }];
                let original = text("original-url");
                if !original.is_empty() && original != url {
                    info.push(DetailRow {
                        kind: s("stacked"),
                        title: s("原 URL"),
                        value: s(&original),
                        arg: s(format!("copy:{original}")),
                        ..Default::default()
                    });
                }
                info.push(text_row("方法", &text("method"), false));
                if !text("proto").is_empty() {
                    info.push(text_row("协议", &text("proto"), false));
                }
                if status != 0 || pending {
                    let mut row = text_row(
                        "状态",
                        &if status != 0 {
                            http_capture::status_text(status, &reason)
                        } else {
                            "进行中".into()
                        },
                        false,
                    );
                    row.tone = http_capture::status_tone(status);
                    info.push(row);
                }
                if !error.is_empty() {
                    info.push(DetailRow {
                        kind: s("stacked"),
                        title: s("错误"),
                        value: s(&error),
                        arg: s(format!("copy:{error}")),
                        ..Default::default()
                    });
                }
                if let Some(route) = e.get("route").filter(|r| r.is_object()) {
                    let rt = |k: &str| {
                        route
                            .get(k)
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    };
                    let (rule, payload) = (rt("rule"), rt("rule-payload"));
                    if !rule.is_empty() {
                        let value = if payload.is_empty() {
                            rule
                        } else {
                            format!("{rule}({payload})")
                        };
                        info.push(text_row("规则", &value, false));
                    }
                    let chains: Vec<&str> = route
                        .get("chains")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect();
                    if !chains.is_empty() {
                        let value = chains.iter().rev().copied().collect::<Vec<_>>().join(" → ");
                        info.push(text_row("代理链", &value, false));
                    }
                }
                let start = http_capture::rfc3339_ms(&text("start"));
                info.push(text_row(
                    "创建时间",
                    &requests::full_time(start, cx.offset),
                    false,
                ));
                if request_size > 0 {
                    info.push(text_row("请求体", &crate::size(request_size), false));
                }
                if !pending {
                    info.push(text_row("响应体", &crate::size(response_size), false));
                }
                let mut out = vec![section("基础信息", info)];
                if status != 0 && !pending {
                    out.push(section(
                        "耗时",
                        vec![
                            text_row(
                                "等待响应",
                                &http_capture::millis_text(ttfb.clamp(0, duration)),
                                false,
                            ),
                            text_row("总计", &http_capture::millis_text(duration), false),
                        ],
                    ));
                }
                let actions: Vec<DetailRow> = e
                    .get("actions")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        let at =
                            |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                        let module = at("module");
                        DetailRow {
                            kind: s("stacked"),
                            title: s(http_capture::action_label(&at("kind"))),
                            value: s(at("detail")),
                            labels: labels([
                                (
                                    match at("phase").as_str() {
                                        "request" => "请求".to_string(),
                                        "response" => "响应".to_string(),
                                        other => other.to_string(),
                                    },
                                    0,
                                ),
                                (
                                    if module.is_empty() {
                                        String::new()
                                    } else {
                                        format!("来自 {module}")
                                    },
                                    0,
                                ),
                            ]),
                            ..Default::default()
                        }
                    })
                    .collect();
                if !actions.is_empty() {
                    out.push(section("命中的规则", actions));
                }
                if let Some(conn) = e
                    .get("connection-id")
                    .and_then(Value::as_i64)
                    .and_then(|id| self.conns.conns.get(&id))
                {
                    out.push(section("所属连接", vec![conn_link(conn, cx.now)]));
                }
                out
            }
            side => {
                let response = side == 2;
                let mut out = Vec::new();
                let head = headers(if response {
                    "response-header"
                } else {
                    "request-header"
                });
                let size = if response {
                    response_size
                } else {
                    request_size
                };
                let title = if response { "响应体" } else { "请求体" };
                if !head.is_empty() {
                    out.push(section(if response { "响应头" } else { "请求头" }, head));
                }
                if size > 0 && pending {
                    out.push(section(title, vec![text_row("进行中", "", false)]));
                } else if let Some(body) = &self.bodies[response as usize] {
                    if !body.is_empty() {
                        out.push(section(
                            title,
                            vec![DetailRow {
                                kind: s("code"),
                                value: s(body),
                                ..Default::default()
                            }],
                        ));
                    }
                }
                out
            }
        };
        let empty = if sections.is_empty() {
            if pending {
                "进行中".to_string()
            } else if tab == 2 {
                "暂无响应".into()
            } else {
                "暂无请求".into()
            }
        } else {
            String::new()
        };
        ReqDetail {
            well: s(text("method")),
            well_label: true,
            title: s(http_capture::path_of(&url)),
            labels: head,
            menu: true,
            tabs: model(tabs),
            tab,
            sections: model(sections),
            empty: s(empty),
            ..base
        }
    }
}

fn group(items: Vec<(i64, RecordRow)>, offset: i64) -> Vec<(i64, Vec<RecordRow>)> {
    requests::minute_groups(items, |(t, _)| *t)
        .into_iter()
        .map(|(m, rows)| {
            let rows = rows
                .into_iter()
                .map(|(t, mut r)| {
                    r.time = s(requests::full_time(t, offset));
                    r
                })
                .collect();
            (m, rows)
        })
        .collect()
}

/// Where the record `id` starts in `rows`, as the list lays them out.
pub fn row_y(rows: impl IntoIterator<Item = RecordRow>, id: i32) -> Option<f32> {
    let mut y = 0.0;
    for r in rows {
        if !r.header && r.id == id {
            return Some(y);
        }
        y += if r.header { 40.0 } else { 64.0 };
    }
    None
}

fn section(title: &str, rows: Vec<DetailRow>) -> DetailSection {
    DetailSection {
        title: s(title),
        plain: false,
        rows: model(rows),
        footer: SharedString::new(),
    }
}

fn rail(title: String, note: String, trailing: String, mark: i32) -> DetailRow {
    DetailRow {
        kind: s("rail"),
        title: s(title),
        note: s(note),
        trailing: s(trailing),
        mark,
        ..Default::default()
    }
}

fn text_row(title: &str, value: &str, copyable: bool) -> DetailRow {
    DetailRow {
        kind: s("text"),
        title: s(title),
        value: s(value),
        arg: s(if copyable {
            format!("copy:{value}")
        } else {
            String::new()
        }),
        ..Default::default()
    }
}

/// `RelatedConnectionItem`: the app (else the host), then how it failed, or where it went
/// with the node it left by and how long it has been open.
fn conn_link(c: &Conn, now: i64) -> DetailRow {
    let (note, labels) = match &c.failure {
        Some((kind, _)) => (String::new(), labels([(requests::failure_label(kind), 1)])),
        None => {
            let state = if c.is_active() { "活动" } else { "已关闭" };
            (
                requests::endpoint(&c.dest_ip, &c.dest_port),
                labels([
                    (c.chains.first().cloned().unwrap_or_default(), 0),
                    (format!("{state} {}", c.lifetime(now)), 0),
                ]),
            )
        }
    };
    DetailRow {
        kind: s("link"),
        title: s(if c.process.is_empty() {
            &c.host
        } else {
            &c.process
        }),
        note: s(note),
        labels,
        arg: s(format!("conn:{}", c.id)),
        ..Default::default()
    }
}

/// The connection's HTTP requests (`MitmExchangeItem`): the path, then the method with
/// how the request ended.
fn exchange_link(e: &Summary) -> DetailRow {
    DetailRow {
        kind: s("link"),
        title: s(http_capture::path_of(&e.url)),
        note: s(&e.method),
        labels: outcome_labels(
            e.status,
            "",
            &e.error,
            e.pending,
            vec![
                crate::size(e.response_size),
                http_capture::millis_text(e.duration),
                e.mark(),
            ],
        ),
        arg: s(format!("http:{}", e.id)),
        ..Default::default()
    }
}

fn conn_well(c: &Conn) -> String {
    if c.process.is_empty() {
        monogram(&c.title().replace('.', " "))
    } else {
        monogram(&c.process)
    }
}

fn conn_row(c: &Conn, repeats: usize, now: i64) -> RecordRow {
    let mut parts = match &c.failure {
        Some((kind, _)) => vec![(requests::failure_label(kind).to_string(), 1)],
        None => vec![
            (c.chains.last().cloned().unwrap_or_default(), 0),
            (c.lifetime(now), 0),
        ],
    };
    if repeats > 1 {
        parts.push((format!("×{repeats}"), 0));
    }
    if c.mitm {
        parts.push(("MITM".to_string(), 0));
    }
    let active = c.is_active();
    let total = crate::size(c.upload + c.download);
    let (up, down) = c.speeds(now);
    RecordRow {
        id: c.id as i32,
        well: s(conn_well(c)),
        app: s(&c.process),
        title: s(c.name()),
        labels: labels(parts),
        top: s(if active {
            crate::speed(up + down)
        } else {
            total.clone()
        }),
        top_strong: active,
        top_tone: if active { 0 } else { 4 },
        bottom: s(if active { total } else { String::new() }),
        ..Default::default()
    }
}

fn dns_row(q: &Query) -> RecordRow {
    RecordRow {
        id: q.seq as i32,
        well: s(&q.kind),
        well_label: true,
        title: s(&q.domain),
        line: s(if q.is_failed() {
            String::new()
        } else {
            q.summary()
        }),
        labels: labels([
            (if q.is_failed() { q.failure_title() } else { "" }, 1),
            (dns_queries::initiator_label(&q.initiator), 0),
            (if q.cached { "缓存" } else { "" }, 0),
        ]),
        top: s(format!("{} ms", q.delay)),
        top_tone: 4,
        ..Default::default()
    }
}

fn http_row(e: &Summary) -> RecordRow {
    let (status, tone) = e.status_text();
    RecordRow {
        id: e.id as i32,
        well: s(&e.method),
        well_label: true,
        title: s(if e.path.is_empty() { &e.url } else { &e.path }),
        line: s(&e.host),
        labels: labels([(http_capture::millis_text(e.duration), 0), (e.mark(), 0)]),
        top: s(status),
        top_strong: true,
        top_tone: if tone == 0 { 4 } else { tone },
        bottom: s(crate::size(e.response_size)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_connections_open_on_the_active_ones_and_name_an_empty_status() {
        let mut st = ReqState::default();
        st.open_feed(0);
        assert_eq!((st.segment, st.filters[0].status), (CONNECTIONS, 1));
        assert_eq!(st.empty(false).0, "暂无活动连接");
        st.open_feed(1);
        assert_eq!(st.filters[0].status, 0);
        assert_eq!(st.empty(false).0, "暂无连接");
        st.open_feed(2);
        st.filters[2].status = 2;
        assert_eq!(st.empty(false).0, "暂无命中缓存的DNS查询");
        st.query = "a".into();
        assert_eq!(st.empty(false).0, "没有匹配的结果");
    }

    #[test]
    fn the_dashboard_sheet_keeps_its_own_view() {
        let mut st = ReqState {
            segment: HTTP,
            query: "github".into(),
            selected: [Some(4), None, None],
            ..Default::default()
        };
        st.filters[0].status = 1;
        st.open_sheet(DNS);
        assert_eq!(st.segment, DNS);
        assert_eq!(st.query, "");
        assert_eq!(st.selected, [None; 3]);
        assert_eq!(st.filters[0].status, 0);
        st.query = "dns".into();
        st.open_sheet(CONNECTIONS);
        assert_eq!(st.segment, CONNECTIONS);
        assert_eq!(st.query, "");
        st.close_sheet();
        assert_eq!(st.segment, HTTP);
        assert_eq!(st.query, "github");
        assert_eq!(st.selected, [Some(4), None, None]);
        assert_eq!(st.filters[0].status, 1);
        st.close_sheet();
        assert_eq!(st.query, "github");
    }
    use serde_json::json;

    fn cx<'a>(groups: &'a Groups, delays: &'a HashMap<String, i32>) -> Ctx<'a> {
        Ctx {
            now: 200_000,
            offset: 0,
            capturing: false,
            groups,
            delays,
            mitm_hosts: None,
        }
    }

    fn conn(id: i64, host: &str) -> Value {
        json!({"id": id, "start": 60_000 * id, "network": "tcp", "destinationIp": "1.2.3.4", "destinationPort": 443,
               "host": host, "process": "chrome", "chains": ["香港 01", "节点选择"], "rule": "MATCH"})
    }

    #[test]
    fn rows_group_by_minute_newest_first() {
        let mut st = ReqState::default();
        st.conns.take(
            &json!({"connections": [conn(1, "a.com"), conn(2, "b.com"), conn(2, "b.com")]}),
            150_000,
        );
        st.selected[0] = Some(2);
        let groups = Groups::parse(&json!({}));
        let delays = HashMap::new();
        let rows = st.rows(&cx(&groups, &delays));
        let titles: Vec<String> = rows
            .iter()
            .map(|r| {
                if r.header {
                    format!("#{}", r.label)
                } else {
                    r.title.to_string()
                }
            })
            .collect();
        assert_eq!(titles, vec!["#00:02", "b.com", "#00:01", "a.com"]);
        assert!(rows[1].first && rows[1].last && rows[1].selected && !rows[3].selected);
        assert_eq!(rows[1].well, "C");
        assert_eq!(monogram("com.google.android.youtube"), "Y");
        assert_eq!(st.status_counts()[1], "2");
        assert_eq!(st.status_actions()[1].label, "活动");
        assert_eq!(rows[1].time, rows[0].time);
        assert!(rows[1].time.starts_with("1970-01-01 00:02:"));
        assert_eq!(row_y(rows.iter().cloned(), 1), Some(40.0 + 64.0 + 40.0));
        assert_eq!(row_y(rows.iter().cloned(), 9), None);
        assert_eq!(rows[1].line, "");
        assert_eq!(texts(&rows[1].labels), ["节点选择", "1:20"]);
    }

    fn texts(labels: &ModelRc<RecordLabel>) -> Vec<String> {
        labels.iter().map(|l| l.text.to_string()).collect()
    }

    fn tones(labels: &ModelRc<RecordLabel>) -> Vec<i32> {
        labels.iter().map(|l| l.tone).collect()
    }

    #[test]
    fn record_lines_carry_their_labels() {
        let q = |rcode: &str, cached: bool| {
            Query::parse(&json!({"id": 1, "domain": "a.com", "type": "A", "initiator": "app",
                "cached": cached, "answers": ["1.2.3.4"], "rcode": rcode, "delay": 12, "time": 1000}))
            .unwrap()
        };
        let ok = dns_row(&q("NOERROR", true));
        assert_eq!(ok.line, "1.2.3.4");
        assert_eq!(texts(&ok.labels), ["应用", "缓存"]);
        let failed = q("NXDOMAIN", false);
        let bad = dns_row(&failed);
        assert_eq!(bad.line, "");
        assert_eq!(texts(&bad.labels), [failed.failure_title(), "应用"]);
        assert_eq!(tones(&bad.labels), [1, 0]);

        let mut c = Conn::parse(&conn(1, "a.com")).unwrap();
        c.mitm = true;
        let row = conn_row(&c, 3, 90_000);
        assert_eq!(texts(&row.labels), ["节点选择", "0:30", "×3", "MITM"]);
        let link = conn_link(&c, 90_000);
        assert_eq!(
            (link.title.as_str(), link.note.as_str()),
            ("chrome", "1.2.3.4:443")
        );
        assert_eq!(texts(&link.labels), ["香港 01", "活动 0:30"]);

        assert_eq!(
            texts(&outcome_labels(
                404,
                "Not Found",
                "",
                false,
                vec!["1 KB".into()]
            )),
            ["404 Not Found", "1 KB"]
        );
        assert_eq!(tones(&outcome_labels(404, "", "", false, Vec::new())), [1]);
        assert_eq!(
            texts(&outcome_labels(0, "", "", true, vec!["1 KB".into()])),
            ["进行中"]
        );
        assert_eq!(
            tones(&outcome_labels(0, "", "reset", false, vec!["1 KB".into()])),
            [1]
        );
    }

    #[test]
    fn row_menus_filter_and_copy() {
        let mut st = ReqState::default();
        st.conns
            .take(&json!({"connections": [conn(1, "a.com")]}), 70_000);
        let ids: Vec<String> = st
            .row_actions(0, 1)
            .iter()
            .map(|a| a.id.to_string())
            .collect();
        assert_eq!(
            ids,
            vec!["close:1", "copy-host:1", "only-app:1", "hide-host:1"]
        );
        assert_eq!(st.copy_text("copy-host", 1).as_deref(), Some("a.com"));
        st.filter_by("hide-host", 1);
        let groups = Groups::parse(&json!({}));
        let delays = HashMap::new();
        assert!(st.rows(&cx(&groups, &delays)).is_empty());
        assert_eq!(st.empty(false).0, "没有匹配的结果");
        assert_eq!(st.condition_lines()[0].label, "主机");
    }

    #[test]
    fn drafts_preview_and_submit() {
        let mut st = ReqState::default();
        st.conns.take(
            &json!({"connections": [conn(1, "a.com"), conn(2, "b.com")]}),
            70_000,
        );
        st.open_draft(None);
        st.draft.as_mut().unwrap().value = "a.c".into();
        let (suggestions, shows, ok) = st.draft_outcome();
        assert!(ok);
        assert_eq!(shows, "将筛出 1 条连接");
        assert_eq!(suggestions[0].value, "a.com");
        assert!(st.submit_draft());
        st.open_draft(Some(0));
        st.draft.as_mut().unwrap().operator = 1;
        assert!(st.submit_draft());
        assert!(st.filters[0].conditions[0].excludes);
    }

    #[test]
    fn details_read_like_flclash() {
        let mut st = ReqState::default();
        st.conns
            .take(&json!({"connections": [conn(1, "a.com")]}), 70_000);
        st.selected[0] = Some(1);
        let groups = Groups::parse(
            &json!({"groups": [{"name": "节点选择", "type": "Selector"}], "nodes": [{"name": "香港 01", "type": "Trojan"}]}),
        );
        let delays = HashMap::from([("香港 01".to_string(), 88)]);
        let d = st.detail(&cx(&groups, &delays));
        assert!(d.has && d.closable);
        assert_eq!(d.tabs.row_count(), 2);
        st.tab[0] = 1;
        let d = st.detail(&cx(&groups, &delays));
        let path = d.sections.row_data(0).unwrap();
        assert_eq!(path.title, "链路");
        let hop = path.rows.row_data(path.rows.row_count() - 1).unwrap();
        assert_eq!(
            (hop.title.as_str(), hop.note.as_str(), hop.trailing.as_str()),
            ("香港 01", "节点", "88 ms")
        );
        assert_eq!(texts(&hop.labels), ["Trojan"]);
        assert!(!(0..d.sections.row_count())
            .any(|i| d.sections.row_data(i).unwrap().title == "HTTPS 解密"));
        let hosts = vec!["a.com".to_string()];
        let mitm = Ctx {
            mitm_hosts: Some(&hosts),
            ..cx(&groups, &delays)
        };
        let d = st.detail(&mitm);
        let decrypt = d.sections.row_data(2).unwrap();
        assert_eq!(decrypt.title, "HTTPS 解密");
        let row = decrypt.rows.row_data(0).unwrap();
        assert_eq!(
            (row.title.as_str(), row.arg.as_str()),
            ("排除解密", "mitm-host:true:a.com")
        );
        st.segment = DNS;
        assert_eq!(st.detail(&cx(&groups, &delays)).empty, "选择一次查询");
        assert_eq!(grouped(1234567), "1,234,567");
    }

    use slint::Model;
}
