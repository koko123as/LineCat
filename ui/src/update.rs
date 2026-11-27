//! Self update: which release file this build installs, the newest release of its line and
//! the checksum a download is verified against. FlClash: lib/common/app_update.dart,
//! lib/common/release_asset.dart, lib/common/release_version.dart (`newestOfAllReleases`).

use serde_json::Value;
use std::cmp::Ordering;

pub const REPOSITORY: &str = "koko123as/LineCat";
const FILE_PREFIX: &str = "FlClash";

pub const STARTED: &str = "已开始下载更新，可在首页查看进度";
pub const READY: &str = "更新已下载，可在首页安装";
pub const FAILED: &str = "更新失败";
pub const DOWNLOAD_FAILED: &str = "无法下载更新。";
pub const NOT_VERIFIED: &str = "下载的更新未通过校验。";
pub const FILE_MISSING: &str = "未找到更新文件，请在发布页手动下载。";
pub const INSTALL_FAILED: &str = "无法安装更新，可以前往发布页手动下载。";
pub const GO_DOWNLOAD: &str = "前往下载";
pub const TEST: &str = "更新下载测试";

/// `selfUpdateAssetFor`: the APK's arch for this build, None where none is published.
pub fn self_update_arch() -> Option<&'static str> {
    if !cfg!(target_os = "android") {
        return None;
    }
    if cfg!(target_arch = "aarch64") {
        Some("arm64")
    } else if cfg!(target_arch = "arm") {
        Some("armv7")
    } else if cfg!(target_arch = "x86_64") {
        Some("amd64")
    } else {
        None
    }
}

/// `releaseVersionOfTag`: GitHub rewrites `+` in asset names.
pub fn version_of_tag(tag: &str) -> String {
    tag.strip_prefix('v').unwrap_or(tag).replace('+', ".")
}

fn is_file_version(v: &str) -> bool {
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && pre.is_none_or(|p| {
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
        })
}

pub fn names_release_files(tag: &str) -> bool {
    is_file_version(&version_of_tag(tag))
}

pub fn asset_name(tag: &str, arch: &str) -> String {
    format!("{FILE_PREFIX}-{}-android-{arch}.apk", version_of_tag(tag))
}

pub fn checksums_name(tag: &str) -> String {
    format!("{FILE_PREFIX}-{}-SHA256SUMS", version_of_tag(tag))
}

pub fn asset_url(tag: &str, name: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{name}")
}

pub fn release_page(tag: &str) -> String {
    format!("https://github.com/{REPOSITORY}/releases/tag/{tag}")
}

/// `releaseChecksum`: `<sha256> <name>` or `<sha256> *<name>` lines.
pub fn checksum(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let line = line.trim();
        let (hash, rest) = line.split_at_checked(64)?;
        let file = rest
            .strip_prefix("  ")
            .or_else(|| rest.strip_prefix(" *"))
            .or_else(|| rest.strip_prefix(' '))?;
        (hash.bytes().all(|b| b.is_ascii_hexdigit()) && file == name).then(|| hash.to_lowercase())
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<String>,
    build: u64,
}

impl Version {
    pub fn parse(tag: &str) -> Option<Self> {
        let t = tag.trim();
        let t = t.strip_prefix('v').unwrap_or(t);
        let (rest, build) = match t.split_once('+') {
            Some((r, b)) if !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()) => {
                (r, b.parse().ok()?)
            }
            Some(_) => return None,
            None => (t, 0),
        };
        let (core, pre) = match rest.split_once('-') {
            Some((c, p)) => (c, p.split('.').map(String::from).collect::<Vec<_>>()),
            None => (rest, Vec::new()),
        };
        if pre
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        {
            return None;
        }
        let nums: Vec<u64> = core
            .split('.')
            .map(|n| {
                (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| n.parse().ok())
                    .flatten()
            })
            .collect::<Option<_>>()?;
        let core: [u64; 3] = nums.try_into().ok()?;
        Some(Version { core, pre, build })
    }

    /// The rs line names itself in its first prerelease identifier.
    pub fn is_rs(&self) -> bool {
        self.pre.first().is_some_and(|p| p == "rs")
    }

    fn precedence(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                _ => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let order = identifier(a, b);
                        if order != Ordering::Equal {
                            return order;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.precedence(other).then(self.build.cmp(&other.build)))
    }
}

fn identifier(a: &str, b: &str) -> Ordering {
    fn num(s: &str) -> Option<&str> {
        (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit())).then_some(s)
    }
    match (num(a), num(b)) {
        (Some(x), Some(y)) => {
            let (x, y) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// `newestOfAllReleases` on one page of the release list: the newest rs tag that names
/// release files and carries its checksums.
pub fn newest_of(releases: &Value) -> Option<String> {
    let mut found: Vec<(Version, String)> = releases
        .as_array()?
        .iter()
        .filter(|r| r.get("draft").and_then(Value::as_bool) != Some(true))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?.to_string();
            let v = Version::parse(&tag).filter(Version::is_rs)?;
            let sums = checksums_name(&tag);
            let has_sums = r
                .get("assets")?
                .as_array()?
                .iter()
                .any(|a| a.get("name").and_then(Value::as_str) == Some(sums.as_str()));
            (has_sums && names_release_files(&tag)).then_some((v, tag))
        })
        .collect();
    found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    found.into_iter().next().map(|(_, tag)| tag)
}

/// `releaseTagsOfFeed`: the tags the release feed's entry links name.
pub fn release_tags_of_feed(feed: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut rest = feed;
    while let Some(at) = rest.find("<link") {
        rest = &rest[at + 5..];
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let attrs = &rest[..tag_end];
        let Some(h) = attrs.find("href=\"") else {
            continue;
        };
        let href = &attrs[h + 6..];
        let href = &href[..href.find('"').unwrap_or(href.len())];
        let path = href.split(['?', '#']).next().unwrap_or("");
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if let Some(i) = segments.iter().position(|s| *s == "tag") {
            if let Some(tag) = segments.get(i + 1) {
                tags.push(tag.to_string());
            }
        }
    }
    tags
}

/// `_candidates`: the rs tags that name release files, newest first.
pub fn feed_candidates(tags: &[String]) -> Vec<String> {
    let mut found: Vec<(Version, String)> = tags
        .iter()
        .filter(|t| names_release_files(t))
        .filter_map(|t| Some((Version::parse(t).filter(Version::is_rs)?, t.clone())))
        .collect();
    found.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
    found.dedup_by(|a, b| a.1 == b.1);
    found.into_iter().map(|(_, t)| t).collect()
}

const CHANGELOG_JSON: &str = "<!-- flclash:changelog:json";
const NOTES_BEGIN: &str = "<!-- flclash:changelog:begin -->";
const NOTES_END: &str = "<!-- flclash:changelog:end -->";

/// `parseReleaseChangelog` and `visibleGroups`: the first version's groups with their
/// titles (`changelogGroupTitle`), when the body carries schema 2's JSON.
pub fn changelog_groups(body: &str) -> Option<Vec<(&'static str, Vec<String>)>> {
    let start = body.find(CHANGELOG_JSON)? + CHANGELOG_JSON.len();
    let end = start + body[start..].find("-->")?;
    let json: Value = serde_json::from_str(&body[start..end]).ok()?;
    if json.get("schemaVersion").and_then(Value::as_i64) != Some(2) {
        return None;
    }
    let version = json.get("versions")?.as_array()?.first()?;
    let groups = version.get("groups").and_then(Value::as_array);
    Some(
        groups
            .into_iter()
            .flatten()
            .filter_map(|g| {
                let title = match g.get("type").and_then(Value::as_str)? {
                    "breaking" => "重大变更",
                    "feat" => "新功能",
                    "fix" => "问题修复",
                    "perf" => "性能优化",
                    "revert" => "已回滚",
                    _ => return None,
                };
                let entries: Vec<String> = g
                    .get("entries")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|e| {
                        e.get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect();
                (!entries.is_empty()).then_some((title, entries))
            })
            .collect(),
    )
}

/// `parseReleaseBody`: the `- ` lines between the notes markers, else of the whole body.
pub fn release_body_items(body: &str) -> Vec<String> {
    let scoped = match body.find(NOTES_BEGIN) {
        Some(b) => {
            let rest = &body[b + NOTES_BEGIN.len()..];
            &rest[..rest.find(NOTES_END).unwrap_or(rest.len())]
        }
        None => body,
    };
    scoped
        .lines()
        .filter_map(|l| {
            let l = l.trim_start_matches([' ', '\t']);
            let rest = l.strip_prefix('-')?;
            rest.starts_with([' ', '\t'])
                .then(|| rest.trim().to_string())
                .filter(|t| !t.is_empty())
        })
        .collect()
}

/// `_releaseSpan`: the tag, then the changelog's groups under their titles, else the
/// body's list items.
pub fn release_text(tag: &str, body: &str) -> String {
    let mut text = format!("{tag}\n");
    match changelog_groups(body) {
        Some(groups) => {
            for (title, entries) in groups {
                text.push_str(&format!("\n{title}\n"));
                for e in entries {
                    text.push_str(&format!("• {e}\n"));
                }
            }
        }
        None => {
            let items = release_body_items(body);
            if !items.is_empty() {
                text.push('\n');
            }
            for i in items {
                text.push_str(&format!("- {i}\n"));
            }
        }
    }
    text.trim_end().to_string()
}

/// `checkUpdateResultHandle`'s question; an empty `no` keeps the dialog's own 取消.
#[derive(Debug, PartialEq)]
pub struct Offer {
    pub text: String,
    pub yes: &'static str,
    pub no: &'static str,
}

pub fn offer(tag: &str, notes: &str, installs: bool, by_user: bool) -> Offer {
    Offer {
        text: release_text(tag, notes),
        yes: if installs { "更新" } else { GO_DOWNLOAD },
        no: if by_user { "" } else { "不再提示" },
    }
}

#[derive(Debug, PartialEq)]
pub enum Failure {
    Cancelled,
    /// `ReleaseFileMissingException`: the release page is opened instead.
    Missing,
    NotVerified,
    Failed(String),
}

impl Failure {
    /// `userFacingErrorMessage`; None where nothing is shown in a dialog.
    pub fn message(&self) -> Option<&'static str> {
        match self {
            Failure::Cancelled | Failure::Missing => None,
            Failure::NotVerified => Some(NOT_VERIFIED),
            Failure::Failed(_) => Some(DOWNLOAD_FAILED),
        }
    }
}

pub fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// `packageInstallFailed`: neither `STATUS_SUCCESS` nor `STATUS_FAILURE_ABORTED`.
pub fn install_failed(status: i64) -> bool {
    status != 0 && status != 3
}

/// The step between progress reports: a percent, or a MiB while the size is unknown.
pub fn progress_step(total: u64) -> u64 {
    if total > 0 {
        (total / 100).max(1)
    } else {
        1 << 20
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_follow_the_tag() {
        assert_eq!(
            asset_name("v0.9.4-rs.1+2", "arm64"),
            "FlClash-0.9.4-rs.1.2-android-arm64.apk"
        );
        assert_eq!(checksums_name("v1.0.0"), "FlClash-1.0.0-SHA256SUMS");
        assert!(names_release_files("v1.2.0-rs.3"));
        assert!(!names_release_files("nightly"));
        assert!(!names_release_files("v1.2.0-beta-1"));
        let hash = "A".repeat(64);
        let sums = format!("{hash}  other.apk\n{hash} *FlClash-1.apk\n");
        assert_eq!(checksum(&sums, "FlClash-1.apk"), Some("a".repeat(64)));
        assert_eq!(checksum(&sums, "missing"), None);
    }

    #[test]
    fn versions_order_as_semver_with_rebuilds() {
        let v = |t: &str| Version::parse(t).unwrap();
        assert!(v("v1.0.0") > v("v1.0.0-rs.9"));
        assert!(v("v1.0.0-rs.10") > v("v1.0.0-rs.9"));
        assert!(v("v1.0.0-rs.3+1") > v("v1.0.0-rs.3"));
        assert!(v("v1.0.0-rs.4") > v("v1.0.0-rs.3+1"));
        assert!(v("v1.0.0-rs.a") > v("v1.0.0-rs.1"));
        assert!(Version::parse("v1.0").is_none());
        assert!(v("v1.0.0-rs.1").is_rs() && !v("v1.0.0-beta").is_rs());
    }

    #[test]
    fn newest_rs_release_with_checksums() {
        let rel = |tag: &str, sums: bool, draft: bool| {
            let assets = if sums {
                json!([{"name": checksums_name(tag)}])
            } else {
                json!([])
            };
            json!({"tag_name": tag, "draft": draft, "assets": assets})
        };
        let list = json!([
            rel("v0.9.5-rs.1", false, false),
            rel("v0.9.6-rs.1", true, true),
            rel("v0.9.4-rs.2", true, false),
            rel("v0.9.4", true, false),
            rel("v0.9.4-rs.1", true, false),
        ]);
        assert_eq!(newest_of(&list).as_deref(), Some("v0.9.4-rs.2"));
        assert!(install_failed(1) && !install_failed(0) && !install_failed(3));
    }

    #[test]
    fn feed_tags_fall_back_newest_first() {
        let feed = r#"<feed><link type="text/html" rel="alternate" href="https://github.com/o/r/releases"/>
<entry><link rel="alternate" type="text/html" href="https://github.com/o/r/releases/tag/v0.9.5-rs.1"/></entry>
<entry><link rel="alternate" href="https://github.com/o/r/releases/tag/v0.9.6-rs.2"/></entry>
<entry><link href="https://github.com/o/r/releases/tag/nightly"/></entry>
<entry><link href="https://github.com/o/r/releases/tag/v0.9.7"/></entry></feed>"#;
        let tags = release_tags_of_feed(feed);
        assert_eq!(
            tags,
            vec!["v0.9.5-rs.1", "v0.9.6-rs.2", "nightly", "v0.9.7"]
        );
        assert_eq!(feed_candidates(&tags), vec!["v0.9.6-rs.2", "v0.9.5-rs.1"]);
    }

    #[test]
    fn release_notes_group_as_the_changelog() {
        let body = r#"intro
<!-- flclash:changelog:json {"schemaVersion":2,"versions":[{"version":"1","tag":"v1","groups":[
 {"type":"feat","entries":[{"id":"a","text":"Add A"},{"id":"b","text":"Add B"}]},
 {"type":"chore","entries":[{"id":"c","text":"skip"}]},
 {"type":"fix","entries":[]},
 {"type":"breaking","entries":[{"id":"d","text":"Drop D"}]}]}]} -->"#;
        assert_eq!(
            release_text("v1", body),
            "v1\n\n新功能\n• Add A\n• Add B\n\n重大变更\n• Drop D"
        );
        let old = body.replace("\"schemaVersion\":2", "\"schemaVersion\":1");
        assert_eq!(release_text("v1", &old), "v1");
        let plain = "x\n<!-- flclash:changelog:begin -->\n- one\n  - two \n-three\n<!-- flclash:changelog:end -->\n- out";
        assert_eq!(release_text("v2", plain), "v2\n\n- one\n- two");
        assert_eq!(release_body_items("- a\n- b"), vec!["a", "b"]);
    }

    #[test]
    fn offer_and_failures_read_as_flclash() {
        let o = offer("v1.0.0-rs.1", "- notes ", true, false);
        assert_eq!(o.text, "v1.0.0-rs.1\n\n- notes");
        assert_eq!((o.yes, o.no), ("更新", "不再提示"));
        let o = offer("v1", "", false, true);
        assert_eq!((o.text.as_str(), o.yes, o.no), ("v1", GO_DOWNLOAD, ""));
        assert_eq!(Failure::NotVerified.message(), Some(NOT_VERIFIED));
        assert_eq!(Failure::Failed("x".into()).message(), Some(DOWNLOAD_FAILED));
        assert_eq!(Failure::Cancelled.message(), None);
        assert_eq!(hex(&crate::mitm::sha256(b"")[..2]), "e3b0");
    }
}
