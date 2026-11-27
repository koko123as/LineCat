//! The icon sets of 进阶配置 → 图标集 (lib/views/config/icon_sets.dart, `IconSetsAction`):
//! galleries in the Quantumult X / Loon format, kept in the settings with their icons.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct IconSetIcon {
    pub name: String,
    pub url: String,
}

impl IconSetIcon {
    /// `IconSetIconExt.label`: the name's stem with underscores as spaces.
    pub fn label(&self) -> String {
        crate::rule_sets::file_stem(&self.name).replace('_', " ")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct IconSet {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icons: Vec<IconSetIcon>,
    pub updated: u64,
}

impl IconSet {
    pub fn is_remote(&self) -> bool {
        !self.url.is_empty()
    }

    pub fn cover(&self) -> &str {
        self.icons.first().map(|i| i.url.as_str()).unwrap_or("")
    }
}

pub fn is_http_url(value: &str) -> bool {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"));
    rest.and_then(|r| r.split(['/', '?', '#']).next())
        .map(|host| !host.is_empty() && !host.starts_with(':') && !host.contains(' '))
        .unwrap_or(false)
}

/// `urlFileName`: the last non-empty path segment.
pub fn url_file_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let path = path.split_once("://").map(|(_, p)| p).unwrap_or(path);
    path.split_once('/')
        .map(|(_, p)| p)
        .unwrap_or("")
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string()
}

/// `parseIconSet`: the gallery's name and its icons, each url once; Err with
/// 不是有效的图标集 for anything else or a gallery without icons.
pub fn parse(content: &str) -> Result<(String, Vec<IconSetIcon>), String> {
    let invalid = || "不是有效的图标集".to_string();
    let doc: Value = serde_json::from_str(content).map_err(|_| invalid())?;
    let items = doc
        .get("icons")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let mut seen = std::collections::HashSet::new();
    let mut icons = Vec::new();
    for item in items {
        let Some(url) = item.get("url").and_then(Value::as_str) else {
            continue;
        };
        if !is_http_url(url) || !seen.insert(url.to_string()) {
            continue;
        }
        let name = match item.get("name").and_then(Value::as_str).map(str::trim) {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => url_file_name(url),
        };
        icons.push(IconSetIcon {
            name,
            url: url.to_string(),
        });
    }
    if icons.is_empty() {
        return Err(invalid());
    }
    let name = doc
        .get("name")
        .and_then(Value::as_str)
        .map(|n| n.trim().to_string())
        .unwrap_or_default();
    Ok((name, icons))
}

/// `_nameOf`: the given name, the gallery's, then the fallback, else 图标集.
pub fn name_of(name: &str, parsed: &str, fallback: &str) -> String {
    [name, parsed, fallback]
        .iter()
        .map(|c| c.trim())
        .find(|c| !c.is_empty())
        .unwrap_or("图标集")
        .to_string()
}

pub fn url_fallback(url: &str) -> String {
    crate::rule_sets::file_stem(&url_file_name(url))
}

pub fn search<'a>(icons: &'a [IconSetIcon], query: &str) -> Vec<&'a IconSetIcon> {
    icons
        .iter()
        .filter(|i| crate::proxies::matches(query, &i.label()))
        .collect()
}

/// `num.compact` (English): 999, 1.2K, 12K, 1.5M.
pub fn compact(n: usize) -> String {
    let (v, unit) = match n {
        0..=999 => return n.to_string(),
        1000..=999_999 => (n as f64 / 1e3, "K"),
        _ => (n as f64 / 1e6, "M"),
    };
    if v < 10.0 {
        let text = format!("{v:.1}");
        format!("{}{unit}", text.trim_end_matches(".0"))
    } else {
        format!("{}{unit}", v.round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galleries_keep_named_http_icons_once() {
        let (name, icons) = parse(
            r#"{"name":" Qure ","icons":[
                {"name":"Hong_Kong.png","url":"https://h.example/a/Hong_Kong.png"},
                {"url":"https://h.example/a/Japan.png?x=1"},
                {"name":"dup","url":"https://h.example/a/Hong_Kong.png"},
                {"name":"ftp","url":"ftp://h.example/x.png"},
                "junk"]}"#,
        )
        .unwrap();
        assert_eq!(name, "Qure");
        assert_eq!(icons.len(), 2);
        assert_eq!(icons[0].label(), "Hong Kong");
        assert_eq!(icons[1].name, "Japan.png");
        assert_eq!(icons[1].label(), "Japan");
    }

    #[test]
    fn anything_else_is_not_a_gallery() {
        for bad in ["", "[]", r#"{"icons":[]}"#, r#"{"icons":[{"url":"x"}]}"#] {
            assert_eq!(parse(bad).unwrap_err(), "不是有效的图标集");
        }
    }

    #[test]
    fn names_fall_back_in_order() {
        assert_eq!(name_of(" a ", "b", "c"), "a");
        assert_eq!(name_of("", " ", "c"), "c");
        assert_eq!(name_of("", "", ""), "图标集");
        assert_eq!(url_fallback("https://h.example/g/qure.json?t=1"), "qure");
    }

    #[test]
    fn urls_need_a_host() {
        assert!(is_http_url("https://h.example/x.png"));
        assert!(is_http_url("http://h.example"));
        assert!(!is_http_url("https:///x.png"));
        assert!(!is_http_url("file:///x.png"));
    }

    #[test]
    fn search_and_counts() {
        let icons = vec![
            IconSetIcon {
                name: "United_States.png".into(),
                url: "u".into(),
            },
            IconSetIcon {
                name: "Japan.png".into(),
                url: "j".into(),
            },
        ];
        assert_eq!(search(&icons, "states UNITED").len(), 1);
        assert_eq!(search(&icons, "").len(), 2);
        assert_eq!(compact(999), "999");
        assert_eq!(compact(1200), "1.2K");
        assert_eq!(compact(2000), "2K");
        assert_eq!(compact(15_400), "15K");
        assert_eq!(compact(1_500_000), "1.5M");
    }
}
