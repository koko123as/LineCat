//! Self update (lib/common/app_update.dart, release_asset.dart, release_version.dart and
//! `request.newestOfAllReleases`): which release file this build installs, the newest
//! release of its line and the checksum a download is verified against.

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

/// `checkUpdateResultHandle`'s question; an empty `no` keeps the dialog's own 取消.
#[derive(Debug, PartialEq)]
pub struct Offer {
    pub text: String,
    pub yes: &'static str,
    pub no: &'static str,
}

pub fn offer(tag: &str, notes: &str, installs: bool, by_user: bool) -> Offer {
    let notes = notes.trim();
    Offer {
        text: if notes.is_empty() {
            tag.to_string()
        } else {
            format!("{tag}\n\n{notes}")
        },
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
    fn offer_and_failures_read_as_flclash() {
        let o = offer("v1.0.0-rs.1", " notes ", true, false);
        assert_eq!(o.text, "v1.0.0-rs.1\n\nnotes");
        assert_eq!((o.yes, o.no), ("更新", "不再提示"));
        let o = offer("v1", "", false, true);
        assert_eq!((o.text.as_str(), o.yes, o.no), ("v1", GO_DOWNLOAD, ""));
        assert_eq!(Failure::NotVerified.message(), Some(NOT_VERIFIED));
        assert_eq!(Failure::Failed("x".into()).message(), Some(DOWNLOAD_FAILED));
        assert_eq!(Failure::Cancelled.message(), None);
        assert_eq!(hex(&crate::mitm::sha256(b"")[..2]), "e3b0");
    }
}
