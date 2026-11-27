//! The editor's completion: keys and values by the YAML path at the caret against
//! `editor_schema`, names the document defines, then the document's own words
//! (FlClash: lib/features/editor/clash_completion.dart, yaml_lines.dart,
//! completion_matcher.dart; code_forge's local suggestions). Offsets are UTF-8 bytes.

use crate::editor_schema::{self as schema, Kind, Node, Scalar, NODES};
use std::collections::HashSet;

const MAX_SUGGESTIONS: usize = 60;
const MAX_WORDS: usize = 5_000;
const INDENT_UNIT: &str = "  ";
const TARGETS: [&str; 6] = [
    "DIRECT",
    "REJECT",
    "REJECT-DROP",
    "PASS",
    "PASS-RULE",
    "COMPATIBLE",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub label: String,
    pub detail: String,
    /// Inserted in place of the label as the text before and after where the caret
    /// lands; a newline keeps the line's indent and a tab is one indent step.
    pub snippet: Option<(String, String)>,
}

impl Suggestion {
    fn plain(label: &str) -> Self {
        Suggestion {
            label: label.to_string(),
            detail: String::new(),
            snippet: None,
        }
    }

    fn with_detail(label: &str, detail: &str) -> Self {
        Suggestion {
            detail: detail.to_string(),
            ..Suggestion::plain(label)
        }
    }
}

/// Accepting a suggestion replaces `prefix`, the text before the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub prefix: String,
    pub items: Vec<Suggestion>,
}

/// What an editor completes against: a schema's node, or only the document's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Words,
    Yaml(u16),
}

/// `EditorSchema` by the name an editor page gives it; anything else completes words.
pub fn source_of(name: &str) -> Source {
    match name {
        "config" => Source::Yaml(schema::CONFIG),
        "dns" => Source::Yaml(schema::DNS),
        "ntp" => Source::Yaml(schema::NTP),
        "sniffer" => Source::Yaml(schema::SNIFFER),
        "proxy" => Source::Yaml(schema::PROXY),
        "proxy-group" => Source::Yaml(schema::PROXY_GROUP),
        "rules" => Source::Yaml(schema::RULES),
        "share-links" => Source::Yaml(schema::SHARE_LINKS),
        "provider" => Source::Yaml(schema::PROVIDER),
        _ => Source::Words,
    }
}

fn is_word_char(c: char) -> bool {
    matches!(c,
        '0'..='9' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{0590}'..='\u{06FF}' | '\u{08A0}'..='\u{08FF}'
        | '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}')
}

/// The document's words, at most `MAX_WORDS`, those starting with a digit left out.
pub fn words(text: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut start = None;
    let ends = text.char_indices().map(Some).chain(std::iter::once(None));
    for item in ends {
        match item {
            Some((i, c)) if is_word_char(c) => {
                start.get_or_insert(i);
            }
            other => {
                if let Some(s) = start.take() {
                    let end = other.map_or(text.len(), |(i, _)| i);
                    let word = &text[s..end];
                    if out.len() < MAX_WORDS && !word.starts_with(|c: char| c.is_ascii_digit()) {
                        out.insert(word.to_string());
                    }
                }
            }
        }
    }
    out
}

fn sorted_words(words: &HashSet<String>) -> Vec<&str> {
    let mut all: Vec<&str> = words.iter().map(String::as_str).collect();
    all.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    all
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn starts_segment(c: &[char], i: usize) -> bool {
    let previous = c[i - 1];
    matches!(previous, '-' | '_' | '.') || (previous.is_lowercase() && c[i].is_uppercase())
}

/// A prefix beats a segment prefix, which beats the letters in order.
pub fn score(candidate: &str, typed: &str) -> Option<i32> {
    if typed.is_empty() {
        return Some(0);
    }
    let c: Vec<char> = candidate.chars().collect();
    let lc: Vec<char> = c.iter().map(|&x| lower(x)).collect();
    let lt: Vec<char> = typed.chars().map(lower).collect();
    let len = c.len() as i32;
    let at = |i: usize| lc.len() >= i + lt.len() && lc[i..i + lt.len()] == lt[..];
    if at(0) {
        return Some(3000 - len + if candidate.starts_with(typed) { 100 } else { 0 });
    }
    if let Some(i) = (1..c.len()).find(|&i| starts_segment(&c, i) && at(i)) {
        return Some(2000 - i as i32 - len);
    }
    if lc.first() != lt.first() {
        return None;
    }
    let mut matched = 1;
    for &x in &lc[1..] {
        if matched == lt.len() {
            break;
        }
        if x == lt[matched] {
            matched += 1;
        }
    }
    (matched == lt.len()).then_some(1000 - len)
}

pub fn rank(typed: &str, candidates: impl IntoIterator<Item = Suggestion>) -> Vec<Suggestion> {
    let mut seen = HashSet::new();
    let mut scored = Vec::new();
    for s in candidates {
        if !seen.insert(s.label.clone()) || (s.label == typed && s.snippet.is_none()) {
            continue;
        }
        if let Some(n) = score(&s.label, typed) {
            scored.push((n, s));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, s)| s)
        .collect()
}

fn complete_from(
    typed: &str,
    candidates: impl IntoIterator<Item = Suggestion>,
) -> Option<Completion> {
    if typed.is_empty() {
        return None;
    }
    Some(Completion {
        prefix: typed.to_string(),
        items: rank(typed, candidates),
    })
}

fn plain<'a>(labels: impl IntoIterator<Item = &'a str>) -> Vec<Suggestion> {
    labels.into_iter().map(Suggestion::plain).collect()
}

/// The line split as `^( *)(-(?: +|$))?(.*)$`: its indent, the item's dash with its
/// spaces, and the rest.
fn split_line(text: &str) -> (usize, Option<&str>, &str) {
    let indent = text.len() - text.trim_start_matches(' ').len();
    let rest = &text[indent..];
    if let Some(after) = rest.strip_prefix('-') {
        let spaces = after.len() - after.trim_start_matches(' ').len();
        if after.is_empty() || spaces > 0 {
            return (indent, Some(&rest[..1 + spaces]), &after[spaces..]);
        }
    }
    (indent, None, rest)
}

fn indent_of(text: &str) -> usize {
    text.len() - text.trim_start_matches(' ').len()
}

fn is_item_at(text: &str, indent: usize) -> bool {
    let b = text.as_bytes();
    indent < b.len() && b[indent] == b'-' && (indent + 1 == b.len() || b[indent + 1] == b' ')
}

/// `unquoteYaml`: trimmed, a trailing comment dropped, one pair of quotes taken off.
pub fn unquote(text: &str) -> String {
    let mut value = text.trim();
    let comment = if value.starts_with('#') {
        Some(0)
    } else {
        value.find(" #")
    };
    if let Some(at) = comment {
        value = value[..at].trim_end();
    }
    let quoted = |q: char| value.len() >= 2 && value.starts_with(q) && value.ends_with(q);
    if quoted('\'') || quoted('"') {
        return value[1..value.len() - 1].to_string();
    }
    value.to_string()
}

/// The key a line's content opens and what follows its colon. A quoted key ends at its
/// quote; a bare one at the first colon followed by `space` (whitespace, or also the end
/// unless `space_required`), and never takes a `#`.
fn split_key(content: &str, dash_needs_more: bool, space_required: bool) -> Option<(&str, &str)> {
    let colon_ok = |at: usize| {
        content.as_bytes().get(at) == Some(&b':')
            && match content[at + 1..].chars().next() {
                None => !space_required,
                Some(c) => c.is_whitespace(),
            }
    };
    let first = content.chars().next()?;
    let colon = if first == '\'' || first == '"' {
        let close = content[1..].find(first)? + 1;
        if !colon_ok(close + 1) {
            return None;
        }
        close + 1
    } else {
        if first.is_whitespace() || matches!(first, '#' | '{' | '[') {
            return None;
        }
        let mut from = first.len_utf8();
        if first == '-' && dash_needs_more {
            let second = content[1..].chars().next()?;
            if second.is_whitespace() || second == '#' {
                return None;
            }
            from += second.len_utf8();
        }
        let mut found = None;
        for (i, c) in content[from..].char_indices() {
            if c == '#' {
                return None;
            }
            if colon_ok(from + i) {
                found = Some(from + i);
                break;
            }
        }
        found?
    };
    Some((&content[..colon], content[colon + 1..].trim_start()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct YamlLine<'a> {
    indent: usize,
    is_item: bool,
    content_column: usize,
    content: &'a str,
    key: Option<String>,
    value: Option<String>,
}

impl<'a> YamlLine<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let (indent, dash, content) = split_line(text);
        if dash.is_none() && (content.is_empty() || content.starts_with('#')) {
            return None;
        }
        let first_quote = matches!(content.chars().next(), Some('\'' | '"'));
        let key = if first_quote || !content.starts_with('-') {
            split_key(content, false, false)
        } else {
            split_key(content, true, false)
        };
        Some(YamlLine {
            indent,
            is_item: dash.is_some(),
            content_column: indent + dash.map_or(0, str::len),
            content,
            key: key.map(|(k, _)| unquote(k)),
            value: key.map(|(_, v)| unquote(v)),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Key(String),
    Item,
}

struct Scope {
    path: Vec<Step>,
    siblings: Vec<(String, String)>,
}

fn put(list: &mut Vec<(String, String)>, key: String, value: String) {
    if !list.iter().any(|(k, _)| *k == key) {
        list.push((key, value));
    }
}

struct Lines<'a>(Vec<&'a str>);

impl<'a> Lines<'a> {
    fn within(&self, index: usize, max_indent: usize) -> Option<YamlLine<'a>> {
        let text = self.0[index];
        (indent_of(text) <= max_indent)
            .then(|| YamlLine::parse(text))
            .flatten()
    }

    /// The path to the block mapping holding `column` on `line`, and the keys it
    /// already has on other lines; None when an ancestor is not a bare key.
    fn scope(
        &self,
        line: usize,
        column: usize,
        starts_item: bool,
        item_indent: usize,
    ) -> Option<Scope> {
        let mut path = Vec::new();
        let mut siblings = Vec::new();
        self.following_siblings(line, column, &mut siblings);
        let mut index = line as isize - 1;
        let mut column = column;
        let mut collecting = true;
        if starts_item {
            path.push(Step::Item);
            collecting = false;
            (index, column) = self.list_owner(index, item_indent, &mut path)?;
        }
        while index >= 0 && (column > 0 || collecting) {
            let current = self.within(index as usize, column);
            index -= 1;
            let Some(current) = current else { continue };
            if current.content_column > column {
                continue;
            }
            if current.content_column == column {
                if collecting {
                    if let Some(key) = &current.key {
                        put(
                            &mut siblings,
                            key.clone(),
                            current.value.clone().unwrap_or_default(),
                        );
                    }
                }
                if !current.is_item {
                    continue;
                }
                path.push(Step::Item);
                collecting = false;
                (index, column) = self.list_owner(index, current.indent, &mut path)?;
                continue;
            }
            let key = current.key.clone()?;
            if current.value.as_deref().is_some_and(|v| !v.is_empty()) {
                return None;
            }
            path.push(Step::Key(key));
            collecting = false;
            if current.is_item {
                path.push(Step::Item);
                (index, column) = self.list_owner(index, current.indent, &mut path)?;
            } else {
                column = current.indent;
            }
        }
        path.reverse();
        Some(Scope { path, siblings })
    }

    fn list_owner(
        &self,
        from: isize,
        dash_column: usize,
        path: &mut Vec<Step>,
    ) -> Option<(isize, usize)> {
        let mut index = from;
        while index >= 0 {
            let text = self.0[index as usize];
            index -= 1;
            let indent = indent_of(text);
            if indent > dash_column || (indent == dash_column && is_item_at(text, indent)) {
                continue;
            }
            let Some(current) = YamlLine::parse(text) else {
                continue;
            };
            let key = current.key.clone()?;
            if current.value.as_deref().is_some_and(|v| !v.is_empty()) {
                return None;
            }
            path.push(Step::Key(key));
            if current.is_item {
                path.push(Step::Item);
                return self.list_owner(index, current.indent, path);
            }
            return Some((index, current.indent));
        }
        Some((-1, 0))
    }

    fn following_siblings(&self, line: usize, column: usize, siblings: &mut Vec<(String, String)>) {
        for index in line + 1..self.0.len() {
            let Some(current) = self.within(index, column) else {
                continue;
            };
            if current.content_column > column {
                continue;
            }
            if current.content_column < column || current.is_item {
                return;
            }
            if let Some(key) = current.key {
                put(siblings, key, current.value.unwrap_or_default());
            }
        }
    }
}

/// The names rules and groups refer to, read from the document's top-level sections.
#[derive(Default)]
struct Names {
    groups: Vec<(String, String)>,
    proxies: Vec<(String, String)>,
    proxy_providers: Vec<String>,
    rule_providers: Vec<String>,
    sub_rules: Vec<String>,
}

struct Item {
    column: usize,
    name: Option<String>,
    kind: Option<String>,
}

/// `name` and `type` of a flow mapping such as `{name: a, type: ss}`.
fn read_flow(content: &str, item: &mut Item) {
    let b = content.as_bytes();
    let mut pos = 0;
    while pos <= b.len() {
        let mut tries = Vec::with_capacity(2);
        if pos == 0 {
            tries.push(0);
        }
        if pos < b.len() && (b[pos] == b'{' || b[pos] == b',') {
            tries.push(pos + 1);
        }
        let found = tries.into_iter().find_map(|at| flow_field(content, at));
        match found {
            Some((field, value, end)) => {
                if field == "name" {
                    item.name = Some(unquote(value));
                } else {
                    item.kind = Some(unquote(value));
                }
                pos = end.max(pos + 1);
            }
            None => pos += 1,
        }
    }
}

fn flow_field(content: &str, at: usize) -> Option<(&'static str, &str, usize)> {
    let rest = &content[at..];
    let after_ws = rest.trim_start();
    let field = ["name", "type"]
        .into_iter()
        .find(|f| after_ws.starts_with(f))?;
    let after_field = after_ws[field.len()..].trim_start();
    let after_colon = after_field.strip_prefix(':')?.trim_start();
    let start = content.len() - after_colon.len();
    let len = match after_colon.chars().next() {
        Some(q @ ('\'' | '"')) => match after_colon[1..].find(q) {
            Some(close) => close + 2,
            None => after_colon.find([',', '}']).unwrap_or(after_colon.len()),
        },
        _ => after_colon.find([',', '}']).unwrap_or(after_colon.len()),
    };
    Some((field, &content[start..start + len], start + len))
}

impl Names {
    fn read(lines: &[&str]) -> Names {
        let mut names = Names::default();
        let mut section: Option<String> = None;
        let mut child_indent: Option<usize> = None;
        let mut item: Option<Item> = None;
        fn flush(names: &mut Names, section: &Option<String>, item: &mut Option<Item>) {
            if let Some(Item {
                name: Some(name),
                kind,
                ..
            }) = item.take()
            {
                if !name.is_empty() {
                    let target = if section.as_deref() == Some("proxy-groups") {
                        &mut names.groups
                    } else {
                        &mut names.proxies
                    };
                    put(target, name, kind.unwrap_or_default());
                }
            }
        }
        for text in lines {
            let Some(line) = YamlLine::parse(text) else {
                continue;
            };
            if line.indent == 0 && !line.is_item {
                flush(&mut names, &section, &mut item);
                section = line.key.clone();
                child_indent = None;
                continue;
            }
            match section.as_deref() {
                Some("proxies" | "proxy-groups") => {
                    if line.is_item && line.indent == *child_indent.get_or_insert(line.indent) {
                        flush(&mut names, &section, &mut item);
                        let mut fresh = Item {
                            column: line.content_column,
                            name: None,
                            kind: None,
                        };
                        if line.content.starts_with('{') {
                            read_flow(line.content, &mut fresh);
                            item = Some(fresh);
                            flush(&mut names, &section, &mut item);
                            continue;
                        }
                        item = Some(fresh);
                    }
                    if let Some(current) = item.as_mut().filter(|i| i.column == line.content_column)
                    {
                        match line.key.as_deref() {
                            Some("name") => current.name = line.value.clone(),
                            Some("type") => current.kind = line.value.clone(),
                            _ => {}
                        }
                    }
                }
                Some(s @ ("proxy-providers" | "rule-providers" | "sub-rules")) => {
                    if !line.is_item
                        && line.key.is_some()
                        && line.indent == *child_indent.get_or_insert(line.indent)
                    {
                        let list = match s {
                            "proxy-providers" => &mut names.proxy_providers,
                            "rule-providers" => &mut names.rule_providers,
                            _ => &mut names.sub_rules,
                        };
                        list.push(line.key.clone().unwrap_or_default());
                    }
                }
                _ => {}
            }
        }
        flush(&mut names, &section, &mut item);
        names
    }

    fn policies(&self, with_global: bool) -> Vec<Suggestion> {
        let mut out: Vec<Suggestion> = self
            .groups
            .iter()
            .chain(&self.proxies)
            .map(|(name, kind)| Suggestion::with_detail(name, kind))
            .collect();
        let global = with_global.then_some("GLOBAL");
        for target in TARGETS.into_iter().chain(global) {
            if !self.groups.iter().any(|(g, _)| g == target) {
                out.push(Suggestion::plain(target));
            }
        }
        out
    }
}

fn node(id: u16) -> &'static Node {
    &NODES[id as usize]
}

/// The fields of a map whose `type` is `kind`, every variant's when it is unknown.
fn fields_for(n: &Node, kind: Option<&str>) -> Vec<(&'static str, u16)> {
    let mut out: Vec<(&'static str, u16)> = n.fields.to_vec();
    let variant = n.variants.iter().find(|(t, _)| Some(*t) == kind);
    let extra: Vec<schema::Fields> = match variant {
        Some((_, f)) => vec![*f],
        None => n.variants.iter().map(|(_, f)| *f).collect(),
    };
    for (key, id) in extra.into_iter().flatten() {
        match out.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = *id,
            None => out.push((*key, *id)),
        }
    }
    out
}

fn hint(n: &Node) -> String {
    match n.kind {
        Kind::Map => "{…}".into(),
        Kind::List => "[…]".into(),
        Kind::Scalar if n.values.len() > 3 => format!("{} | …", n.values[..3].join(" | ")),
        Kind::Scalar => n.values.join(" | "),
    }
}

fn resolve(root: u16, path: &[Step]) -> Option<u16> {
    let mut id = root;
    for step in path {
        let n = node(id);
        id = match (n.kind, step) {
            (Kind::Map, Step::Key(key)) => fields_for(n, None)
                .into_iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
                .or(n.entry)?,
            (Kind::List, Step::Item) => n.entry?,
            _ => return None,
        };
    }
    Some(id)
}

fn key_typed(typed: &str) -> bool {
    typed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// The text before the caret that `[\w-]*$` matches.
fn word_before(before: &str) -> &str {
    let keep = before
        .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        .len();
    &before[keep..]
}

struct Request<'a> {
    text: &'a str,
    lines: Vec<&'a str>,
    line: usize,
    line_text: &'a str,
    column: usize,
    names: Option<Names>,
}

impl Request<'_> {
    fn names(&mut self) -> &Names {
        let lines = &self.lines;
        self.names.get_or_insert_with(|| Names::read(lines))
    }

    fn words(&self) -> Vec<Suggestion> {
        plain(sorted_words(&words(self.text)))
    }
}

fn yaml(root: u16, r: &mut Request) -> Option<Completion> {
    let line_text: &str = r.line_text;
    let before = &line_text[..r.column];
    if before.contains(" #") || before.trim_start().starts_with('#') {
        return None;
    }
    let (indent, dash, typed) = split_line(before);
    let starts_item = dash.is_some();
    let column = indent + dash.map_or(0, str::len);
    let scope = Lines(r.lines.clone()).scope(r.line, column, starts_item, indent);
    let found = scope.as_ref().and_then(|s| resolve(root, &s.path));
    let (Some(scope), Some(id)) = (scope, found) else {
        return words_at(r, before);
    };
    let n = node(id);
    let rest = &line_text[r.column..];
    if n.kind == Kind::Scalar {
        return scalar_item(r, n, typed);
    }
    if n.kind != Kind::Map {
        return None;
    }
    let kind = scope
        .siblings
        .iter()
        .find(|(k, _)| k == "type")
        .map(|(_, v)| v.as_str());
    let fields = fields_for(n, kind);
    if let Some((key, value_typed)) = split_key(typed, false, true) {
        let key = unquote(key);
        let value = fields
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .or(n.entry);
        return match value {
            Some(v) => value_of(r, node(v), value_typed),
            None => words_at(r, before),
        };
    }
    if typed.is_empty() || !key_typed(typed) {
        return None;
    }
    if fields.is_empty() {
        return words_at(r, before);
    }
    let extra = if starts_item {
        " ".repeat(column - indent)
    } else {
        String::new()
    };
    let at_end = rest.trim().is_empty();
    let items = fields
        .iter()
        .filter(|(k, _)| !scope.siblings.iter().any(|(s, _)| s == k))
        .map(|&(k, v)| key_suggestion(k, node(v), at_end, &extra));
    Some(Completion {
        prefix: typed.to_string(),
        items: rank(typed, items.collect::<Vec<_>>()),
    })
}

fn key_suggestion(key: &str, value: &Node, at_end: bool, extra: &str) -> Suggestion {
    let body = match value.kind {
        Kind::Scalar => (format!("{key}: "), String::new()),
        Kind::Map => (format!("{key}:\n{extra}\t"), String::new()),
        Kind::List => (format!("{key}:\n{extra}\t- "), String::new()),
    };
    Suggestion {
        label: key.to_string(),
        detail: hint(value),
        snippet: at_end.then_some(body),
    }
}

fn value_of(r: &mut Request, n: &Node, typed: &str) -> Option<Completion> {
    if typed.starts_with('[') {
        let entry = n.entry.filter(|_| n.kind == Kind::List)?;
        let cut = typed.rfind(['[', ',']).map_or(0, |i| i + 1);
        return scalar(r, node(entry), typed[cut..].trim_start());
    }
    if n.kind != Kind::Scalar {
        return None;
    }
    let text = typed.strip_prefix(['\'', '"']).unwrap_or(typed);
    scalar(r, n, text)
}

fn scalar_item(r: &mut Request, n: &Node, typed: &str) -> Option<Completion> {
    match n.scalar {
        Scalar::Rule => rule(r, typed, true),
        Scalar::RulePayload => rule(r, typed, false),
        _ => scalar(r, n, typed),
    }
}

fn scalar(r: &mut Request, n: &Node, typed: &str) -> Option<Completion> {
    if typed.is_empty() {
        return None;
    }
    let candidates = match n.scalar {
        Scalar::Policy => r.names().policies(false),
        Scalar::Outbound => r.names().policies(true),
        Scalar::ProxyProvider => plain(r.names().proxy_providers.iter().map(String::as_str)),
        Scalar::RuleProvider => plain(r.names().rule_providers.iter().map(String::as_str)),
        Scalar::SubRule => plain(r.names().sub_rules.iter().map(String::as_str)),
        _ => {
            let mut all = plain(n.values.iter().copied());
            if n.values.is_empty() {
                all.extend(r.words());
            }
            all
        }
    };
    complete_from(typed, candidates)
}

fn rule(r: &mut Request, typed: &str, with_policy: bool) -> Option<Completion> {
    let kind = typed.split(',').next().unwrap_or("").trim().to_uppercase();
    if typed.contains('(') {
        let depth = typed.matches('(').count() as isize - typed.matches(')').count() as isize;
        if depth > 0 {
            let inner = &typed[typed.rfind('(').map_or(0, |i| i + 1)..];
            return rule_parts(r, inner, false, true);
        }
        let tail = &typed[typed.rfind(')').map_or(0, |i| i + 1)..];
        if !with_policy || !tail.starts_with(',') || tail[1..].contains(',') {
            return None;
        }
        let text = tail[1..].trim_start();
        let candidates = if kind == "SUB-RULE" {
            plain(r.names().sub_rules.iter().map(String::as_str))
        } else {
            r.names().policies(true)
        };
        return complete_from(text, candidates);
    }
    rule_parts(r, typed, with_policy, false)
}

fn rule_parts(r: &mut Request, typed: &str, with_policy: bool, nested: bool) -> Option<Completion> {
    let parts: Vec<&str> = typed.split(',').collect();
    let text = parts.last().copied().unwrap_or("").trim_start();
    let kind = parts[0].trim().to_uppercase();
    let no_resolve = schema::NO_RESOLVE_RULE_TYPES.contains(&kind.as_str());
    match parts.len() {
        1 => {
            let types = schema::RULE_TYPES.iter().filter(|(t, _)| {
                !(nested && *t == "MATCH") && (with_policy || !matches!(*t, "MATCH" | "SUB-RULE"))
            });
            let items: Vec<Suggestion> = types
                .map(|&(t, example)| {
                    let snippet = if matches!(t, "AND" | "OR" | "NOT") {
                        (format!("{t},(("), "))".to_string())
                    } else {
                        (format!("{t},"), String::new())
                    };
                    Suggestion {
                        snippet: Some(snippet),
                        ..Suggestion::with_detail(t, example)
                    }
                })
                .collect();
            complete_from(text, items)
        }
        2 if kind == "MATCH" => {
            if with_policy {
                let candidates = r.names().policies(true);
                complete_from(text, candidates)
            } else {
                None
            }
        }
        2 => {
            let candidates = if kind == "RULE-SET" {
                plain(r.names().rule_providers.iter().map(String::as_str))
            } else {
                let values = schema::RULE_PAYLOAD_VALUES.iter().find(|(t, _)| *t == kind);
                plain(values.map_or(&[][..], |(_, v)| *v).iter().copied())
            };
            complete_from(text, candidates)
        }
        3 if with_policy && !nested => {
            let candidates = r.names().policies(true);
            complete_from(text, candidates)
        }
        3 | 4 if no_resolve => complete_from(text, plain(schema::RULE_OPTIONS.iter().copied())),
        _ => None,
    }
}

fn words_at(r: &Request, before: &str) -> Option<Completion> {
    complete_from(word_before(before), r.words())
}

fn line_at(text: &str, caret: usize) -> (usize, usize, &str) {
    let start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let end = text[caret..].find('\n').map_or(text.len(), |i| caret + i);
    let index = text[..start].matches('\n').count();
    (index, start, &text[start..end])
}

/// code_forge's word suggestions when an editor has no source: words starting with the
/// word before the caret, shortest first.
fn plain_words(text: &str, caret: usize) -> Option<Completion> {
    let (_, start, line) = line_at(text, caret);
    let before = &line[..caret - start];
    let keep = before.trim_end_matches(is_word_char).len();
    let prefix = &before[keep..];
    if prefix.is_empty() || prefix.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let all = words(text);
    let items: Vec<Suggestion> = sorted_words(&all)
        .into_iter()
        .filter(|w| *w != prefix && w.starts_with(prefix))
        .take(MAX_SUGGESTIONS)
        .map(Suggestion::plain)
        .collect();
    Some(Completion {
        prefix: prefix.to_string(),
        items,
    })
}

/// The suggestions for the caret at byte `caret` of `text`, None to close the popup.
pub fn complete(source: Source, text: &str, caret: usize) -> Option<Completion> {
    if caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let done = match source {
        Source::Words => plain_words(text, caret),
        Source::Yaml(root) => {
            let (line, start, line_text) = line_at(text, caret);
            let mut r = Request {
                text,
                lines: text
                    .split('\n')
                    .map(|l| l.strip_suffix('\r').unwrap_or(l))
                    .collect(),
                line,
                line_text,
                column: caret - start,
                names: None,
            };
            yaml(root, &mut r)
        }
    };
    done.filter(|c| !c.items.is_empty())
}

/// How wide the popup's widest row is, in monospace cells: a wide character takes two
/// and the smaller detail 0.85 each, two cells apart from the label.
pub fn cells(items: &[Suggestion]) -> f32 {
    let width = |t: &str| -> f32 {
        t.chars()
            .map(|c| if (c as u32) < 0x1100 { 1.0 } else { 2.0 })
            .sum()
    };
    items
        .iter()
        .map(|s| {
            let detail = width(&s.detail);
            width(&s.label)
                + if detail > 0.0 {
                    2.0 + detail * 0.85
                } else {
                    0.0
                }
        })
        .fold(0.0, f32::max)
}

/// The text after accepting `s` for `prefix` before `caret`, and where the caret goes.
pub fn accept(text: &str, caret: usize, prefix: &str, s: &Suggestion) -> (String, usize) {
    let (_, line_start, line) = line_at(text, caret);
    let start = caret.saturating_sub(prefix.len()).max(line_start);
    let (before, after) = match &s.snippet {
        None => (s.label.clone(), String::new()),
        Some((before, after)) => {
            let indent = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
            let expand = |t: &str| {
                t.replace('\n', &format!("\n{indent}"))
                    .replace('\t', INDENT_UNIT)
            };
            (expand(before), expand(after))
        }
    };
    let mut out = String::with_capacity(text.len() + before.len() + after.len());
    out.push_str(&text[..start]);
    out.push_str(&before);
    out.push_str(&after);
    out.push_str(&text[caret..]);
    (out, start + before.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(source: Source, marked: &str) -> Option<Completion> {
        let caret = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        complete(source, &text, caret)
    }

    fn labels(c: Option<Completion>) -> Vec<String> {
        c.map(|c| c.items.into_iter().map(|s| s.label).collect())
            .unwrap_or_default()
    }

    const CONFIG: Source = Source::Yaml(schema::CONFIG);

    #[test]
    fn scores_prefixes_over_segments_over_letters() {
        assert!(score("mode", "mo").unwrap() > score("log-mode", "mo").unwrap());
        assert!(score("log-mode", "mo").unwrap() > score("mxoxde", "mo").unwrap());
        assert_eq!(score("abc", "x"), None);
        assert!(score("skipCert", "ce").is_some());
        assert_eq!(score("anything", ""), Some(0));
    }

    #[test]
    fn splits_keys_as_the_yaml_patterns() {
        assert_eq!(
            split_key("mode: rule", false, false),
            Some(("mode", "rule"))
        );
        assert_eq!(split_key("mode:", false, false), Some(("mode", "")));
        assert_eq!(split_key("mode:", false, true), None);
        assert_eq!(split_key("'a b': c", false, false), Some(("'a b'", "c")));
        assert_eq!(
            split_key("url: http://x", false, false),
            Some(("url", "http://x"))
        );
        assert_eq!(split_key("a#b: c", false, false), None);
        assert_eq!(split_key("-x: 1", true, false), Some(("-x", "1")));
        assert_eq!(unquote(" 'a' # note"), "a");
    }

    #[test]
    fn top_level_keys_complete_with_snippets() {
        let c = at(CONFIG, "mo|").unwrap();
        assert_eq!(c.prefix, "mo");
        assert_eq!(c.items[0].label, "mode");
        let (text, caret) = accept("mo", 2, &c.prefix, &c.items[0]);
        assert_eq!((text.as_str(), caret), ("mode: ", 6));
        let dns = at(CONFIG, "dn|").unwrap();
        let s = dns.items.iter().find(|s| s.label == "dns").unwrap();
        assert_eq!(accept("dn", 2, "dn", s).0, "dns:\n  ");
    }

    #[test]
    fn keys_already_there_are_left_out() {
        assert!(!labels(at(CONFIG, "mode: rule\nmo|")).contains(&"mode".to_string()));
    }

    #[test]
    fn values_follow_the_key() {
        assert_eq!(labels(at(CONFIG, "mode: r|")), ["rule"]);
        assert_eq!(labels(at(CONFIG, "ipv6: t|")), ["true"]);
    }

    #[test]
    fn nested_keys_follow_the_path() {
        let c = labels(at(CONFIG, "dns:\n  enable: true\n  enh|"));
        assert!(c.contains(&"enhanced-mode".to_string()), "{c:?}");
        let fake = labels(at(CONFIG, "dns:\n  enhanced-mode: f|"));
        assert_eq!(fake, ["fake-ip"]);
    }

    #[test]
    fn list_items_take_the_variant_of_their_type() {
        let text = "proxies:\n  - name: a\n    type: ss\n    cip|";
        assert!(labels(at(CONFIG, text)).contains(&"cipher".to_string()));
        let item = at(CONFIG, "proxies:\n  - na|").unwrap();
        let s = item.items.iter().find(|s| s.label == "name").unwrap();
        assert_eq!(
            accept("proxies:\n  - na", 15, "na", s).0,
            "proxies:\n  - name: "
        );
    }

    #[test]
    fn rules_complete_types_and_policies() {
        let text = "proxy-groups:\n  - name: Auto\n    type: url-test\nrules:\n  - DOMAIN-SU|";
        let c = at(CONFIG, text).unwrap();
        assert_eq!(c.items[0].label, "DOMAIN-SUFFIX");
        let (after, _) = accept("DOMAIN-SU", 9, &c.prefix, &c.items[0]);
        assert_eq!(after, "DOMAIN-SUFFIX,");
        let policy = format!("{}\n", text.replace("DOMAIN-SU|", "DOMAIN,a.test,Au|"));
        assert_eq!(labels(at(CONFIG, &policy)), ["Auto"]);
        let and = at(CONFIG, "rules:\n  - AN|").unwrap();
        let (after, caret) = accept("AN", 2, "AN", &and.items[0]);
        assert_eq!((after.as_str(), caret), ("AND,(())", 6));
    }

    #[test]
    fn flow_proxies_name_policies() {
        let text = "proxies:\n  - {name: \"HK 1\", type: ss}\nrules:\n  - MATCH,H|";
        let c = at(CONFIG, text).unwrap();
        assert_eq!(c.items[0].label, "HK 1");
        assert_eq!(c.items[0].detail, "ss");
    }

    #[test]
    fn unknown_places_fall_back_to_words() {
        let text = "x-custom:\n  alpha-beta: 1\n  al|";
        assert_eq!(labels(at(CONFIG, text)), ["alpha"]);
    }

    #[test]
    fn comments_complete_nothing() {
        assert_eq!(at(CONFIG, "# mo|"), None);
        assert_eq!(at(CONFIG, "mode: rule # r|"), None);
    }

    #[test]
    fn plain_words_complete_shortest_first() {
        let c = at(Source::Words, "alphabet alpha alp|");
        assert_eq!(labels(c), ["alpha", "alphabet"]);
        assert_eq!(at(Source::Words, "1abc 1a|"), None);
        assert_eq!(labels(at(Source::Words, "名前 名|")), ["名前"]);
    }

    #[test]
    fn words_skip_digit_starts() {
        let mut w: Vec<String> = words("a1 1abc _x café 名前 8080").into_iter().collect();
        w.sort();
        assert_eq!(w, ["_x", "a1", "caf", "名前"]);
    }

    #[test]
    fn cells_count_wide_characters_twice() {
        let s = |label: &str, detail: &str| Suggestion::with_detail(label, detail);
        assert_eq!(cells(&[s("ab", ""), s("名前", "")]), 4.0);
        assert!((cells(&[s("mode", "a")]) - 6.85).abs() < 1e-4);
        assert_eq!(cells(&[]), 0.0);
    }

    #[test]
    fn sections_map_to_roots() {
        assert_eq!(source_of("dns"), Source::Yaml(schema::DNS));
        assert_eq!(source_of(""), Source::Words);
        let dns = labels(at(source_of("dns"), "enhanced-mode: r|"));
        assert_eq!(dns, ["redir-host"]);
    }
}
