//! A captured body made ready to show: text, indented when it is JSON, Protobuf read without a
//! schema, or the bytes of an image or of anything that does not read as text.
//! FlClash: lib/common/mitm_body.dart, lib/common/mitm_protobuf.dart.

use serde_json::Value;
use std::io::Read;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Text,
    Image,
    Binary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Text,
    Json,
    JavaScript,
    Xml,
}

/// `MitmBody`: one side of a request as `mitm.body` keeps it, decoded unless `encoding` names
/// the coding the Core could not undo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Raw {
    /// Base64.
    pub data: String,
    pub truncated: bool,
    pub encoding: String,
}

impl Raw {
    pub fn parse(v: &Value) -> Raw {
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Raw {
            data: text("data"),
            truncated: v.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            encoding: text("encoding"),
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        base64_decode(&self.data)
    }
}

/// `MitmBodyContent`.
#[derive(Clone, Debug)]
pub struct Content {
    pub kind: Kind,
    pub bytes: Arc<Vec<u8>>,
    pub text: String,
    /// The start of `text` a detail page shows in place.
    pub preview: String,
    pub language: Language,
    pub truncated: bool,
    pub encoding: String,
    pub protobuf: bool,
    /// A charset other than UTF-8 the text is declared in.
    pub charset: String,
}

impl Content {
    fn new(kind: Kind, bytes: &Arc<Vec<u8>>, truncated: bool) -> Content {
        Content {
            kind,
            bytes: bytes.clone(),
            text: String::new(),
            preview: String::new(),
            language: Language::Text,
            truncated,
            encoding: String::new(),
            protobuf: false,
            charset: String::new(),
        }
    }

    pub fn is_cut(&self) -> bool {
        self.preview.len() < self.text.len()
    }

    /// The type a body's header names after its size.
    pub fn type_label(&self, content_type: &str) -> String {
        if self.protobuf {
            return "Protobuf".into();
        }
        match self.kind {
            Kind::Image => "图片".into(),
            Kind::Binary => BINARY.into(),
            Kind::Text => match self.language {
                Language::Json => "JSON".into(),
                Language::JavaScript => "JavaScript".into(),
                Language::Xml if content_type.to_lowercase().contains("html") => "HTML".into(),
                Language::Xml => "XML".into(),
                Language::Text => "文本".into(),
            },
        }
    }

    /// What a body's section says under it.
    pub fn footer(&self) -> String {
        let mut lines = Vec::new();
        if !self.encoding.is_empty() {
            lines.push(undecoded(&self.encoding));
        } else if self.truncated {
            lines.push(TRUNCATED.to_string());
        }
        if self.protobuf {
            lines.push(PROTOBUF_GUESS.to_string());
        }
        lines.join("\n")
    }
}

pub const BINARY: &str = "二进制数据";
pub const TRUNCATED: &str = "只记录了这段内容的开头部分。";
pub const NOT_KEPT: &str = "内存紧张时未记录这段内容。";
const PROTOBUF_GUESS: &str = "按 Protobuf 解析，没有 .proto：只有字段编号，类型是推测的。";

pub fn undecoded(encoding: &str) -> String {
    format!("无法解码 {encoding}，显示的是传输时的原始数据。")
}

const PREVIEW_LINES: usize = 30;
const PREVIEW_CHARS: usize = 3000;
const MAX_CONTROL_SHARE: f64 = 0.01;
const MIN_OFFSET_DIGITS: usize = 6;
const CHARSET_SNIFF_BYTES: usize = 1024;
const MAX_DEPTH: usize = 16;
const MAX_HEX_BYTES: usize = 256;
const MAX_GUESSED_FIELD_NUMBER: u64 = 1 << 16;
pub const GRPC_INFLATE_LIMIT: usize = 8 << 20;

pub fn header_value(header: &[(String, Vec<String>)], name: &str) -> String {
    let lower = name.to_lowercase();
    header
        .iter()
        .find(|(k, v)| k.to_lowercase() == lower && !v.is_empty())
        .map(|(_, v)| v[0].clone())
        .unwrap_or_default()
}

/// `readMitmBody`; runs off the UI thread, a body is up to the Core's size limit.
pub fn read(raw: &Raw, content_type: &str) -> Content {
    let bytes = Arc::new(raw.bytes());
    let truncated = raw.truncated;
    let mime = mime_of(content_type);
    let binary = || {
        let mut c = Content::new(Kind::Binary, &bytes, truncated);
        c.encoding = raw.encoding.clone();
        c
    };
    if !raw.encoding.is_empty() {
        return binary();
    }
    if mime.starts_with("image/") && !mime.contains("svg") {
        return Content::new(Kind::Image, &bytes, truncated);
    }
    let parsed = |text: Option<String>| {
        text.map(|text| {
            let mut c = Content::new(Kind::Text, &bytes, truncated);
            c.preview = preview_of(&text);
            c.text = text;
            c.protobuf = true;
            c
        })
    };
    if mime.starts_with("application/grpc") {
        if let Some(c) = parsed(grpc_text(&bytes, truncated)) {
            return c;
        }
    } else if mime.contains("proto") {
        if let Some(c) = parsed(protobuf_text(&bytes, truncated, false)) {
            return c;
        }
    }
    let charset = declared_charset(&bytes, content_type, &mime);
    let text = utf8_text(&bytes, truncated).and_then(readable_text);
    let Some(text) = text else {
        if !charset.is_empty() {
            let mut c = Content::new(Kind::Binary, &bytes, truncated);
            c.charset = charset;
            return c;
        }
        let guessed = if !truncated && may_be_protobuf(&mime) {
            parsed(protobuf_text(&bytes, false, true))
        } else {
            None
        };
        return guessed.unwrap_or_else(binary);
    };
    let language = language_of(&mime, &text);
    let shown = if language == Language::Json {
        indented(&text)
    } else {
        text
    };
    let mut c = Content::new(Kind::Text, &bytes, truncated);
    c.preview = preview_of(&shown);
    c.text = shown;
    c.language = language;
    c.charset = charset;
    c
}

fn mime_of(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

fn may_be_protobuf(mime: &str) -> bool {
    mime.is_empty() || mime == "application/octet-stream"
}

/// The value after `key` and `=` in `text` from `at`: word characters, `.`, `:` and `-`.
fn charset_value(text: &str, at: usize) -> Option<String> {
    let rest = text[at..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix(['"', '\'']).unwrap_or(rest);
    let value: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        .collect();
    (!value.is_empty()).then_some(value)
}

/// The first `<meta …charset=…` or `<?xml …encoding=…`, the last such key inside its tag.
fn markup_charset(head: &str) -> Option<String> {
    let lower = head.to_lowercase();
    let mut best: Option<(usize, String)> = None;
    for (tag, key) in [("<meta", "charset"), ("<?xml", "encoding")] {
        let mut from = 0;
        while let Some(i) = lower[from..].find(tag) {
            let start = from + i;
            let body_start = start + tag.len();
            let end = lower[body_start..]
                .find('>')
                .map_or(lower.len(), |e| body_start + e);
            let found = lower[body_start..end]
                .match_indices(key)
                .filter(|(k, _)| *k > 0)
                .filter_map(|(k, _)| charset_value(&lower, body_start + k + key.len()))
                .last();
            if let Some(value) = found {
                if best.as_ref().is_none_or(|(b, _)| start < *b) {
                    best = Some((start, value));
                }
                break;
            }
            from = body_start;
        }
    }
    best.map(|(_, v)| v)
}

/// Empty for UTF-8. Pages often declare it only in their markup, which HTML keeps to the
/// first 1024 bytes.
fn declared_charset(bytes: &[u8], content_type: &str, mime: &str) -> String {
    let mut found = content_type
        .match_indices("charset")
        .find_map(|(i, k)| charset_value(content_type, i + k.len()));
    if found.is_none() && (mime.is_empty() || mime.contains("ml")) {
        let head: String = bytes
            .iter()
            .take(CHARSET_SNIFF_BYTES)
            .map(|&b| b as char)
            .collect();
        found = markup_charset(&head);
    }
    let charset = found.unwrap_or_default().to_lowercase();
    if matches!(charset.as_str(), "utf-8" | "utf8" | "us-ascii" | "ascii") {
        String::new()
    } else {
        charset
    }
}

fn is_control(c: char) -> bool {
    (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r')
}

/// A body cut at the limit can end inside a character.
fn utf8_text(bytes: &[u8], truncated: bool) -> Option<String> {
    if truncated {
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        String::from_utf8(bytes.to_vec()).ok()
    }
}

/// `mitmBodyText`: None for bytes that are not UTF-8 or carry control characters.
pub fn body_text(bytes: &[u8], truncated: bool) -> Option<String> {
    utf8_text(bytes, truncated).filter(|t| !t.chars().any(is_control))
}

/// `mitmBodyCopyText`: a body whose text `body_text` would not hand back is copied as Base64.
pub fn copy_text(raw: &Raw) -> String {
    if !raw.encoding.is_empty() {
        return raw.data.clone();
    }
    body_text(&raw.bytes(), raw.truncated).unwrap_or_else(|| raw.data.clone())
}

/// Logs carry stray colour codes; a NUL or a denser run is a binary format that happens to
/// be valid UTF-8.
fn readable_text(text: String) -> Option<String> {
    let mut controls = 0usize;
    let mut count = 0usize;
    for c in text.chars() {
        if c == '\0' {
            return None;
        }
        if is_control(c) {
            controls += 1;
        }
        count += 1;
    }
    if controls == 0 {
        return Some(text);
    }
    if controls as f64 > count as f64 * MAX_CONTROL_SHARE {
        return None;
    }
    Some(
        text.chars()
            .map(|c| if is_control(c) { '\u{fffd}' } else { c })
            .collect(),
    )
}

fn language_of(mime: &str, text: &str) -> Language {
    if mime.contains("json") {
        return Language::Json;
    }
    if mime.contains("javascript") || mime.contains("ecmascript") {
        return Language::JavaScript;
    }
    if mime.contains("xml") || mime.contains("html") {
        return Language::Xml;
    }
    let start = text.trim_start();
    if mime.is_empty() && (start.starts_with('{') || start.starts_with('[')) {
        return Language::Json;
    }
    Language::Text
}

/// JSON indented by two spaces, its tokens kept as written; text that is not JSON as it is.
pub fn indented(text: &str) -> String {
    if serde_json::from_str::<serde::de::IgnoredAny>(text).is_err() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    let newline = |out: &mut String, depth: usize| {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                let mut escaped = false;
                for c in chars.by_ref() {
                    out.push(c);
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        break;
                    }
                }
            }
            '{' | '[' => {
                let close = if c == '{' { '}' } else { ']' };
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
                out.push(c);
                if chars.peek() == Some(&close) {
                    chars.next();
                    out.push(close);
                } else {
                    depth += 1;
                    newline(&mut out, depth);
                }
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
                newline(&mut out, depth);
                out.push(c);
            }
            ',' => {
                out.push(c);
                newline(&mut out, depth);
            }
            ':' => out.push_str(": "),
            c if c.is_whitespace() => {}
            c => out.push(c),
        }
    }
    out
}

fn preview_of(text: &str) -> String {
    let mut lines = 0;
    for (n, (i, c)) in text.char_indices().enumerate() {
        if n == PREVIEW_CHARS {
            return text[..i].to_string();
        }
        if c == '\n' {
            lines += 1;
            if lines == PREVIEW_LINES {
                return text[..i].to_string();
            }
        }
    }
    text.to_string()
}

pub fn hex_offset_digits(length: usize) -> usize {
    MIN_OFFSET_DIGITS.max(format!("{length:x}").len())
}

pub fn hex_line_length(length: usize, per_line: usize) -> usize {
    hex_offset_digits(length) + 2 + per_line * 3 + 1 + per_line
}

/// `mitmHexLine`: the offset, `per_line` bytes in hex, then the same bytes as ASCII.
pub fn hex_line(bytes: &[u8], offset: usize, per_line: usize) -> String {
    let digits = hex_offset_digits(bytes.len());
    let end = (offset + per_line).min(bytes.len());
    let mut hex = String::with_capacity(per_line * 3);
    let mut ascii = String::with_capacity(per_line);
    for i in offset..offset + per_line {
        if i >= end {
            hex.push_str("   ");
            continue;
        }
        let byte = bytes[i];
        hex.push_str(&format!("{byte:02x} "));
        ascii.push(if (0x20..0x7f).contains(&byte) {
            byte as char
        } else {
            '.'
        });
    }
    format!("{offset:0digits$x}  {hex} {ascii}")
}

/// The most bytes a line holds, in fours, whose line fits `columns` characters.
pub fn hex_per_line(length: usize, columns: usize) -> usize {
    let mut per_line = 16;
    while per_line > 4 && hex_line_length(length, per_line) > columns {
        per_line -= 4;
    }
    per_line
}

enum Field {
    Line(String),
    Nested(u64, Vec<Field>),
}

/// `mitmProtobufText`: `bytes` as `protoc --decode_raw` reads them. A field of bytes shows as
/// text first: the same bytes often also parse as a message. `cut` takes a body that stops
/// inside a field up to that field; `guessed` is for a body no header calls Protobuf, and
/// asks for plausible numbers.
pub fn protobuf_text(bytes: &[u8], cut: bool, guessed: bool) -> Option<String> {
    let mut numbers = Vec::new();
    let fields = parse_fields(bytes, 0, cut, &mut numbers)?;
    if fields.is_empty() || (guessed && numbers.iter().any(|n| *n > MAX_GUESSED_FIELD_NUMBER)) {
        return None;
    }
    let mut out = String::new();
    write_fields(&mut out, &fields, "");
    Some(out.trim_end().to_string())
}

fn varint(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for (k, &byte) in bytes.iter().enumerate().skip(at).take(10) {
        let shift = (k - at) * 7;
        if shift < 64 {
            value |= ((byte & 0x7f) as u64) << shift;
        }
        if byte & 0x80 == 0 {
            return Some((value, k + 1));
        }
    }
    None
}

/// `numbers` gathers only the top level's field numbers.
fn parse_fields(bytes: &[u8], depth: usize, cut: bool, numbers: &mut Vec<u64>) -> Option<Vec<Field>> {
    let mut fields = Vec::new();
    let mut offset = 0;
    let end = bytes.len();
    macro_rules! stopped {
        () => {
            return if cut && !fields.is_empty() {
                Some(fields)
            } else {
                None
            }
        };
    }
    while offset < end {
        let Some((tag, after_key)) = varint(bytes, offset) else {
            stopped!()
        };
        let number = tag >> 3;
        if number == 0 || number > 0x1fff_ffff {
            stopped!()
        }
        offset = after_key;
        numbers.push(number);
        match tag & 7 {
            0 => {
                let Some((value, next)) = varint(bytes, offset) else {
                    stopped!()
                };
                offset = next;
                fields.push(Field::Line(format!("{number}: {value}")));
            }
            1 => {
                if offset + 8 > end {
                    stopped!()
                }
                let mut b = [0u8; 8];
                b.copy_from_slice(&bytes[offset..offset + 8]);
                offset += 8;
                let value = u64::from_le_bytes(b);
                fields.push(Field::Line(format!("{number}: 0x{value:016x}")));
            }
            2 => {
                let Some((length, start)) = varint(bytes, offset) else {
                    stopped!()
                };
                if length > (end - start) as u64 {
                    stopped!()
                }
                let stop = start + length as usize;
                offset = stop;
                fields.push(length_delimited(&bytes[start..stop], number, depth));
            }
            5 => {
                if offset + 4 > end {
                    stopped!()
                }
                let mut b = [0u8; 4];
                b.copy_from_slice(&bytes[offset..offset + 4]);
                offset += 4;
                let value = u32::from_le_bytes(b);
                fields.push(Field::Line(format!("{number}: 0x{value:08x}")));
            }
            _ => stopped!(),
        }
    }
    Some(fields)
}

fn length_delimited(body: &[u8], number: u64, depth: usize) -> Field {
    if let Some(text) = std::str::from_utf8(body)
        .ok()
        .filter(|t| !t.chars().any(is_control))
    {
        let quoted = serde_json::to_string(text).unwrap_or_default();
        return Field::Line(format!("{number}: {quoted}"));
    }
    let nested = if depth < MAX_DEPTH {
        parse_fields(body, depth + 1, false, &mut Vec::new())
    } else {
        None
    };
    match nested {
        Some(fields) if !fields.is_empty() => Field::Nested(number, fields),
        _ => Field::Line(format!("{number}: {}", short_hex(body))),
    }
}

fn write_fields(out: &mut String, fields: &[Field], indent: &str) {
    for field in fields {
        match field {
            Field::Line(line) => {
                out.push_str(indent);
                out.push_str(line);
                out.push('\n');
            }
            Field::Nested(number, inner) => {
                out.push_str(&format!("{indent}{number} {{\n"));
                write_fields(out, inner, &format!("{indent}  "));
                out.push_str(&format!("{indent}}}\n"));
            }
        }
    }
}

fn short_hex(bytes: &[u8]) -> String {
    let shown = &bytes[..bytes.len().min(MAX_HEX_BYTES)];
    let hex: String = shown.iter().map(|b| format!("{b:02x}")).collect();
    if bytes.len() > MAX_HEX_BYTES {
        format!("<{hex}… {} bytes>", bytes.len())
    } else {
        format!("<{hex}>")
    }
}

/// `mitmGrpcText`: the messages of a gRPC or gRPC-Web body, each read as Protobuf.
pub fn grpc_text(bytes: &[u8], truncated: bool) -> Option<String> {
    let mut out = String::new();
    let mut inflated = 0usize;
    let mut offset = 0usize;
    let mut index = 0usize;
    while offset + 5 <= bytes.len() {
        let flag = bytes[offset];
        let length = u32::from_be_bytes([
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
        ]) as usize;
        let start = offset + 5;
        let end = start.saturating_add(length);
        if end > bytes.len() {
            if !truncated {
                return None;
            }
            break;
        }
        let frame = &bytes[start..end];
        index += 1;
        out.push_str(&format!("# {index}\n"));
        // gRPC-Web sends its trailers as a last frame of header lines.
        if flag & 0x80 != 0 {
            let lines: String = frame.iter().map(|&b| b as char).collect();
            out.push_str(lines.trim());
            out.push('\n');
        } else {
            let compressed = flag & 1 != 0;
            let (message, over_limit) = if compressed {
                gunzip(frame, GRPC_INFLATE_LIMIT.saturating_sub(inflated))
            } else {
                (Some(frame.to_vec()), false)
            };
            if compressed {
                inflated += message.as_ref().map_or(0, Vec::len);
            }
            if over_limit {
                out.push_str(&format!(
                    "解压后超过 {} MB，显示压缩数据\n",
                    GRPC_INFLATE_LIMIT >> 20
                ));
            }
            let text = message
                .as_deref()
                .and_then(|m| protobuf_text(m, false, false));
            out.push_str(&text.unwrap_or_else(|| short_hex(message.as_deref().unwrap_or(frame))));
            out.push('\n');
        }
        offset = end;
    }
    if index == 0 || (!truncated && offset != bytes.len()) {
        return None;
    }
    Some(out.trim_end().to_string())
}

/// No bytes for a frame that is not compressed data or inflates past `limit`, which the
/// second field tells.
fn gunzip(frame: &[u8], limit: usize) -> (Option<Vec<u8>>, bool) {
    let reader: Box<dyn Read + '_> = if frame.starts_with(&[0x1f, 0x8b]) {
        Box::new(flate2::read::MultiGzDecoder::new(frame))
    } else {
        Box::new(flate2::read::ZlibDecoder::new(frame))
    };
    let mut out = Vec::new();
    match reader.take(limit as u64 + 1).read_to_end(&mut out) {
        Ok(_) if out.len() > limit => (None, true),
        Ok(_) => (Some(out), false),
        Err(_) => (None, false),
    }
}

pub fn base64_encode(bytes: &[u8]) -> String {
    crate::backup::base64(bytes)
}

/// The Core's Base64; anything it would not write reads as no bytes.
pub fn base64_decode(s: &str) -> Vec<u8> {
    crate::net_images::base64(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(bytes: &[u8]) -> Raw {
        Raw {
            data: base64_encode(bytes),
            ..Default::default()
        }
    }

    #[test]
    fn base64_round_trips() {
        for text in ["", "a", "ab", "abc", "abcd", "hello world"] {
            assert_eq!(base64_decode(&base64_encode(text.as_bytes())), text.as_bytes());
        }
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(&[0xff, 0xfe]), "//4=");
    }

    #[test]
    fn json_is_indented_and_typed() {
        let c = read(&raw(br#"{"a":[1,2],"b":{},"c":"x,{y}:"}"#), "application/json");
        assert_eq!(c.kind, Kind::Text);
        assert_eq!(c.language, Language::Json);
        assert_eq!(
            c.text,
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {},\n  \"c\": \"x,{y}:\"\n}"
        );
        assert_eq!(c.type_label("application/json"), "JSON");
        assert!(!c.is_cut());
        assert_eq!(indented("{not json"), "{not json");
        let sniffed = read(&raw(b" [1]"), "");
        assert_eq!(sniffed.language, Language::Json);
        assert_eq!(sniffed.text, "[\n  1\n]");
    }

    #[test]
    fn types_follow_the_header() {
        let html = read(&raw(b"<p>hi</p>"), "text/html; charset=utf-8");
        assert_eq!(html.type_label("text/html; charset=utf-8"), "HTML");
        assert_eq!(html.charset, "");
        assert_eq!(read(&raw(b"<a/>"), "application/xml").type_label("application/xml"), "XML");
        assert_eq!(read(&raw(b"x=1"), "text/javascript").type_label(""), "JavaScript");
        assert_eq!(read(&raw(b"plain"), "text/plain").type_label(""), "文本");
        let png = read(&raw(&[0x89, b'P', b'N', b'G']), "image/png");
        assert_eq!((png.kind, png.type_label("")), (Kind::Image, "图片".to_string()));
        assert_eq!(read(&raw(b"<svg/>"), "image/svg+xml").kind, Kind::Text);
        let bin = read(&raw(&[0, 1, 2, 0xff]), "application/zip");
        assert_eq!((bin.kind, bin.type_label("")), (Kind::Binary, BINARY.to_string()));
    }

    #[test]
    fn notices_say_what_was_not_kept() {
        let mut r = raw(b"abc");
        r.encoding = "br".into();
        let c = read(&r, "text/plain");
        assert_eq!(c.kind, Kind::Binary);
        assert_eq!(c.footer(), undecoded("br"));
        let mut r = raw("中文".as_bytes());
        r.data = base64_encode(&"中文".as_bytes()[..4]);
        r.truncated = true;
        let c = read(&r, "text/plain");
        assert_eq!(c.kind, Kind::Text);
        assert!(c.text.starts_with('中'));
        assert_eq!(c.footer(), TRUNCATED);
    }

    #[test]
    fn declared_charsets_are_kept() {
        let c = read(&raw(&[0xc4, 0xe3]), "text/plain; charset=GBK");
        assert_eq!((c.kind, c.charset.as_str()), (Kind::Binary, "gbk"));
        let page = b"<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=Shift_JIS\">\xff";
        assert_eq!(read(&raw(page), "text/html").charset, "shift_jis");
        let xml = b"<?xml version=\"1.0\" encoding='ISO-8859-1'?><a>\xe9</a>";
        assert_eq!(read(&raw(xml), "").charset, "iso-8859-1");
    }

    #[test]
    fn control_characters_decide_text() {
        assert_eq!(body_text(b"a\tb\n", false).as_deref(), Some("a\tb\n"));
        assert_eq!(body_text(b"a\x1bb", false), None);
        assert_eq!(body_text(&[0xff], false), None);
        let mut log = "x".repeat(200).into_bytes();
        log[10] = 0x1b;
        let c = read(&raw(&log), "text/plain");
        assert_eq!(c.kind, Kind::Text);
        assert!(c.text.contains('\u{fffd}'));
        assert_eq!(read(&raw(b"a\0b"), "text/plain").kind, Kind::Binary);
        assert_eq!(copy_text(&raw(&[0, 1])), "AAE=");
        assert_eq!(copy_text(&raw(b"text")), "text");
    }

    #[test]
    fn previews_stop_at_thirty_lines() {
        let text = (0..40).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        let c = read(&raw(text.as_bytes()), "text/plain");
        assert_eq!(c.preview.lines().count(), 30);
        assert!(c.is_cut());
        let long = "y".repeat(5000);
        assert_eq!(read(&raw(long.as_bytes()), "text/plain").preview.len(), 3000);
    }

    #[test]
    fn hex_lines_pad_the_last_line() {
        let bytes: Vec<u8> = (0..20).map(|i| b'A' + i).collect();
        assert_eq!(
            hex_line(&bytes, 0, 16),
            "000000  41 42 43 44 45 46 47 48 49 4a 4b 4c 4d 4e 4f 50  ABCDEFGHIJKLMNOP"
        );
        assert_eq!(
            hex_line(&[0, 0x7f, b'z'], 0, 4),
            "000000  00 7f 7a     ..z"
        );
        assert_eq!(hex_offset_digits(0x1234567), 7);
        assert_eq!(hex_line_length(20, 16), 6 + 2 + 48 + 1 + 16);
        assert_eq!(hex_per_line(20, 80), 16);
        assert_eq!(hex_per_line(20, 50), 8);
        assert_eq!(hex_per_line(20, 1), 4);
    }

    #[test]
    fn protobuf_reads_without_a_schema() {
        let inner = [0x08, 0x96, 0x01];
        let mut msg = vec![0x0a, 0x02, b'h', b'i', 0x12, inner.len() as u8];
        msg.extend_from_slice(&inner);
        msg.extend_from_slice(&[0x1d, 1, 0, 0, 0]);
        assert_eq!(
            protobuf_text(&msg, false, false).unwrap(),
            "1: \"hi\"\n2 {\n  1: 150\n}\n3: 0x00000001"
        );
        let c = read(&raw(&msg), "application/x-protobuf");
        assert!(c.protobuf);
        assert_eq!(c.type_label(""), "Protobuf");
        assert!(c.footer().contains("Protobuf"));
        assert_eq!(protobuf_text(&msg[..3], false, false), None);
        assert_eq!(protobuf_text(&msg[..3], true, false), None);
        assert!(protobuf_text(&msg[..5], true, false).is_some());
        assert_eq!(protobuf_text(&[0x80, 0x80, 0x80, 0x08, 0x00], false, true), None);
    }

    #[test]
    fn grpc_frames_read_one_by_one() {
        let message = [0x08, 0x01];
        let mut body = vec![0, 0, 0, 0, 2];
        body.extend_from_slice(&message);
        let trailer = b"grpc-status: 0\r\n";
        body.push(0x80);
        body.extend_from_slice(&(trailer.len() as u32).to_be_bytes());
        body.extend_from_slice(trailer);
        assert_eq!(
            grpc_text(&body, false).unwrap(),
            "# 1\n1: 1\n# 2\ngrpc-status: 0"
        );
        let c = read(&raw(&body), "application/grpc-web+proto");
        assert_eq!((c.kind, c.protobuf), (Kind::Text, true));
        assert_eq!(grpc_text(&body[..6], false), None);
        assert_eq!(grpc_text(&body[..6], true), None);
    }

    #[test]
    fn grpc_inflates_gzip_frames() {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(&[0x08, 0x2a]).unwrap();
        let zipped = gz.finish().unwrap();
        let mut body = vec![1];
        body.extend_from_slice(&(zipped.len() as u32).to_be_bytes());
        body.extend_from_slice(&zipped);
        assert_eq!(grpc_text(&body, false).unwrap(), "# 1\n1: 42");
    }

    #[test]
    fn raw_bodies_parse() {
        let r = Raw::parse(&json!({"data": "aGk=", "truncated": true, "encoding": "br"}));
        assert_eq!(r.bytes(), b"hi");
        assert!(r.truncated);
        assert_eq!(r.encoding, "br");
        let header = vec![("content-type".to_string(), vec!["text/plain".to_string()])];
        assert_eq!(header_value(&header, "Content-Type"), "text/plain");
        assert_eq!(header_value(&header, "Location"), "");
    }
}
