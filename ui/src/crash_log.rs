//! 工具 → 崩溃日志: the app process's last exit as the shell's `hello` reports it, and the
//! `crash.txt` the panic hook and a failed `run` write.

use crate::boot_guard::{ExitInfo, ExitReason};

/// The tail shown on the page; 复制 takes the whole file.
pub const MAX_LINES: usize = 300;
pub const MAX_CHARS: usize = 24 * 1024;

pub const NO_LOG: &str = "无崩溃日志";

pub fn reason_label(reason: ExitReason) -> &'static str {
    use ExitReason::*;
    match reason {
        Unknown => "未知原因",
        ExitSelf => "自行退出",
        Signaled => "被信号终止",
        LowMemory => "内存不足",
        Crash => "崩溃",
        CrashNative => "原生崩溃",
        Anr => "应用无响应（ANR）",
        InitializationFailure => "初始化失败",
        PermissionChange => "权限变更",
        ExcessiveResourceUsage => "资源占用过多",
        UserRequested => "用户请求停止",
        UserStopped => "被用户停止",
        DependencyDied => "依赖进程退出",
        Other => "被系统停止",
        Freezer => "被系统冻结",
        PackageStateChange => "应用状态变更",
        PackageUpdated => "应用已更新",
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitView {
    pub reason: String,
    pub time: String,
    pub description: String,
    pub crashed: bool,
}

/// `offset` is the device zone's offset from UTC at the exit, in seconds.
pub fn exit_view(exit: Option<&ExitInfo>, offset: i64) -> Option<ExitView> {
    let exit = exit?;
    Some(ExitView {
        reason: reason_label(exit.reason).into(),
        time: wall_time(exit.timestamp, offset),
        description: exit
            .description
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_string(),
        crashed: exit.reason.is_crash(),
    })
}

/// "2026-11-28 16:46:05" for a unix millisecond stamp shifted by `offset` seconds.
pub fn wall_time(ms: i64, offset: i64) -> String {
    let secs = ms.div_euclid(1000) + offset;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogView {
    pub text: String,
    /// Lines left out above the tail.
    pub hidden_lines: usize,
}

impl LogView {
    pub fn note(&self) -> String {
        if self.hidden_lines == 0 {
            String::new()
        } else {
            format!("已省略前 {} 行，复制可得全文", self.hidden_lines)
        }
    }
}

pub fn log_view(text: &str) -> Option<LogView> {
    let text = text.trim_end();
    if text.trim().is_empty() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut start = lines.len().saturating_sub(MAX_LINES);
    let mut chars: usize = lines[start..].iter().map(|l| l.chars().count() + 1).sum();
    while chars > MAX_CHARS && start + 1 < lines.len() {
        chars -= lines[start].chars().count() + 1;
        start += 1;
    }
    let mut shown = lines[start..].join("\n");
    if shown.chars().count() > MAX_CHARS {
        let skip = shown.chars().count() - MAX_CHARS;
        shown = shown.chars().skip(skip).collect();
    }
    Some(LogView {
        text: shown,
        hidden_lines: start,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exit_reason_has_a_chinese_label() {
        for code in 0..=17 {
            let label = reason_label(ExitReason::from_code(code));
            assert!(!label.is_empty() && !label.is_ascii(), "{code}");
        }
        assert_eq!(reason_label(ExitReason::Crash), "崩溃");
        assert_eq!(reason_label(ExitReason::CrashNative), "原生崩溃");
        assert_eq!(reason_label(ExitReason::LowMemory), "内存不足");
        assert_eq!(reason_label(ExitReason::Signaled), "被信号终止");
        assert!(reason_label(ExitReason::Anr).contains("ANR"));
    }

    #[test]
    fn wall_time_is_the_stamp_in_the_given_zone() {
        assert_eq!(wall_time(0, 0), "1970-01-01 00:00:00");
        // 2026-11-28 08:46:05 UTC.
        let ms = 1_795_855_565_000;
        assert_eq!(wall_time(ms, 0), "2026-11-28 08:46:05");
        assert_eq!(wall_time(ms, 8 * 3600), "2026-11-28 16:46:05");
        assert_eq!(wall_time(ms, -9 * 3600), "2026-11-27 23:46:05");
        assert_eq!(wall_time(951_782_400_000, 0), "2000-02-29 00:00:00");
    }

    #[test]
    fn an_exit_shows_its_reason_time_and_description() {
        assert_eq!(exit_view(None, 0), None);
        let exit = ExitInfo {
            reason: ExitReason::CrashNative,
            timestamp: 1_795_855_565_000,
            description: Some(" signal 11 \n".into()),
        };
        let v = exit_view(Some(&exit), 8 * 3600).unwrap();
        assert_eq!(v.reason, "原生崩溃");
        assert_eq!(v.time, "2026-11-28 16:46:05");
        assert_eq!(v.description, "signal 11");
        assert!(v.crashed);
        let quiet = ExitInfo {
            reason: ExitReason::UserStopped,
            timestamp: 0,
            description: None,
        };
        let v = exit_view(Some(&quiet), 0).unwrap();
        assert_eq!(v.description, "");
        assert!(!v.crashed);
    }

    #[test]
    fn a_blank_log_shows_nothing() {
        assert_eq!(log_view(""), None);
        assert_eq!(log_view(" \n\t\n"), None);
        let v = log_view("native panic: boom\nat a\n").unwrap();
        assert_eq!(v.text, "native panic: boom\nat a");
        assert_eq!(v.note(), "");
    }

    #[test]
    fn a_long_log_keeps_its_tail() {
        let text: String = (0..MAX_LINES + 20).map(|i| format!("line {i}\n")).collect();
        let v = log_view(&text).unwrap();
        assert_eq!(v.hidden_lines, 20);
        assert!(v.text.starts_with("line 20\n"));
        assert!(v.text.ends_with(&format!("line {}", MAX_LINES + 19)));
        assert!(v.note().contains("20"));

        let wide = "x".repeat(MAX_CHARS / 4);
        let text = [wide.as_str(); 8].join("\n");
        let v = log_view(&text).unwrap();
        assert!(v.text.chars().count() <= MAX_CHARS);
        assert_eq!(v.hidden_lines, 8 - v.text.lines().count());

        let single = "y".repeat(MAX_CHARS + 10);
        let v = log_view(&single).unwrap();
        assert_eq!(v.text.chars().count(), MAX_CHARS);
        assert_eq!(v.hidden_lines, 0);
    }
}
