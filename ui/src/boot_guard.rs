//! FlClash's `BootGuard` (lib/common/boot_guard.dart, boot_record.dart): a launch that never
//! reached running skips the automatic setup the next time, and a second one in a row
//! deselects the profile it started with. The shell reports no `ApplicationExitInfo`, so
//! every unfinished launch counts, as FlClash counts one with no exit info.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CLEAR_THRESHOLD: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Starting,
    Running,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    /// None once the app closed.
    pub stage: Option<Stage>,
    pub profile_id: Option<String>,
    pub started_at: i64,
    pub failure_count: u32,
    pub last_failed_profile_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Recovery {
    #[default]
    None,
    SkipAutoSetup,
    ClearProfile,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decision {
    pub recovery: Recovery,
    pub failure_count: u32,
    pub failed_profile_id: Option<String>,
}

impl Decision {
    pub fn is_degraded(&self) -> bool {
        self.recovery != Recovery::None
    }

    /// `_showCrashRecoveryTip`: the alert's title and text, given the failed profile's label.
    pub fn tip(&self, label: &str) -> Option<(&'static str, String)> {
        match self.recovery {
            Recovery::None => None,
            Recovery::SkipAutoSetup => Some((
                "启动未完成",
                "应用上次在启动过程中意外退出。已跳过本次自动配置，你可以手动启动重试。".into(),
            )),
            Recovery::ClearProfile => Some((
                "检测到崩溃",
                format!("应用连续两次未能完成启动。为打断崩溃循环，已取消选中配置 {label}，并跳过本次自动配置，你可以随时重新选中它。"),
            )),
        }
    }
}

/// `resolveBootDecision` with no exit info.
pub fn resolve(record: Option<&Record>) -> Decision {
    let Some(record) = record.filter(|r| r.stage == Some(Stage::Starting)) else {
        return Decision::default();
    };
    let failure_count = record.failure_count + 1;
    let clear = failure_count >= CLEAR_THRESHOLD && record.profile_id.is_some();
    Decision {
        recovery: if clear {
            Recovery::ClearProfile
        } else {
            Recovery::SkipAutoSetup
        },
        failure_count,
        failed_profile_id: record.profile_id.clone(),
    }
}

/// `evaluate`'s record: this launch starting with `profile_id`, the one left after recovery.
pub fn starting(
    record: Option<&Record>,
    decision: &Decision,
    profile_id: Option<String>,
    now: i64,
) -> Record {
    Record {
        stage: Some(Stage::Starting),
        profile_id,
        started_at: now,
        failure_count: decision.failure_count,
        last_failed_profile_id: decision
            .failed_profile_id
            .clone()
            .or_else(|| record.and_then(|r| r.last_failed_profile_id.clone())),
    }
}

/// `markRunning`: a launch that recovered keeps its count until one finishes cleanly.
pub fn running(record: &Record, decision: &Decision) -> Record {
    Record {
        stage: Some(Stage::Running),
        failure_count: if decision.is_degraded() {
            record.failure_count
        } else {
            0
        },
        ..record.clone()
    }
}

pub fn read(path: &Path) -> Option<Record> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Written through a temporary file, so a crash mid-write leaves the last record.
pub fn write(path: &Path, record: &Record) {
    let Ok(bytes) = serde_json::to_vec(record) else {
        return;
    };
    let tmp = path.with_extension("json.part");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(stage: Option<Stage>, profile: Option<&str>, failures: u32) -> Record {
        Record {
            stage,
            profile_id: profile.map(String::from),
            started_at: 1,
            failure_count: failures,
            last_failed_profile_id: None,
        }
    }

    #[test]
    fn a_finished_or_closed_launch_changes_nothing() {
        assert!(!resolve(None).is_degraded());
        assert!(!resolve(Some(&record(None, Some("1"), 0))).is_degraded());
        assert!(!resolve(Some(&record(Some(Stage::Running), Some("1"), 0))).is_degraded());
    }

    #[test]
    fn one_unfinished_launch_skips_the_setup_and_two_deselect_the_profile() {
        let first = resolve(Some(&record(Some(Stage::Starting), Some("7"), 0)));
        assert_eq!(first.recovery, Recovery::SkipAutoSetup);
        assert_eq!(first.failure_count, 1);
        let next = starting(None, &first, Some("7".into()), 5);
        assert_eq!(next.failure_count, 1);
        assert_eq!(next.last_failed_profile_id.as_deref(), Some("7"));
        let second = resolve(Some(&next));
        assert_eq!(second.recovery, Recovery::ClearProfile);
        assert_eq!(second.failed_profile_id.as_deref(), Some("7"));
        let without = resolve(Some(&record(Some(Stage::Starting), None, 3)));
        assert_eq!(without.recovery, Recovery::SkipAutoSetup);
    }

    #[test]
    fn running_resets_the_count_only_after_a_clean_launch() {
        let r = record(Some(Stage::Starting), Some("1"), 1);
        assert_eq!(running(&r, &Decision::default()).failure_count, 0);
        let degraded = resolve(Some(&r));
        let r = starting(Some(&r), &degraded, None, 2);
        let after = running(&r, &degraded);
        assert_eq!(after.stage, Some(Stage::Running));
        assert_eq!(after.failure_count, 2);
    }

    #[test]
    fn the_tips_follow_flclash() {
        assert!(Decision::default().tip("a").is_none());
        let d = Decision {
            recovery: Recovery::ClearProfile,
            ..Default::default()
        };
        let (title, text) = d.tip("机场").unwrap();
        assert_eq!(title, "检测到崩溃");
        assert!(text.contains("已取消选中配置 机场"), "{text}");
    }

    #[test]
    fn the_record_round_trips_through_its_file() {
        let dir = std::env::temp_dir().join(format!("boot-guard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boot.json");
        let r = record(Some(Stage::Starting), Some("3"), 1);
        write(&path, &r);
        assert_eq!(read(&path), Some(r));
        std::fs::write(&path, b"{").unwrap();
        assert_eq!(read(&path), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
