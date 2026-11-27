//! The launch guard (FlClash: lib/common/boot_guard.dart, boot_record.dart): a launch that
//! never reached running is a failure; the next one skips the automatic setup, and a second
//! in a row with a profile selected clears it, to break a crash loop. The shell reports no
//! process exit reasons, so an unfinished launch always counts.

use serde::{Deserialize, Serialize};

pub const CLEAR_THRESHOLD: i64 = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BootRecord {
    /// `starting`, `running`, or none once the app closed.
    pub stage: Option<String>,
    pub profile_id: Option<String>,
    pub started_at: u64,
    pub failure_count: i64,
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
    pub failure_count: i64,
    pub failed_profile_id: Option<String>,
}

impl Decision {
    pub fn degraded(&self) -> bool {
        self.recovery != Recovery::None
    }
}

/// `resolveBootDecision` with no exit info.
pub fn decide(record: Option<&BootRecord>) -> Decision {
    let Some(r) = record.filter(|r| r.stage.as_deref() == Some("starting")) else {
        return Decision::default();
    };
    let failure_count = r.failure_count + 1;
    let clear = failure_count >= CLEAR_THRESHOLD && r.profile_id.is_some();
    Decision {
        recovery: if clear {
            Recovery::ClearProfile
        } else {
            Recovery::SkipAutoSetup
        },
        failure_count,
        failed_profile_id: r.profile_id.clone(),
    }
}

pub fn starting(
    previous: Option<&BootRecord>,
    d: &Decision,
    profile_id: Option<String>,
    now: u64,
) -> BootRecord {
    BootRecord {
        stage: Some("starting".into()),
        profile_id: if d.recovery == Recovery::ClearProfile {
            None
        } else {
            profile_id
        },
        started_at: now,
        failure_count: d.failure_count,
        last_failed_profile_id: d
            .failed_profile_id
            .clone()
            .or_else(|| previous.and_then(|p| p.last_failed_profile_id.clone())),
    }
}

/// `markRunning`: a launch that got through resets the count unless it was degraded.
pub fn running(r: &BootRecord, d: &Decision) -> BootRecord {
    BootRecord {
        stage: Some("running".into()),
        failure_count: if d.degraded() { r.failure_count } else { 0 },
        ..r.clone()
    }
}

pub fn closed(r: &BootRecord) -> BootRecord {
    BootRecord {
        stage: None,
        failure_count: 0,
        ..r.clone()
    }
}

pub fn tip(d: &Decision, profile_label: &str) -> Option<(&'static str, String)> {
    match d.recovery {
        Recovery::None => None,
        Recovery::SkipAutoSetup => Some((
            "启动未完成",
            "应用上次在启动过程中意外退出。已跳过本次自动配置，你可以手动启动重试。".into(),
        )),
        Recovery::ClearProfile => Some((
            "检测到崩溃",
            format!(
                "应用连续两次未能完成启动。为打断崩溃循环，已取消选中配置 {profile_label}，并跳过本次自动配置，你可以随时重新选中它。"
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(stage: Option<&str>, profile: Option<&str>, failures: i64) -> BootRecord {
        BootRecord {
            stage: stage.map(String::from),
            profile_id: profile.map(String::from),
            failure_count: failures,
            ..Default::default()
        }
    }

    #[test]
    fn a_finished_launch_needs_nothing() {
        assert_eq!(decide(None), Decision::default());
        assert!(!decide(Some(&record(Some("running"), Some("p"), 0))).degraded());
        assert!(!decide(Some(&record(None, Some("p"), 0))).degraded());
    }

    #[test]
    fn unfinished_launches_skip_then_clear() {
        let first = decide(Some(&record(Some("starting"), Some("p"), 0)));
        assert_eq!(first.recovery, Recovery::SkipAutoSetup);
        let next = starting(None, &first, Some("p".into()), 5);
        assert_eq!(next.failure_count, 1);
        let second = decide(Some(&next));
        assert_eq!(second.recovery, Recovery::ClearProfile);
        assert_eq!(second.failed_profile_id.as_deref(), Some("p"));
        let cleared = starting(Some(&next), &second, Some("p".into()), 6);
        assert_eq!(cleared.profile_id, None);
        assert_eq!(cleared.last_failed_profile_id.as_deref(), Some("p"));
        let no_profile = decide(Some(&record(Some("starting"), None, 3)));
        assert_eq!(no_profile.recovery, Recovery::SkipAutoSetup);
    }

    #[test]
    fn running_resets_only_a_clean_launch() {
        let r = record(Some("starting"), Some("p"), 1);
        assert_eq!(running(&r, &Decision::default()).failure_count, 0);
        let degraded = decide(Some(&r));
        assert_eq!(running(&r, &degraded).failure_count, 1);
        assert_eq!(closed(&r).stage, None);
        assert!(tip(&degraded, "我的").unwrap().1.contains("我的"));
        assert_eq!(tip(&Decision::default(), ""), None);
    }
}
