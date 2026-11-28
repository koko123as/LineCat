//! The dashboard cards added to match FlClash's (FlClash: lib/views/dashboard): service status, connection
//! stats, the intranet IP card's LAN switch, the IP quality sheet and the card editor.

use crate::dash_table::{self, Feed};
use crate::dashboard;
use crate::drag;
use crate::settings_app::Effect;
use crate::{
    App, AppWindow, DashAdd, DashCell, DashCol, DashProfileDetail, DashRow, DashService, DashStats,
    DashStatus, ProviderRow, ServiceManageRow,
};
use serde_json::{json, Value};
use slint::{ModelRc, VecModel};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// One service's last answer: the Core's status, delay, where it saw the request from and
/// the node it left through; `failed` when the call failed.
#[derive(Clone, Default)]
pub(crate) struct ServiceState {
    pub status: Option<String>,
    pub delay: i64,
    pub region: String,
    pub ip: String,
    pub node: String,
    pub failed: bool,
}

#[derive(Default)]
pub(crate) struct Services {
    pub checks: std::collections::HashMap<String, ServiceState>,
    /// The services a check is out for.
    pub probing: std::collections::HashSet<String>,
}

use dashboard::service_shown as shown_count;

/// `ConnectionsTableCard`'s state: its feed, the header sort, the table's width, whether
/// the dashboard shows it, and a column drag's starting widths and overrides.
pub(crate) struct DashTable {
    pub feed: Feed,
    pub sort: Option<(String, bool)>,
    pub width: f32,
    pub shown: bool,
    pub resize: Option<(usize, Vec<f32>, BTreeMap<String, f32>)>,
    /// The widths while a drag moves, saved when it ends.
    pub live: Option<BTreeMap<String, f32>>,
}

impl Default for DashTable {
    fn default() -> Self {
        DashTable {
            feed: Feed::Connections,
            sort: None,
            width: 0.0,
            shown: false,
            resize: None,
            live: None,
        }
    }
}

impl App {
    pub(crate) fn bind_dashboard(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_provider_act(move |action, name| a.provider_act(&action, &name));
        let a = self.clone();
        ui.on_provider_editor_back(move |text| a.provider_editor_back(text.into()));
        let a = self.clone();
        ui.on_dash_move(move |from, to| a.move_dashboard(from as usize, to as usize));
        let a = self.clone();
        ui.on_dash_table_select(move |feed| {
            {
                let mut t = a.dash_table.lock().unwrap();
                t.feed = Feed::from_index(feed);
                t.sort = None;
            }
            a.publish_table();
        });
        let a = self.clone();
        ui.on_dash_table_sort(move |id| {
            {
                let mut t = a.dash_table.lock().unwrap();
                let cols = dash_table::columns(t.feed, t.width);
                if let Some(col) = cols.iter().find(|c| c.id == id.as_str()) {
                    t.sort = dash_table::next_sort(t.sort.take(), col);
                }
            }
            a.publish_table();
        });
        let a = self.clone();
        ui.on_dash_table_width(move |w| {
            a.dash_table.lock().unwrap().width = w;
            a.publish_table();
        });
        let a = self.clone();
        ui.on_dash_table_resize_start(move |boundary| {
            let base = a.table_overrides();
            let mut t = a.dash_table.lock().unwrap();
            let cols = dash_table::columns(t.feed, t.width);
            let widths = dash_table::widths(&cols, t.width, &base);
            t.resize = Some((boundary.max(0) as usize, widths, base));
        });
        let a = self.clone();
        ui.on_dash_table_resize(move |delta| {
            {
                let mut t = a.dash_table.lock().unwrap();
                let cols = dash_table::columns(t.feed, t.width);
                let Some((b, widths, base)) = &t.resize else {
                    return;
                };
                t.live = Some(dash_table::resized(&cols, widths, base, *b, delta));
            }
            a.publish_table();
        });
        let a = self.clone();
        ui.on_dash_table_resize_end(move || {
            let (feed, live) = {
                let mut t = a.dash_table.lock().unwrap();
                t.resize = None;
                (t.feed, t.live.take())
            };
            if let Some(live) = live {
                a.save_table_widths(feed, live);
            }
        });
        let a = self.clone();
        ui.on_dash_table_reset(move || {
            let feed = {
                let mut t = a.dash_table.lock().unwrap();
                t.resize = None;
                t.live = None;
                t.feed
            };
            a.save_table_widths(feed, BTreeMap::new());
            a.publish_table();
        });
        let a = self.clone();
        ui.on_dash_table_shown(move |shown| {
            a.dash_table.lock().unwrap().shown = shown;
            a.publish_table();
        });
        let a = self.clone();
        ui.on_open_service_ip(move |ip, region| a.open_service_ip(ip.into(), region.into()));
        let a = self.clone();
        ui.on_dash_card_height(move |i, h| {
            let Ok(i) = usize::try_from(i) else { return };
            let mut heights = a.dash_heights.lock().unwrap();
            if heights.len() <= i {
                heights.resize(i + 1, 0.0);
            }
            heights[i] = h;
        });
        let a = self.clone();
        ui.on_dash_drag_target(move |from, shift, gap, count| {
            let heights = a.dash_column(count);
            drag::target(&heights, gap, from.max(0) as usize, shift) as i32
        });
        let a = self.clone();
        ui.on_dash_drag_offset(move |from, to, i, gap| {
            let heights = a.dash_heights.lock().unwrap();
            drag::offset(
                &heights,
                gap,
                from.max(0) as usize,
                to.max(0) as usize,
                i.max(0) as usize,
            )
        });
        let a = self.clone();
        ui.on_dash_probe_services(move || a.probe_services());
        let a = self.clone();
        ui.on_dash_check_service(move |id| {
            let names: Vec<String> = if id.is_empty() {
                a.enabled_services()
                    .iter()
                    .map(|(i, _)| i.to_string())
                    .collect()
            } else {
                vec![id.to_string()]
            };
            a.check_services(names);
        });
        let a = self.clone();
        ui.on_dash_service_move(move |from, to| {
            let order: Vec<String> = a
                .ordered_services()
                .iter()
                .map(|(i, _)| i.to_string())
                .collect();
            a.update_settings(Effect::None, move |s| {
                let mut order = order;
                crate::reorder::move_item(&mut order, from as usize, to as usize);
                s.service_order = order;
            });
            a.publish_services();
            a.probe_services();
        });
        let a = self.clone();
        ui.on_dash_service_toggle(move |id, on| {
            let id = id.to_string();
            a.update_settings(Effect::None, move |s| {
                s.disabled_services.retain(|d| *d != id);
                if !on {
                    s.disabled_services.push(id);
                }
            });
            a.publish_services();
            a.probe_services();
        });
        let a = self.clone();
        ui.on_dash_set_lan(move |on| {
            a.update_general(Effect::Reapply, |g| g.allow_lan = on);
            a.publish_dash_flags();
        });
        let a = self.clone();
        ui.on_dash_set_hide_ip(move |on| {
            a.update_settings(Effect::None, |s| s.hide_ip = on);
            a.publish_dash_flags();
        });
        let a = self.clone();
        ui.on_release_memory(move || a.release_memory());
        let a = self.clone();
        ui.on_open_memory(move || a.publish_memory());
        let a = self.clone();
        ui.on_open_profile_detail(move || {
            let custom = a
                .store
                .lock()
                .unwrap()
                .active()
                .filter(|p| p.is_custom())
                .map(|p| p.id.clone());
            match custom {
                Some(id) => a.open_custom(id, false),
                None => {
                    a.ui(|ui| ui.set_profile_sheet_open(true));
                    a.load_profile_detail();
                }
            }
        });
        self.publish_dash_flags();
        self.publish_services();
    }

    /// `watchEngineStatus`: the services that did not start, for the bar's badge.
    pub(crate) fn watch_engine(
        self: &Arc<Self>,
        bridge: &crate::bridge::Bridge,
    ) -> Result<(), String> {
        let weak = Arc::downgrade(self);
        bridge.watch(
            "engine.state",
            json!({}),
            Box::new(move |status| {
                let Some(app) = weak.upgrade() else { return };
                let faults = dashboard::service_faults(&status);
                let count = faults.len() as i32;
                let text = faults.join("\n\n");
                app.ui(move |ui| {
                    ui.set_dash_fault_count(count);
                    ui.set_dash_faults_text(text.into());
                });
            }),
        )
    }

    /// `MemorySnapshot.resolve` on Android: the Core shares the process, so the process
    /// RSS is the app and shared total.
    pub(crate) fn publish_memory(self: &Arc<Self>) {
        let rss = dashboard::resident_memory().unwrap_or(0);
        let text = if rss > 0 {
            crate::size(rss)
        } else {
            String::new()
        };
        self.ui(move |ui| {
            ui.set_dash_mem(crate::DashMemory {
                total: text.clone().into(),
                app: text.into(),
                shared: true,
            });
        });
    }

    /// `_handleRelease`: `core.releaseMemory`, then the banner with what was freed.
    fn release_memory(self: &Arc<Self>) {
        let app = self.clone();
        self.ui(|ui| ui.set_mem_releasing(true));
        std::thread::spawn(move || {
            let before = dashboard::resident_memory().unwrap_or(0);
            let result = app
                .bridge()
                .and_then(|b| b.call("core.releaseMemory", json!({}), Duration::from_secs(10)));
            std::thread::sleep(Duration::from_millis(500));
            let after = dashboard::resident_memory().unwrap_or(0);
            app.publish_memory();
            app.ui(|ui| ui.set_mem_releasing(false));
            match result {
                Ok(_) => app.toast(dashboard::released_text(before, after)),
                Err(_) => app.toast("释放内存失败"),
            }
        });
    }

    /// `ProfileDetailSheet`: the current profile's counts from `profile.inspect` and its
    /// providers from `provider.list`.
    fn load_profile_detail(self: &Arc<Self>) {
        let (path, updated) = {
            let store = self.store.lock().unwrap();
            match store.active() {
                Some(p) => (
                    Some(store.profile_path(&p.id)),
                    crate::dash_profile(p, false)
                        .updated
                        .trim_start_matches("更新于 ")
                        .to_string(),
                ),
                None => (None, String::new()),
            }
        };
        let blank = DashProfileDetail {
            updated: updated.clone().into(),
            script: "无".into(),
            ..Default::default()
        };
        self.ui(move |ui| {
            ui.set_profile_detail(blank);
            ui.set_profile_providers(ModelRc::new(VecModel::<ProviderRow>::default()));
        });
        let app = self.clone();
        std::thread::spawn(move || {
            let read = || -> Result<(Value, Vec<Value>), String> {
                let bridge = app.bridge()?;
                let path = path.ok_or_else(|| "no profile".to_string())?;
                let inspect = bridge.call(
                    "profile.inspect",
                    json!({"profile": crate::profile_ref(&path)}),
                    Duration::from_secs(20),
                )?;
                let providers = bridge
                    .call("provider.list", json!({}), Duration::from_secs(10))
                    .ok()
                    .and_then(|r| r.get("providers").and_then(Value::as_array).cloned())
                    .unwrap_or_default();
                Ok((inspect, providers))
            };
            let (counts, providers) = match read() {
                Ok((inspect, providers)) => (
                    dashboard::profile_stats(&inspect, &providers).map(|n| n.to_string()),
                    providers,
                ),
                Err(_) => (std::array::from_fn(|_| "-".to_string()), Vec::new()),
            };
            app.set_ext_providers(&providers);
            app.ui(move |ui| {
                let [groups, proxies, rules, providers] = counts;
                ui.set_profile_detail(DashProfileDetail {
                    groups: groups.into(),
                    proxies: proxies.into(),
                    rules: rules.into(),
                    providers: providers.into(),
                    updated: updated.into(),
                    script: "无".into(),
                    remote: false,
                });
            });
        });
    }

    /// `showIpQualitySheet` for a service's address: its region at once, then the lookup.
    fn open_service_ip(self: &Arc<Self>, ip: String, region: String) {
        let shown = ip.clone();
        self.ui(move |ui| {
            ui.set_ipq_ip(shown.clone().into());
            ui.set_ipq_other(DashStatus {
                ip: shown.into(),
                country: region.into(),
                probing: true,
                ..Default::default()
            });
            ui.set_ip_sheet_open(true);
        });
        self.spawn(move |app| {
            let quality = dashboard::lookup_quality(&ip);
            app.ui(move |ui| {
                if ui.get_ipq_ip().as_str() != ip {
                    return;
                }
                let mut s = ui.get_ipq_other();
                let (org, kind, level) = quality
                    .map(|q| (q.org, q.kind, q.level))
                    .unwrap_or_else(|| (String::new(), "类型未知".into(), 0));
                s.org = org.into();
                s.kind = kind.into();
                s.quality = level;
                s.probing = false;
                ui.set_ipq_other(s);
            });
        });
    }

    /// The editor column's card heights, as many as it shows.
    fn dash_column(&self, count: i32) -> Vec<f32> {
        let mut heights = self.dash_heights.lock().unwrap().clone();
        heights.resize(count.max(0) as usize, 0.0);
        heights
    }

    /// `_reorder` in the card editor.
    fn move_dashboard(self: &Arc<Self>, from: usize, to: usize) {
        {
            let mut store = self.store.lock().unwrap();
            let mut list = dashboard::normalize(
                &store
                    .state
                    .settings
                    .dashboard
                    .clone()
                    .unwrap_or_else(dashboard::default_widgets),
            );
            dashboard::reorder(&mut list, from, to);
            store.state.settings.dashboard = Some(list);
            store.save();
        }
        self.publish_dashboard_widgets();
    }

    /// The cards the editor can add, each with its label and description.
    pub(crate) fn dash_addable(list: &[String]) -> Vec<DashAdd> {
        dashboard::WIDGETS
            .iter()
            .filter(|w| !list.iter().any(|l| l == *w))
            .map(|w| DashAdd {
                id: (*w).into(),
                label: dashboard::widget_label(w).into(),
                desc: dashboard::widget_description(w).into(),
            })
            .collect()
    }

    pub(crate) fn publish_dash_flags(self: &Arc<Self>) {
        let (lan, hide, port) = {
            let s = &self.store.lock().unwrap().state.settings;
            (s.general.allow_lan, s.hide_ip, s.general.mixed_port)
        };
        self.ui(move |ui| {
            ui.set_dash_allow_lan(lan);
            ui.set_dash_port(port.to_string().into());
            ui.set_dash_hide_ip(hide);
        });
    }

    fn ordered_services(&self) -> Vec<(&'static str, &'static str)> {
        let st = self.store.lock().unwrap();
        dashboard::ordered_services(&st.state.settings.service_order)
    }

    fn enabled_services(&self) -> Vec<(&'static str, &'static str)> {
        let st = self.store.lock().unwrap();
        let s = &st.state.settings;
        dashboard::enabled_services(&s.service_order, &s.disabled_services)
    }

    fn shown_services(&self) -> Vec<(&'static str, &'static str)> {
        let (wide, columns, _) = *self.dash_layout.lock().unwrap();
        let mut v = self.enabled_services();
        v.truncate(shown_count(wide, columns));
        v
    }

    /// `ServiceStatus.probe`: the services the card shows, checked through the rules while
    /// running, directly otherwise.
    pub(crate) fn probe_services(self: &Arc<Self>) {
        if !self.dash_layout.lock().unwrap().2 {
            return;
        }
        let names = self
            .shown_services()
            .iter()
            .map(|(i, _)| i.to_string())
            .collect();
        self.check_services(names);
    }

    /// Checks the named services, leaving out any a check is already out for.
    pub(crate) fn check_services(self: &Arc<Self>, names: Vec<String>) {
        let names: Vec<String> = {
            let mut s = self.services.lock().unwrap();
            let fresh: Vec<String> = names
                .into_iter()
                .filter(|n| !s.probing.contains(n))
                .collect();
            s.probing.extend(fresh.iter().cloned());
            fresh
        };
        if names.is_empty() {
            return;
        }
        self.publish_services();
        let proxy = if self.run_state.lock().unwrap().as_str() == "STARTED"
            && !crate::on_demand_app::parked()
        {
            ""
        } else {
            "DIRECT"
        };
        self.spawn(move |app| {
            let result = app.bridge().and_then(|b| {
                b.call(
                    "probe.services",
                    json!({"proxy": proxy, "names": names, "timeout": 10_000}),
                    Duration::from_secs(20),
                )
            });
            {
                let mut s = app.services.lock().unwrap();
                for n in &names {
                    s.probing.remove(n);
                }
                match result {
                    Ok(r) => {
                        for item in r
                            .get("items")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                        {
                            let text = |k: &str| {
                                item.get(k)
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                    .to_string()
                            };
                            let node = item
                                .get("chains")
                                .and_then(Value::as_array)
                                .and_then(|c| c.first())
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            s.checks.insert(
                                text("name"),
                                ServiceState {
                                    status: item
                                        .get("status")
                                        .and_then(Value::as_str)
                                        .map(Into::into),
                                    delay: item.get("delay").and_then(Value::as_i64).unwrap_or(0),
                                    region: text("region"),
                                    ip: text("ip"),
                                    node,
                                    failed: false,
                                },
                            );
                        }
                    }
                    Err(_) => {
                        for n in &names {
                            let e = s.checks.entry(n.clone()).or_default();
                            e.failed = true;
                            e.status = None;
                        }
                    }
                }
            }
            app.publish_services();
        });
    }

    pub(crate) fn publish_services(self: &Arc<Self>) {
        let (wide, columns, _) = *self.dash_layout.lock().unwrap();
        let shown = shown_count(wide, columns);
        let ordered = self.ordered_services();
        let enabled = self.enabled_services();
        let (cells, rows, verdict, checking) = {
            let s = self.services.lock().unwrap();
            let cell = |id: &str, label: &str| {
                let c = s.checks.get(id).cloned().unwrap_or_default();
                let probing = s.probing.contains(id);
                let (text, tone) =
                    dashboard::service_status(c.status.as_deref(), c.delay, probing, c.failed);
                DashService {
                    id: id.into(),
                    label: label.into(),
                    status: text.into(),
                    tone,
                    dot: dashboard::service_dot(c.status.as_deref(), c.failed),
                    place: dashboard::service_where(&c.region, &c.ip, &c.node).into(),
                    loading: probing,
                    ip: c.ip.clone().into(),
                    region: c.region.clone().into(),
                }
            };
            let cells: Vec<DashService> = enabled
                .iter()
                .take(shown)
                .map(|(i, l)| cell(i, l))
                .collect();
            let answers: Vec<(Option<String>, bool)> = enabled
                .iter()
                .take(shown)
                .map(|(i, _)| {
                    let c = s.checks.get(*i).cloned().unwrap_or_default();
                    (c.status, c.failed)
                })
                .collect();
            let rows: Vec<DashService> = enabled
                .iter()
                .map(|(i, l)| {
                    let mut r = cell(i, l);
                    if r.loading {
                        r.status = "加载中…".into();
                        r.tone = 0;
                    }
                    r
                })
                .collect();
            let checking = enabled.iter().any(|(i, _)| s.probing.contains(*i));
            (cells, rows, dashboard::service_verdict(&answers), checking)
        };
        let manage: Vec<ServiceManageRow> = ordered
            .iter()
            .map(|(id, label)| {
                let pos = enabled.iter().position(|(e, _)| e == id);
                ServiceManageRow {
                    id: (*id).into(),
                    label: (*label).into(),
                    enabled: pos.is_some(),
                    shown: pos.is_some_and(|p| p < shown),
                    locked: pos.is_some() && enabled.len() == 1,
                }
            })
            .collect();
        let problems = verdict.ends_with("项异常");
        self.ui(move |ui| {
            ui.set_dash_services(ModelRc::new(VecModel::from(cells)));
            ui.set_dash_service_rows(ModelRc::new(VecModel::from(rows)));
            ui.set_dash_service_manage(ModelRc::new(VecModel::from(manage)));
            ui.set_dash_services_checking(checking);
            ui.set_dash_service_verdict(verdict.into());
            ui.set_dash_service_warn(problems);
        });
    }

    /// The current feed's saved column widths.
    fn table_overrides(self: &Arc<Self>) -> BTreeMap<String, f32> {
        let feed = self.dash_table.lock().unwrap().feed;
        let store = self.store.lock().unwrap();
        let saved = &store.state.settings.table_column_widths;
        saved
            .get(dash_table::table_id(feed))
            .cloned()
            .unwrap_or_default()
    }

    /// `saveColumnWidths`: kept by table id, an empty set removed.
    fn save_table_widths(self: &Arc<Self>, feed: Feed, widths: BTreeMap<String, f32>) {
        self.update_settings(Effect::None, move |s| {
            let id = dash_table::table_id(feed).to_string();
            if widths.is_empty() {
                s.table_column_widths.remove(&id);
            } else {
                s.table_column_widths.insert(id, widths);
            }
        });
    }

    /// The table's columns and rows, while the dashboard shows it.
    pub(crate) fn publish_table(self: &Arc<Self>) {
        let (feed, sort, width, live) = {
            let t = self.dash_table.lock().unwrap();
            if !t.shown {
                return;
            }
            (t.feed, t.sort.clone(), t.width, t.live.clone())
        };
        let overrides = live.unwrap_or_else(|| self.table_overrides());
        let now = crate::requests_app::now_ms();
        let offset = crate::requests_app::utc_offset(now);
        let cols = dash_table::columns(feed, width);
        let widths = dash_table::widths(&cols, width, &overrides);
        let (rows, counts, tcp, udp) = {
            let req = self.req.lock().unwrap();
            let rows = dash_table::rows(
                feed,
                &req.conns,
                &req.dns,
                &cols,
                sort.as_ref(),
                now,
                offset,
            );
            let active: Vec<_> = req.conns.conns.values().filter(|c| c.is_active()).collect();
            let tcp = active
                .iter()
                .filter(|c| c.network.eq_ignore_ascii_case("tcp"))
                .count();
            let counts =
                [active.len(), req.conns.conns.len(), req.dns.queries.len()].map(dashboard::count);
            (rows, counts, tcp, active.len() - tcp)
        };
        let mut x = 0.0;
        let heads: Vec<DashCol> = cols
            .iter()
            .zip(widths)
            .map(|(c, w)| {
                let sort = match &sort {
                    Some((id, desc)) if id == c.id => 1 + *desc as i32,
                    _ => 0,
                };
                let col = DashCol {
                    id: c.id.into(),
                    label: c.label.into(),
                    x,
                    w,
                    end: c.end,
                    sort,
                };
                x += w;
                col
            })
            .collect();
        self.ui(move |ui| {
            if ui.get_tab() != 0 {
                return;
            }
            let rows: Vec<DashRow> = rows
                .into_iter()
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
                .collect();
            ui.set_dash_table_feed(match feed {
                Feed::Connections => 0,
                Feed::Requests => 1,
                Feed::Dns => 2,
            });
            ui.set_dash_table_counts(ModelRc::new(VecModel::from(
                counts.map(slint::SharedString::from).to_vec(),
            )));
            ui.set_dash_table_tcp(dashboard::count(tcp).into());
            ui.set_dash_table_udp(dashboard::count(udp).into());
            ui.set_dash_table_cols(ModelRc::new(VecModel::from(heads)));
            ui.set_dash_table_rows(ModelRc::new(VecModel::from(rows)));
        });
    }

    /// `ConnectionStatsCard`'s three counts.
    pub(crate) fn publish_stats(self: &Arc<Self>) {
        let stats = {
            let req = self.req.lock().unwrap();
            DashStats {
                connections: dashboard::count(req.conns.active_count()).into(),
                requests: dashboard::count(req.conns.conns.len()).into(),
                dns: dashboard::count(req.dns.queries.len()).into(),
            }
        };
        self.ui(move |ui| ui.set_dash_stats(stats));
    }

    /// The window reports how the dashboard is laid out (wide, the service card's columns,
    /// whether it shows), so the card checks as many services as it shows.
    pub(crate) fn dash_layout_changed(self: &Arc<Self>, wide: bool, columns: i32, shown: bool) {
        let changed = {
            let mut l = self.dash_layout.lock().unwrap();
            let changed = *l != (wide, columns, shown);
            *l = (wide, columns, shown);
            changed
        };
        if changed && shown && self.configured.load(Ordering::SeqCst) {
            self.publish_services();
            self.probe_services();
        }
    }
}
