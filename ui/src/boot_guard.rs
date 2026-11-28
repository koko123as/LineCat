//! FlClash's `BootGuard` (lib/common/boot_guard.dart, boot_record.dart): a launch that never
//! reached running skips the automatic setup the next time, and a second one in a row
//! deselects the profile it started with. A launch Android's `ApplicationExitInfo` says was
//! stopped from outside is not counted; with no exit info (below API 30) every one counts.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub const CLEAR_THRESHOLD: u32 = 2;

/// `ApplicationExitInfo.REASON_*`, by code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitReason {
    Unknown,
    ExitSelf,
    Signaled,
    LowMemory,
    Crash,
    CrashNative,
    Anr,
    InitializationFailure,
    PermissionChange,
    ExcessiveResourceUsage,
    UserRequested,
    UserStopped,
    DependencyDied,
    Other,
    Freezer,
    PackageStateChange,
    PackageUpdated,
}

impl ExitReason {
    pub fn from_code(code: i64) -> Self {
        use ExitReason::*;
        match code {
            1 => ExitSelf,
            2 => Signaled,
            3 => LowMemory,
            4 => Crash,
            5 => CrashNative,
            6 => Anr,
            7 => InitializationFailure,
            8 => PermissionChange,
            9 => ExcessiveResourceUsage,
            10 => UserRequested,
            11 => UserStopped,
            12 => DependencyDied,
            13 => Other,
            14 => Freezer,
            15 => PackageStateChange,
            16 => PackageUpdated,
            _ => Unknown,
        }
    }

    pub fn is_crash(self) -> bool {
        use ExitReason::*;
        matches!(self, Crash | CrashNative | Anr | InitializationFailure)
    }

    /// `Unknown` and `Other` are neither, so they count like a launch with no exit info.
    pub fn is_external_stop(self) -> bool {
        use ExitReason::*;
        matches!(
            self,
            ExitSelf
                | Signaled
                | LowMemory
                | PermissionChange
                | ExcessiveResourceUsage
                | UserRequested
                | UserStopped
                | DependencyDied
                | Freezer
                | PackageStateChange
                | PackageUpdated
        )
    }
}

/// The app process's newest exit, as the shell's `hello` reports it under `lastExit`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitInfo {
    pub reason: ExitReason,
    pub timestamp: i64,
    pub description: Option<String>,
}

impl ExitInfo {
    pub fn from_json(value: &Value) -> Option<Self> {
        let timestamp = value.get("timestamp")?.as_i64()?;
        let reason = value.get("reason").and_then(Value::as_i64).unwrap_or(0);
        Some(Self {
            reason: ExitReason::from_code(reason),
            timestamp,
            description: value
                .get("description")
                .and_then(Value::as_str)
                .map(String::from),
        })
    }
}

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
    /// The newest exit timestamp already judged, so one exit vetoes or confirms only once.
    pub handled_exit_at: i64,
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
    pub crash_confirmed: bool,
    pub exit_reason: Option<ExitReason>,
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

/// `resolveBootDecision`: an exit counts only when it is newer than the one last judged and
/// not older than the unfinished launch.
pub fn resolve(record: Option<&Record>, exit: Option<&ExitInfo>) -> Decision {
    let Some(record) = record.filter(|r| r.stage == Some(Stage::Starting)) else {
        return Decision::default();
    };
    let reason = exit
        .filter(|e| e.timestamp > record.handled_exit_at && e.timestamp >= record.started_at)
        .map(|e| e.reason);
    if reason.is_some_and(ExitReason::is_external_stop) {
        return Decision {
            exit_reason: reason,
            ..Decision::default()
        };
    }
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
        crash_confirmed: reason.is_some_and(ExitReason::is_crash),
        exit_reason: reason,
    }
}

/// `evaluate`'s record: this launch starting with `profile_id`, the one left after recovery.
pub fn starting(
    record: Option<&Record>,
    decision: &Decision,
    profile_id: Option<String>,
    now: i64,
    exit: Option<&ExitInfo>,
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
        handled_exit_at: record
            .map_or(0, |r| r.handled_exit_at)
            .max(exit.map_or(0, |e| e.timestamp)),
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
            handled_exit_at: 0,
        }
    }

    fn exit(code: i64, timestamp: i64) -> ExitInfo {
        ExitInfo {
            reason: ExitReason::from_code(code),
            timestamp,
            description: None,
        }
    }

    #[test]
    fn exit_codes_split_into_crashes_external_stops_and_the_rest() {
        use ExitReason::*;
        assert_eq!(ExitReason::from_code(4), Crash);
        assert_eq!(ExitReason::from_code(5), CrashNative);
        assert_eq!(ExitReason::from_code(6), Anr);
        assert_eq!(ExitReason::from_code(10), UserRequested);
        assert_eq!(ExitReason::from_code(99), Unknown);
        for crash in [Crash, CrashNative, Anr, InitializationFailure] {
            assert!(crash.is_crash() && !crash.is_external_stop(), "{crash:?}");
        }
        for stop in [
            ExitSelf,
            Signaled,
            LowMemory,
            UserRequested,
            UserStopped,
            PackageUpdated,
        ] {
            assert!(stop.is_external_stop() && !stop.is_crash(), "{stop:?}");
        }
        for neither in [Unknown, Other] {
            assert!(
                !neither.is_crash() && !neither.is_external_stop(),
                "{neither:?}"
            );
        }
    }

    #[test]
    fn exit_info_needs_a_timestamp() {
        let info = ExitInfo::from_json(
            &serde_json::json!({"reason": 6, "timestamp": 42, "description": "anr"}),
        )
        .unwrap();
        assert_eq!(info.reason, ExitReason::Anr);
        assert_eq!(info.timestamp, 42);
        assert_eq!(info.description.as_deref(), Some("anr"));
        assert_eq!(
            ExitInfo::from_json(&serde_json::json!({"reason": "4", "timestamp": 1}))
                .unwrap()
                .reason,
            ExitReason::Unknown
        );
        assert!(ExitInfo::from_json(&serde_json::json!({"reason": 4})).is_none());
        assert!(ExitInfo::from_json(&serde_json::json!("crash")).is_none());
    }

    #[test]
    fn an_external_stop_is_not_a_failed_launch() {
        let r = record(Some(Stage::Starting), Some("7"), 1);
        for code in [1, 2, 3, 10, 11] {
            let d = resolve(Some(&r), Some(&exit(code, 1500)));
            assert_eq!(d.recovery, Recovery::None, "{code}");
            assert_eq!(d.failure_count, 0, "{code}");
            assert!(d.exit_reason.is_some_and(ExitReason::is_external_stop));
        }
    }

    #[test]
    fn a_crash_is_confirmed_and_counted() {
        let r = record(Some(Stage::Starting), Some("7"), 1);
        let d = resolve(Some(&r), Some(&exit(5, 1500)));
        assert_eq!(d.recovery, Recovery::ClearProfile);
        assert_eq!(d.failure_count, 2);
        assert!(d.crash_confirmed);
        assert_eq!(d.exit_reason, Some(ExitReason::CrashNative));
        let other = resolve(Some(&r), Some(&exit(13, 1500)));
        assert_eq!(other.recovery, Recovery::ClearProfile);
        assert!(!other.crash_confirmed);
    }

    #[test]
    fn a_stale_or_judged_exit_is_ignored() {
        let r = record(Some(Stage::Starting), Some("7"), 0);
        let older = resolve(Some(&r), Some(&exit(10, 0)));
        assert_eq!(older.recovery, Recovery::SkipAutoSetup);
        assert_eq!(older.exit_reason, None);
        let judged = Record {
            handled_exit_at: 1600,
            ..r
        };
        let again = resolve(Some(&judged), Some(&exit(10, 1500)));
        assert_eq!(again.recovery, Recovery::SkipAutoSetup);
        assert_eq!(again.exit_reason, None);
    }

    #[test]
    fn the_judged_exit_is_remembered() {
        let r = Record {
            handled_exit_at: 900,
            ..record(Some(Stage::Running), Some("7"), 0)
        };
        let d = Decision::default();
        assert_eq!(
            starting(Some(&r), &d, None, 2000, Some(&exit(10, 1500))).handled_exit_at,
            1500
        );
        assert_eq!(
            starting(Some(&r), &d, None, 2000, Some(&exit(10, 800))).handled_exit_at,
            900
        );
        assert_eq!(
            starting(Some(&r), &d, None, 2000, None).handled_exit_at,
            900
        );
        let after = running(&starting(Some(&r), &d, None, 2000, None), &d);
        assert_eq!(after.handled_exit_at, 900);
    }

    #[test]
    fn a_finished_or_closed_launch_changes_nothing() {
        assert!(!resolve(None, None).is_degraded());
        assert!(!resolve(Some(&record(None, Some("1"), 0)), None).is_degraded());
        assert!(!resolve(Some(&record(Some(Stage::Running), Some("1"), 0)), None).is_degraded());
    }

    #[test]
    fn one_unfinished_launch_skips_the_setup_and_two_deselect_the_profile() {
        let first = resolve(Some(&record(Some(Stage::Starting), Some("7"), 0)), None);
        assert_eq!(first.recovery, Recovery::SkipAutoSetup);
        assert_eq!(first.failure_count, 1);
        let next = starting(None, &first, Some("7".into()), 5, None);
        assert_eq!(next.failure_count, 1);
        assert_eq!(next.last_failed_profile_id.as_deref(), Some("7"));
        let second = resolve(Some(&next), None);
        assert_eq!(second.recovery, Recovery::ClearProfile);
        assert_eq!(second.failed_profile_id.as_deref(), Some("7"));
        let without = resolve(Some(&record(Some(Stage::Starting), None, 3)), None);
        assert_eq!(without.recovery, Recovery::SkipAutoSetup);
    }

    #[test]
    fn running_resets_the_count_only_after_a_clean_launch() {
        let r = record(Some(Stage::Starting), Some("1"), 1);
        assert_eq!(running(&r, &Decision::default()).failure_count, 0);
        let degraded = resolve(Some(&r), None);
        let r = starting(Some(&r), &degraded, None, 2, None);
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
