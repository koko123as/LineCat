//! The rule sets of 进阶配置 → 规则集 (lib/views/config/providers.dart, `ClashProvider`):
//! kept in the settings, their files under the Core's `providers/rules`, where
//! `ruleSet.compile` downloads and compiles them.

use serde::{Deserialize, Serialize};

pub const NAME_LIMIT: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RuleSet {
    pub id: String,
    pub label: String,
    pub url: String,
    /// classical, domain or ipcidr, as `ruleSet.compile` reads the source.
    pub behavior: String,
    pub format: String,
}

impl Default for RuleSet {
    fn default() -> Self {
        RuleSet {
            id: String::new(),
            label: String::new(),
            url: String::new(),
            behavior: "classical".into(),
            format: "yaml".into(),
        }
    }
}

impl RuleSet {
    pub fn is_remote(&self) -> bool {
        !self.url.is_empty()
    }

    /// An mrs set is a zstd stream no text editor can round-trip.
    pub fn is_text(&self) -> bool {
        self.format != "mrs"
    }

    pub fn is_editable(&self) -> bool {
        !self.is_remote() && self.is_text()
    }

    /// Keyed by the url too, so an edited url downloads afresh.
    pub fn file_name(&self) -> String {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in format!("{}@{}", self.id, self.url).bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("{hash:016x}")
    }
}

/// `_validateLabel`: empty is allowed only where a name is optional.
pub fn label_issue(
    sets: &[RuleSet],
    except: Option<&str>,
    label: &str,
    optional: bool,
) -> Option<String> {
    let label = label.trim();
    if label.is_empty() {
        return (!optional).then(|| "名称不能为空".into());
    }
    if sets
        .iter()
        .any(|s| Some(s.id.as_str()) != except && s.label == label)
    {
        return Some("名称当前已存在".into());
    }
    None
}

pub fn url_issue(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return Some("URL不能为空".into());
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Some("仅支持远程资源".into());
    }
    None
}

/// `uniqueLabelFor`: the name cut to the limit, or `(n)` after it until it is free.
pub fn unique_label(name: &str, fallback: &str, taken: impl Fn(&str) -> bool) -> String {
    let take = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let name = name.trim();
    let label = take(if name.is_empty() { fallback } else { name }, NAME_LIMIT)
        .trim()
        .to_string();
    if !taken(&label) {
        return label;
    }
    let (stem, start) = match counter(&label) {
        Some((at, n)) => (&label[..at], n + 1),
        None => (label.as_str(), 1),
    };
    (start..)
        .map(|i| {
            let suffix = format!("({i})");
            take(stem, NAME_LIMIT - suffix.chars().count()) + &suffix
        })
        .find(|c| !taken(c))
        .unwrap_or_default()
}

fn counter(label: &str) -> Option<(usize, u64)> {
    let inner = label.strip_suffix(')')?;
    let open = inner.rfind('(')?;
    let digits = &inner[open + 1..];
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((open, digits.parse().ok()?))
}

/// `urlFileName.fileStem`: the last path segment without its extension.
pub fn url_stem(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or("");
    let path = path.split_once("://").map(|(_, p)| p).unwrap_or(path);
    let segment = path
        .split_once('/')
        .map(|(_, p)| p)
        .unwrap_or("")
        .rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("");
    file_stem(segment)
}

pub fn file_stem(name: &str) -> String {
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[..dot].to_string(),
        _ => name.to_string(),
    }
}

/// `showNamedUrlsForm` in batch: each new URL a set named uniquely from its file.
pub fn batch_imports(sets: &[RuleSet], input: &str) -> Vec<(String, String)> {
    let mut taken: Vec<String> = sets.iter().map(|s| s.label.clone()).collect();
    let mut out = Vec::new();
    for url in crate::mitm::module_urls(input) {
        if sets.iter().any(|s| s.url == url) {
            continue;
        }
        let label = unique_label(&url_stem(&url), "规则集", |l| {
            taken.iter().any(|t| t == l)
        });
        taken.push(label.clone());
        out.push((label, url));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(id: &str, label: &str) -> RuleSet {
        RuleSet {
            id: id.into(),
            label: label.into(),
            ..Default::default()
        }
    }

    #[test]
    fn labels_must_be_given_and_free() {
        let sets = [set("1", "广告"), set("2", "直连")];
        assert_eq!(
            label_issue(&sets, None, " ", false).as_deref(),
            Some("名称不能为空")
        );
        assert_eq!(label_issue(&sets, None, " ", true), None);
        assert_eq!(
            label_issue(&sets, None, "广告", false).as_deref(),
            Some("名称当前已存在")
        );
        assert_eq!(label_issue(&sets, Some("1"), "广告", false), None);
    }

    #[test]
    fn urls_must_be_http() {
        assert_eq!(url_issue("").as_deref(), Some("URL不能为空"));
        assert_eq!(url_issue("ftp://a/b").as_deref(), Some("仅支持远程资源"));
        assert_eq!(url_issue(" https://a/b.yaml "), None);
    }

    #[test]
    fn unique_labels_count_up() {
        let taken = ["ads", "ads(1)", "x(3)"];
        let free = |l: &str| taken.contains(&l);
        assert_eq!(unique_label("ads", "规则集", free), "ads(2)");
        assert_eq!(unique_label("x(3)", "规则集", free), "x(4)");
        assert_eq!(unique_label("  ", "规则集", free), "规则集");
        assert_eq!(
            unique_label(&"a".repeat(70), "f", |_| false)
                .chars()
                .count(),
            NAME_LIMIT
        );
    }

    #[test]
    fn names_come_from_the_url_path() {
        assert_eq!(url_stem("https://h.example/r/ads.yaml?x=1"), "ads");
        assert_eq!(url_stem("https://h.example/r/list/"), "list");
        assert_eq!(url_stem("https://h.example"), "");
        assert_eq!(file_stem(".hidden"), ".hidden");
    }

    #[test]
    fn file_names_follow_the_url_and_editing_needs_text() {
        let mut s = set("1", "a");
        let local = s.file_name();
        s.url = "https://h/a.yaml".into();
        assert_ne!(local, s.file_name());
        assert!(!s.is_editable());
        s.url.clear();
        s.format = "mrs".into();
        assert!(!s.is_editable() && !s.is_text());
    }

    #[test]
    fn several_urls_import_as_new_sets() {
        let sets = [RuleSet {
            label: "ads".into(),
            url: "https://a.test/known.yaml".into(),
            ..Default::default()
        }];
        let input = "https://a.test/ads.yaml, https://b.test/ads.yaml\nhttps://a.test/known.yaml https://a.test/ads.yaml";
        let got = batch_imports(&sets, input);
        assert_eq!(
            got,
            [
                ("ads(1)".to_string(), "https://a.test/ads.yaml".to_string()),
                ("ads(2)".to_string(), "https://b.test/ads.yaml".to_string()),
            ]
        );
    }
}
