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
        lead: "".into(),
        chip: "".into(),
        trailing: "".into(),
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
        open("theme", "paintbrush", "主题"),
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
    let mut other = Vec::new();
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
    let (core, core_footer) = crate::overrides::GENERAL.inline(
        &settings.general_override,
        vec![
            row("hosts", "open", "", "Hosts", &hosts),
            toggle("append-system-dns", "追加系统DNS", g.append_system_dns),
        ],
    );
    vec![
        section(
            "启动与后台",
            "自动运行会在打开应用时启动代理。",
            vec![
                toggle("auto-run", "自动运行", g.auto_run),
                open("on-demand", "", "按需运行"),
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
        section("内核", &core_footer, core),
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

/// `MapEditView`: each name and its value, tap to edit; the description under them.
pub fn map(entries: &std::collections::BTreeMap<String, String>, desc: &str) -> Vec<SetSection> {
    let rows: Vec<SetRow> = entries
        .iter()
        .map(|(k, v)| value(&format!("entry:{k}"), k, v))
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    vec![section("", desc, rows)]
}

/// `AdvancedConfigView`: the pages built so far.
pub fn advanced(_settings: &Settings) -> Vec<SetSection> {
    vec![
        section(
            "",
            "",
            vec![
                open("network", "key", "网络"),
                open("dns", "dns", "DNS"),
                open("rules", "rules", "附加规则"),
                open("scripts", "code", "脚本"),
            ],
        ),
        section(
            "自定义",
            "",
            vec![
                open("rule-sets", "resources", "规则集"),
                open("filters", "filter", "筛选"),
            ],
        ),
    ]
}

fn count(items: &[String]) -> String {
    if items.is_empty() {
        "无".into()
    } else {
        format!("{} 项", items.len())
    }
}

/// `NetworkListView` on Android: `VpnSections` and `TunSection`.
pub fn network(settings: &Settings) -> Vec<SetSection> {
    let n = &settings.network;
    let mut footer = Vec::new();
    if settings.general.authentication.enable {
        footer.push("认证启用时，系统代理不生效。");
    }
    footer.push("排除域名仅在系统代理启用时生效。");
    vec![
        section("", "", vec![toggle("vpn", "VPN", n.vpn)]),
        section(
            "VPN",
            &footer.join("\n"),
            vec![
                toggle("system-proxy", "系统代理", n.system_proxy),
                row(
                    "bypass-domain",
                    "open",
                    "",
                    "排除域名",
                    &count(&n.bypass_domain),
                ),
                toggle("allow-bypass", "允许应用绕过VPN", n.allow_bypass),
                toggle("ipv6", "IPv6", n.ipv6),
                toggle("dns-hijacking", "DNS劫持", n.dns_hijacking),
            ],
        ),
        section(
            "虚拟网卡",
            "",
            vec![
                value("mtu", "MTU", &n.mtu.to_string()),
                value("congestion", "TCP 拥塞控制", &n.congestion_controller),
                toggle("bypass-private", "绕过私有路由地址", n.bypass_private_route),
                row(
                    "route-address",
                    "open",
                    "",
                    "路由地址",
                    &count(&n.route_address),
                ),
                row(
                    "route-exclude",
                    "open",
                    "",
                    "排除路由地址",
                    &count(&n.route_exclude_address),
                ),
            ],
        ),
    ]
}

/// `ConfigListEditItem`'s page: one row per item, tap to edit; the description under them.
pub fn list(items: &[String], desc: &str) -> Vec<SetSection> {
    let rows: Vec<SetRow> = items
        .iter()
        .enumerate()
        .map(|(i, v)| row(&format!("item:{i}"), "action", "", v, ""))
        .collect();
    if rows.is_empty() {
        return Vec::new();
    }
    vec![section("", desc, rows)]
}

/// `PortForm._validate` on Android (mixed, http and socks): each in 1024..=49151, the
/// last two may be 0 (off), and no two the same.
pub fn check_ports(values: [&str; 3]) -> Result<[i64; 3], String> {
    const LABELS: [&str; 3] = ["混合端口", "端口", "Socks端口"];
    let mut out = [0i64; 3];
    for (i, v) in values.iter().enumerate() {
        let v = v.trim();
        let label = LABELS[i];
        if v.is_empty() {
            return Err(format!("{label}不能为空"));
        }
        let port: i64 = v.parse().map_err(|_| format!("{label}必须为数字"))?;
        if !(i > 0 && port == 0) && !(1024..=49151).contains(&port) {
            return Err(format!("{label} 必须在 1024 到 49151 之间"));
        }
        if port != 0 && out[..i].contains(&port) {
            return Err("请输入不同的端口".into());
        }
        out[i] = port;
    }
    Ok(out)
}

/// `UserAgentsView`: the default, then each one with its edit button; the picked one checked.
pub fn user_agents(settings: &Settings) -> Vec<SetSection> {
    let g = &settings.general;
    let mut rows = vec![SetRow {
        on: g.global_ua.is_none(),
        ..row("ua:", "check", "", "默认", "")
    }];
    rows.extend(g.user_agents.iter().enumerate().map(|(i, u)| SetRow {
        on: g.global_ua.as_deref() == Some(u.as_str()),
        ..row(&format!("ua:{i}"), "check", "edit", u, "")
    }));
    vec![section("", "", rows)]
}

/// `validateCidr`: an IPv4 or IPv6 address with a prefix length in range.
pub fn valid_cidr(value: &str) -> bool {
    let Some((addr, prefix)) = value.split_once('/') else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u8>() else {
        return false;
    };
    match addr.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(_)) => prefix <= 32,
        Ok(std::net::IpAddr::V6(_)) => prefix <= 128,
        Err(_) => false,
    }
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
                row("live-status-test", "action", "", "实时状态卡片测试", ""),
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
                    "link:https://github.com/chen08209/FlClash",
                    "link",
                    "code",
                    "原项目",
                    "github.com/chen08209/FlClash",
                ),
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
        "congestion" => crate::store::CONGESTION_CONTROLLERS
            .iter()
            .map(|c| {
                let on = *c == settings.network.congestion_controller;
                (c.to_string(), c.to_string(), on)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The input dialogs' checks: the value as saved, or what is wrong with it.
pub fn check_input(key: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    match key {
        "auth-user" => Ok(value.replace(':', "")),
        k if (k == "user-agent-add" || k.starts_with("ua-edit:")) && value.is_empty() => {
            Err("用户代理不能为空".into())
        }
        "dav-file" if value.is_empty() => Err("文件不能为空".into()),
        "mtu" => {
            let (min, max, _) = crate::store::TUN_MTU;
            let mtu: i64 = value.parse().map_err(|_| "MTU必须为数字".to_string())?;
            if !(min..=max).contains(&mtu) {
                return Err(format!("MTU必须在 {min} 到 {max} 之间"));
            }
            Ok(mtu.to_string())
        }
        k if k.starts_with("list:") => {
            if value.is_empty() {
                return Err("值不能为空".into());
            }
            let target = k["list:".len()..].rsplit_once(':').map_or("", |(t, _)| t);
            if target.starts_with("route-") && !valid_cidr(value) {
                return Err("请输入有效的 CIDR".into());
            }
            if let Some((id, path)) = target.split_once(':') {
                if let Some(spec) = crate::overrides::spec(id) {
                    spec.check(path, value)?;
                }
            }
            Ok(value.to_string())
        }
        k if k.starts_with("ov:") => {
            let (id, path) = k[3..].split_once(':').unwrap_or_default();
            match crate::overrides::spec(id) {
                Some(spec) => spec.check(path, value).map(|_| value.to_string()),
                None => Ok(value.to_string()),
            }
        }
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
    #[test]
    fn cidr_and_network_rows() {
        use super::*;
        assert_eq!(check_ports(["7890", "0", "0"]), Ok([7890, 0, 0]));
        let mut s = Settings::default();
        s.general.user_agents = vec!["a/1".into(), "b/2".into()];
        s.general.global_ua = Some("b/2".into());
        let rows: Vec<SetRow> = user_agents(&s)[0].rows.iter().collect();
        assert_eq!(rows.len(), 3);
        assert!(!rows[0].on && !rows[1].on && rows[2].on);
        assert_eq!(rows[2].id, "ua:1");
        assert!(check_input("ua-edit:0", " ").is_err());
        assert_eq!(
            check_ports(["7890", "7890", "0"]).unwrap_err(),
            "请输入不同的端口"
        );
        assert_eq!(
            check_ports(["0", "0", "0"]).unwrap_err(),
            "混合端口 必须在 1024 到 49151 之间"
        );
        assert_eq!(check_ports(["7890", "", "0"]).unwrap_err(), "端口不能为空");
        assert_eq!(
            check_ports(["7890", "x", "0"]).unwrap_err(),
            "端口必须为数字"
        );
        assert!(valid_cidr("10.0.0.0/8"));
        assert!(valid_cidr("fd00::/8"));
        assert!(!valid_cidr("10.0.0.0/33"));
        assert!(!valid_cidr("10.0.0.0"));
        assert!(!valid_cidr("example.com/8"));
        assert_eq!(
            check_input("mtu", "1279").unwrap_err(),
            "MTU必须在 1280 到 65535 之间"
        );
        assert_eq!(check_input("mtu", " 1500 ").unwrap(), "1500");
        assert!(check_input("list:route-address:new", "1.2.3.4").is_err());
        assert!(check_input("list:bypass-domain:new", "*.lan").is_ok());
        let s = Settings::default();
        let n = network(&s);
        assert_eq!(n.len(), 3);
        assert_eq!(list(&["a".into(), "b".into()], "")[0].rows.row_count(), 2);
        assert!(list(&[], "").is_empty());
        assert!(check_input("list:dns:fallback-filter.ipcidr:new", "1.2.3.4").is_err());
        assert!(check_input("ov:dns:listen", "1.2.3.4").is_err());
        assert!(check_input("ov:dns:listen", "0.0.0.0:53").is_ok());
    }

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
                vec!["about"],
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
        assert_eq!(all[2], vec!["developer", "about"]);
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
        let s = Settings {
            locale: Some("zh_CN".into()),
            ..Default::default()
        };
        let c = choices(&s, "language");
        assert_eq!(c.len(), 8);
        assert!(c.iter().any(|(id, _, on)| id == "zh" && *on));
    }

    #[test]
    fn inputs_are_checked_like_the_forms() {
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
