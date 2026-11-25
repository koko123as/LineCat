//! `LiveStatusButton` and the deck of lib/views/dashboard/widgets/live_status*.dart: what
//! is happening now and still needs the user, in order of importance.

use serde_json::Value;

/// `ConfigWarning`, the ones at level warning.
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub path: String,
    pub message: String,
}

/// `warningsOf` then `.warnings`: the info level ones are dropped.
pub fn warnings_of(reply: &Value) -> Vec<Warning> {
    let list = reply.get("warnings").and_then(Value::as_array);
    list.into_iter()
        .flatten()
        .filter(|w| w.get("level").and_then(Value::as_str) == Some("warning"))
        .map(|w| {
            let text = |k: &str| w.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            Warning {
                path: text("path"),
                message: text("message"),
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Core {
    Stopped,
    Starting,
    Running,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Core(Core),
    Downloading {
        tag: String,
        received: i64,
        total: i64,
    },
    Ready {
        tag: String,
    },
    Consent(Vec<String>),
    Warnings(Vec<Warning>),
}

/// What the statuses are read from.
#[derive(Clone, Debug, Default)]
pub struct Sources {
    pub core: Option<Core>,
    /// Whether the Core runs in its own process; on Android it runs in the app's.
    pub separate_core: bool,
    pub update: Option<Status>,
    pub consent: Vec<String>,
    pub warnings: Vec<Warning>,
}

/// `watchLiveStatuses`.
pub fn statuses(s: &Sources) -> Vec<Status> {
    let mut out = Vec::new();
    match &s.core {
        Some(Core::Stopped) => out.push(Status::Core(Core::Stopped)),
        Some(Core::Starting) if s.separate_core => out.push(Status::Core(Core::Starting)),
        _ => {}
    }
    if let Some(u) = &s.update {
        out.push(u.clone());
    }
    if !s.consent.is_empty() {
        out.push(Status::Consent(s.consent.clone()));
    }
    if !s.warnings.is_empty() {
        out.push(Status::Warnings(s.warnings.clone()));
    }
    if s.separate_core && s.core == Some(Core::Running) {
        out.push(Status::Core(Core::Running));
    }
    out
}

fn is_running(s: &Status) -> bool {
    *s == Status::Core(Core::Running)
}

/// Tones shared by the pill and the cards: 0 neutral, 1 error, 2 warning, 3 success,
/// 4 primary.
pub const NEUTRAL: i32 = 0;
pub const ERROR: i32 = 1;
pub const WARNING: i32 = 2;
pub const SUCCESS: i32 = 3;
pub const PRIMARY: i32 = 4;

/// `_LiveStatusPill`, or the check of a running core alone (`kind` "running").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pill {
    pub kind: String,
    pub tone: i32,
    pub text: String,
    pub progress: f32,
}

fn kind(s: &Status) -> &'static str {
    match s {
        Status::Core(Core::Stopped) => "stopped",
        Status::Core(Core::Starting) => "starting",
        Status::Core(Core::Running) => "running",
        Status::Downloading { .. } => "downloading",
        Status::Ready { .. } => "ready",
        Status::Consent(_) => "consent",
        Status::Warnings(_) => "warnings",
    }
}

fn percent(received: i64, total: i64) -> Option<i64> {
    (total > 0).then(|| received * 100 / total)
}

/// None while nothing is happening.
pub fn pill(list: &[Status]) -> Option<Pill> {
    let top = list.first()?;
    if is_running(top) {
        return Some(Pill {
            kind: "running".into(),
            tone: SUCCESS,
            ..Pill::default()
        });
    }
    let count = list.iter().filter(|s| !is_running(s)).count();
    let tone = match top {
        Status::Core(Core::Stopped) => ERROR,
        Status::Warnings(_) | Status::Consent(_) => WARNING,
        Status::Ready { .. } => SUCCESS,
        _ => NEUTRAL,
    };
    let (text, progress) = match top {
        Status::Ready { .. } => ("100%".to_string(), 1.0),
        Status::Downloading {
            received, total, ..
        } => match percent(*received, *total) {
            Some(p) => (format!("{p}%"), *received as f32 / *total as f32),
            None => (crate::size(*received), 0.0),
        },
        _ if count > 1 => (count.to_string(), 0.0),
        Status::Warnings(w) => (w.len().to_string(), 0.0),
        _ => (String::new(), 0.0),
    };
    Some(Pill {
        kind: kind(top).into(),
        tone,
        text,
        progress,
    })
}

/// One card: its hero's tint, the two lines, the action and the question an action
/// asks first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub kind: String,
    pub tint: i32,
    pub title: String,
    pub body: String,
    pub action: String,
    pub action_tone: i32,
    pub ask_title: String,
    pub ask_body: String,
    pub ask_label: String,
    /// The download's percent or size, the update's tag, the module names.
    pub big: String,
    pub tag: String,
    pub progress: f32,
    pub chips: Vec<String>,
    pub warnings: Vec<Warning>,
}

/// `_progressText`.
fn progress_text(received: i64, total: i64) -> String {
    if total <= 0 {
        crate::size(received)
    } else {
        format!("{} / {}", crate::size(received), crate::size(total))
    }
}

/// `_StatusCard`'s content. `reason` is why the Core stopped, when known.
pub fn card(s: &Status, reason: Option<&str>, installed: &str) -> Card {
    let base = Card {
        kind: kind(s).into(),
        ..Card::default()
    };
    match s {
        Status::Core(Core::Stopped) => Card {
            tint: ERROR,
            title: "核心已停止".into(),
            body: reason.unwrap_or("应用与核心的连接已断开").into(),
            action: "重启核心".into(),
            action_tone: ERROR,
            ..base
        },
        Status::Core(Core::Starting) => Card {
            tint: PRIMARY,
            title: "正在启动核心".into(),
            body: "正在连接核心".into(),
            ..base
        },
        Status::Core(Core::Running) => Card {
            tint: NEUTRAL,
            title: "核心运行中".into(),
            body: "应用已连接到核心".into(),
            action: "强制重启".into(),
            ask_title: "强制重启核心？".into(),
            ask_body: "进行中的连接会断开".into(),
            ask_label: "重启".into(),
            ..base
        },
        Status::Downloading {
            tag,
            received,
            total,
        } => Card {
            tint: PRIMARY,
            title: "正在下载更新".into(),
            body: progress_text(*received, *total),
            action: "取消".into(),
            ask_title: "要停止下载更新吗？".into(),
            ask_body: "已下载的部分会被删除".into(),
            ask_label: "停止".into(),
            big: match percent(*received, *total) {
                Some(p) => format!("{p}%"),
                None => crate::size(*received),
            },
            tag: tag.clone(),
            progress: if *total > 0 {
                *received as f32 / *total as f32
            } else {
                0.0
            },
            ..base
        },
        Status::Ready { tag } => Card {
            tint: SUCCESS,
            title: "更新已就绪".into(),
            body: "更新已下载，点击安装".into(),
            action: "安装更新".into(),
            action_tone: SUCCESS,
            chips: vec![
                installed.trim_start_matches('v').to_string(),
                tag.trim_start_matches('v').to_string(),
            ],
            ..base
        },
        Status::Consent(names) => {
            let mut chips: Vec<String> = names.iter().take(2).cloned().collect();
            if names.len() > 2 {
                chips.push(format!("+{}", names.len() - 2));
            }
            Card {
                tint: WARNING,
                title: "模块已关闭".into(),
                body: if names.len() == 1 {
                    format!("{} 的更新带来了你未同意过的脚本或网络改动", names[0])
                } else {
                    format!(
                        "{} 个模块的更新带来了你未同意过的脚本或网络改动",
                        names.len()
                    )
                },
                action: "查看".into(),
                action_tone: WARNING,
                chips,
                ..base
            }
        }
        Status::Warnings(items) => Card {
            tint: WARNING,
            title: "配置警告".into(),
            body: format!("加载配置时有 {} 条警告", items.len()),
            action: "查看全部".into(),
            warnings: items.clone(),
            ..base
        },
    }
}

/// `showLiveStatusPreview`: one card of every kind, for the developer page.
pub fn preview() -> Vec<Status> {
    const MB: i64 = 1024 * 1024;
    let w = |path: &str, message: &str| Warning {
        path: path.into(),
        message: message.into(),
    };
    vec![
        Status::Core(Core::Stopped),
        Status::Core(Core::Starting),
        Status::Ready {
            tag: "v9.9.9".into(),
        },
        Status::Downloading {
            tag: "v9.9.9".into(),
            received: 18 * MB,
            total: 43 * MB,
        },
        Status::Consent(vec!["Ads".into(), "Scripts".into()]),
        Status::Warnings(vec![
            w(
                "proxies[12]",
                "masque is not supported, the proxy was removed",
            ),
            w("sniffer.override", "unknown key"),
            w(
                "rules[48]",
                "the rule names a missing group and goes to DIRECT instead",
            ),
            w("tun.stack", "mixed runs as the userspace stack"),
            w(
                "external-ui",
                "key is not supported by this core and is ignored",
            ),
        ]),
        Status::Core(Core::Running),
    ]
}

/// `_TicksPainter`'s sixty ticks in a 132 box as two paths, the lit ones and the rest.
pub fn ticks(value: f32) -> (String, String) {
    let lit = (value.clamp(0.0, 1.0) * 60.0).round() as usize;
    let (mut on, mut off) = (String::new(), String::new());
    for i in 0..60 {
        let angle = i as f32 / 60.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let outer = 62.0;
        let inner = outer - if i % 5 == 0 { 13.0 } else { 8.0 };
        let (s, c) = angle.sin_cos();
        let seg = format!(
            "M{:.1} {:.1}L{:.1} {:.1}",
            66.0 + c * outer,
            66.0 + s * outer,
            66.0 + c * inner,
            66.0 + s * inner
        );
        if i < lit {
            on.push_str(&seg);
        } else {
            off.push_str(&seg);
        }
    }
    (on, off)
}

/// The deck's front after a drag of `dy` (down brings the next card forward), past
/// `_flipDistance` or flung faster than `_flipVelocity`.
pub fn flip(front: usize, count: usize, dy: f32, velocity: f32) -> usize {
    if count < 2 {
        return 0;
    }
    if dy > 60.0 || velocity > 700.0 {
        (front + 1) % count
    } else if dy < -60.0 || velocity < -700.0 {
        (front + count - 1) % count
    } else {
        front
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_warning_level_items_count() {
        let w = warnings_of(&json!({"warnings": [
            {"level": "warning", "path": "a", "message": "x"},
            {"level": "info", "path": "b", "message": "y"}]}));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].path, "a");
        assert!(warnings_of(&json!({})).is_empty());
    }

    #[test]
    fn statuses_follow_the_importance_order() {
        let warn = vec![Warning {
            path: String::new(),
            message: "m".into(),
        }];
        let s = Sources {
            core: Some(Core::Stopped),
            warnings: warn.clone(),
            consent: vec!["Ads".into()],
            ..Default::default()
        };
        let l = statuses(&s);
        assert_eq!(
            l.iter().map(kind).collect::<Vec<_>>(),
            ["stopped", "consent", "warnings"]
        );
        let s = Sources {
            core: Some(Core::Running),
            ..Default::default()
        };
        assert!(statuses(&s).is_empty());
        let s = Sources {
            core: Some(Core::Starting),
            ..Default::default()
        };
        assert!(statuses(&s).is_empty());
        let s = Sources {
            core: Some(Core::Running),
            separate_core: true,
            warnings: warn,
            ..Default::default()
        };
        assert_eq!(kind(statuses(&s).last().unwrap()), "running");
    }

    #[test]
    fn pill_counts_and_shows_progress() {
        assert_eq!(pill(&[]), None);
        let p = pill(&[Status::Core(Core::Running)]).unwrap();
        assert_eq!(p.kind, "running");
        let warn = Status::Warnings(vec![
            Warning {
                path: String::new(),
                message: "a".into(),
            };
            3
        ]);
        assert_eq!(pill(&[warn.clone()]).unwrap().text, "3");
        let both = [Status::Core(Core::Stopped), warn.clone()];
        let p = pill(&both).unwrap();
        assert_eq!((p.text.as_str(), p.tone), ("2", ERROR));
        let dl = Status::Downloading {
            tag: "v1".into(),
            received: 25,
            total: 100,
        };
        let p = pill(&[dl, warn]).unwrap();
        assert_eq!((p.text.as_str(), p.progress), ("25%", 0.25));
        let p = pill(&[Status::Core(Core::Stopped)]).unwrap();
        assert_eq!(p.text, "");
    }

    #[test]
    fn cards_read_as_flclash() {
        let c = card(&Status::Core(Core::Stopped), None, "1.0.0");
        assert_eq!(
            (c.body.as_str(), c.action_tone),
            ("应用与核心的连接已断开", ERROR)
        );
        let c = card(&Status::Core(Core::Stopped), Some("boom"), "1.0.0");
        assert_eq!(c.body, "boom");
        let c = card(
            &Status::Consent(vec!["A".into(), "B".into(), "C".into()]),
            None,
            "",
        );
        assert_eq!(c.chips, ["A", "B", "+1"]);
        assert!(c.body.starts_with("3 个模块"));
        let c = card(&Status::Ready { tag: "v2.0".into() }, None, "v1.0");
        assert_eq!(c.chips, ["1.0", "2.0"]);
        let c = card(&preview()[3], None, "");
        assert_eq!(
            (c.big.as_str(), c.body.as_str()),
            ("41%", "18.0 MB / 43.0 MB")
        );
        assert_eq!(preview().len(), 7);
    }

    #[test]
    fn ticks_split_by_the_share() {
        let (on, off) = ticks(0.25);
        assert_eq!(on.matches('M').count(), 15);
        assert_eq!(off.matches('M').count(), 45);
        assert!(off.starts_with("M66.0 4.0") || on.starts_with("M66.0 4.0"));
        assert!(ticks(0.0).0.is_empty());
    }

    #[test]
    fn a_drag_flips_past_the_distance() {
        assert_eq!(flip(0, 3, 80.0, 0.0), 1);
        assert_eq!(flip(0, 3, -80.0, 0.0), 2);
        assert_eq!(flip(2, 3, 30.0, 0.0), 2);
        assert_eq!(flip(2, 3, 10.0, 900.0), 0);
        assert_eq!(flip(0, 1, 200.0, 0.0), 0);
    }
}
