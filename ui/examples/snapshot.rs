//! Renders every page with sample data to PNG files, so the layout can be checked on a
//! desktop without a device: `cargo run --example snapshot -- <out dir>`.
use seasoncat_ui::*;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{Model, ModelRc, Rgb8Pixel, VecModel};
use std::rc::Rc;

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn sample_requests(now: i64) -> requests_view::ReqState {
    use serde_json::json;
    let mut st = requests_view::ReqState::default();
    let t = |ago: i64| now - ago * 1000;
    let conn = |id: i64,
                ago: i64,
                host: &str,
                process: &str,
                chains: serde_json::Value,
                rule: &str,
                payload: &str| {
        json!({"id": id, "start": t(ago), "network": "tcp", "sourceIp": "172.19.0.1", "sourcePort": 40000 + id,
               "destinationIp": "198.18.0.12", "destinationPort": 443, "host": host, "process": process,
               "processPath": format!("/data/app/{process}"), "uid": 10234, "chains": chains, "rule": rule,
               "rulePayload": payload, "ruleIndex": 12, "dnsMode": "fake-ip", "destinationGeoIp": ["US"],
               "destinationAsn": "AS15169 Google LLC", "upload": 18_432 * id, "download": 1_204_224 * id,
               "dnsQueries": [7],
               "timing": {"accepted": t(ago), "dnsStart": 1200, "dnsEnd": 13_400, "routed": 14_000,
                          "dialStart": 14_500, "connected": 92_000, "dialEnd": 120_000}})
    };
    let mut failed = conn(
        5,
        200,
        "api.example.invalid",
        "com.example.app",
        json!(["DIRECT"]),
        "MATCH",
        "",
    );
    failed["failure"] =
        json!({"kind": "timeout", "message": "dial tcp 203.0.113.5:443: i/o timeout"});
    let mut mitm = conn(
        6,
        20,
        "www.google.com",
        "com.android.chrome",
        json!(["香港 01", "节点选择"]),
        "DOMAIN-SUFFIX",
        "google.com",
    );
    mitm["mitm"] = json!(true);
    st.conns.take(
        &json!({"connections": [
            conn(1, 300, "www.youtube.com", "com.google.android.youtube", json!(["香港 01", "节点选择"]), "GEOSITE", "youtube"),
            conn(2, 240, "", "com.tencent.mm", json!(["DIRECT"]), "GEOIP", "CN"),
            conn(3, 150, "github.com", "com.github.android", json!(["日本 03", "自动选择", "节点选择"]), "DOMAIN-SUFFIX", "github.com"),
            conn(4, 90, "dns.google", "", json!(["DIRECT"]), "", ""),
            failed, mitm
        ]}),
        now - 3000,
    );
    st.conns.take(&json!({"counters": [{"id": 1, "upload": 60_000, "download": 2_600_000}, {"id": 6, "upload": 9000, "download": 380_000}], "closed": [2, 5], "ended": [{"id": 2, "reason": "remote", "endedAt": t(100)}]}), now - 1000);
    for (i, (domain, kind, rcode, cached, ago)) in [
        ("www.youtube.com", "A", "NOERROR", false, 301),
        ("github.com", "AAAA", "NOERROR", true, 151),
        ("api.example.invalid", "A", "NXDOMAIN", false, 201),
        ("www.google.com", "HTTPS", "NOERROR", false, 21),
    ]
    .iter()
    .enumerate()
    {
        st.dns.push(&json!({"id": 6 + i as i64, "domain": domain, "type": kind, "initiator": "app",
            "upstream": "https://dns.alidns.com/dns-query", "cached": cached,
            "answers": if *rcode == "NOERROR" { json!(["198.18.0.12", "198.18.0.13"]) } else { json!([]) },
            "rcode": rcode, "delay": 18 + i as i64 * 7, "time": t(*ago)}));
    }
    let start = |ago: i64| {
        let secs = (now / 1000 - ago).rem_euclid(86_400);
        let days = (now / 1000 - ago).div_euclid(86_400);
        let (y, m, d) = requests::civil(days);
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.120Z",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        )
    };
    let ex = |id: i64, ago: i64, method: &str, path: &str, status: i64, kind: &str| {
        json!({"id": id, "start": start(ago), "duration": 230 + id * 40, "method": method,
               "url": format!("https://www.google.com{path}"), "status": status, "response-size": 2048 * id,
               "host": "www.google.com", "path": path, "status-class": "", "kind": kind,
               "connection-id": 6, "app": {"process": "com.android.chrome"},
               "action-kinds": if id == 3 { json!(["header-rewrite"]) } else { json!([]) }, "rewritten": id == 3})
    };
    st.http.take(vec![http_capture::Page::parse(&json!({"version": 3, "oldest": 1, "exchanges": [
        ex(1, 19, "GET", "/", 200, "html"), ex(2, 18, "POST", "/gen_204?atyp=i", 204, "other"),
        ex(3, 17, "GET", "/complete/search?q=slint", 200, "json"), ex(4, 15, "GET", "/favicon.ico", 404, "image")]}))]);
    st.exchange = Some(
        json!({"id": 3, "start": start(17), "duration": 350, "ttfb": 120, "method": "GET",
        "url": "https://www.google.com/complete/search?q=slint", "proto": "HTTP/2.0", "status": 200, "reason": "OK",
        "request-size": 0, "response-size": 6144, "connection-id": 6,
        "request-header": {"Accept": ["*/*"], "User-Agent": ["Mozilla/5.0 (Linux; Android 15)"]},
        "response-header": {"Content-Type": ["application/json; charset=UTF-8"], "Cache-Control": ["private, max-age=3600"]},
        "actions": [{"kind": "header-rewrite", "phase": "response", "detail": "Cache-Control", "module": ""}],
        "route": {"rule": "DOMAIN-SUFFIX", "rule-payload": "google.com", "chains": ["香港 01", "节点选择"]}}),
    );
    st.bodies = [
        None,
        Some("[\"slint\",[\"slint ui\",\"slint rust\",\"slint android\"]]".into()),
    ];
    st
}

/// The requests page's properties for a shot: `req:<segment>:<selected>:<tab>:<extra>`.
fn requests_shot(
    ui: &AppWindow,
    st: &mut requests_view::ReqState,
    now: i64,
    overlay: &str,
    width: u32,
) {
    st.cond_width = if width > 1000 {
        264.0
    } else {
        width as f32 - 128.0
    };
    let parts: Vec<&str> = overlay.split(':').collect();
    let num = |i: usize| parts.get(i).and_then(|p| p.parse::<i64>().ok());
    st.segment = num(1).unwrap_or(0) as i32;
    st.selected = [None; 3];
    let seg = st.segment as usize;
    st.selected[seg] = num(2).filter(|id| *id > 0);
    st.tab[seg] = num(3).unwrap_or(0) as i32;
    let extra = parts.get(4).copied().unwrap_or("");
    st.filters = Default::default();
    if extra == "filter" {
        st.filters[0].status = 1;
        st.filters[0].add(requests::Condition {
            field: "host".into(),
            value: "google".into(),
            excludes: false,
            equals: false,
        });
        st.filters[0].add(requests::Condition {
            field: "host".into(),
            value: "youtube".into(),
            excludes: false,
            equals: false,
        });
        for v in ["facebook.com", "twitter", "instagram.com"] {
            st.filters[0].add(requests::Condition {
                field: "host".into(),
                value: v.into(),
                excludes: false,
                equals: false,
            });
        }
        st.filters[0].add(requests::Condition {
            field: "app".into(),
            value: "com.tencent.mm".into(),
            excludes: true,
            equals: true,
        });
    }
    let groups = proxies::Groups::parse(
        &serde_json::json!({"groups": [{"name": "节点选择", "type": "Selector"}, {"name": "自动选择", "type": "URLTest"}],
        "nodes": [{"name": "香港 01", "type": "Shadowsocks"}, {"name": "日本 03", "type": "Trojan"}]}),
    );
    let delays = std::collections::HashMap::from([("香港 01".to_string(), 86)]);
    let cx = requests_view::Ctx {
        now,
        offset: 8 * 3600,
        capturing: true,
        groups: &groups,
        delays: &delays,
        mitm_hosts: Some(&[]),
    };
    let model = |v: Vec<RecordRow>| ModelRc::new(VecModel::from(v));
    ui.set_req_rows(model(st.rows(&cx)));
    ui.set_req_segment(st.segment);
    ui.set_req_shows_http(true);
    ui.set_req_statuses(ModelRc::new(VecModel::from(st.status_labels())));
    ui.set_req_status(st.filters().status);
    ui.set_req_conditions(ModelRc::new(VecModel::from(st.condition_lines())));
    let (empty, desc) = st.empty(false);
    ui.set_req_empty(empty.into());
    ui.set_req_empty_desc(desc.into());
    ui.set_req_detail(st.detail(&cx));
    ui.set_req_sheet_open(extra == "sheet");
    ui.set_req_searching(false);
    ui.set_req_actions_open(extra == "menu" || extra == "row");
    if extra == "menu" {
        let mitm = store::MitmSettings {
            enable: true,
            capture: true,
            ..Default::default()
        };
        st.limit_open = true;
        ui.set_req_actions(ModelRc::new(VecModel::from(st.page_actions(&mitm))));
    } else if extra == "row" {
        ui.set_req_actions(ModelRc::new(VecModel::from(st.row_actions(0, 6))));
    }
    ui.set_cond_open(extra == "cond");
    if extra == "cond" {
        st.open_draft(None);
        st.draft.as_mut().unwrap().value = "goo".into();
        let (suggestions, shows, ready) = st.draft_outcome();
        let fields: Vec<slint::SharedString> = requests_view::fields(0)
            .iter()
            .map(|f| f.1.into())
            .collect();
        ui.set_cond_fields(ModelRc::new(VecModel::from(fields)));
        ui.set_cond_field(0);
        ui.set_cond_op(0);
        ui.set_cond_value("goo".into());
        ui.set_cond_suggestions(ModelRc::new(VecModel::from(suggestions)));
        ui.set_cond_shows(shows.into());
        ui.set_cond_ready(ready);
        ui.set_cond_values_in("当前连接里的主机".into());
        ui.set_cond_tip("同一字段的条件满足其一即可，不同字段的都要满足。搜索框里也能这样写：host:github.com -app:slack".into());
    }
}

/// The tools list, a settings page's sections and the picker or input over them.
fn settings_shot(ui: &AppWindow, width: u32, page: &str, overlay: &str) {
    ui.set_mitm_loading("".into());
    ui.set_confirm_open(false);
    ui.set_confirm_yes("".into());
    ui.set_confirm_no("".into());
    ui.set_confirm_danger(false);
    let mut s = store::Settings {
        developer_mode: true,
        ..Default::default()
    };
    s.general.open_logs = true;
    s.general
        .hosts
        .insert("router.lan".into(), "192.168.1.1".into());
    s.general
        .hosts
        .insert("nas.lan".into(), "192.168.1.2, 192.168.1.3".into());
    if overlay == "more" {
        s.general.external_controller = true;
        s.general.secret = "Zx81kQp0aLm3Nw7T".into();
        s.general.authentication = store::Authentication {
            enable: true,
            username: "flclash".into(),
            password: "8e2f0a4c1b9d7e63".into(),
        };
        for k in ["test-url", "ipv6"] {
            seasoncat_ui::overrides::GENERAL.add(&mut s.general_override, k);
        }
    }
    if overlay.starts_with("custom") {
        s.primary_colors.push(0x5856D6);
        s.primary_color = Some(0x5856D6);
        s.color_style = 1;
        s.pure_black = true;
    }
    if let Some(scale) = overlay.strip_prefix("scale:") {
        s.text_scale = store::TextScale {
            enable: true,
            scale: scale.parse().unwrap_or(1.0),
        };
    }
    if overlay == "font-picked" {
        s.font_family = Some("Noto Sans SC".into());
    }
    if overlay == "docked" {
        s.floating_navigation_bar = false;
        s.tab_animation = 0;
    }
    apply_theme(ui, &s);
    if page == "font" {
        let families: Vec<slint::SharedString> = [
            "casual",
            "cursive",
            "monospace",
            "Noto Sans SC",
            "sans-serif",
            "sans-serif-condensed",
            "sans-serif-medium",
            "sans-serif-smallcaps",
            "serif",
            "serif-monospace",
            "source-sans-pro",
        ]
        .map(Into::into)
        .to_vec();
        ui.set_font_families(ModelRc::new(VecModel::from(families)));
        ui.set_font_searchable(true);
        ui.set_font_loading(false);
    }
    if page == "icon-set" {
        let tiles = sample_icon_sets()[0]
            .icons
            .iter()
            .filter(|i| overlay != "search" || i.label().contains("Kong"))
            .map(|i| IconTile {
                url: i.url.as_str().into(),
                label: i.label().into(),
                selected: false,
            })
            .collect::<Vec<_>>();
        ui.set_iconset_title("Qure 彩色国旗与服务图标合集".into());
        ui.set_iconset_empty("暂无搜索结果".into());
        ui.set_iconset_searching(overlay == "search");
        ui.set_iconset_query(if overlay == "search" { "Kong" } else { "" }.into());
        ui.set_iconset_tiles(ModelRc::new(VecModel::from(if overlay == "none" {
            Vec::new()
        } else {
            tiles
        })));
    }
    if page == "theme-color" {
        show_color_page(ui, &s, 0x5856D6);
        ui.set_color_mode(match overlay {
            "spectrum" => 1,
            "sliders" => 2,
            _ => 0,
        });
    }
    let m = |v: Vec<SetSection>| ModelRc::new(VecModel::from(v));
    ui.set_tools_sections(m(settings_view::tools(&s, width <= 600, &[])));
    let sections = match page {
        "general" => settings_view::general(&s),
        "developer" => settings_view::developer(&s, true),
        "about" => settings_view::about(&s),
        "map" => settings_view::map(&s.general.hosts, ""),
        "dns" => {
            if overlay != "empty" {
                for k in ["enhanced-mode", "nameserver", "listen", "ipv6", "fallback"] {
                    seasoncat_ui::overrides::DNS.add(&mut s.dns_override, k);
                }
                seasoncat_ui::overrides::DNS.set(
                    &mut s.dns_override,
                    "enhanced-mode",
                    "fake-ip".into(),
                );
            }
            seasoncat_ui::overrides::DNS.sections(&s.dns_override)
        }
        "advanced" => settings_view::advanced(&s),
        "network" => settings_view::network(&s),
        "user-agents" => {
            s.general.global_ua = s.general.user_agents.first().cloned();
            settings_view::user_agents(&s)
        }
        "list" => settings_view::list(&s.network.bypass_domain, ""),
        "filters" | "scripts" => {
            if overlay != "empty" {
                for (l, r) in [
                    ("香港", "(?i)港|hk|hong ?kong"),
                    ("日本", "(?i)日本|jp|japan"),
                    ("美国", "(?i)美|us|united states"),
                ] {
                    s.filters.push(seasoncat_ui::items::Filter {
                        label: l.into(),
                        regex: r.into(),
                    });
                }
                for (i, (l, u)) in [("main", true), ("rename-nodes", false)].iter().enumerate() {
                    s.scripts.push(seasoncat_ui::items::Script {
                        id: i.to_string(),
                        label: (*l).into(),
                        url: u.then(|| "https://example.com/main.js".into()),
                        updated: 0,
                    });
                }
            }
            seasoncat_ui::items_sections(&s, page).unwrap_or_default()
        }
        "rules" | "rule-form" | "rule-presets" => {
            if overlay != "empty" {
                for (i, line) in [
                    "DOMAIN-SUFFIX,google.com,GLOBAL",
                    "IP-CIDR,192.168.0.0/16,DIRECT,no-resolve",
                    "AND,((NETWORK,UDP),(DST-PORT,443)),REJECT-DROP",
                    "PROCESS-NAME-REGEX,(?i)^(qbittorrent|transmission.*|deluge.*|aria2c|motrix)(\\.exe)?$,DIRECT",
                ]
                .iter()
                .enumerate()
                {
                    s.added_rules.push(seasoncat_ui::rules::parse(line, i as u64 + 1));
                }
            }
            seasoncat_ui::rules_sections(&s, page).unwrap_or_default()
        }
        "rule-sets" => {
            if overlay != "empty" {
                for (i, (l, u, b, f)) in [
                    (
                        "广告拦截",
                        "https://example.com/reject.yaml",
                        "domain",
                        "yaml",
                    ),
                    ("局域网", "", "ipcidr", "text"),
                    (
                        "流媒体服务超长名称用于测试省略显示",
                        "https://example.com/media.mrs",
                        "classical",
                        "mrs",
                    ),
                ]
                .iter()
                .enumerate()
                {
                    s.rule_sets.push(seasoncat_ui::rule_sets::RuleSet {
                        id: i.to_string(),
                        label: (*l).into(),
                        url: (*u).into(),
                        behavior: (*b).into(),
                        format: (*f).into(),
                    });
                }
            }
            seasoncat_ui::rule_sets_sections(
                &s.rule_sets,
                std::path::Path::new("/nonexistent"),
                page,
            )
            .unwrap_or_default()
        }
        "mitm" => {
            s.mitm.hostname = sample_hosts();
            s.mitm.enable = true;
            s.mitm_modules = sample_modules();
            let cert = if overlay == "nocert" {
                Some(Ok(None))
            } else {
                Some(Ok(Some(seasoncat_ui::mitm::Certificate {
                    subject: "SeasonCat MITM CA 2026-11-26".into(),
                    ..Default::default()
                })))
            };
            ui.set_mitm_loading(
                if overlay == "loading" {
                    "生成证书"
                } else {
                    ""
                }
                .into(),
            );
            let trust = match overlay {
                "trust-user" => Some(Some(Ok(seasoncat_ui::ca_trust::TrustCheck {
                    user: true,
                    root: true,
                    ..Default::default()
                }))),
                "trust-none" => Some(Some(Ok(Default::default()))),
                "trust-remove" => {
                    ui.set_confirm_title("SeasonCat MITM CA 2026-11-26".into());
                    ui.set_confirm_text("有效期至 2036-11-26\n\nSHA-256\nAB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89".into());
                    ui.set_confirm_action("mitm-ca-remove".into());
                    ui.set_confirm_yes(ca_trust::REMOVE.into());
                    ui.set_confirm_no("关闭".into());
                    ui.set_confirm_danger(true);
                    ui.set_confirm_open(true);
                    Some(Some(Ok(ca_trust::TrustCheck {
                        user: true,
                        ..Default::default()
                    })))
                }
                _ => None,
            };
            seasoncat_ui::mitm_sections(&s, &cert, trust.as_ref(), 1)
        }
        "mitm-capture" => {
            s.mitm.enable = true;
            s.mitm.capture = true;
            seasoncat_ui::mitm_capture_sections(&s.mitm, &[])
        }
        "mitm-hosts" => {
            if overlay != "empty" {
                s.mitm.hostname = sample_hosts();
            }
            let issues = std::collections::BTreeMap::from([(
                "[api.bad-host.example".to_string(),
                "invalid hostname: unexpected character '['".to_string(),
            )]);
            seasoncat_ui::mitm_hosts_sections(&s.mitm.hostname, &issues)
        }
        "mitm-modules" => {
            if overlay != "empty" {
                s.mitm_modules = sample_modules();
            }
            let busy = std::collections::BTreeSet::from(["3".to_string()]);
            seasoncat_ui::mitm_modules_sections(&s.mitm_modules, &busy)
        }
        "mitm-module" if overlay == "local" => {
            let mut m = sample_modules()[1].clone();
            m.resources.insert("rewrite.js".into(), "0a1b.js".into());
            m.resources.insert("blank.gif".into(), "0c2d.gif".into());
            seasoncat_ui::mitm_module_sections(&m, Some(&sample_read()), &[], false, "3 小时前")
        }
        "mitm-module" => {
            let m = &sample_modules()[0];
            let net = seasoncat_ui::mitm_network::Network::from_core_json(&serde_json::json!({
                "nameserver": {"replace": true, "after": ["https://dns.example.com/dns-query"]},
                "hosts": [{"domain": "api.example.com", "values": ["10.0.0.2"]}],
                "routeAddress": ["10.0.0.0/8", "172.16.0.0/12"],
                "routeExcludeAddress": {"before": ["192.168.0.0/16"], "after": ["1.1.1.1/32"]},
            }));
            let later = [(
                "工作网络".to_string(),
                seasoncat_ui::mitm_network::Network::from_core_json(&serde_json::json!({
                    "hosts": [{"domain": "api.example.com", "values": ["10.0.0.3"]}],
                })),
            )];
            let user = seasoncat_ui::mitm_network::UserNetwork {
                dns_keys: vec!["nameserver".into()],
                ..Default::default()
            };
            let states = seasoncat_ui::mitm_network::change_states(&net, &later, &user);
            seasoncat_ui::mitm_module_sections(m, Some(&sample_read()), &states, false, "3 小时前")
        }
        "mitm-lines" => seasoncat_ui::mitm_lines_sections(&sample_read(), "url-rewrite"),
        "custom-proxies" => {
            if overlay != "empty" {
                s.custom_proxies = sample_custom_proxies();
            }
            let query = if overlay == "search" { "hk" } else { "" };
            let mut errors = std::collections::HashMap::new();
            if let Some(p) = s.custom_proxies.get(3) {
                let key = serde_json::to_string(&p.definition).unwrap_or_default();
                errors.insert(key, "invalid private key".to_string());
            }
            let selected = if overlay == "select" {
                s.custom_proxies
                    .iter()
                    .take(2)
                    .map(|p| p.id.clone())
                    .collect()
            } else {
                std::collections::BTreeSet::new()
            };
            seasoncat_ui::custom_proxies_sections(&s.custom_proxies, query, &errors, &selected)
        }
        "custom-proxy" => {
            let draft = sample_draft(overlay);
            ui.set_cp_issue(
                seasoncat_ui::custom_proxy_issue(&draft, &sample_custom_proxies()).into(),
            );
            if overlay == "add" {
                let mut last = None;
                let items: Vec<ActionItem> = draft
                    .addable()
                    .into_iter()
                    .map(|(sec, f)| {
                        let heading = if last != Some(sec) { sec.label() } else { "" };
                        last = Some(sec);
                        ActionItem {
                            id: f.name.into(),
                            heading: heading.into(),
                            label: f.label.into(),
                            enabled: true,
                            ..Default::default()
                        }
                    })
                    .collect();
                ui.set_choice_title("添加设置项".into());
                ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            }
            seasoncat_ui::custom_proxy_sections(&draft)
        }
        "icon-sets" => {
            if overlay != "empty" {
                s.icon_sets = sample_icon_sets();
            }
            seasoncat_ui::icon_sets_sections(&s.icon_sets, page).unwrap_or_default()
        }
        "backup" => {
            if overlay.starts_with("dav") {
                s.dav = Some(seasoncat_ui::backup::Dav {
                    uri: "https://dav.example.com/remote.php/dav/files/me".into(),
                    user: "flclash-user@example.com".into(),
                    password: "secret".into(),
                    ..Default::default()
                });
            }
            seasoncat_ui::backup_app_sections(&s, Some(true))
        }
        _ => Vec::new(),
    };
    ui.set_dav_open(overlay == "dav-form");
    if overlay == "dav-form" {
        ui.set_dav_bound(true);
        ui.set_dav_uri("https://dav.example.com/remote.php/dav/files/me".into());
        ui.set_dav_user("flclash-user@example.com".into());
        ui.set_dav_pass("secret".into());
    }
    if page == "access" {
        let apps = [
            ("com.tencent.mm", "微信", true),
            ("com.eg.android.AlipayGphone", "支付宝", true),
            ("org.telegram.messenger", "Telegram", false),
            ("com.google.android.youtube", "YouTube", false),
            ("com.android.chrome", "Chrome", true),
            ("com.taobao.taobao", "淘宝", false),
            (
                "com.ss.android.ugc.aweme.with.a.very.long.package.name",
                "抖音极速版超长的应用名字用于测试省略",
                false,
            ),
            ("com.spotify.music", "Spotify", false),
            ("com.github.android", "GitHub", false),
            ("com.twitter.android", "X", false),
            ("com.zhihu.android", "知乎", true),
        ];
        let rows: Vec<AccessRow> = apps
            .iter()
            .map(|(n, l, c)| AccessRow {
                name: (*n).into(),
                label: (*l).into(),
                monogram: l
                    .chars()
                    .next()
                    .unwrap()
                    .to_uppercase()
                    .collect::<String>()
                    .into(),
                checked: *c,
            })
            .collect();
        let on = overlay != "off";
        ui.set_acc_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_acc_enabled(on);
        ui.set_acc_changed(on);
        ui.set_acc_can_select_all(true);
        ui.set_acc_loading(false);
        ui.set_acc_needs_permission(overlay == "perm");
        ui.set_acc_authorize(false);
        ui.set_acc_status(
            if on {
                "选中应用将会被排除在VPN之外"
            } else {
                "应用访问控制已关闭"
            }
            .into(),
        );
        ui.set_acc_count("已选 4".into());
    }
    seasoncat_ui::set_targets(("bypass-domain", "network"), ("hosts", "general"));
    let mut meta = seasoncat_ui::page_meta(&s, page);
    if page == "custom-proxy" && overlay != "new" {
        meta.title = "编辑代理".into();
    }
    if page == "mitm-module" {
        meta.title = "去广告合集".into();
    }
    if page == "mitm-lines" {
        meta.title = "URL 重写".into();
    }
    if page == "custom-proxies" && overlay == "search" {
        meta.action2 = "".into();
    }
    meta.selecting = page == "custom-proxies" && overlay == "select";
    ui.on_page_meta(move |_, _| meta.clone());
    ui.set_set_searching(page == "custom-proxies" && overlay == "search");
    ui.set_set_query(
        if page == "custom-proxies" && overlay == "search" {
            "hk"
        } else {
            ""
        }
        .into(),
    );
    ui.set_page_sections(m(sections));
    ui.set_choice_open(
        overlay.starts_with("choice:") || (page == "custom-proxy" && overlay == "add"),
    );
    if let Some(key) = overlay.strip_prefix("choice:") {
        ui.set_choice_title("语言".into());
        let items: Vec<ActionItem> = settings_view::choices(&s, key)
            .into_iter()
            .map(|(id, label, checked)| ActionItem {
                id: id.into(),
                label: label.into(),
                check: true,
                checked,
                enabled: true,
                ..Default::default()
            })
            .collect();
        ui.set_choice_items(ModelRc::new(VecModel::from(items)));
    }
    ui.set_input_open(false);
    ui.set_ports_open(overlay == "input:port");
    if overlay == "input:port" {
        ui.set_port_mixed("80".into());
        ui.set_port_http("0".into());
        ui.set_port_socks("7891".into());
    }
}

/// The service card's cells, or the sheet's rows (one still checking).
fn service_cells(n: usize, sheet: bool) -> Vec<DashService> {
    let services = [
        ("available", 132, "US", "104.28.1.9", ""),
        ("available", 88, "", "", "日本 03"),
        ("blocked", 0, "HK", "203.0.113.20", ""),
        ("timeout", 0, "", "", ""),
        ("available", 0, "SG", "198.51.100.7", ""),
        ("unavailable", 0, "", "", "香港 01"),
        ("available", 410, "US", "104.28.1.9", ""),
        ("available", 95, "JP", "192.0.2.55", ""),
    ];
    dashboard::SERVICES
        .iter()
        .zip(services)
        .take(n)
        .enumerate()
        .map(|(i, ((id, label), (st, delay, region, ip, node)))| {
            let loading = sheet && i == 3;
            let (text, tone) = if loading {
                ("加载中…".into(), 0)
            } else {
                dashboard::service_status(Some(st), delay, false, false)
            };
            DashService {
                id: (*id).into(),
                label: (*label).into(),
                status: text.into(),
                tone,
                dot: dashboard::service_dot(Some(st), false),
                place: dashboard::service_where(region, ip, node).into(),
                loading,
                ip: (*ip).into(),
                region: (*region).into(),
            }
        })
        .collect()
}

fn sample_hosts() -> Vec<String> {
    [
        "*.googlevideo.com",
        "-*.apple.com",
        "api.example.com:8443",
        "[api.bad-host.example",
        "very-long-subdomain-for-testing-the-elided-row.cdn.example-provider.com",
    ]
    .map(String::from)
    .to_vec()
}

fn sample_modules() -> Vec<seasoncat_ui::mitm::MitmModule> {
    use seasoncat_ui::mitm::MitmModule;
    [
        (
            "1",
            "去广告合集",
            "https://example.com/AdBlock.sgmodule",
            true,
        ),
        ("2", "本地重写", "", false),
        (
            "3",
            "B站增强模块（超长名称用于测试省略显示效果）",
            "https://example.com/bili.plugin",
            true,
        ),
    ]
    .into_iter()
    .map(|(id, name, url, enabled)| MitmModule {
        id: id.into(),
        name: name.into(),
        url: url.into(),
        enabled,
        description: "移除开屏广告与信息流广告。".into(),
        system: "iOS".into(),
        ..Default::default()
    })
    .collect()
}

fn sample_read() -> seasoncat_ui::mitm::ModuleRead {
    let source = "#!name=去广告合集\n[URL Rewrite]\n^https?://ad\\.example\\.com/ - reject\n^https?://api\\.example\\.com/v1/splash(\n^https?://img\\.example\\.com/ad/.+ - reject-img\n[Script]\nsign = type=cron,cronexp=0 9 * * *,script-path=sign.js\n[Rule]\nDOMAIN-SUFFIX,ads.example.com,REJECT\n[Panel]\n";
    let read = serde_json::json!({
        "arguments": [
            {"name": "splash", "kind": {"type": "switch"}, "default": "true", "label": "去开屏广告", "desc": "关闭后保留开屏页"},
            {"name": "mode", "kind": {"type": "select", "options": ["严格", "宽松"]}, "default": "严格", "label": "过滤模式"}
        ],
        "views": {
            "url-rewrite": [{"index": 3, "view": {}}, {"index": 5, "view": {}}],
            "script": [{"index": 7, "view": {"type": "script", "name": "sign", "kind": "cron"}}]
        },
        "issues": {"url-rewrite": [{"index": 4, "message": "regex parse error: unclosed group"}]},
        "rules": [{"line": 9, "rule": "DOMAIN-SUFFIX,ads.example.com", "policy": {"builtin": "REJECT"}}],
        "ignored": [{"line": 10, "text": "[Panel]", "reason": "section", "detail": "Panel"}]
    });
    seasoncat_ui::mitm::read_module(source, &read)
}

fn sample_custom_proxies() -> Vec<seasoncat_ui::custom_proxies::CustomProxy> {
    use seasoncat_ui::custom_proxies::CustomProxy;
    [
        serde_json::json!({"name": "hk-01", "type": "ss", "server": "hk.example.com", "port": 443,
            "cipher": "aes-128-gcm", "password": "secret"}),
        serde_json::json!({"name": "日本东京 IPLC 专线超长名称用于测试省略显示", "type": "vmess",
            "server": "jp.example.com", "port": 8443, "uuid": "b831381d-6324-4d53-ad4f-8cda48b30811"}),
        serde_json::json!({"name": "hk-01", "type": "trojan", "server": "hk2.example.com", "port": 443}),
        serde_json::json!({"name": "us-wireguard", "type": "wireguard", "private-key": "k"}),
        serde_json::json!({"name": "", "type": "hysteria2", "server": "sg.example.com"}),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, d)| CustomProxy {
        id: i.to_string(),
        definition: d,
        ..Default::default()
    })
    .collect()
}

/// "new": an empty form after a save; otherwise a vless proxy with options listed.
fn sample_draft(overlay: &str) -> seasoncat_ui::custom_proxies::Draft {
    use seasoncat_ui::custom_proxies::{field, CustomProxy, Draft};
    if overlay == "new" {
        let mut d = Draft::open(
            CustomProxy {
                id: String::new(),
                definition: serde_json::json!({"name": "", "type": "ss", "password": "p@ssw0rd"}),
                ..Default::default()
            },
            true,
        );
        d.submitted = true;
        d.core_error = Some("missing server".into());
        return d;
    }
    if overlay == "easytier" {
        return Draft::open(
            CustomProxy {
                id: "8".into(),
                definition: serde_json::json!({"name": "家里", "type": "easytier",
                    "network-name": "home-net", "network-secret": "s3cret-key",
                    "peers": ["tcp://public.easytier.example:11010"], "exit-nodes": []}),
                empty_options: vec!["interfaceName".into()],
            },
            false,
        );
    }
    let mut d = Draft::open(
        CustomProxy {
            id: "9".into(),
            definition: serde_json::json!({"name": "hk-03", "type": "vless",
                "server": "hk.example.com", "port": 443, "uuid": "b831381d-6324-4d53-ad4f-8cda48b30811",
                "udp": true, "tls": true, "servername": "cdn.example.com", "alpn": ["h2", "http/1.1"]}),
            ..Default::default()
        },
        false,
    );
    d.add_option(field("network").unwrap());
    d.set_option(field("network").unwrap(), Some("ws".into()));
    d.set_option(field("wsPath").unwrap(), Some("/ray".into()));
    if overlay == "dup" {
        d.proxy = d.proxy.with_value("name", Some("hk-01".into()));
    }
    d
}

fn sample_icon_sets() -> Vec<seasoncat_ui::icon_sets::IconSet> {
    use seasoncat_ui::icon_sets::{IconSet, IconSetIcon};
    let names = [
        "Hong_Kong",
        "Taiwan",
        "Japan",
        "Singapore",
        "United_States",
        "Korea",
        "Netflix",
        "YouTube",
        "Telegram",
        "Spotify",
        "Disney+",
        "Very_Long_Service_Name_Here",
        "Final",
        "Global",
        "China",
        "Apple",
        "Google",
        "Microsoft",
    ];
    vec![
        IconSet {
            id: "1".into(),
            name: "Qure 彩色国旗与服务图标合集".into(),
            url: "https://example.com/qure.json".into(),
            icons: names
                .iter()
                .map(|n| IconSetIcon {
                    name: format!("{n}.png"),
                    url: format!("https://example.com/{n}.png"),
                })
                .collect(),
            updated: 0,
        },
        IconSet {
            id: "2".into(),
            name: "本地图标".into(),
            url: String::new(),
            icons: (0..1234)
                .map(|i| IconSetIcon {
                    name: format!("i{i}.png"),
                    url: format!("https://example.com/i{i}.png"),
                })
                .collect(),
            updated: 0,
        },
    ]
}

/// The custom profile's groups section and the group form, by overlay.
fn snapshot_groups(ui: &AppWindow, overlay: &str, w: u32) {
    use serde_json::json;
    let data = proxy_groups::Data {
        groups: vec![
            (
                1,
                json!({"name": "节点选择", "type": "select", "proxies": ["自动选择", "香港 01", "日本 01", "DIRECT"]}),
            ),
            (
                2,
                json!({"name": "自动选择", "type": "url-test", "proxies": ["香港 01", "日本 01"], "interval": 300}),
            ),
            (
                3,
                json!({"name": "故障转移", "type": "fallback", "proxies": ["香港 01", "失效节点"],
                       "url": "https://www.gstatic.com/generate_204", "interval": 600, "lazy": false,
                       "max-failed-times": 3, "hidden": true, "empty-fallback": "香港 01", "filter": "(?i)港|hk"}),
            ),
            (
                4,
                json!({"name": "负载均衡", "type": "load-balance", "use": ["机场订阅"], "strategy": "round-robin"}),
            ),
            (
                5,
                json!({"name": "一个名字很长很长很长很长的策略组名称用来检查省略", "type": "select", "proxies": ["节点选择"]}),
            ),
            (
                6,
                json!({"name": "循环A", "type": "select", "proxies": ["循环B"]}),
            ),
            (
                7,
                json!({"name": "循环B", "type": "smart", "proxies": ["循环A"]}),
            ),
        ],
        proxies: ["香港 01".to_string(), "日本 01".to_string()].into(),
        proxy_types: [
            ("香港 01".to_string(), "ss".to_string()),
            ("日本 01".to_string(), "vmess".to_string()),
        ]
        .into(),
        proxy_providers: ["机场订阅".to_string()].into(),
        provider_types: [("机场订阅".to_string(), "URL".to_string())].into(),
        dialers: Default::default(),
    };
    let listed = matches!(
        overlay,
        "groups" | "gselect" | "gsort" | "gmenu" | "gdelete"
    );
    let selected: std::collections::BTreeSet<u64> = if overlay == "gselect" {
        [2, 3].into()
    } else {
        Default::default()
    };
    let rows = if listed {
        group_rows(&data, &selected)
    } else {
        Vec::new()
    };
    let titles: Vec<slint::SharedString> = rows.iter().map(|r| r.name.clone()).collect();
    let kinds: Vec<slint::SharedString> = rows.iter().map(|r| r.kind.clone()).collect();
    if listed {
        ui.set_custom_group_count(rows.len() as i32);
        ui.set_custom_issues(proxy_groups::profile_issues(&data).len() as i32);
    }
    ui.set_custom_groups(ModelRc::new(VecModel::from(rows)));
    ui.set_custom_group_titles(ModelRc::new(VecModel::from(titles)));
    ui.set_custom_group_kinds(ModelRc::new(VecModel::from(kinds)));
    ui.set_custom_group_selecting(overlay == "gselect");
    ui.set_custom_group_sorting(overlay == "gsort");
    if overlay == "gmenu" {
        ui.set_ctx_names(ModelRc::default());
        ui.set_ctx_items(ModelRc::new(VecModel::from(vec![
            ActionItem {
                id: "copy".into(),
                glyph: "copy".into(),
                label: "复制".into(),
                enabled: true,
                ..Default::default()
            },
            ActionItem {
                id: "delete".into(),
                glyph: "delete".into(),
                label: "删除".into(),
                danger: true,
                enabled: true,
                ..Default::default()
            },
        ])));
        ui.set_ctx_x(16.0);
        ui.set_ctx_y(470.0);
        ui.set_ctx_w(w as f32 - 32.0);
        ui.set_ctx_h(64.0);
        ui.set_ctx_kind("pg".into());
    }
    let mut draft = match overlay {
        "gnew" => GroupDraft::open(0, json!({"name": "", "type": "select"}), true),
        "glb" => GroupDraft::open(4, data.groups[3].1.clone(), false),
        _ => GroupDraft::open(3, data.groups[2].1.clone(), false),
    };
    if overlay == "gnew" {
        draft.submitted = true;
    }
    if overlay == "glb" {
        draft.add_option(proxy_groups::Opt::Strategy, "");
    }
    if overlay == "gsave" {
        draft.def["interval"] = json!(900);
    }
    let presets = vec![("(?i)港|hk".to_string(), "香港节点".to_string())];
    ui.set_pg_title(
        if overlay == "gnew" {
            "添加策略组"
        } else {
            "编辑策略组"
        }
        .into(),
    );
    let mut tail = group_form_sections(&draft, &data);
    let head: Vec<SetSection> = tail.drain(..1).collect();
    ui.set_pg_head(ModelRc::new(VecModel::from(head)));
    ui.set_pg_members(ModelRc::new(VecModel::from(member_items(
        &draft, &data, &presets,
    ))));
    ui.set_pg_tail(ModelRc::new(VecModel::from(tail)));
    if overlay.starts_with("gpick") {
        use proxy_groups::{Member, MemberKind};
        let origin = data.groups[0].1.clone();
        let picks = [
            Member::new(MemberKind::Proxy, "故障转移"),
            Member::new(MemberKind::Filter, "(?i)港|hk"),
        ];
        let expanded: std::collections::BTreeSet<String> = [
            "策略组".to_string(),
            "筛选".to_string(),
            "自动包含".to_string(),
        ]
        .into();
        let query = if overlay == "gpicks" { "循环" } else { "" };
        let (sections, searchable) = member_picker_sections(
            1,
            &origin,
            &data,
            &presets,
            &picks,
            if overlay == "gpicke" { "zzz" } else { query },
            &expanded,
        );
        ui.set_pg_pick_sections(ModelRc::new(VecModel::from(sections)));
        ui.set_pg_pick_searchable(searchable);
        ui.set_pg_pick_empty(
            if overlay == "gpicke" {
                "没有匹配的结果"
            } else {
                "暂无数据"
            }
            .into(),
        );
        ui.set_pg_pick_searching(overlay != "gpick");
        ui.set_pg_pick_query(if overlay == "gpicke" { "zzz" } else { query }.into());
    }
    if overlay.starts_with("gicon") {
        let sets = if overlay == "giconempty" {
            Vec::new()
        } else {
            sample_icon_sets()
        };
        let value = match overlay {
            "giconempty" => "",
            "gicondata" => "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
            _ => sets[0].icons[6].url.as_str(),
        };
        let records = vec![seasoncat_ui::icon_picker::IconRecord {
            url: "https://example.com/icons/Netflix_Letter.png".into(),
            last_accessed_ms: 1,
        }];
        let query = if overlay == "giconsearch" { "on" } else { "" };
        let v = icon_view(value, "Netflix 港", None, query, &sets, &records);
        show_icon_view(ui, v);
        ui.set_gi_searching(overlay == "giconsearch");
        ui.set_gi_query(query.into());
    }
    ui.set_pg_issue(proxy_groups::issues_message(&draft.issues(&data)).into());
    if overlay == "gyaml" {
        ui.set_editor_title("故障转移".into());
        ui.set_editor_text(proxy_groups::to_yaml(&data.groups[2].1).into());
        ui.set_editor_readonly(false);
        ui.set_editor_parent("group".into());
    }
    if overlay == "gsave" || overlay == "gdelete" {
        ui.set_confirm_title("提示".into());
        if overlay == "gsave" {
            ui.set_confirm_text("是否保存更改？".into());
            ui.set_confirm_action("pg-save".into());
        } else {
            ui.set_confirm_text("节点选择, 自动选择 仍被当前配置的规则、策略组、拨号代理、DNS 或 NTP 引用，删除后这些引用会失效。确定删除吗？".into());
            ui.set_confirm_action("pg-delete".into());
        }
        ui.set_confirm_open(true);
    }
}

/// The custom profile's rules, the rule form, the presets and the rules' quick edit, by
/// overlay.
/// The custom profile's pages past the overview ("pm:<page>[:empty]") and a proxy
/// provider's options ("pform", its filter picker "pfpick").
fn snapshot_more(ui: &AppWindow, overlay: &str) {
    use seasoncat_ui::custom_profile as cp;
    use serde_json::json;
    let pm = overlay.strip_prefix("pm:");
    if pm.is_none() && overlay != "pform" && overlay != "pfpick" {
        ui.set_pg_provider(false);
        return;
    }
    let empty = pm.is_some_and(|p| p.ends_with(":empty"));
    let page = pm.map(|p| p.trim_end_matches(":empty")).unwrap_or("");
    let mut s = store::State::default();
    let mut p = cp::new_profile("c".into(), "自定义分流", 1);
    for (id, name, url) in [
        ("1", "机场订阅", "https://example.com/sub"),
        (
            "2",
            "备用线路很长很长很长的订阅名称用来检查省略",
            "https://example.com/b",
        ),
    ] {
        s.profiles.push(store::Profile {
            id: id.into(),
            name: name.into(),
            url: url.into(),
            updated: 1,
            ..Default::default()
        });
    }
    s.settings.custom_proxies = sample_custom_proxies();
    let names: Vec<String> = s.settings.custom_proxies.iter().map(|c| c.name()).collect();
    if !empty {
        let mut proxies: Vec<serde_json::Value> = names
            .iter()
            .filter(|n| !n.is_empty())
            .map(|n| json!(n))
            .collect();
        proxies.push(json!("DIRECT"));
        for (id, def) in [
            (
                1,
                json!({"name": "节点选择", "type": "select", "proxies": proxies, "use": ["机场订阅", "备用线路很长很长很长的订阅名称用来检查省略", "已删除订阅"]}),
            ),
            (
                2,
                json!({"name": "落地", "type": "select", "proxies": [names[0]]}),
            ),
        ] {
            s.proxy_groups.push(cp::ProxyGroupRow {
                id,
                profile_id: "c".into(),
                definition: def,
                order: None,
            });
        }
        s.settings.rule_sets.push(seasoncat_ui::rule_sets::RuleSet {
            id: "r".into(),
            label: "广告拦截".into(),
            url: "https://example.com/reject.yaml".into(),
            behavior: "domain".into(),
            format: "yaml".into(),
        });
        for (i, line) in ["RULE-SET,广告拦截,REJECT", "RULE-SET,流媒体,节点选择"]
            .iter()
            .enumerate()
        {
            s.profile_rules
                .push(seasoncat_ui::profile_rules::parse(line, i as u64 + 1, "c"));
        }
        let ids: Vec<String> = s
            .settings
            .custom_proxies
            .iter()
            .map(|c| c.id.clone())
            .collect();
        s.proxy_dialers.push(cp::ProxyDialer {
            profile_id: "c".into(),
            proxy_id: ids[1].clone(),
            target: "落地".into(),
        });
        s.proxy_dialers.push(cp::ProxyDialer {
            profile_id: "c".into(),
            proxy_id: ids[0].clone(),
            target: "节点选择".into(),
        });
        use seasoncat_ui::tun_sniffer_ntp::{with_ports, NTP, SNIFFER, TUN};
        let o = &mut p.overrides;
        for k in [
            "enable",
            "server",
            "interval",
            "dialer-proxy",
            "write-to-system",
        ] {
            NTP.add(&mut o.ntp, k);
        }
        NTP.set(&mut o.ntp, "enable", true.into());
        NTP.set(&mut o.ntp, "dialer-proxy", "已删除策略组".into());
        for k in [
            "enable",
            "force-dns-mapping",
            "skip-domain",
            "sniff.HTTP",
            "sniff.TLS",
        ] {
            SNIFFER.add(&mut o.sniffer, k);
        }
        SNIFFER.set(&mut o.sniffer, "enable", true.into());
        SNIFFER.set(
            &mut o.sniffer,
            "skip-domain",
            json!(["rule-set:广告拦截", "+.apple.com"]),
        );
        SNIFFER.set(
            &mut o.sniffer,
            "sniff.TLS",
            with_ports(&json!({}), vec!["443".into(), "8443".into()]),
        );
        for k in [
            "enable",
            "enhanced-mode",
            "nameserver",
            "nameserver-policy",
            "fake-ip-filter",
        ] {
            cp::PROFILE_DNS.add(&mut o.dns, k);
        }
        cp::PROFILE_DNS.set(&mut o.dns, "enable", true.into());
        cp::PROFILE_DNS.set(
            &mut o.dns,
            "nameserver",
            json!([
                "https://dns.google/dns-query#节点选择",
                "tls://1.1.1.1#缺失代理",
                "223.5.5.5"
            ]),
        );
        cp::PROFILE_DNS.set(
            &mut o.dns,
            "nameserver-policy",
            json!({"rule-set:广告拦截": "rcode://success"}),
        );
        for k in ["mtu", "strict-route", "dns-hijack"] {
            TUN.add(&mut o.tun, k);
        }
        o.proxy_providers.insert("机场订阅".into(), json!({"override": {"ip-version": "ipv4-prefer", "udp": true}, "health-check": {"interval": 600}, "filter": "(?i)港|hk`(?i)日本|jp", "exclude-filter": "过期"}));
    }
    s.profiles.insert(0, p);
    s.settings.filters.push(seasoncat_ui::items::Filter {
        label: "香港节点".into(),
        regex: "(?i)港|hk".into(),
    });
    s.settings.filters.push(seasoncat_ui::items::Filter {
        label: "美国节点".into(),
        regex: "(?i)美|us".into(),
    });
    if let Some(id) = page.strip_prefix("yaml-") {
        use seasoncat_ui::tun_sniffer_ntp::{NTP, SNIFFER};
        let o = &s.profiles[0].overrides;
        let (spec, st) = match id {
            "dns" => (&cp::PROFILE_DNS, &o.dns),
            "ntp" => (&NTP, &o.ntp),
            _ => (&SNIFFER, &o.sniffer),
        };
        ui.set_editor_title(spec.title.into());
        ui.set_editor_text(seasoncat_ui::override_yaml::override_yaml(spec, st).into());
        ui.set_editor_readonly(false);
        return;
    }
    if !page.is_empty() {
        let (meta, sections, issue) =
            seasoncat_ui::profile_more_view(&s, "c", page, std::path::Path::new("/nonexistent"));
        ui.set_pm_meta(meta);
        ui.set_pm_sections(ModelRc::new(VecModel::from(sections)));
        ui.set_pm_issue(issue.into());
        return;
    }
    let data = proxy_groups::data(&s, "c");
    let presets: Vec<(String, String)> = s
        .settings
        .filters
        .iter()
        .map(|f| (f.regex.clone(), f.label.clone()))
        .collect();
    let options = seasoncat_ui::providers::options_of(&s, "c", "机场订阅");
    let draft = GroupDraft::open_provider("机场订阅", options.clone());
    let mut tail = group_form_sections(&draft, &data);
    let head: Vec<SetSection> = tail.drain(..1).collect();
    ui.set_pg_title("机场订阅".into());
    ui.set_pg_provider(true);
    ui.set_pg_head(ModelRc::new(VecModel::from(head)));
    ui.set_pg_members(ModelRc::new(VecModel::from(member_items(
        &draft, &data, &presets,
    ))));
    ui.set_pg_tail(ModelRc::new(VecModel::from(tail)));
    ui.set_pg_issue("".into());
    if overlay == "pfpick" {
        let (mut sections, searchable) = member_picker_sections(
            0,
            &options,
            &data,
            &presets,
            &[],
            "",
            &["筛选".to_string(), "排除筛选".to_string()].into(),
        );
        sections.retain(|c| c.label == "筛选" || c.label == "排除筛选");
        ui.set_pg_pick_sections(ModelRc::new(VecModel::from(sections)));
        ui.set_pg_pick_searchable(searchable);
        ui.set_pg_pick_searching(false);
        ui.set_pg_pick_empty("暂无数据".into());
    }
}

/// 扩展 ("ext:main", "ext:select", "ext:apply"), 编辑全局规则 ("ext:global",
/// "ext:global-empty") and a custom profile's 更多 with module exits ("pmexits").
fn snapshot_extend(ui: &AppWindow, overlay: &str) {
    use seasoncat_ui::extend::{self, exit_row};
    use seasoncat_ui::rules::{parse, Rule};
    if !overlay.starts_with("ext:") && overlay != "pmexits" {
        return;
    }
    let mut s = store::State::default();
    s.profiles.push(store::Profile {
        id: "p".into(),
        name: "机场订阅".into(),
        url: "https://example.com/sub".into(),
        updated: 1,
        ..Default::default()
    });
    let exit = |mut r: Rule| {
        r.custom_exit = true;
        r
    };
    if overlay != "ext:global-empty" {
        s.settings.added_rules = vec![
            parse("GEOSITE,private,DIRECT", 1),
            exit(parse("DOMAIN-SUFFIX,company.example.com,公司", 2)),
            parse("AND,((NETWORK,UDP),(DST-PORT,443)),REJECT-DROP", 3),
            parse(
                "PROCESS-NAME-REGEX,(?i)^(qbittorrent|transmission.*|deluge.*|aria2c)(\\.exe)?$,DIRECT",
                4,
            ),
        ];
    }
    extend::set_disabled(&mut s, "p", 3, true);
    extend::set_own_rules(
        &mut s,
        "p",
        &[
            exit(parse("DOMAIN-SUFFIX,home.example.com,家里", 10)),
            parse("IP-CIDR,192.168.0.0/16,DIRECT,no-resolve", 11),
            parse(
                "DOMAIN-KEYWORD,very-long-keyword-for-checking-elide-in-rows,REJECT",
                12,
            ),
        ],
    );
    s.profiles[0]
        .exit_targets
        .insert("公司".into(), "已删除策略组".into());
    s.profiles[0]
        .exit_targets
        .insert("广告".into(), "REJECT".into());
    let names: std::collections::BTreeSet<String> =
        ["节点选择".to_string(), "自动选择".into(), "香港 01".into()].into();
    let modules = vec![(
        "广告".to_string(),
        vec!["去广告".to_string(), "净化".to_string()],
    )];
    if overlay == "pmexits" {
        let mut c = seasoncat_ui::custom_profile::new_profile("c".into(), "自定义分流", 1);
        c.exit_targets.insert("广告".into(), "REJECT".into());
        s.profiles.push(c);
        let (meta, mut sections, issue) =
            seasoncat_ui::profile_more_view(&s, "c", "more", std::path::Path::new("/nonexistent"));
        let rows: Vec<_> = extend::exits(&[], &modules, true)
            .iter()
            .map(|e| exit_row(e, Some("REJECT"), Some(&names), None, Some(&modules[0].1)))
            .collect();
        sections.extend(exit_section(&rows));
        ui.set_pm_meta(meta);
        ui.set_pm_sections(ModelRc::new(VecModel::from(sections)));
        ui.set_pm_issue(issue.into());
        return;
    }
    if overlay.starts_with("ext:global") {
        let (meta, sections) = extend_global_view(&s, "p", "");
        ui.set_ext_meta(meta);
        ui.set_ext_sections(ModelRc::new(VecModel::from(sections)));
        ui.set_ext_searching(false);
        return;
    }
    s.settings.scripts.push(items::Script {
        id: "s1".into(),
        label: "去广告与地区分组".into(),
        ..Default::default()
    });
    s.profiles[0].script_id = Some("s1".into());
    let p = s.profiles[0].clone();
    let mut all = extend::own_rules(&s, "p");
    all.extend(extend::enabled_global(&s, "p"));
    let rows: Vec<_> = extend::exits(&all, &modules, false)
        .iter()
        .map(|e| {
            exit_row(
                e,
                p.exit_targets.get(e).map(String::as_str),
                Some(&names),
                Some("节点选择"),
                modules.iter().find(|(x, _)| x == e).map(|(_, m)| m),
            )
        })
        .collect();
    let selected: std::collections::BTreeSet<u64> = if overlay == "ext:select" {
        [10, 12].into()
    } else {
        Default::default()
    };
    let (top, rules, bottom) = extend_view(&s, "p", &selected, &rows);
    ui.set_ext_top(ModelRc::new(VecModel::from(top)));
    ui.set_ext_rules(ModelRc::new(VecModel::from(rules)));
    ui.set_ext_bottom(ModelRc::new(VecModel::from(bottom)));
    ui.set_ext_selecting(!selected.is_empty());
    ui.set_ext_apply(overlay == "ext:apply");
    ui.set_ext_applying(false);
}

fn snapshot_rules(ui: &AppWindow, overlay: &str, w: u32) {
    use seasoncat_ui::profile_rules::{self as pr, Targets};
    if !overlay.starts_with("r-") {
        return;
    }
    let data = proxy_groups::Data {
        groups: vec![
            (
                1,
                serde_json::json!({"name": "节点选择", "type": "select", "proxies": ["自动选择", "香港 01"]}),
            ),
            (
                2,
                serde_json::json!({"name": "自动选择", "type": "url-test", "proxies": ["香港 01"]}),
            ),
        ],
        proxies: ["香港 01".to_string(), "日本 01".to_string()].into(),
        proxy_types: [
            ("香港 01".to_string(), "ss".to_string()),
            ("日本 01".to_string(), "vmess".to_string()),
        ]
        .into(),
        ..Default::default()
    };
    let t = Targets::of(&data, ["广告拦截".to_string(), "国内直连".to_string()]);
    let lines = [
        "DOMAIN-SUFFIX,google.com,节点选择",
        "DOMAIN-KEYWORD,一个很长很长很长很长很长很长的关键字用来检查省略效果,自动选择",
        "RULE-SET,广告拦截,REJECT",
        "RULE-SET,失效规则集,REJECT",
        "AND,((NETWORK,UDP),(DST-PORT,443)),REJECT-DROP",
        "IP-CIDR,192.168.0.0/16,DIRECT,no-resolve",
        "GEOIP,CN,一个名字很长很长很长很长的策略组名称",
        "DST-PORT,80-x,DIRECT",
        "PROCESS-NAME,com.tencent.mm,香港 01",
        "DOMAIN,example.com,PASS",
        "GEOSITE,cn,DIRECT",
        "MATCH,节点选择",
    ];
    let rules: Vec<seasoncat_ui::custom_profile::ProfileRule> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| pr::parse(l, i as u64 + 1, "p"))
        .collect();
    let shown: Vec<&seasoncat_ui::custom_profile::ProfileRule> = if overlay == "r-search" {
        rules
            .iter()
            .filter(|r| pr::matches(r, "rule_set"))
            .collect()
    } else if overlay == "r-empty" {
        Vec::new()
    } else {
        rules.iter().collect()
    };
    let selected: std::collections::BTreeSet<u64> = if overlay == "r-select" {
        [2, 3, 5].into()
    } else {
        Default::default()
    };
    ui.set_pr_rows(ModelRc::new(VecModel::from(rule_rows(
        &shown, &t, &selected,
    ))));
    ui.set_pr_total(if overlay == "r-empty" {
        0
    } else {
        rules.len() as i32
    });
    ui.set_pr_selecting(overlay == "r-select");
    ui.set_pr_sorting(overlay == "r-sort");
    ui.set_pr_searching(overlay == "r-search");
    ui.set_pr_query(
        if overlay == "r-search" {
            "rule_set"
        } else {
            ""
        }
        .into(),
    );
    let titles: Vec<slint::SharedString> =
        rules.iter().map(|r| pr::name(&r.action).into()).collect();
    let subtitles: Vec<slint::SharedString> = rules
        .iter()
        .map(|r| {
            r.raw()
                .split_once(',')
                .map_or("", |(_, b)| b)
                .to_string()
                .into()
        })
        .collect();
    ui.set_pr_sort_titles(ModelRc::new(VecModel::from(titles)));
    ui.set_pr_sort_subtitles(ModelRc::new(VecModel::from(subtitles)));
    let menu = |items: Vec<(&str, &str, &str, bool)>| {
        ModelRc::new(VecModel::from(
            items
                .into_iter()
                .map(|(id, glyph, label, danger)| ActionItem {
                    id: id.into(),
                    glyph: glyph.into(),
                    label: label.into(),
                    danger,
                    enabled: true,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
    };
    if overlay == "r-menu" {
        ui.set_ctx_names(ModelRc::default());
        ui.set_ctx_items(menu(vec![
            ("copy", "copy", "复制", false),
            ("delete", "delete", "删除", true),
        ]));
        ui.set_ctx_x(16.0);
        ui.set_ctx_y(64.0 + 56.0 * 2.0);
        ui.set_ctx_w(w as f32 - 32.0);
        ui.set_ctx_h(56.0);
        ui.set_ctx_kind("pr".into());
    }
    if overlay == "r-add" {
        ui.set_ctx_names(ModelRc::default());
        ui.set_ctx_items(menu(vec![
            ("add", "plus", "添加", false),
            ("presets", "bolt", "快捷操作", false),
            ("quick-edit", "compose", "快速编辑", false),
        ]));
        ui.set_ctx_x(w as f32 - 48.0);
        ui.set_ctx_y(8.0);
        ui.set_ctx_w(40.0);
        ui.set_ctx_h(40.0);
        ui.set_ctx_kind("pr-add".into());
    }
    let draft = match overlay {
        "r-new" => RuleDraft::open(pr::new_rule(0, "p"), true),
        "r-set" => RuleDraft::open(rules[3].clone(), false),
        "r-match" => RuleDraft::open(rules[11].clone(), false),
        _ => RuleDraft::open(rules[5].clone(), false),
    };
    let presets = overlay == "r-presets";
    let (sections, issue) = if presets {
        (rule_preset_sections(&[0, 3].into()), String::new())
    } else {
        rule_form_sections(&draft, &t)
    };
    ui.set_pr_form_title(
        if presets {
            "快捷操作"
        } else if draft.is_new {
            "添加规则"
        } else {
            "编辑规则"
        }
        .into(),
    );
    ui.set_pr_form_sections(ModelRc::new(VecModel::from(sections)));
    ui.set_pr_form_issue(issue.into());
    ui.set_pr_form_action("check".into());
    if overlay == "r-type" || overlay == "r-target" {
        let item = |id: &str, label: &str, sub: &str, checked: bool, heading: &str| ActionItem {
            id: id.into(),
            label: label.into(),
            subtitle: sub.into(),
            heading: heading.into(),
            check: true,
            checked,
            enabled: true,
            ..Default::default()
        };
        let items: Vec<ActionItem> = if overlay == "r-type" {
            pr::ACTIONS
                .iter()
                .filter(|(a, _)| *a != "SUB-RULE")
                .map(|(a, d)| item(a, &pr::name(a), d, *a == "IP-CIDR", ""))
                .collect()
        } else {
            let mut v: Vec<ActionItem> = pr::TARGETS
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    item(
                        n,
                        n,
                        "",
                        *n == "DIRECT",
                        if i == 0 { "基础策略" } else { "" },
                    )
                })
                .collect();
            v.push(item("g1", "节点选择", "Selector", false, "规则目标"));
            v.push(item("g2", "自动选择", "URLTest", false, ""));
            v.push(item("p1", "香港 01", "ss", false, "本地代理"));
            v
        };
        ui.set_choice_title(
            if overlay == "r-type" {
                "代理类型"
            } else {
                "分流策略"
            }
            .into(),
        );
        ui.set_choice_items(ModelRc::new(VecModel::from(items)));
        ui.set_choice_open(true);
    }
    if overlay == "r-yaml" {
        let refs: Vec<&seasoncat_ui::custom_profile::ProfileRule> = rules.iter().collect();
        ui.set_editor_title("规则".into());
        ui.set_editor_text(pr::encode_list(&refs).into());
        ui.set_editor_readonly(false);
        ui.set_editor_parent("rules".into());
    }
    if overlay == "r-save" || overlay == "r-delete" {
        ui.set_confirm_title("提示".into());
        if overlay == "r-save" {
            ui.set_confirm_text("是否保存更改？".into());
            ui.set_confirm_action("pr-save".into());
        } else {
            ui.set_confirm_text("确定删除选中的规则吗？".into());
            ui.set_confirm_action("pr-delete".into());
        }
        ui.set_confirm_open(true);
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let (w, h) = (412u32, 892u32);
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    window.set_size(slint::PhysicalSize::new(w, h));
    let heights = Rc::new(std::cell::RefCell::new(Vec::<f32>::new()));
    let hs = heights.clone();
    ui.on_dash_card_height(move |i, h| {
        let mut v = hs.borrow_mut();
        if v.len() <= i as usize {
            v.resize(i as usize + 1, 0.0);
        }
        v[i as usize] = h;
    });
    let hs = heights.clone();
    ui.on_dash_drag_target(move |from, shift, gap, count| {
        let mut v = hs.borrow().clone();
        v.resize(count.max(0) as usize, 0.0);
        drag::target(&v, gap, from.max(0) as usize, shift) as i32
    });
    let hs = heights.clone();
    ui.on_dash_drag_offset(move |from, to, i, gap| {
        drag::offset(
            &hs.borrow(),
            gap,
            from.max(0) as usize,
            to.max(0) as usize,
            i.max(0) as usize,
        )
    });

    apply_theme(&ui, &store::Settings::default());
    ui.set_run_state("STARTED".into());
    ui.set_dash_run_time("02:13:57".into());
    ui.set_dash_status(DashStatus {
        ip: "203.0.113.42".into(),
        country: "US".into(),
        kind: "住宅".into(),
        quality: 1,
        org: "Example Broadband".into(),
        probing: false,
        failed: false,
    });
    let mut up = std::collections::VecDeque::new();
    let mut down = std::collections::VecDeque::new();
    for i in 0..30i64 {
        up.push_back(20_000 + (i * 7919 % 13) * 4000);
        down.push_back(800_000 + (i * 104729 % 17) * 90_000);
    }
    let (ul, uf) = dashboard::sparkline(&up);
    let (dl, df) = dashboard::sparkline(&down);
    ui.set_dash_traffic(DashTraffic {
        up_value: "50.6".into(),
        up_unit: "KB/s".into(),
        up_total: "累计 1.2 GB".into(),
        up_peak: "峰值 73.5 KB/s".into(),
        up_line: ul.into(),
        up_fill: uf.into(),
        down_value: "2.1".into(),
        down_unit: "MB/s".into(),
        down_total: "累计 8.6 GB".into(),
        down_peak: "峰值 2.2 MB/s".into(),
        down_line: dl.into(),
        down_fill: df.into(),
    });
    ui.set_dash_profile(DashProfile {
        id: "1".into(),
        name: "我的机场".into(),
        end: "剩余 30 天".into(),
        warn: false,
        updated: "更新于 3 分钟前".into(),
        usage: true,
        percent: "42%".into(),
        used: "42.0 GB / 100.0 GB".into(),
        ratio: 0.42,
        syncable: true,
        busy: false,
    });
    ui.set_dash_widgets(ModelRc::new(VecModel::from(
        dashboard::default_widgets()
            .into_iter()
            .map(slint::SharedString::from)
            .collect::<Vec<_>>(),
    )));
    let shown = dashboard::default_widgets();
    ui.set_dash_addable(ModelRc::new(VecModel::from(
        dashboard::WIDGETS
            .iter()
            .filter(|w| !shown.iter().any(|s| s == *w))
            .map(|w| DashAdd {
                id: (*w).into(),
                label: dashboard::widget_label(w).into(),
                desc: dashboard::widget_description(w).into(),
            })
            .collect::<Vec<_>>(),
    )));
    ui.set_dash_services(ModelRc::new(VecModel::from(service_cells(2, false))));
    ui.set_dash_service_rows(ModelRc::new(VecModel::from(service_cells(8, true))));
    ui.set_dash_service_manage(ModelRc::new(VecModel::from(
        dashboard::SERVICES
            .iter()
            .enumerate()
            .map(|(i, (id, label))| ServiceManageRow {
                id: (*id).into(),
                label: (*label).into(),
                enabled: i != 3 && i != 9,
                shown: i < 2,
                locked: false,
            })
            .collect::<Vec<_>>(),
    )));
    ui.set_dash_service_verdict("3 项异常".into());
    ui.set_dash_service_warn(true);
    ui.set_dash_stats(DashStats {
        connections: dashboard::count(23).into(),
        requests: dashboard::count(1482).into(),
        dns: dashboard::count(310).into(),
    });
    ui.set_dash_local_ip("192.168.1.10".into());
    ui.set_dash_memory("228.8 MB".into());
    ui.set_dash_mem(DashMemory {
        total: "228.8 MB".into(),
        app: "228.8 MB".into(),
        shared: true,
    });
    ui.set_profile_detail(DashProfileDetail {
        groups: "12".into(),
        proxies: "86".into(),
        rules: "1532".into(),
        providers: "2".into(),
        updated: "3 分钟前".into(),
        script: "无".into(),
        remote: true,
    });
    ui.set_profile_providers(ModelRc::new(VecModel::from(vec![
        ProviderRow {
            name: "机场节点".into(),
            subtitle: "代理 · HTTP · 42".into(),
        },
        ProviderRow {
            name: "reject-list".into(),
            subtitle: "规则 · HTTP · 18230".into(),
        },
    ])));
    ui.set_profile_name("我的机场".into());
    ui.set_up_speed("12.3 KB/s".into());
    ui.set_down_speed("1.4 MB/s".into());
    ui.set_up_total("共 3.2 MB".into());
    ui.set_down_total("共 120.5 MB".into());
    ui.set_app_version("v0.9.5".into());
    if let Some(icon) = settings_view::app_icon() {
        ui.set_app_icon(icon);
    }
    ui.set_profiles(ModelRc::new(VecModel::from(vec![
        ProfileItem {
            id: "1".into(),
            name: "我的机场".into(),
            subtitle: "35.2 GB / 200.0 GB".into(),
            trailing: "3 分钟前".into(),
            expire: "2027-03-01".into(),
            expired: false,
            active: true,
            busy: false,
            is_url: true,
            is_custom: false,
            groups: "".into(),
        },
        ProfileItem {
            id: "2".into(),
            name: "备用".into(),
            subtitle: "远程".into(),
            trailing: "2 天前".into(),
            expire: "2026-10-01".into(),
            expired: true,
            active: false,
            busy: true,
            is_url: true,
            is_custom: false,
            groups: "".into(),
        },
        ProfileItem {
            id: "3".into(),
            name: "本地配置.yaml".into(),
            subtitle: "本地".into(),
            trailing: "1 个月前".into(),
            expire: "".into(),
            expired: false,
            active: false,
            busy: false,
            is_url: false,
            is_custom: false,
            groups: "".into(),
        },
        ProfileItem {
            id: "4".into(),
            name: "自定义分流".into(),
            subtitle: "自定义".into(),
            trailing: "".into(),
            expire: "".into(),
            expired: false,
            active: false,
            busy: false,
            is_url: false,
            is_custom: true,
            groups: "12".into(),
        },
    ])));
    ui.set_sort_titles(ModelRc::new(VecModel::from(
        ui.get_profiles().iter().map(|p| p.name).collect::<Vec<_>>(),
    )));
    ui.set_edit_is_url(true);
    ui.set_edit_name("我的机场".into());
    ui.set_edit_url("https://sub.example.com/api/v1/client/subscribe?token=abc".into());
    ui.set_edit_auto(true);
    ui.set_edit_interval("1440".into());
    ui.set_edit_file_info("86.4 KB · 3 分钟前".into());
    ui.set_editor_title("我的机场".into());
    ui.set_editor_text("mixed-port: 7890\nmode: rule\nproxies:\n  - name: 香港 01\n    type: ss\n    server: hk.example.com\n    port: 443\n".into());
    ui.set_editor_readonly(true);
    let node = |name: &str, kind: &str, delay: i32, selected: bool, pinned: bool| NodeItem {
        name: name.into(),
        kind: kind.into(),
        delay,
        selected,
        pinned,
        addable: !kind.contains('('),
    };
    let nodes = vec![
        node("自动选择", "URLTest(日本 03)", 92, false, false),
        node("香港 01", "Shadowsocks", 86, true, false),
        node("日本 03", "Trojan", 1203, false, false),
        node("美国 洛杉矶 高速 IPLC 专线", "VMess", -1, false, false),
        node("新加坡", "Hysteria2", -3, false, false),
        node("台湾", "VLESS", -4, false, false),
        node("韩国", "Shadowsocks", 0, false, false),
    ];
    let group =
        |name: &str, kind: &str, now: &str, expanded: bool, nodes: Vec<NodeItem>| ProxyGroup {
            name: name.into(),
            kind: kind.into(),
            now: now.into(),
            count: nodes.len() as i32,
            monogram: name.chars().next().unwrap().to_string().into(),
            expanded,
            testing: false,
            nodes: ModelRc::new(VecModel::from(nodes)),
        };
    ui.set_proxy_groups(ModelRc::new(VecModel::from(vec![
        group("节点选择", "Selector", "香港 01", true, nodes.clone()),
        group(
            "自动选择",
            "URLTest",
            "日本 03",
            false,
            vec![node("日本 03", "Trojan", 1203, true, true)],
        ),
        group("流媒体", "Selector", "节点选择", false, vec![]),
    ])));
    ui.set_dash_groups(ModelRc::new(VecModel::from(vec![
        group("节点选择", "Selector", "香港 01", false, vec![]),
        group("自动选择", "URLTest", "日本 03", false, vec![]),
        group("流媒体", "Selector", "节点选择", false, vec![]),
        group("Telegram", "Selector", "新加坡", false, vec![]),
    ])));
    let records = [
        ("info", "[TCP] 192.168.1.5:51234(com.android.chrome) --> www.google.com:443 match RuleSet(proxy) using 节点选择[香港 01]"),
        ("warning", "[DNS] resolve example.invalid timeout"),
        ("error", "[TCP] dial 美国 洛杉矶 高速 (match Match()) 192.168.1.5:40122 --> api.example.invalid:443 error: connect: connection refused"),
        ("info", "Start initial configuration in progress"),
        ("debug", "[UDP] 192.168.1.5:5353 --> 224.0.0.251:5353 doesn't match any rule using DIRECT"),
    ];
    let logs = VecModel::<LogItem>::default();
    for (i, (level, text)) in records.iter().enumerate().rev() {
        let r = logs::Record {
            level: (*level).into(),
            date_time: "2026-11-25 03:41:07".into(),
            payload: (*text).into(),
        };
        logs.push(log_item(i, &r));
    }
    ui.set_log_offered(ModelRc::new(VecModel::from(vec![
        "debug".into(),
        "info".into(),
        "warning".into(),
        "error".into(),
    ])));
    ui.set_log_detail(logs.row_data(2).unwrap());
    ui.set_logs(ModelRc::new(logs));
    let _ = ui.get_logs().row_count();

    // (file, tab, open group, tools or profiles page, theme mode, overlay)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut req = sample_requests(now);
    // The wide dashboard's table, laid out for the width it reports.
    let table = Rc::new(sample_requests(now));
    let w = ui.as_weak();
    ui.on_dash_table_width(move |width| {
        let Some(ui) = w.upgrade() else { return };
        let cols = dash_table::columns(dash_table::Feed::Connections, width);
        let widths = dash_table::widths(&cols, width, &Default::default());
        let rows = dash_table::rows(
            dash_table::Feed::Connections,
            &table.conns,
            &table.dns,
            &cols,
            None,
            now,
            0,
        );
        let mut x = 0.0;
        let heads: Vec<DashCol> = cols
            .iter()
            .zip(widths)
            .map(|(c, w)| {
                let col = DashCol {
                    id: c.id.into(),
                    label: c.label.into(),
                    x,
                    w,
                    end: c.end,
                    sort: if c.id == "download" { 2 } else { 0 },
                };
                x += w;
                col
            })
            .collect();
        ui.set_dash_table_cols(ModelRc::new(VecModel::from(heads)));
        ui.set_dash_table_rows(ModelRc::new(VecModel::from(
            rows.into_iter()
                .map(|r| DashRow {
                    cells: ModelRc::new(VecModel::from(
                        r.into_iter()
                            .map(|c| DashCell {
                                text: c.text.into(),
                                tone: c.tone,
                                badge: c.badge.into(),
                            })
                            .collect::<Vec<_>>(),
                    )),
                })
                .collect::<Vec<_>>(),
        )));
        ui.set_dash_table_counts(ModelRc::new(VecModel::from(vec![
            "4".into(),
            "6".into(),
            "128".into(),
        ])));
        ui.set_dash_table_tcp("3".into());
        ui.set_dash_table_udp("1".into());
    });
    ui.set_res_items(ModelRc::new(VecModel::from(
        [
            ("mmdb", "MMDB", "3 分钟前", "8.2 MB", false),
            ("asn", "ASN", "2 天前", "10.4 MB", true),
            ("geosite", "GEOSITE", "未知", "", false),
        ]
        .iter()
        .map(|(key, title, sub, chip, busy)| ResourceItem {
            key: (*key).into(),
            title: (*title).into(),
            subtitle: (*sub).into(),
            chip: (*chip).into(),
            url: resources::default_url(key).into(),
            busy: *busy,
        })
        .collect::<Vec<_>>(),
    )));
    let remote = |rows: &[(&str, &str, &str, &str, bool)]| {
        ModelRc::new(VecModel::from(
            rows.iter()
                .map(|(key, title, sub, chip, busy)| ResourceItem {
                    key: (*key).into(),
                    title: (*title).into(),
                    subtitle: (*sub).into(),
                    chip: (*chip).into(),
                    url: "https://example.com/r".into(),
                    busy: *busy,
                })
                .collect::<Vec<_>>(),
        ))
    };
    ui.set_res_icon_sets(remote(&[("icon:a", "Qure", "3 天前", "1.2K", false)]));
    ui.set_res_rule_sets(remote(&[
        ("rule:r", "reject-domains", "2 小时前", "182 KB", false),
        ("rule:s", "telegram-cidr", "未知", "", true),
    ]));
    ui.set_res_scripts(remote(&[
        ("script:1", "去广告与地区分组", "5 小时前", "2.4 KB", false),
        (
            "script:2",
            "rename-nodes-with-a-very-long-script-name-for-elide",
            "未知",
            "",
            true,
        ),
    ]));
    ui.set_res_modules(remote(&[("module:m", "去广告", "1 天前", "", false)]));
    ui.set_res_auto(true);
    ui.set_res_interval("24 小时".into());
    ui.on_res_check(|kind, value| {
        let e = if kind == "interval" {
            resources::check_interval(&value).err()
        } else {
            resources::check_url(&value).err()
        };
        e.unwrap_or_default().into()
    });
    ui.on_res_digits(|value| resources::digits_only(&value).into());
    ui.on_input_check(|key, value, _| {
        settings_view::check_input(&key, &value)
            .err()
            .unwrap_or_default()
            .into()
    });
    let shots: &[(&str, i32, &str, &str, i32, &str)] = &[
        ("requests", 3, "", "requests", 1, "req:0:0:0"),
        ("requests-filter", 3, "", "requests", 1, "req:0:0:0:filter"),
        (
            "requests-filter-small",
            3,
            "",
            "requests",
            1,
            "req:0:0:0:filter",
        ),
        ("requests-filter-wide", 4, "", "", 1, "req:0:0:0:filter"),
        ("requests-filter-tablet", 4, "", "", 1, "req:0:0:0:filter"),
        ("requests-dns", 3, "", "requests", 1, "req:2:0:0"),
        ("requests-http", 3, "", "requests", 1, "req:1:0:0"),
        ("requests-sheet", 3, "", "requests", 1, "req:0:6:0:sheet"),
        (
            "requests-info-sheet",
            3,
            "",
            "requests",
            1,
            "req:0:3:2:sheet",
        ),
        (
            "requests-info-sheet-small",
            3,
            "",
            "requests",
            1,
            "req:0:3:2:sheet",
        ),
        ("requests-info-sheet-wide", 4, "", "", 1, "req:0:3:2"),
        (
            "requests-info-sheet-tablet",
            4,
            "",
            "",
            1,
            "req:0:3:2:sheet",
        ),
        (
            "requests-sheet-info",
            3,
            "",
            "requests",
            1,
            "req:0:3:2:sheet",
        ),
        (
            "requests-sheet-failed",
            3,
            "",
            "requests",
            1,
            "req:0:5:1:sheet",
        ),
        (
            "requests-sheet-dns",
            3,
            "",
            "requests",
            1,
            "req:2:3:0:sheet",
        ),
        (
            "requests-sheet-http",
            3,
            "",
            "requests",
            1,
            "req:1:3:0:sheet",
        ),
        (
            "requests-sheet-http-response",
            3,
            "",
            "requests",
            1,
            "req:1:3:2:sheet",
        ),
        ("requests-menu", 3, "", "requests", 1, "req:0:0:0:menu"),
        ("requests-row-menu", 3, "", "requests", 1, "req:0:0:0:row"),
        ("requests-cond", 3, "", "requests", 1, "req:0:0:0:cond"),
        ("requests-small", 3, "", "requests", 1, "req:0:0:0"),
        ("requests-wide", 4, "", "", 1, "req:0:3:2"),
        ("requests-timeline-wide", 4, "", "", 1, "req:0:6:0"),
        ("requests-empty-wide", 4, "", "", 1, "req:0:0:0"),
        ("requests-dns-wide", 4, "", "", 1, "req:2:3:0"),
        ("requests-http-wide", 4, "", "", 1, "req:1:3:0"),
        ("requests-tablet", 4, "", "", 1, "req:0:6:0:sheet"),
        ("requests-dark", 3, "", "requests", 2, "req:0:0:0"),
        ("resources", 3, "", "resources", 1, ""),
        ("resources-interval", 3, "", "resources", 1, "res:interval"),
        ("resources-url", 3, "", "resources", 1, "res:url"),
        ("resources-wide", 3, "", "resources", 1, ""),
        ("resources-tablet", 3, "", "resources", 1, ""),
        ("resources-small", 3, "", "resources", 1, ""),
        ("tools-wide", 3, "", "", 1, ""),
        ("dashboard", 0, "", "", 1, ""),
        ("popt", 1, "节点选择", "", 1, "options"),
        ("popt-small", 1, "节点选择", "", 1, "options"),
        ("popt-wide", 1, "节点选择", "", 1, "options"),
        ("popt-tablet", 1, "节点选择", "", 1, "options"),
        ("popt-layout", 1, "节点选择", "", 1, "options-layout"),
        ("popt-layout-small", 1, "节点选择", "", 1, "options-layout"),
        ("popt-layout-wide", 1, "节点选择", "", 1, "options-layout"),
        ("popt-layout-tablet", 1, "节点选择", "", 1, "options-layout"),
        ("nav-noproxies", 0, "", "", 1, "noproxies"),
        ("nav-noproxies-small", 0, "", "", 1, "noproxies"),
        ("nav-noproxies-wide", 0, "", "", 1, "noproxies"),
        ("nav-noproxies-tablet", 0, "", "", 1, "noproxies"),
        ("dash-svcsheet", 0, "", "", 1, "svcsheet"),
        ("dash-svcsheet-small", 0, "", "", 1, "svcsheet"),
        ("dash-svcsheet-wide", 0, "", "", 1, "svcsheet"),
        ("dash-svcsheet-tablet", 0, "", "", 1, "svcsheet"),
        ("dash-svcip", 0, "", "", 1, "svcip"),
        ("dash-svcip-small", 0, "", "", 1, "svcip"),
        ("dash-svcip-wide", 0, "", "", 1, "svcip"),
        ("dash-svcip-tablet", 0, "", "", 1, "svcip"),
        ("dash-preview", 0, "", "", 1, "dashpreview"),
        ("dash-preview-small", 0, "", "", 1, "dashpreview"),
        ("dash-preview-wide", 0, "", "", 1, "dashpreview"),
        ("dash-preview-tablet", 0, "", "", 1, "dashpreview"),
        ("dash-svcmanage", 0, "", "", 1, "svcsheet-manage"),
        ("dash-svcmanage-small", 0, "", "", 1, "svcsheet-manage"),
        ("dash-svcmanage-wide", 0, "", "", 1, "svcsheet-manage"),
        ("dash-svcmanage-tablet", 0, "", "", 1, "svcsheet-manage"),
        ("dash-memsheet", 0, "", "", 1, "memsheet"),
        ("dash-memsheet-small", 0, "", "", 1, "memsheet"),
        ("dash-memsheet-wide", 0, "", "", 1, "memsheet"),
        ("dash-memsheet-tablet", 0, "", "", 1, "memsheet"),
        ("dash-profsheet", 0, "", "", 1, "profsheet"),
        ("dash-profsheet-small", 0, "", "", 1, "profsheet"),
        ("dash-profsheet-wide", 0, "", "", 1, "profsheet"),
        ("dash-profsheet-tablet", 0, "", "", 1, "profsheet"),
        ("dash-feed", 0, "", "", 1, "feed:0"),
        ("dash-feed-small", 0, "", "", 1, "feed:0"),
        ("dash-feed-wide", 0, "", "", 1, "feed:0"),
        ("dash-feed-tablet", 0, "", "", 1, "feed:0"),
        ("dash-feed-dns", 0, "", "", 1, "feed:2"),
        ("dash-feed-tall", 0, "", "", 1, "feed:0:tall"),
        ("dash-groupsheet", 0, "", "", 1, "groupsheet"),
        ("dash-groupsheet-small", 0, "", "", 1, "groupsheet"),
        ("dash-groupsheet-wide", 0, "", "", 1, "groupsheet"),
        ("dash-groupsheet-tablet", 0, "", "", 1, "groupsheet"),
        ("dash-picker-empty", 0, "", "", 1, "picker-empty"),
        ("dash-picker-empty-small", 0, "", "", 1, "picker-empty"),
        ("dash-picker-empty-wide", 0, "", "", 1, "picker-empty"),
        ("dash-picker-empty-tablet", 0, "", "", 1, "picker-empty"),
        ("dash-picker", 0, "", "", 1, "picker"),
        ("dash-picker-small", 0, "", "", 1, "picker"),
        ("dash-picker-wide", 0, "", "", 1, "picker"),
        ("dash-picker-tablet", 0, "", "", 1, "picker"),
        ("dash-faults", 0, "", "", 1, "faults"),
        ("dash-faults-small", 0, "", "", 1, "faults"),
        ("dash-faults-wide", 0, "", "", 1, "faults"),
        ("dash-faults-tablet", 0, "", "", 1, "faults"),
        ("speed0-412x892", 0, "", "", 1, "speed:0:0"),
        ("speed1-412x892", 0, "", "", 1, "speed:1.2:39"),
        ("speed2-412x892", 0, "", "", 1, "speed:123.4:1.2"),
        ("speed3-412x892", 0, "", "", 1, "speed:1023.9:888.8"),
        ("speed0-360x640", 0, "", "", 1, "speed:0:0"),
        ("speed1-360x640", 0, "", "", 1, "speed:1.2:39"),
        ("speed2-360x640", 0, "", "", 1, "speed:123.4:1.2"),
        ("speed3-360x640", 0, "", "", 1, "speed:1023.9:888.8"),
        ("dashboard-edit", 0, "", "", 1, "edit"),
        ("dashboard-stopped", 0, "", "", 1, "stopped"),
        ("dashboard-stopped-small", 0, "", "", 1, "stopped"),
        ("dashboard-stopped-wide", 0, "", "", 1, "stopped"),
        ("dashboard-stopped-tablet", 0, "", "", 1, "stopped"),
        ("dashboard-stopped-dark", 0, "", "", 2, "stopped"),
        ("dashboard-tablet", 0, "", "", 1, ""),
        ("dashboard-wide", 0, "", "", 1, ""),
        ("dashboard-cards", 0, "", "", 1, "cards"),
        ("dashboard-cards-small", 0, "", "", 1, "cards"),
        ("dashboard-cards-wide", 0, "", "", 1, "cards"),
        ("dashboard-cards-tablet", 0, "", "", 1, "cards"),
        ("dashboard-cards-tiny", 0, "", "", 1, "cards"),
        ("dashboard-ipsheet", 0, "", "", 1, "ipsheet"),
        ("dashboard-ipsheet-small", 0, "", "", 1, "ipsheet"),
        ("dashboard-ipsheet-wide", 0, "", "", 1, "ipsheet"),
        ("dashboard-failed-small", 0, "", "", 1, "failed"),
        ("dashboard-1280x700", 0, "", "", 1, ""),
        ("dashboard-1000x900", 0, "", "", 1, ""),
        ("live-stopped", 0, "", "", 1, "live:0"),
        ("live-stopped-small", 0, "", "", 1, "live:0"),
        ("live-stopped-wide", 0, "", "", 1, "live:0"),
        ("live-stopped-tablet", 0, "", "", 1, "live:0"),
        ("live-starting", 0, "", "", 1, "live:1"),
        ("live-ready", 0, "", "", 1, "live:2"),
        ("live-ready-dark", 0, "", "", 2, "live:2"),
        ("live-download", 0, "", "", 1, "live:3"),
        ("live-consent", 0, "", "", 1, "live:4"),
        ("live-warnings", 0, "", "", 1, "live:5"),
        ("live-warnings-small", 0, "", "", 1, "live:5"),
        ("live-running-wide", 0, "", "", 1, "live:6"),
        ("live-open", 0, "", "", 1, "live-open"),
        ("live-open-small", 0, "", "", 1, "live-open"),
        ("live-open-wide", 0, "", "", 1, "live-open"),
        ("pill-stopped", 0, "", "", 1, "pill:stopped"),
        ("pill-warnings-small", 0, "", "", 1, "pill:warnings"),
        ("pill-download-wide", 0, "", "", 1, "pill:downloading"),
        ("pill-running-tablet", 0, "", "", 2, "pill:running"),
        ("dashboard-table-1280x1000", 0, "", "", 1, ""),
        ("dashboard-table-1600x1100", 0, "", "", 2, ""),
        ("dashboard-edit", 0, "", "", 1, "edit"),
        ("dashboard-edit-small", 0, "", "", 1, "edit"),
        ("dashboard-edit-412x2300", 0, "", "", 1, "edit"),
        ("dashboard-edit-360x2300", 0, "", "", 1, "edit"),
        ("dashboard-drag-down", 0, "", "", 1, "drag:300:140"),
        ("dashboard-drag-down-more", 0, "", "", 1, "drag:300:260"),
        ("dashboard-drag-up", 0, "", "", 1, "drag:560:-120"),
        ("proxies-wide", 1, "节点选择", "", 1, ""),
        ("pmenu-node", 1, "节点选择", "", 1, "nodemenu"),
        ("pmenu-node-small", 1, "节点选择", "", 1, "nodemenu"),
        ("pmenu-node-wide", 1, "节点选择", "", 1, "nodemenu"),
        ("pmenu-node-tablet", 1, "节点选择", "", 1, "nodemenu"),
        ("pmenu-group", 1, "节点选择", "", 1, "groupmenu"),
        ("pmenu-group-small", 1, "节点选择", "", 1, "groupmenu"),
        ("pmenu-group-wide", 1, "节点选择", "", 1, "groupmenu"),
        ("pmenu-group-tablet", 1, "节点选择", "", 1, "groupmenu"),
        ("plist-wide", 1, "节点选择", "", 1, "tabs-list"),
        ("plist-tablet", 1, "节点选择", "", 1, "tabs-list"),
        ("ptab", 1, "节点选择", "", 1, "tabs"),
        ("ptab-small", 1, "节点选择", "", 1, "tabs"),
        ("ptab-wide", 1, "节点选择", "", 1, "tabs"),
        ("ptab-tablet", 1, "节点选择", "", 1, "tabs"),
        ("profiles-tablet", 2, "", "", 1, ""),
        ("proxies", 1, "节点选择", "", 1, ""),
        ("proxies-tab", 1, "节点选择", "", 1, "tab"),
        ("proxies-tab-loose", 1, "节点选择", "", 1, "loose"),
        ("proxies-options", 1, "节点选择", "", 1, "options"),
        ("proxies-search", 1, "节点选择", "", 1, "search"),
        ("profiles", 2, "", "", 1, ""),
        ("profiles-add-menu", 2, "", "", 1, "add-menu"),
        ("profiles-add-menu-tv", 2, "", "", 1, "add-menu-tv"),
        ("profiles-send-phone", 2, "", "", 1, "send-phone"),
        ("profiles-send-phone-small", 2, "", "", 1, "send-phone"),
        ("profiles-send-phone-tablet", 2, "", "", 1, "send-phone"),
        ("profiles-send-phone-wide", 2, "", "", 1, "send-phone"),
        ("profiles-small", 2, "", "", 1, ""),
        ("profiles-menu-custom", 2, "", "", 1, "menu-custom"),
        ("profiles-menu-custom-small", 2, "", "", 1, "menu-custom"),
        ("custom", 2, "", "custom", 1, ""),
        ("custom-small", 2, "", "custom", 1, "long"),
        ("custom-wide", 2, "", "custom", 1, ""),
        ("custom-tablet", 2, "", "custom", 1, ""),
        ("custom-groups", 2, "", "custom", 1, "groups"),
        ("custom-groups-small", 2, "", "custom", 1, "groups"),
        ("custom-groups-wide", 2, "", "custom", 1, "groups"),
        ("custom-groups-tablet", 2, "", "custom", 1, "groups"),
        ("custom-groups-select", 2, "", "custom", 1, "gselect"),
        ("custom-groups-select-small", 2, "", "custom", 1, "gselect"),
        ("custom-groups-sort", 2, "", "custom", 1, "gsort"),
        ("custom-groups-sort-small", 2, "", "custom", 1, "gsort"),
        ("custom-groups-menu", 2, "", "custom", 1, "gmenu"),
        ("custom-groups-delete", 2, "", "custom", 1, "gdelete"),
        ("group-form", 2, "", "group", 1, "gform"),
        ("pm-providers", 2, "", "pmore", 1, "pm:providers"),
        ("pm-providers-small", 2, "", "pmore", 1, "pm:providers"),
        ("pm-providers-wide", 2, "", "pmore", 1, "pm:providers"),
        ("pm-providers-tablet", 2, "", "pmore", 1, "pm:providers"),
        ("pm-more", 2, "", "pmore", 1, "pm:more"),
        ("pm-more-small", 2, "", "pmore", 1, "pm:more"),
        ("pm-more-wide", 2, "", "pmore", 1, "pm:more"),
        ("pm-more-tablet", 2, "", "pmore", 1, "pm:more"),
        ("pm-dialers", 2, "", "pmore", 1, "pm:dialers"),
        ("pm-dialers-small", 2, "", "pmore", 1, "pm:dialers"),
        ("pm-dialers-wide", 2, "", "pmore", 1, "pm:dialers"),
        ("pm-dialers-tablet", 2, "", "pmore", 1, "pm:dialers"),
        ("pm-dns", 2, "", "pmore", 1, "pm:dns"),
        ("pm-dns-small", 2, "", "pmore", 1, "pm:dns"),
        ("pm-dns-wide", 2, "", "pmore", 1, "pm:dns"),
        ("pm-dns-tablet", 2, "", "pmore", 1, "pm:dns"),
        ("pm-ntp", 2, "", "pmore", 1, "pm:ntp"),
        ("pm-ntp-small", 2, "", "pmore", 1, "pm:ntp"),
        ("pm-ntp-wide", 2, "", "pmore", 1, "pm:ntp"),
        ("pm-ntp-tablet", 2, "", "pmore", 1, "pm:ntp"),
        ("pm-sniffer", 2, "", "pmore", 1, "pm:sniffer"),
        ("pm-sniffer-small", 2, "", "pmore", 1, "pm:sniffer"),
        ("pm-sniffer-wide", 2, "", "pmore", 1, "pm:sniffer"),
        ("pm-sniffer-tablet", 2, "", "pmore", 1, "pm:sniffer"),
        ("pm-yaml-sniffer", 2, "", "editor", 1, "pm:yaml-sniffer"),
        (
            "pm-yaml-sniffer-small",
            2,
            "",
            "editor",
            1,
            "pm:yaml-sniffer",
        ),
        ("pm-yaml-dns-wide", 2, "", "editor", 1, "pm:yaml-dns"),
        ("pm-yaml-ntp-tablet", 2, "", "editor", 1, "pm:yaml-ntp"),
        ("pm-tun", 2, "", "pmore", 1, "pm:tun"),
        ("pm-tun-small", 2, "", "pmore", 1, "pm:tun"),
        ("pm-tun-wide", 2, "", "pmore", 1, "pm:tun"),
        ("pm-tun-tablet", 2, "", "pmore", 1, "pm:tun"),
        ("pform", 2, "", "group", 1, "pform"),
        ("pform-small", 2, "", "group", 1, "pform"),
        ("pform-wide", 2, "", "group", 1, "pform"),
        ("pform-tablet", 2, "", "group", 1, "pform"),
        ("pm-sniff", 2, "", "pmore", 1, "pm:sniff:sniff.TLS"),
        ("pm-sniff-small", 2, "", "pmore", 1, "pm:sniff:sniff.TLS"),
        ("pm-list", 2, "", "pmore", 1, "pm:list:dns:nameserver"),
        ("pm-list-small", 2, "", "pmore", 1, "pm:list:dns:nameserver"),
        ("pm-ntp-empty", 2, "", "pmore", 1, "pm:ntp:empty"),
        ("pm-ntp-empty-small", 2, "", "pmore", 1, "pm:ntp:empty"),
        (
            "pm-providers-empty",
            2,
            "",
            "pmore",
            1,
            "pm:providers:empty",
        ),
        ("extend", 2, "", "extend", 1, "ext:main"),
        ("extend-scripts", 2, "", "extend", 1, "ext:main"),
        ("extend-scripts-small", 2, "", "extend", 1, "ext:main"),
        ("extend-scripts-wide", 2, "", "extend", 1, "ext:main"),
        ("extend-scripts-tablet", 2, "", "extend", 1, "ext:main"),
        ("extend-small", 2, "", "extend", 1, "ext:main"),
        ("extend-wide", 2, "", "extend", 1, "ext:main"),
        ("extend-tablet", 2, "", "extend", 1, "ext:main"),
        ("extend-select", 2, "", "extend", 1, "ext:select"),
        ("extend-select-small", 2, "", "extend", 1, "ext:select"),
        ("extend-select-wide", 2, "", "extend", 1, "ext:select"),
        ("extend-select-tablet", 2, "", "extend", 1, "ext:select"),
        ("extend-apply", 2, "", "extend", 1, "ext:apply"),
        ("extend-apply-small", 2, "", "extend", 1, "ext:apply"),
        ("extend-apply-wide", 2, "", "extend", 1, "ext:apply"),
        ("extend-apply-tablet", 2, "", "extend", 1, "ext:apply"),
        ("extend-global", 2, "", "ext", 1, "ext:global"),
        ("extend-global-small", 2, "", "ext", 1, "ext:global"),
        ("extend-global-wide", 2, "", "ext", 1, "ext:global"),
        ("extend-global-tablet", 2, "", "ext", 1, "ext:global"),
        ("pm-exits", 2, "", "pmore", 1, "pmexits"),
        ("pm-exits-small", 2, "", "pmore", 1, "pmexits"),
        ("pm-exits-wide", 2, "", "pmore", 1, "pmexits"),
        ("pm-exits-tablet", 2, "", "pmore", 1, "pmexits"),
        ("extend-global-empty", 2, "", "ext", 1, "ext:global-empty"),
        ("pform-pick", 2, "", "group-members", 1, "pfpick"),
        ("pform-pick-small", 2, "", "group-members", 1, "pfpick"),
        ("group-form-small", 2, "", "group", 1, "gform"),
        ("group-form-wide", 2, "", "group", 1, "gform"),
        ("group-form-tablet", 2, "", "group", 1, "gform"),
        ("group-form-new", 2, "", "group", 1, "gnew"),
        ("group-form-new-small", 2, "", "group", 1, "gnew"),
        ("group-form-lb", 2, "", "group", 1, "glb"),
        ("group-form-save", 2, "", "group", 1, "gsave"),
        ("group-members", 2, "", "group-members", 1, "gpick"),
        ("group-members-small", 2, "", "group-members", 1, "gpick"),
        ("group-members-wide", 2, "", "group-members", 1, "gpick"),
        ("group-members-tablet", 2, "", "group-members", 1, "gpick"),
        ("group-members-search", 2, "", "group-members", 1, "gpicks"),
        ("group-members-none", 2, "", "group-members", 1, "gpicke"),
        ("group-icon", 2, "", "group-icon", 1, "gicon"),
        ("group-icon-small", 2, "", "group-icon", 1, "gicon"),
        ("group-icon-wide", 2, "", "group-icon", 1, "gicon"),
        ("group-icon-tablet", 2, "", "group-icon", 1, "gicon"),
        ("group-icon-search", 2, "", "group-icon", 1, "giconsearch"),
        ("group-icon-empty", 2, "", "group-icon", 1, "giconempty"),
        ("group-icon-data", 2, "", "group-icon", 1, "gicondata"),
        ("group-yaml", 2, "", "editor", 1, "gyaml"),
        ("group-yaml-small", 2, "", "editor", 1, "gyaml"),
        ("rules", 2, "", "rules", 1, "r-list"),
        ("rules-small", 2, "", "rules", 1, "r-list"),
        ("rules-wide", 2, "", "rules", 1, "r-list"),
        ("rules-tablet", 2, "", "rules", 1, "r-list"),
        ("rules-select", 2, "", "rules", 1, "r-select"),
        ("rules-select-small", 2, "", "rules", 1, "r-select"),
        ("rules-sort", 2, "", "rules", 1, "r-sort"),
        ("rules-sort-small", 2, "", "rules", 1, "r-sort"),
        ("rules-menu", 2, "", "rules", 1, "r-menu"),
        ("rules-add", 2, "", "rules", 1, "r-add"),
        ("rules-add-wide", 2, "", "rules", 1, "r-add"),
        ("rules-search", 2, "", "rules", 1, "r-search"),
        ("rules-empty", 2, "", "rules", 1, "r-empty"),
        ("rules-delete", 2, "", "rules", 1, "r-delete"),
        ("rule-form", 2, "", "rule", 1, "r-form"),
        ("rule-form-small", 2, "", "rule", 1, "r-form"),
        ("rule-form-wide", 2, "", "rule", 1, "r-form"),
        ("rule-form-tablet", 2, "", "rule", 1, "r-form"),
        ("rule-form-new", 2, "", "rule", 1, "r-new"),
        ("rule-form-new-small", 2, "", "rule", 1, "r-new"),
        ("rule-form-set", 2, "", "rule", 1, "r-set"),
        ("rule-form-match", 2, "", "rule", 1, "r-match"),
        ("rule-form-type", 2, "", "rule", 1, "r-type"),
        ("rule-form-type-small", 2, "", "rule", 1, "r-type"),
        ("rule-form-target", 2, "", "rule", 1, "r-target"),
        ("rule-form-save", 2, "", "rule", 1, "r-save"),
        ("rule-presets", 2, "", "rule-presets", 1, "r-presets"),
        ("rule-presets-small", 2, "", "rule-presets", 1, "r-presets"),
        ("rule-presets-wide", 2, "", "rule-presets", 1, "r-presets"),
        ("rule-presets-tablet", 2, "", "rule-presets", 1, "r-presets"),
        ("rules-yaml", 2, "", "editor", 1, "r-yaml"),
        ("rules-yaml-small", 2, "", "editor", 1, "r-yaml"),
        ("rules-yaml-wide", 2, "", "editor", 1, "r-yaml"),
        ("rules-yaml-tablet", 2, "", "editor", 1, "r-yaml"),
        ("custom-issues", 2, "", "custom", 1, "issues"),
        ("custom-issues-small", 2, "", "custom", 1, "issues"),
        ("custom-import", 2, "", "custom", 1, "cimport"),
        ("custom-import-small", 2, "", "custom", 1, "cimport"),
        ("custom-import-wide", 2, "", "custom", 1, "cimport"),
        ("custom-import-tablet", 2, "", "custom", 1, "cimport"),
        ("custom-importing", 2, "", "custom", 1, "cimporting"),
        ("custom-importing-small", 2, "", "custom", 1, "cimporting"),
        ("custom-replace", 2, "", "custom", 1, "creplace"),
        ("custom-replace-small", 2, "", "custom", 1, "creplace"),
        ("custom-replace-wide", 2, "", "custom", 1, "creplace"),
        ("custom-replace-tablet", 2, "", "custom", 1, "creplace"),
        ("custom-menu", 2, "", "custom", 1, "cmenu"),
        ("custom-menu-wide", 2, "", "custom", 1, "cmenu"),
        ("custom-placeholder", 2, "", "placeholder", 1, "custom"),
        (
            "custom-placeholder-small",
            2,
            "",
            "placeholder",
            1,
            "custom",
        ),
        ("profiles-link", 2, "", "", 1, "link"),
        ("profiles-menu", 2, "", "", 1, "menu"),
        ("profiles-confirm", 2, "", "", 1, "confirm"),
        ("profiles-edit", 2, "", "edit", 1, ""),
        ("profiles-preview", 2, "", "preview", 1, ""),
        ("profiles-sort", 2, "", "sort", 1, ""),
        ("profiles-sort-small", 2, "", "sort", 1, ""),
        ("profiles-sort-wide", 2, "", "sort", 1, ""),
        ("profiles-sort-tablet", 2, "", "sort", 1, ""),
        ("profiles-wide", 2, "", "", 1, ""),
        ("profiles-menu-small", 2, "", "", 1, "menu"),
        ("profiles-menu-wide", 2, "", "", 1, "menu"),
        ("profiles-add-menu-wide", 2, "", "", 1, "add-menu"),
        ("tools", 3, "", "", 1, ""),
        ("logs", 3, "", "logs", 1, ""),
        ("logs-small", 3, "", "logs", 1, ""),
        ("logs-wide", 3, "", "logs", 1, ""),
        ("logs-tablet", 3, "", "logs", 1, ""),
        ("logs-filter", 3, "", "logs", 1, "log-filter"),
        ("logs-filter-small", 3, "", "logs", 1, "log-filter"),
        ("logs-detail", 3, "", "logs", 1, "log-detail"),
        ("logs-detail-small", 3, "", "logs", 1, "log-detail"),
        ("logs-detail-wide", 3, "", "logs", 1, "log-detail"),
        ("logs-search", 3, "", "logs", 1, "log-search"),
        ("logs-empty", 3, "", "logs", 1, "log-empty"),
        ("theme", 3, "", "theme", 1, ""),
        ("dashboard-dark", 0, "", "", 2, ""),
        ("proxies-dark", 1, "节点选择", "", 2, ""),
        ("profiles-dark", 2, "", "", 2, ""),
        ("dashboard-small", 0, "", "", 1, ""),
        ("proxies-small", 1, "节点选择", "", 1, ""),
        ("proxies-long", 1, "节点选择", "", 1, "long"),
        ("proxies-long-small", 1, "节点选择", "", 1, "long"),
        ("proxies-long-wide", 1, "节点选择", "", 1, "long"),
        ("proxies-long-tablet", 1, "节点选择", "", 1, "long"),
        ("proxies-long-compact", 1, "节点选择", "", 1, "long-compact"),
        ("proxies-long-loose", 1, "节点选择", "", 1, "long-loose"),
        ("proxies-long-scrolled", 1, "节点选择", "", 1, "long-scroll"),
        (
            "proxies-long-scrolled-small",
            1,
            "节点选择",
            "",
            1,
            "long-scroll",
        ),
        (
            "proxies-long-scrolled-wide",
            1,
            "节点选择",
            "",
            1,
            "long-scroll",
        ),
        (
            "proxies-long-scrolled-tablet",
            1,
            "节点选择",
            "",
            1,
            "long-scroll",
        ),
        ("profiles-small", 2, "", "", 1, ""),
        ("tools-small", 3, "", "", 1, ""),
        ("dashboard-tiny", 0, "", "", 1, ""),
        ("profiles-tiny", 2, "", "", 1, ""),
        ("dashboard-inset-small", 0, "", "", 1, "inset"),
        ("dashboard-260x450", 0, "", "", 1, ""),
        ("dashboard-inset-260x450", 0, "", "", 1, "inset"),
        ("proxies-260x450", 1, "节点选择", "", 1, ""),
        ("profiles-260x450", 2, "", "", 1, ""),
        ("tools-260x450", 3, "", "", 1, ""),
        ("dashboard-300x500", 0, "", "", 1, ""),
        ("dashboard-inset-300x500", 0, "", "", 1, "inset"),
        ("tools-300x500", 3, "", "", 1, ""),
        ("dashboard-surface-260x450", 0, "", "", 1, "surface"),
        ("set-tools", 3, "", "", 1, ""),
        ("set-tools-small", 3, "", "", 1, ""),
        ("set-tools-wide", 3, "", "", 1, ""),
        ("set-tools-tablet", 3, "", "", 1, ""),
        ("set-general", 3, "", "general", 1, ""),
        ("set-general-small", 3, "", "general", 1, ""),
        ("set-general-wide", 3, "", "general", 1, ""),
        ("set-general-tablet", 3, "", "general", 1, ""),
        ("set-general-open", 3, "", "general", 1, "more"),
        ("set-general-open-small", 3, "", "general", 1, "more"),
        ("set-general-open-wide", 3, "", "general", 1, "more"),
        ("set-general-open-tablet", 3, "", "general", 1, "more"),
        ("set-general-open-412x2000", 3, "", "general", 1, "more"),
        ("set-general-open-360x2000", 3, "", "general", 1, "more"),
        ("set-general-412x1900", 3, "", "general", 1, ""),
        ("set-developer", 3, "", "developer", 1, ""),
        ("set-developer-small", 3, "", "developer", 1, ""),
        ("set-developer-wide", 3, "", "developer", 1, ""),
        ("set-developer-tablet", 3, "", "developer", 1, ""),
        ("set-about", 3, "", "about", 1, ""),
        ("set-about-small", 3, "", "about", 1, ""),
        ("set-about-wide", 3, "", "about", 1, ""),
        ("set-about-tablet", 3, "", "about", 1, ""),
        ("set-hosts", 3, "", "map", 1, ""),
        ("set-hosts-small", 3, "", "map", 1, ""),
        ("set-hosts-wide", 3, "", "map", 1, ""),
        ("set-hosts-tablet", 3, "", "map", 1, ""),
        ("set-filters", 3, "", "filters", 1, ""),
        ("set-filters-small", 3, "", "filters", 1, ""),
        ("set-filters-wide", 3, "", "filters", 1, ""),
        ("set-filters-tablet", 3, "", "filters", 1, ""),
        ("set-scripts", 3, "", "scripts", 1, ""),
        ("set-script-editor", 3, "", "editor", 1, "script-ed"),
        ("set-script-editor-small", 3, "", "editor", 1, "script-ed"),
        ("set-script-editor-wide", 3, "", "editor", 1, "script-ed"),
        ("set-script-editor-tablet", 3, "", "editor", 1, "script-ed"),
        ("set-script-editor-ask", 3, "", "editor", 1, "script-ask"),
        (
            "set-script-editor-ask-wide",
            3,
            "",
            "editor",
            1,
            "script-ask",
        ),
        ("set-rs-editor", 3, "", "editor", 1, "rs-ed"),
        ("set-rs-editor-small", 3, "", "editor", 1, "rs-ed"),
        ("set-rs-editor-wide", 3, "", "editor", 1, "rs-ed"),
        ("set-rs-editor-tablet", 3, "", "editor", 1, "rs-ed"),
        ("set-rs-editor-ask", 3, "", "editor", 1, "rs-ask"),
        ("set-scripts-small", 3, "", "scripts", 1, ""),
        ("set-scripts-wide", 3, "", "scripts", 1, ""),
        ("set-scripts-tablet", 3, "", "scripts", 1, ""),
        ("set-scripts-empty", 3, "", "scripts", 1, "empty"),
        ("set-scripts-empty-small", 3, "", "scripts", 1, "empty"),
        ("set-scripts-empty-wide", 3, "", "scripts", 1, "empty"),
        ("set-scripts-empty-tablet", 3, "", "scripts", 1, "empty"),
        ("set-filters-empty", 3, "", "filters", 1, "empty"),
        ("set-filters-empty-small", 3, "", "filters", 1, "empty"),
        ("set-filters-empty-wide", 3, "", "filters", 1, "empty"),
        ("set-filters-empty-tablet", 3, "", "filters", 1, "empty"),
        ("set-rulesets-empty-small", 3, "", "rule-sets", 1, "empty"),
        ("set-rulesets-empty-wide", 3, "", "rule-sets", 1, "empty"),
        ("set-rulesets-empty-tablet", 3, "", "rule-sets", 1, "empty"),
        ("set-iconsets-empty-small", 3, "", "icon-sets", 1, "empty"),
        ("set-iconsets-empty-wide", 3, "", "icon-sets", 1, "empty"),
        ("set-iconsets-empty-tablet", 3, "", "icon-sets", 1, "empty"),
        ("set-addrules-empty", 3, "", "rules", 1, "empty"),
        ("set-addrules-empty-small", 3, "", "rules", 1, "empty"),
        ("set-addrules-empty-wide", 3, "", "rules", 1, "empty"),
        ("set-addrules-empty-tablet", 3, "", "rules", 1, "empty"),
        ("set-mhosts-empty-small", 3, "", "mitm-hosts", 1, "empty"),
        ("set-mhosts-empty-wide", 3, "", "mitm-hosts", 1, "empty"),
        ("set-mhosts-empty-tablet", 3, "", "mitm-hosts", 1, "empty"),
        ("set-mmods-empty-small", 3, "", "mitm-modules", 1, "empty"),
        ("set-mmods-empty-wide", 3, "", "mitm-modules", 1, "empty"),
        ("set-mmods-empty-tablet", 3, "", "mitm-modules", 1, "empty"),
        ("set-rulesets", 3, "", "rule-sets", 1, ""),
        ("set-rulesets-small", 3, "", "rule-sets", 1, ""),
        ("set-rulesets-wide", 3, "", "rule-sets", 1, ""),
        ("set-rulesets-tablet", 3, "", "rule-sets", 1, ""),
        ("set-rulesets-empty", 3, "", "rule-sets", 1, "empty"),
        ("set-mitm", 3, "", "mitm", 1, ""),
        ("set-mitm-small", 3, "", "mitm", 1, ""),
        ("set-mitm-wide", 3, "", "mitm", 1, ""),
        ("set-mitm-tablet", 3, "", "mitm", 1, ""),
        ("set-mitm-nocert", 3, "", "mitm", 1, "nocert"),
        ("set-mitm-loading", 3, "", "mitm", 1, "loading"),
        ("set-mitm-trust", 3, "", "mitm", 1, "trust-user"),
        ("set-mitm-trust-small", 3, "", "mitm", 1, "trust-user"),
        ("set-mitm-trust-wide", 3, "", "mitm", 1, "trust-user"),
        ("set-mitm-trust-tablet", 3, "", "mitm", 1, "trust-none"),
        ("set-mitm-trust-remove", 3, "", "mitm", 1, "trust-remove"),
        (
            "set-mitm-trust-remove-small",
            3,
            "",
            "mitm",
            1,
            "trust-remove",
        ),
        ("set-mitm-capture", 3, "", "mitm-capture", 1, ""),
        ("set-mitm-hosts", 3, "", "mitm-hosts", 1, ""),
        ("set-mitm-hosts-small", 3, "", "mitm-hosts", 1, ""),
        ("set-mitm-hosts-wide", 3, "", "mitm-hosts", 1, ""),
        ("set-mitm-hosts-tablet", 3, "", "mitm-hosts", 1, ""),
        ("set-mitm-hosts-empty", 3, "", "mitm-hosts", 1, "empty"),
        ("set-mitm-modules", 3, "", "mitm-modules", 1, ""),
        ("set-mitm-modules-small", 3, "", "mitm-modules", 1, ""),
        ("set-mitm-modules-wide", 3, "", "mitm-modules", 1, ""),
        ("set-mitm-modules-tablet", 3, "", "mitm-modules", 1, ""),
        ("set-mitm-modules-empty", 3, "", "mitm-modules", 1, "empty"),
        ("set-mitm-module", 3, "", "mitm-module", 1, ""),
        ("set-mitm-module-small", 3, "", "mitm-module", 1, ""),
        ("set-mitm-module-wide", 3, "", "mitm-module", 1, ""),
        ("set-mitm-module-tablet", 3, "", "mitm-module", 1, ""),
        ("set-mitm-local", 3, "", "mitm-module", 1, "local"),
        ("set-mitm-local-small", 3, "", "mitm-module", 1, "local"),
        ("set-mitm-local-wide", 3, "", "mitm-module", 1, "local"),
        ("set-mitm-local-tablet", 3, "", "mitm-module", 1, "local"),
        ("set-mitm-lines", 3, "", "mitm-lines", 1, ""),
        ("set-mitm-lines-small", 3, "", "mitm-lines", 1, ""),
        ("set-mitm-lines-wide", 3, "", "mitm-lines", 1, ""),
        ("set-mitm-lines-tablet", 3, "", "mitm-lines", 1, ""),
        ("set-cproxies", 3, "", "custom-proxies", 1, ""),
        ("set-cproxies-small", 3, "", "custom-proxies", 1, ""),
        ("set-cproxies-wide", 3, "", "custom-proxies", 1, ""),
        ("set-cproxies-tablet", 3, "", "custom-proxies", 1, ""),
        ("set-cproxies-empty", 3, "", "custom-proxies", 1, "empty"),
        ("set-cproxies-search", 3, "", "custom-proxies", 1, "search"),
        ("set-cproxies-select", 3, "", "custom-proxies", 1, "select"),
        (
            "set-cproxies-select-small",
            3,
            "",
            "custom-proxies",
            1,
            "select",
        ),
        (
            "set-cproxies-select-wide",
            3,
            "",
            "custom-proxies",
            1,
            "select",
        ),
        (
            "set-cproxies-select-tablet",
            3,
            "",
            "custom-proxies",
            1,
            "select",
        ),
        ("set-cproxies-addmenu", 3, "", "custom-proxies", 1, "cp-add"),
        (
            "set-cproxies-addmenu-small",
            3,
            "",
            "custom-proxies",
            1,
            "cp-add",
        ),
        (
            "set-cproxies-addmenu-wide",
            3,
            "",
            "custom-proxies",
            1,
            "cp-add",
        ),
        (
            "set-cproxies-addmenu-tablet",
            3,
            "",
            "custom-proxies",
            1,
            "cp-add",
        ),
        (
            "set-cproxies-empty-small",
            3,
            "",
            "custom-proxies",
            1,
            "empty",
        ),
        ("set-cproxies-links", 3, "", "editor", 1, "cp-links"),
        ("set-cproxies-links-small", 3, "", "editor", 1, "cp-links"),
        ("set-cproxies-links-wide", 3, "", "editor", 1, "cp-links"),
        ("set-cproxies-links-tablet", 3, "", "editor", 1, "cp-links"),
        ("set-cproxy-link", 3, "", "custom-proxy", 1, "cp-link"),
        ("set-cproxy-link-small", 3, "", "custom-proxy", 1, "cp-link"),
        ("set-cproxy-link-wide", 3, "", "custom-proxy", 1, "cp-link"),
        (
            "set-cproxy-link-tablet",
            3,
            "",
            "custom-proxy",
            1,
            "cp-link",
        ),
        ("set-cproxy-easytier", 3, "", "custom-proxy", 1, "easytier"),
        (
            "set-cproxy-easytier-small",
            3,
            "",
            "custom-proxy",
            1,
            "easytier",
        ),
        (
            "set-cproxy-easytier-wide",
            3,
            "",
            "custom-proxy",
            1,
            "easytier",
        ),
        (
            "set-cproxy-easytier-tablet",
            3,
            "",
            "custom-proxy",
            1,
            "easytier",
        ),
        ("set-cproxy", 3, "", "custom-proxy", 1, ""),
        ("set-cproxy-small", 3, "", "custom-proxy", 1, ""),
        ("set-cproxy-wide", 3, "", "custom-proxy", 1, ""),
        ("set-cproxy-tablet", 3, "", "custom-proxy", 1, ""),
        ("set-cproxy-new", 3, "", "custom-proxy", 1, "new"),
        ("set-cproxy-new-small", 3, "", "custom-proxy", 1, "new"),
        ("set-cproxy-add", 3, "", "custom-proxy", 1, "add"),
        ("set-cproxy-dup", 3, "", "custom-proxy", 1, "dup"),
        ("set-iconsets", 3, "", "icon-sets", 1, ""),
        ("set-iconsets-small", 3, "", "icon-sets", 1, ""),
        ("set-iconsets-wide", 3, "", "icon-sets", 1, ""),
        ("set-iconsets-tablet", 3, "", "icon-sets", 1, ""),
        ("set-iconsets-empty", 3, "", "icon-sets", 1, "empty"),
        ("set-iconset", 3, "", "icon-set", 1, ""),
        ("set-iconset-small", 3, "", "icon-set", 1, ""),
        ("set-iconset-wide", 3, "", "icon-set", 1, ""),
        ("set-iconset-tablet", 3, "", "icon-set", 1, ""),
        ("set-iconset-search", 3, "", "icon-set", 1, "search"),
        ("set-iconset-none", 3, "", "icon-set", 1, "none"),
        ("set-rules", 3, "", "rules", 1, ""),
        ("set-rules-small", 3, "", "rules", 1, ""),
        ("set-rules-wide", 3, "", "rules", 1, ""),
        ("set-rules-tablet", 3, "", "rules", 1, ""),
        ("set-rules-empty", 3, "", "rules", 1, "empty"),
        ("set-rule-form", 3, "", "rule-form", 1, ""),
        ("set-rule-form-small", 3, "", "rule-form", 1, ""),
        ("set-rule-presets", 3, "", "rule-presets", 1, ""),
        ("set-rule-presets-small", 3, "", "rule-presets", 1, ""),
        ("set-dns", 3, "", "dns", 1, ""),
        ("set-dns-small", 3, "", "dns", 1, ""),
        ("set-dns-wide", 3, "", "dns", 1, ""),
        ("set-dns-tablet", 3, "", "dns", 1, ""),
        ("set-dns-empty", 3, "", "dns", 1, "empty"),
        ("set-dns-empty-small", 3, "", "dns", 1, "empty"),
        ("set-theme", 3, "", "theme", 1, ""),
        ("set-theme-small", 3, "", "theme", 1, ""),
        ("set-theme-wide", 3, "", "theme", 1, ""),
        ("set-theme-tablet", 3, "", "theme", 1, ""),
        ("set-theme-dark", 3, "", "theme", 2, ""),
        ("set-theme-custom", 3, "", "theme", 1, "custom"),
        ("set-theme-docked", 3, "", "theme", 1, "docked"),
        ("set-theme-scale-412x2300", 3, "", "theme", 1, "scale:1.25"),
        ("set-font", 3, "", "font", 1, "font-picked"),
        ("set-font-small", 3, "", "font", 2, ""),
        ("set-font-wide", 3, "", "font", 1, "font-picked"),
        ("set-font-tablet", 3, "", "font", 1, ""),
        ("scale14-dash", 0, "", "", 1, "scale:1.4"),
        ("scale14-dash-small", 0, "", "", 1, "scale:1.4"),
        ("scale14-tools", 3, "", "", 1, "scale:1.4"),
        ("scale14-general-small", 3, "", "general", 1, "scale:1.4"),
        ("scale14-profiles", 2, "", "", 1, "scale:1.4"),
        ("scale14-proxies", 1, "", "", 1, "scale:1.4"),
        ("scale08-dash", 0, "", "", 1, "scale:0.8"),
        ("set-theme-docked-small", 3, "", "theme", 2, "docked"),
        ("set-theme-docked-wide", 3, "", "theme", 1, "docked"),
        ("set-theme-docked-tablet", 3, "", "theme", 1, "docked"),
        ("set-theme-full-412x2300", 3, "", "theme", 1, ""),
        ("set-theme-docked-412x2300", 3, "", "theme", 2, "docked"),
        ("set-tools-docked", 3, "", "", 1, "docked"),
        ("set-tools-docked-small", 3, "", "", 2, "docked"),
        ("set-theme-custom-dark", 3, "", "theme", 2, "custom"),
        ("set-theme-custom-small", 3, "", "theme", 1, "custom"),
        ("set-color", 3, "", "theme-color", 1, "grid"),
        ("set-color-small", 3, "", "theme-color", 1, "grid"),
        ("set-color-wide", 3, "", "theme-color", 1, "grid"),
        ("set-color-tablet", 3, "", "theme-color", 1, "grid"),
        ("set-color-spectrum", 3, "", "theme-color", 1, "spectrum"),
        (
            "set-color-spectrum-small",
            3,
            "",
            "theme-color",
            1,
            "spectrum",
        ),
        ("set-color-sliders", 3, "", "theme-color", 1, "sliders"),
        (
            "set-color-sliders-small",
            3,
            "",
            "theme-color",
            1,
            "sliders",
        ),
        ("set-color-sliders-dark", 3, "", "theme-color", 2, "sliders"),
        ("set-language", 3, "", "", 1, "choice:language"),
        ("set-language-small", 3, "", "", 1, "choice:language"),
        ("set-language-wide", 3, "", "", 1, "choice:language"),
        ("set-language-tablet", 3, "", "", 1, "choice:language"),
        ("set-port", 3, "", "general", 1, "input:port"),
        ("set-port-small", 3, "", "general", 1, "input:port"),
        ("set-port-wide", 3, "", "general", 1, "input:port"),
        ("set-port-tablet", 3, "", "general", 1, "input:port"),
        ("set-general-dark", 3, "", "general", 2, ""),
        ("set-advanced", 3, "", "advanced", 1, ""),
        ("set-advanced-wide", 3, "", "advanced", 1, ""),
        ("set-network", 3, "", "network", 1, ""),
        ("set-network-small", 3, "", "network", 1, ""),
        ("set-network-wide", 3, "", "network", 1, ""),
        ("set-network-tablet", 3, "", "network", 1, ""),
        ("set-list", 3, "", "list", 1, ""),
        ("set-list-small", 3, "", "list", 1, ""),
        ("set-ondemand", 3, "", "on-demand", 1, ""),
        ("set-ondemand-small", 3, "", "on-demand", 1, ""),
        ("set-ondemand-wide", 3, "", "on-demand", 1, ""),
        ("set-ondemand-tablet", 3, "", "on-demand", 1, ""),
        ("set-ondemand-empty", 3, "", "on-demand", 1, "od-empty"),
        (
            "set-ondemand-empty-small",
            3,
            "",
            "on-demand",
            1,
            "od-empty",
        ),
        ("set-ondemand-select", 3, "", "on-demand", 1, "od-select"),
        (
            "set-ondemand-select-small",
            3,
            "",
            "on-demand",
            1,
            "od-select",
        ),
        ("set-ondemand-drag", 3, "", "on-demand", 1, "drag:316:70"),
        ("set-general-od", 3, "", "general", 1, ""),
        ("dash-suspended", 0, "", "", 1, "suspended"),
        ("dash-suspended-wide", 0, "", "", 1, "suspended"),
        ("set-ua", 3, "", "user-agents", 1, ""),
        ("set-ua-small", 3, "", "user-agents", 1, ""),
        ("set-ua-wide", 3, "", "user-agents", 1, ""),
        ("set-ua-tablet", 3, "", "user-agents", 1, ""),
        ("set-ua-sort", 3, "", "user-agents", 1, "ua-sort"),
        ("set-ua-sort-small", 3, "", "user-agents", 1, "ua-sort"),
        ("set-ua-sort-wide", 3, "", "user-agents", 1, "ua-sort"),
        ("set-ua-sort-tablet", 3, "", "user-agents", 1, "ua-sort"),
        ("set-ua-menu", 3, "", "user-agents", 1, "ua-menu"),
        ("set-ua-menu-small", 3, "", "user-agents", 1, "ua-menu"),
        ("set-ua-menu-wide", 3, "", "user-agents", 1, "ua-menu"),
        ("set-ua-menu-tablet", 3, "", "user-agents", 1, "ua-menu"),
        ("set-access", 3, "", "access", 1, ""),
        ("set-access-small", 3, "", "access", 1, ""),
        ("set-access-wide", 3, "", "access", 1, ""),
        ("set-access-tablet", 3, "", "access", 1, ""),
        ("set-access-off-small", 3, "", "access", 1, "off"),
        ("set-access-perm", 3, "", "access", 1, "perm"),
        ("set-backup", 3, "", "backup", 1, ""),
        ("set-backup-small", 3, "", "backup", 1, ""),
        ("set-backup-wide", 3, "", "backup", 1, ""),
        ("set-backup-tablet", 3, "", "backup", 1, ""),
        ("set-backup-dav", 3, "", "backup", 1, "dav"),
        ("set-backup-dav-small", 3, "", "backup", 1, "dav"),
        ("set-dav-form", 3, "", "backup", 1, "dav-form"),
        ("set-dav-form-small", 3, "", "backup", 1, "dav-form"),
    ];
    // Every sample url draws a flat square in a colour of its own.
    let images = ui.global::<NetImages>();
    images.on_has(|url, _| url.starts_with("https://example.com/"));
    images.on_image(|url, _| {
        let h = url
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
        let mut p = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(64, 64);
        for px in p.make_mut_slice() {
            *px = slint::Rgba8Pixel::new(h as u8, (h >> 8) as u8, (h >> 16) as u8, 255);
        }
        slint::Image::from_rgba8(p)
    });
    let only = std::env::args().nth(2).unwrap_or_default();
    ui.show().unwrap();
    let mut saved_logs: Option<ModelRc<LogItem>> = None;
    let base_traffic = ui.get_dash_traffic();
    let base_groups = ui.get_proxy_groups();
    let base_profiles = ui.get_profiles();
    for (name, tab, group, page, mode, overlay) in shots {
        if !only.is_empty() && !only.split(',').any(|p| name.contains(p)) {
            continue;
        }
        let sized = name
            .rsplit('-')
            .next()
            .and_then(|s| s.split_once('x'))
            .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
        let (w, h) = if let Some(size) = sized {
            size
        } else if name.ends_with("-tablet") {
            (800u32, 1280u32)
        } else if name.ends_with("-wide") {
            (1280u32, 800u32)
        } else if name.ends_with("-small") {
            (360u32, 640u32)
        } else if name.ends_with("-tiny") {
            (300u32, 520u32)
        } else {
            (412u32, 892u32)
        };
        window.set_size(slint::PhysicalSize::new(w, h));
        ui.set_dash_editing(*overlay == "edit" || overlay.starts_with("drag:"));
        ui.set_ip_sheet_open(matches!(*overlay, "ipsheet" | "svcip"));
        // "svcip": a service row's address opened over the service sheet.
        ui.set_ipq_ip(
            if *overlay == "svcip" {
                "198.51.100.7"
            } else {
                ""
            }
            .into(),
        );
        ui.set_ipq_other(DashStatus {
            ip: "198.51.100.7".into(),
            country: "JP".into(),
            kind: "机房".into(),
            quality: 2,
            org: "Example Cloud".into(),
            probing: false,
            failed: false,
        });
        ui.set_mem_sheet_open(*overlay == "memsheet");
        ui.set_svc_sheet_open(overlay.starts_with("svcsheet") || *overlay == "svcip");
        ui.set_svc_sheet_page(if *overlay == "svcsheet-manage" {
            "manage".into()
        } else {
            "".into()
        });
        ui.set_dash_services(ModelRc::new(VecModel::from(service_cells(2, false))));
        // "noproxies": the Core answered with no groups, so 代理 leaves the bar.
        ui.set_groups_loaded(*overlay == "noproxies");
        if *overlay == "noproxies" {
            ui.set_proxy_groups(ModelRc::new(VecModel::<ProxyGroup>::default()));
        } else {
            ui.set_proxy_groups(base_groups.clone());
        }
        ui.set_profile_sheet_open(*overlay == "profsheet");
        ui.set_picker_open(overlay.starts_with("picker"));
        if *overlay == "picker-empty" {
            ui.set_profiles(ModelRc::new(VecModel::<ProfileItem>::default()));
        } else {
            ui.set_profiles(base_profiles.clone());
        }
        // "feed:<segment>[:tall]": the requests page in the dashboard's sheet.
        ui.set_feed_open(overlay.starts_with("feed:"));
        ui.set_feed_tall(overlay.ends_with(":tall"));
        ui.set_group_sheet_open(*overlay == "groupsheet");
        ui.set_dash_fault_count(if *overlay == "faults" { 2 } else { 0 });
        // "live:<front>": the developer page's deck on that card; "live-open" with the
        // warnings opened; "pill:<kind>": the bar button for one status.
        let live_front = overlay
            .strip_prefix("live:")
            .and_then(|f| f.parse::<usize>().ok());
        let deck = live_front.is_some() || *overlay == "live-open";
        let list = live_status::preview();
        let cards: Vec<LiveCard> = list
            .iter()
            .map(|st| live_app::card_model(live_status::card(st, None, "1.4.2")))
            .collect();
        ui.set_live_cards(ModelRc::new(VecModel::from(cards)));
        ui.set_live_front(if *overlay == "live-open" {
            5
        } else {
            live_front.unwrap_or(0) as i32
        });
        ui.set_live_expanded(*overlay == "live-open");
        ui.set_live_asking(false);
        ui.set_live_open(deck);
        let pill = match overlay.strip_prefix("pill:") {
            Some("stopped") => live_status::pill(&list[..1]),
            Some("warnings") => live_status::pill(&list[5..6]),
            Some("downloading") => live_status::pill(&list[3..6]),
            Some("running") => live_status::pill(&list[6..]),
            _ => None,
        }
        .unwrap_or_default();
        ui.set_live_pill(LivePill {
            kind: pill.kind.into(),
            tone: pill.tone,
            text: pill.text.into(),
            progress: pill.progress,
        });
        let mut st = ui.get_dash_status();
        st.failed = *overlay == "failed";
        ui.set_dash_status(st);
        let cards: Vec<&str> = if *overlay == "cards" {
            vec!["status", "service", "stats", "ip", "memory", "vpn"]
        } else {
            dashboard::default_widgets().iter().map(|_| "").collect()
        };
        if *overlay == "cards" {
            ui.set_dash_widgets(ModelRc::new(VecModel::from(
                cards
                    .iter()
                    .map(|s| slint::SharedString::from(*s))
                    .collect::<Vec<_>>(),
            )));
        } else {
            ui.set_dash_widgets(ModelRc::new(VecModel::from(
                dashboard::default_widgets()
                    .into_iter()
                    .map(slint::SharedString::from)
                    .collect::<Vec<_>>(),
            )));
        }
        ui.set_run_state(
            if *overlay == "stopped" {
                "STOPPED"
            } else {
                "STARTED"
            }
            .into(),
        );
        // "surface": a floating window's surface, larger than the window on every side.
        let (top, bottom, side) = match *overlay {
            "inset" => (24.0, 24.0, 0.0),
            "surface" => (16.0, 16.0, 16.0),
            _ => (0.0, 0.0, 0.0),
        };
        ui.set_insets_known(matches!(*overlay, "inset" | "surface"));
        ui.set_inset_top(top);
        ui.set_inset_bottom(bottom);
        ui.set_inset_left(side);
        ui.set_inset_right(side);
        ui.set_tab(*tab);
        ui.set_current_group((*group).into());
        ui.set_req_sheet_open(false);
        ui.set_req_actions_open(false);
        ui.set_cond_open(false);
        if overlay.starts_with("req") {
            requests_shot(&ui, &mut req, now, overlay, w);
        } else if let Some(seg) = overlay.strip_prefix("feed:") {
            let seg = seg.split(':').next().unwrap_or("0");
            requests_shot(&ui, &mut req, now, &format!("req:{seg}:0:0"), 412);
        }
        ui.set_res_dialog(overlay.strip_prefix("res:").unwrap_or("").into());
        if *overlay == "res:interval" {
            ui.set_res_dialog_title("自动更新间隔".into());
            ui.set_res_dialog_value("24".into());
            ui.set_res_dialog_default("".into());
        } else if *overlay == "res:url" {
            ui.set_res_dialog_title("MMDB".into());
            ui.set_res_dialog_value("https://example.com/geo".into());
            ui.set_res_dialog_default(resources::default_url("mmdb").into());
        }
        let ssids: Vec<slint::SharedString> = if *overlay == "od-empty" {
            Vec::new()
        } else {
            [
                "Home-5G",
                "Office",
                "MEIZU-20Pro-Hotspot-with-a-very-long-name",
            ]
            .iter()
            .map(|s| (*s).into())
            .collect()
        };
        let picked: Vec<bool> = (0..ssids.len())
            .map(|i| *overlay == "od-select" && i != 1)
            .collect();
        ui.set_od_selecting(*overlay == "od-select");
        ui.set_od_ssids(ModelRc::new(VecModel::from(ssids)));
        ui.set_od_selected(ModelRc::new(VecModel::from(picked)));
        ui.set_od_battery(if *overlay == "od-select" { 0 } else { 1 });
        ui.set_od_location(*overlay != "od-empty");
        ui.global::<Suspend>()
            .set_suspended(*overlay == "suspended");
        settings_shot(&ui, w, page, overlay);
        ui.set_set_sorting(*overlay == "ua-sort");
        if *overlay == "ua-sort" {
            ui.set_set_sort_titles(ModelRc::new(VecModel::from(
                store::Settings::default()
                    .general
                    .user_agents
                    .iter()
                    .map(|u| slint::SharedString::from(u.as_str()))
                    .collect::<Vec<_>>(),
            )));
        }
        if *overlay == "ua-menu" {
            ui.set_ctx_kind("ua".into());
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(vec![
                ActionItem {
                    id: "edit".into(),
                    glyph: "edit".into(),
                    label: "编辑".into(),
                    enabled: true,
                    ..Default::default()
                },
                ActionItem {
                    id: "delete".into(),
                    glyph: "delete".into(),
                    label: "删除".into(),
                    danger: true,
                    enabled: true,
                    ..Default::default()
                },
            ])));
            ui.set_ctx_x(16.0);
            ui.set_ctx_y(124.0);
            ui.set_ctx_w(w as f32 - 32.0);
            ui.set_ctx_h(52.0);
        }
        if *tab == 2 {
            ui.set_profiles_page((*page).into());
            ui.set_tools_page("".into());
        } else {
            ui.set_tools_page((*page).into());
            ui.set_profiles_page("".into());
        }
        ui.set_tools_over(false);
        ui.set_tools_editor_title_editable(false);
        if name.starts_with("extend-scripts") {
            ui.invoke_open_tools("scripts".into());
            settings_shot(&ui, w, "scripts", "");
        }
        if *overlay == "cp-add" {
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(
                [
                    ("add", "plus", "添加"),
                    ("quick-edit", "compose", "快速编辑"),
                ]
                .iter()
                .map(|(id, glyph, label)| ActionItem {
                    id: (*id).into(),
                    glyph: (*glyph).into(),
                    label: (*label).into(),
                    enabled: true,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
            )));
            ui.set_ctx_x(w as f32 - 48.0);
            ui.set_ctx_y(8.0);
            ui.set_ctx_w(40.0);
            ui.set_ctx_h(40.0);
            ui.set_ctx_kind("cp-add".into());
        }
        if overlay.starts_with("script-") {
            ui.set_tools_editor_title("".into());
            ui.set_tools_editor_title_editable(true);
            ui.set_tools_editor_guard(true);
            ui.set_tools_editor_text("// main(config) returns the config to run\nconst main = (config) => {\n  return config;\n};".into());
            ui.set_tools_editor_parent("scripts".into());
            ui.set_tools_editor_readonly(false);
        }
        if overlay.starts_with("rs-") {
            ui.set_tools_editor_title("".into());
            ui.set_tools_editor_title_editable(true);
            ui.set_tools_editor_guard(true);
            ui.set_tools_editor_text(
                "payload:\n  - DOMAIN-SUFFIX,ads.example.com\n  - DOMAIN-KEYWORD,tracker".into(),
            );
            ui.set_tools_editor_parent("rule-sets".into());
            ui.set_tools_editor_readonly(false);
        }
        if *overlay == "rs-ask" {
            ui.set_tools_editor_title("广告".into());
            ui.set_confirm_title("提示".into());
            ui.set_confirm_text("是否保存更改？".into());
            ui.set_confirm_action("rs-save".into());
            ui.set_confirm_open(true);
        }
        if *overlay == "script-ask" {
            ui.set_tools_editor_title("签到".into());
            ui.set_confirm_title("提示".into());
            ui.set_confirm_text("是否保存更改？".into());
            ui.set_confirm_action("script-save".into());
            ui.set_confirm_open(true);
        }
        if *overlay == "cp-links" {
            ui.set_tools_editor_title("代理".into());
            ui.set_tools_editor_text("ss://YWVzLTEyOC1nY206cGFzcw@hk.example.com:8388#%E9%A6%99%E6%B8%AF%2001\nvless://0b5d1c2e-7f0a-4c55-9d1e-2c8f7a6b3e11@jp.example.com:443?security=tls&sni=jp.example.com&type=ws&path=%2Fws#%E6%97%A5%E6%9C%AC%2003\ntrojan://secret@sg.example.com:443?sni=sg.example.com#Singapore".into());
            ui.set_tools_editor_parent("custom-proxies".into());
            ui.set_tools_editor_readonly(false);
        }
        if *overlay == "cp-link" {
            ui.set_input_key("cp-link".into());
            ui.set_input_title("快速编辑".into());
            ui.set_input_label("vless://…".into());
            ui.set_input_value("vless://not-a-link".into());
            ui.set_input_label2("".into());
            ui.set_input_removable(false);
            ui.on_input_check(|_, _, _| "无法从链接中读取代理".into());
            ui.set_input_open(true);
        } else {
            ui.on_input_check(|key, value, _| {
                settings_view::check_input(&key, &value)
                    .err()
                    .unwrap_or_default()
                    .into()
            });
        }
        if *overlay == "dashpreview" {
            ui.set_editor_title("我的机场".into());
            ui.set_editor_text("mixed-port: 7890\nmode: rule\nlog-level: error\nproxies:\n  - name: 香港 01\n    type: ss\n".into());
            ui.set_editor_readonly(true);
            ui.set_profiles_page("preview".into());
        }
        ui.set_proxy_style(ProxyStyle {
            kind: if matches!(*overlay, "tab" | "loose" | "tabs" | "groupmenu") {
                0
            } else {
                1
            },
            sort: 0,
            layout: 1,
            card: match *overlay {
                "loose" | "long-loose" => 2,
                "long-compact" => 0,
                _ => 1,
            },
            icon: 0,
            hide_timeout: false,
        });
        ui.set_log_filters(ModelRc::new(VecModel::from(if *overlay == "log-filter" {
            vec!["warning".into(), "error".into()]
        } else {
            vec![]
        })));
        ui.set_log_detail_open(*overlay == "log-detail");
        ui.set_log_searching(*overlay == "log-search");
        if *overlay == "log-empty" {
            saved_logs.get_or_insert_with(|| ui.get_logs());
            ui.set_logs(ModelRc::new(VecModel::<LogItem>::default()));
        } else if let Some(l) = saved_logs.clone() {
            ui.set_logs(l);
        }
        ui.set_proxy_options_open(overlay.starts_with("options"));
        ui.set_proxy_sub(overlay.strip_prefix("options-").unwrap_or("").into());
        ui.set_proxy_searching(*overlay == "search");
        ui.set_add_menu_open(matches!(*overlay, "add-menu" | "add-menu-tv"));
        ui.set_is_tv(*overlay == "add-menu-tv");
        ui.set_qr_open(*overlay == "send-phone");
        if *overlay == "send-phone" {
            let address = "http://192.168.1.20:41234/q7Yc0bW1mF8pLr2xKa9TgA";
            let code = qrcode::QrCode::new(address.as_bytes()).unwrap();
            let n = code.width();
            let colors = code.to_colors();
            let mut p = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(n as u32, n as u32);
            for (i, px) in p.make_mut_slice().iter_mut().enumerate() {
                let v = if colors[i] == qrcode::Color::Dark {
                    0
                } else {
                    255
                };
                *px = slint::Rgba8Pixel::new(v, v, v, 255);
            }
            ui.set_qr_title("从手机发送".into());
            ui.set_qr_image(slint::Image::from_rgba8(p));
            ui.set_qr_kind("send".into());
            ui.set_qr_text(format!("{address}\n\n用手机相机扫描二维码，在打开的页面里粘贴链接。两台设备需要连在同一个网络。").into());
        }
        if matches!(*overlay, "add-menu" | "add-menu-tv" | "menu") {
            // The add button in the bar, or the first card.
            let menu = *overlay == "menu";
            ui.set_ctx_x(if menu { 16.0 } else { w as f32 - 48.0 });
            ui.set_ctx_y(if menu { 60.0 } else { 8.0 });
            ui.set_ctx_w(if menu { w as f32 - 32.0 } else { 40.0 });
            ui.set_ctx_h(if menu { 72.0 } else { 40.0 });
        }
        ui.set_link_open(*overlay == "link");
        ui.set_menu_open(matches!(*overlay, "menu" | "menu-custom"));
        ui.set_menu_more(true);
        ui.set_menu_item(
            base_profiles
                .row_data(if *overlay == "menu-custom" { 3 } else { 0 })
                .unwrap(),
        );
        if *overlay == "menu-custom" {
            ui.set_ctx_x(16.0);
            ui.set_ctx_y(60.0 + 3.0 * 80.0);
            ui.set_ctx_w(w as f32 - 32.0);
            ui.set_ctx_h(72.0);
        }
        ui.set_custom_open(*page == "custom" || *overlay == "custom");
        ui.set_custom_title(
            if *overlay == "long" {
                "一个名字很长很长的自定义分流配置用来检查标题省略"
            } else {
                "自定义分流"
            }
            .into(),
        );
        ui.set_custom_rows(ModelRc::new(VecModel::from(
            [
                ("rules", "rules", "规则", "128", 0),
                ("providers", "resources", "外部资源", "3", 0),
                ("dns", "dns", "DNS", "已开启", 1),
                ("more", "sliders", "更多", "2 项生效", 1),
            ]
            .into_iter()
            .map(|(id, glyph, title, state, tone)| CustomRow {
                id: id.into(),
                glyph: glyph.into(),
                title: title.into(),
                state: state.into(),
                tone: if *overlay == "issues" && id == "rules" {
                    2
                } else {
                    tone
                },
            })
            .collect::<Vec<_>>(),
        )));
        ui.set_custom_issues(if *overlay == "issues" { 3 } else { 0 });
        ui.set_custom_group_count(0);
        ui.set_placeholder_title("规则".into());
        ui.set_custom_menu_open(*overlay == "cmenu");
        if *overlay == "cmenu" {
            ui.set_ctx_x(52.0);
            ui.set_ctx_y(8.0);
            ui.set_ctx_w(140.0);
            ui.set_ctx_h(40.0);
        }
        ui.set_custom_import_open(*overlay == "cimport");
        if *overlay == "cimport" {
            ui.set_ctx_x(w as f32 - 8.0 - 72.0);
            ui.set_ctx_y(10.0);
            ui.set_ctx_w(72.0);
            ui.set_ctx_h(36.0);
        }
        ui.set_custom_importing(*overlay == "cimporting");
        if !matches!(*overlay, "trust-remove" | "script-ask" | "rs-ask") {
            ui.set_confirm_title("提示".into());
            if *overlay == "creplace" {
                ui.set_confirm_text("导入会替换此配置现有的策略组、规则和设置".into());
                ui.set_confirm_action("ci-replace".into());
            } else {
                ui.set_confirm_text("确定删除当前配置吗？".into());
                ui.set_confirm_action("delete".into());
            }
            ui.set_confirm_open(matches!(*overlay, "confirm" | "creplace"));
        }
        // "speed:<up>:<down>": the speed tiles with changing values, rendered in order
        // to catch a value width that stays at an older, shorter text.
        let mut traffic = base_traffic.clone();
        if let Some(v) = overlay.strip_prefix("speed:") {
            let (up, down) = v.split_once(':').unwrap_or((v, v));
            traffic.up_value = up.into();
            traffic.up_unit = "KB/s".into();
            traffic.down_value = down.into();
            traffic.down_unit = "B/s".into();
        }
        ui.set_dash_traffic(traffic);
        // "nodemenu" / "groupmenu": the anchored menus of a node's long press and the tab
        // bar's ▾, over many groups so the tabs overflow.
        if !matches!(*overlay, "ua-menu" | "cp-add") {
            ui.set_ctx_kind("".into());
        }
        snapshot_groups(&ui, overlay, w);
        snapshot_rules(&ui, overlay, w);
        snapshot_more(&ui, overlay);
        snapshot_extend(&ui, overlay);
        // "long*": node names past two lines, as an IPv6 server in the name; "long-scroll"
        // then scrolls into the group, under its pinned header.
        if overlay.starts_with("long") {
            let mut groups: Vec<ProxyGroup> = base_groups.iter().collect();
            let mut nodes: Vec<NodeItem> = groups[0].nodes.iter().collect();
            for (i, n) in nodes.iter_mut().enumerate() {
                n.name = format!("官方 IPv6 | 2606:4700:102::{i} | 443 | MASQUE导航 {i}").into();
                n.kind = "Masque".into();
            }
            let more: Vec<NodeItem> = nodes.clone();
            nodes.extend(more.clone());
            nodes.extend(more);
            groups[0].nodes = ModelRc::new(VecModel::from(nodes));
            groups[0].expanded = true;
            ui.set_proxy_groups(ModelRc::new(VecModel::from(groups)));
        }
        if matches!(*overlay, "groupmenu" | "tabs" | "tabs-list") {
            let mut many: Vec<ProxyGroup> = base_groups.iter().collect();
            for (i, name) in [
                "Telegram",
                "YouTube",
                "Netflix",
                "苹果服务",
                "微软服务",
                "谷歌",
            ]
            .iter()
            .enumerate()
            {
                let from = if *overlay == "tabs-list" && i % 3 == 0 {
                    0
                } else {
                    2
                };
                let mut g = many[from].clone();
                g.name = (*name).into();
                many.push(g);
            }
            ui.set_proxy_group_names(ModelRc::new(VecModel::from(
                many.iter().map(|g| g.name.clone()).collect::<Vec<_>>(),
            )));
            ui.set_proxy_groups(ModelRc::new(VecModel::from(many)));
        }
        if *overlay == "groupmenu" {
            ui.set_ctx_kind("group".into());
            ui.set_ctx_items(ModelRc::default());
            ui.set_ctx_names(ui.get_proxy_group_names());
            ui.set_ctx_x(w as f32 - 44.0);
            ui.set_ctx_y(56.0);
            ui.set_ctx_w(40.0);
            ui.set_ctx_h(40.0);
        }
        if *overlay == "nodemenu" {
            ui.set_ctx_kind("node".into());
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(vec![
                ActionItem {
                    id: "select".into(),
                    glyph: "check-circle".into(),
                    label: "选中".into(),
                    check: true,
                    checked: true,
                    enabled: true,
                    ..Default::default()
                },
                ActionItem {
                    id: "add-local".into(),
                    glyph: "list-add".into(),
                    label: "添加到本地代理".into(),
                    enabled: true,
                    ..Default::default()
                },
            ])));
            ui.set_ctx_x(16.0);
            ui.set_ctx_y(130.0);
            ui.set_ctx_w(w as f32 / 2.0 - 20.0);
            ui.set_ctx_h(76.0);
        }
        ui.global::<Theme>().set_mode(*mode);
        ui.set_theme_mode(*mode);
        slint::platform::update_timers_and_animations();
        // "drag:<y>:<dy>": hold a card at y past the long press, then move the finger by dy.
        let drag = overlay.strip_prefix("drag:").and_then(|v| {
            let (y, dy) = v.split_once(':')?;
            Some((y.parse::<f32>().ok()?, dy.parse::<f32>().ok()?))
        });
        if let Some((y, dy)) = drag {
            use slint::platform::{PointerEventButton, WindowEvent};
            let mut scratch = vec![Rgb8Pixel::default(); (w * h) as usize];
            window.request_redraw();
            window.draw_if_needed(|r| {
                r.render(&mut scratch, w as usize);
            });
            let at = |y: f32| slint::LogicalPosition::new(w as f32 / 2.0, y);
            window.dispatch_event(WindowEvent::PointerPressed {
                position: at(y),
                button: PointerEventButton::Left,
            });
            // Once for the Flickable's delayed press, once for the long press.
            for _ in 0..2 {
                std::thread::sleep(std::time::Duration::from_millis(450));
                slint::platform::update_timers_and_animations();
            }
            for step in 1..=12 {
                window.dispatch_event(WindowEvent::PointerMoved {
                    position: at(y + dy * step as f32 / 12.0),
                });
                slint::platform::update_timers_and_animations();
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
            slint::platform::update_timers_and_animations();
        }
        if *overlay == "long-scroll" {
            use slint::platform::WindowEvent;
            let mut scratch = vec![Rgb8Pixel::default(); (w * h) as usize];
            window.request_redraw();
            window.draw_if_needed(|r| {
                r.render(&mut scratch, w as usize);
            });
            window.dispatch_event(WindowEvent::PointerScrolled {
                position: slint::LogicalPosition::new(w as f32 / 3.0, h as f32 / 2.0),
                delta_x: 0.0,
                delta_y: -360.0,
            });
            slint::platform::update_timers_and_animations();
        }
        let mut buffer = vec![Rgb8Pixel::default(); (w * h) as usize];
        window.request_redraw();
        window.draw_if_needed(|r| {
            r.render(&mut buffer, w as usize);
        });
        let file = std::fs::File::create(format!("{out}/{name}.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let bytes: Vec<u8> = buffer.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        enc.write_header()
            .unwrap()
            .write_image_data(&bytes)
            .unwrap();
        println!("{out}/{name}.png");
        if let Some((y, dy)) = drag {
            window.dispatch_event(slint::platform::WindowEvent::PointerReleased {
                position: slint::LogicalPosition::new(w as f32 / 2.0, y + dy),
                button: slint::platform::PointerEventButton::Left,
            });
            slint::platform::update_timers_and_animations();
        }
    }
}
