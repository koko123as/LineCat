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
    apply_theme(ui, &s);
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
        "developer" => settings_view::developer(&s),
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
    let meta = seasoncat_ui::page_meta(&s, page);
    ui.on_page_meta(move |_, _| meta.clone());
    ui.set_page_sections(m(sections));
    ui.set_choice_open(overlay.starts_with("choice:"));
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
            }
        })
        .collect()
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
        ("set-scripts-small", 3, "", "scripts", 1, ""),
        ("set-scripts-empty", 3, "", "scripts", 1, "empty"),
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
    let only = std::env::args().nth(2).unwrap_or_default();
    ui.show().unwrap();
    let mut saved_logs: Option<ModelRc<LogItem>> = None;
    let base_traffic = ui.get_dash_traffic();
    let base_groups = ui.get_proxy_groups();
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
        ui.set_ip_sheet_open(*overlay == "ipsheet");
        ui.set_mem_sheet_open(*overlay == "memsheet");
        ui.set_svc_sheet_open(overlay.starts_with("svcsheet"));
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
        ui.set_picker_open(*overlay == "picker");
        ui.set_dash_fault_count(if *overlay == "faults" { 2 } else { 0 });
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
        ui.set_proxy_style(ProxyStyle {
            kind: if matches!(*overlay, "tab" | "loose" | "tabs" | "groupmenu") {
                0
            } else {
                1
            },
            sort: 0,
            layout: 1,
            card: if *overlay == "loose" { 2 } else { 1 },
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
        ui.set_add_menu_open(*overlay == "add-menu");
        if matches!(*overlay, "add-menu" | "menu") {
            // The add button in the bar, or the first card.
            let menu = *overlay == "menu";
            ui.set_ctx_x(if menu { 16.0 } else { w as f32 - 48.0 });
            ui.set_ctx_y(if menu { 60.0 } else { 8.0 });
            ui.set_ctx_w(if menu { w as f32 - 32.0 } else { 40.0 });
            ui.set_ctx_h(if menu { 72.0 } else { 40.0 });
        }
        ui.set_link_open(*overlay == "link");
        ui.set_menu_open(*overlay == "menu");
        ui.set_menu_more(true);
        ui.set_menu_item(ui.get_profiles().row_data(0).unwrap());
        ui.set_confirm_title("提示".into());
        ui.set_confirm_text("确定删除当前配置吗？".into());
        ui.set_confirm_action("delete".into());
        ui.set_confirm_open(*overlay == "confirm");
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
        if *overlay != "ua-menu" {
            ui.set_ctx_kind("".into());
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
            ui.set_ctx_items(ModelRc::new(VecModel::from(vec![ActionItem {
                id: "select".into(),
                glyph: "check-circle".into(),
                label: "选中".into(),
                check: true,
                checked: true,
                enabled: true,
                ..Default::default()
            }])));
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
