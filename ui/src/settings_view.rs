//! The tools page and the settings pages under it (lib/views/tools.dart,
//! lib/views/config/general.dart, developer.dart, about.dart), as sections of rows.

use crate::store::{Settings, DELAY_TEST_TIMEOUTS};
use crate::{SetRow, SetSection};
use slint::{ModelRc, VecModel};

/// `AppLocalizations.supportedLocales`, with each one's label (`Locale.label`).
pub const LOCALES: [(&str, &str); 7] = [
    ("en", "英语"),
    ("zh", "中文简体"),
    ("ja", "日语"),
    ("ru", "俄语"),
    ("fr", "法语"),
    ("es", "西班牙语"),
    ("ko", "韩语"),
];

/// The label of a stored locale; older versions saved `zh_CN`.
pub fn locale_label(locale: Option<&str>) -> &'static str {
    let Some(locale) = locale else {
        return "默认";
    };
    let code = locale.split(['_', '-']).next().unwrap_or(locale);
    LOCALES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, l)| *l)
        .unwrap_or("默认")
}

fn row(id: &str, kind: &str, glyph: &str, title: &str, subtitle: &str) -> SetRow {
    SetRow {
        id: id.into(),
        kind: kind.into(),
        glyph: glyph.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        on: false,
        enabled: true,
    }
}

fn open(id: &str, glyph: &str, title: &str) -> SetRow {
    row(id, "open", glyph, title, "")
}

fn toggle(id: &str, title: &str, on: bool) -> SetRow {
    SetRow {
        on,
        ..row(id, "toggle", "", title, "")
    }
}

fn value(id: &str, title: &str, subtitle: &str) -> SetRow {
    row(id, "value", "", title, subtitle)
}

fn section(title: &str, footer: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: footer.into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

/// `ToolsView`: the pages the bar leaves out, then settings, then the rest. `pages` names the
/// settings pages there are.
pub fn tools(settings: &Settings, mobile: bool, pages: &[&str]) -> Vec<SetSection> {
    let mut more = Vec::new();
    if mobile {
        more.push(open("requests", "requests", "请求"));
    }
    more.push(open("resources", "resources", "资源"));
    if settings.general.open_logs {
        more.push(open("logs", "logs", "日志"));
    }
    let has = |p: &str| pages.contains(&p);
    let mut set = vec![
        row(
            "language",
            "value-icon",
            "language",
            "语言",
            locale_label(settings.locale.as_deref()),
        ),
        open("theme", "palette", "主题"),
    ];
    if has("access") {
        set.push(open("access", "apps", "访问控制"));
    }
    if has("backup") {
        set.push(open("backup", "cloud-sync", "备份与恢复"));
    }
    if has("advanced") {
        set.push(open("advanced", "wrench", "进阶配置"));
    }
    set.push(open("general", "settings", "常规"));
    let mut other = vec![open("disclaimer", "gavel", "免责声明")];
    if settings.developer_mode {
        other.push(open("developer", "cpu", "开发者模式"));
    }
    other.push(open("about", "info", "关于"));
    vec![
        section("", "", more),
        section("设置", "", set),
        section("其他", "", other),
    ]
}

/// `GeneralView` on Android.
pub fn general(settings: &Settings) -> Vec<SetSection> {
    let g = &settings.general;
    let mut inbound = vec![
        value("port", "端口", &g.mixed_port.to_string()),
        toggle("allow-lan", "局域网代理", g.allow_lan),
        toggle("external-controller", "外部控制器", g.external_controller),
    ];
    if g.external_controller {
        inbound.push(value("secret", "外部控制器密钥", &g.secret));
        inbound.push(toggle(
            "controller-cors",
            "允许跨域访问",
            g.external_controller_cors,
        ));
    }
    inbound.push(toggle("authentication", "认证", g.authentication.enable));
    if g.authentication.enable {
        inbound.push(value("auth-user", "账号", &g.authentication.username));
        inbound.push(value("auth-pass", "密码", &g.authentication.password));
    }
    let mut inbound_footer = "外部控制器监听 9090 端口。".to_string();
    if g.external_controller {
        inbound_footer.push_str("\n允许网页面板从其他网址连接。");
    }
    let hosts = if g.hosts.is_empty() {
        "无".to_string()
    } else {
        format!("{} 项", g.hosts.len())
    };
    vec![
        section(
            "启动与后台",
            "自动运行会在打开应用时启动代理。",
            vec![
                toggle("auto-run", "自动运行", g.auto_run),
                toggle("minimize-on-exit", "退出时最小化", g.minimize_on_exit),
                toggle("hidden", "从最近任务中隐藏", g.hidden),
                toggle(
                    "stop-action",
                    "通知栏显示停止按钮",
                    g.show_notification_stop_action,
                ),
            ],
        ),
        section(
            "请求",
            "关闭证书校验后，订阅和备份可能遭中间人攻击。",
            vec![
                row(
                    "user-agent",
                    "open",
                    "",
                    "用户代理",
                    g.global_ua.as_deref().unwrap_or("默认"),
                ),
                toggle("check-certificate", "校验 TLS 证书", g.check_certificate),
            ],
        ),
        section("入站", &inbound_footer, inbound),
        section(
            "连接",
            "",
            vec![
                value(
                    "delay-timeout",
                    "延迟测试超时",
                    &format!("{} ms", g.delay_test_timeout),
                ),
                toggle("find-process", "始终查找进程", g.find_process_always),
                toggle("close-connections", "自动关闭连接", g.close_connections),
                toggle("only-proxy", "仅统计代理流量", g.only_statistics_proxy),
            ],
        ),
        section(
            "内核",
            "",
            vec![
                row("hosts", "open", "", "Hosts", &hosts),
                toggle("append-system-dns", "追加系统DNS", g.append_system_dns),
            ],
        ),
        section(
            "日志与诊断",
            "关闭日志捕获会隐藏日志入口。",
            vec![toggle("open-logs", "日志捕获", g.open_logs)],
        ),
    ]
}

/// The app icon the about page shows, assets/images/icon.png as built into the library.
pub fn app_icon() -> Option<slint::Image> {
    let bytes: &[u8] = include_bytes!("../../assets/images/icon.png");
    let mut reader = png::Decoder::new(bytes).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    let pixels = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &buf[..info.buffer_size()],
        info.width,
        info.height,
    );
    Some(slint::Image::from_rgba8(pixels))
}

/// `MapEditView` of the hosts: each name and what it resolves to.
pub fn hosts(settings: &Settings) -> Vec<SetSection> {
    let rows = settings
        .general
        .hosts
        .iter()
        .map(|(k, v)| value(&format!("host:{k}"), k, v))
        .collect();
    vec![section("", "", rows)]
}

/// `DeveloperView`.
pub fn developer(settings: &Settings) -> Vec<SetSection> {
    vec![
        section(
            "",
            "",
            vec![toggle(
                "developer-mode",
                "开发者模式",
                settings.developer_mode,
            )],
        ),
        section(
            "选项",
            "",
            vec![
                row("message-test", "action", "", "消息测试", ""),
                row("logs-test", "action", "", "日志测试", ""),
                row("clear-data", "action", "", "清除数据", ""),
                row("prune-cache", "action", "", "修剪缓存", ""),
            ],
        ),
    ]
}

/// The sections under `AboutView`'s hero.
pub fn about(settings: &Settings) -> Vec<SetSection> {
    vec![
        section(
            "更新",
            "",
            vec![toggle(
                "auto-check-update",
                "自动检查更新",
                settings.auto_check_update,
            )],
        ),
        section(
            "更多",
            "",
            vec![
                row(
                    "link:https://github.com/koko123as/LineCat",
                    "link",
                    "code",
                    "项目",
                    "github.com/koko123as/LineCat",
                ),
                row(
                    "link:https://github.com/chen08209/concord",
                    "link",
                    "cpu",
                    "内核",
                    "github.com/chen08209/concord",
                ),
                row(
                    "link:https://t.me/FlClash",
                    "link",
                    "send",
                    "Telegram",
                    "t.me/FlClash",
                ),
            ],
        ),
        section(
            "其他贡献者",
            "",
            vec![
                row("contributor", "person", "J", "June2", "应用图标设计"),
                row("contributor", "person", "A", "Arue", "应用图标设计"),
            ],
        ),
    ]
}

/// The choices of a picker row, as (id, label, checked).
pub fn choices(settings: &Settings, key: &str) -> Vec<(String, String, bool)> {
    let g = &settings.general;
    match key {
        "language" => {
            let current = settings
                .locale
                .as_deref()
                .map(|l| l.split(['_', '-']).next().unwrap_or(l).to_string());
            std::iter::once(("".to_string(), "默认".to_string(), current.is_none()))
                .chain(
                    LOCALES.iter().map(|(c, l)| {
                        (c.to_string(), l.to_string(), current.as_deref() == Some(*c))
                    }),
                )
                .collect()
        }
        "delay-timeout" => DELAY_TEST_TIMEOUTS
            .iter()
            .map(|t| (t.to_string(), format!("{t} ms"), *t == g.delay_test_timeout))
            .collect(),
        "user-agent" => {
            std::iter::once(("".to_string(), "默认".to_string(), g.global_ua.is_none()))
                .chain(
                    g.user_agents
                        .iter()
                        .map(|u| (u.clone(), u.clone(), g.global_ua.as_deref() == Some(u))),
                )
                .collect()
        }
        _ => Vec::new(),
    }
}

/// `_PortForm`'s check of the mixed port, and the account and secret fields'.
pub fn check_input(key: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    match key {
        "port" => {
            if value.is_empty() {
                return Err("混合端口不能为空".into());
            }
            let port: i64 = value
                .parse()
                .map_err(|_| "混合端口必须为数字".to_string())?;
            if !(1024..=49151).contains(&port) {
                return Err("混合端口 必须在 1024 到 49151 之间".into());
            }
            Ok(port.to_string())
        }
        "auth-user" => Ok(value.replace(':', "")),
        "user-agent-add" if value.is_empty() => Err("用户代理不能为空".into()),
        "dav-file" if value.is_empty() => Err("文件不能为空".into()),
        _ => Ok(value.to_string()),
    }
}

/// `generateRandomSecret`: letters and digits from the system's random source.
pub fn random_secret(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut bytes = vec![0u8; len];
    let filled = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes))
        .is_ok();
    if !filled {
        let mut seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        for b in bytes.iter_mut() {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            *b = seed as u8;
        }
    }
    bytes
        .iter()
        .map(|b| CHARS[*b as usize % CHARS.len()] as char)
        .collect()
}

/// A tag like `v1.2.3` or `1.2.3-pre.1` as comparable numbers; None when it is no version.
pub fn version_parts(tag: &str) -> Option<Vec<u64>> {
    let core = tag.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty())
}

/// Whether the release `tag` is newer than the installed `version`.
pub fn is_newer(tag: &str, version: &str) -> bool {
    match (version_parts(tag), version_parts(version)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    fn ids(sections: &[SetSection]) -> Vec<Vec<String>> {
        sections
            .iter()
            .map(|s| s.rows.iter().map(|r| r.id.to_string()).collect())
            .collect()
    }

    #[test]
    fn tools_follow_flclash_order() {
        let mut s = Settings::default();
        assert_eq!(
            ids(&tools(&s, true, &[])),
            vec![
                vec!["requests", "resources"],
                vec!["language", "theme", "general"],
                vec!["disclaimer", "about"],
            ]
        );
        s.general.open_logs = true;
        s.developer_mode = true;
        let all = ids(&tools(&s, false, &["access", "backup", "advanced"]));
        assert_eq!(all[0], vec!["resources", "logs"]);
        assert_eq!(
            all[1],
            vec!["language", "theme", "access", "backup", "advanced", "general"]
        );
        assert_eq!(all[2], vec!["disclaimer", "developer", "about"]);
    }

    #[test]
    fn general_rows_open_with_their_switches() {
        let mut s = Settings::default();
        let rows = ids(&general(&s));
        assert_eq!(
            rows[2],
            vec!["port", "allow-lan", "external-controller", "authentication"]
        );
        s.general.external_controller = true;
        s.general.authentication.enable = true;
        let rows = ids(&general(&s));
        assert_eq!(
            rows[2],
            vec![
                "port",
                "allow-lan",
                "external-controller",
                "secret",
                "controller-cors",
                "authentication",
                "auth-user",
                "auth-pass"
            ]
        );
        assert!(general(&s)[2].footer.contains("其他网址"));
    }

    #[test]
    fn locales_read_older_names() {
        assert_eq!(locale_label(None), "默认");
        assert_eq!(locale_label(Some("zh_CN")), "中文简体");
        assert_eq!(locale_label(Some("ja")), "日语");
        let mut s = Settings::default();
        s.locale = Some("zh_CN".into());
        let c = choices(&s, "language");
        assert_eq!(c.len(), 8);
        assert!(c.iter().any(|(id, _, on)| id == "zh" && *on));
    }

    #[test]
    fn inputs_are_checked_like_the_forms() {
        assert_eq!(check_input("port", " 7899 ").unwrap(), "7899");
        assert!(check_input("port", "80").is_err());
        assert!(check_input("port", "x").is_err());
        assert!(check_input("port", "").is_err());
        assert_eq!(check_input("auth-user", "a:b").unwrap(), "ab");
        assert!(check_input("user-agent-add", " ").is_err());
    }

    #[test]
    fn secrets_and_versions() {
        let s = random_secret(16);
        assert_eq!(s.len(), 16);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(is_newer("v0.9.6", "0.9.5"));
        assert!(is_newer("v1.0.0-pre.1", "0.9.5"));
        assert!(!is_newer("v0.9.5", "0.9.5"));
        assert!(!is_newer("latest", "0.9.5"));
    }
}
