//! The dashboard cards added to match lib/views/dashboard: service status, connection
//! stats, the intranet IP card's LAN switch, the IP quality sheet and the card editor.

use crate::dashboard;
use crate::settings_app::Effect;
use crate::{App, AppWindow, DashAdd, DashService, DashStats};
use serde_json::{json, Value};
use slint::{ModelRc, VecModel};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// One service's last answer: the Core's status and delay; `failed` when the call failed.
#[derive(Clone, Default)]
pub(crate) struct ServiceState {
    pub status: Option<String>,
    pub delay: i64,
    pub failed: bool,
}

#[derive(Default)]
pub(crate) struct Services {
    pub checks: std::collections::HashMap<String, ServiceState>,
    pub probing: bool,
}

use dashboard::service_shown as shown_count;

impl App {
    pub(crate) fn bind_dashboard(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_dash_move(move |from, to| a.move_dashboard(from as usize, to as usize));
        let a = self.clone();
        ui.on_dash_probe_services(move || a.probe_services());
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
        self.publish_dash_flags();
        self.publish_services();
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

    /// `ServiceStatus.probe`: the shown services checked through the rules while running,
    /// directly otherwise.
    pub(crate) fn probe_services(self: &Arc<Self>) {
        let (wide, columns, shown) = {
            let ui_state = self.dash_layout.lock().unwrap();
            (ui_state.0, ui_state.1, ui_state.2)
        };
        if !shown {
            return;
        }
        let names: Vec<&str> = dashboard::SERVICES
            .iter()
            .take(shown_count(wide, columns))
            .map(|(id, _)| *id)
            .collect();
        {
            let mut s = self.services.lock().unwrap();
            if s.probing {
                return;
            }
            s.probing = true;
        }
        self.publish_services();
        let proxy = if self.run_state.lock().unwrap().as_str() == "STARTED" {
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
                s.probing = false;
                match result {
                    Ok(r) => {
                        for item in r
                            .get("items")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                        {
                            let name = item.get("name").and_then(Value::as_str).unwrap_or("");
                            s.checks.insert(
                                name.into(),
                                ServiceState {
                                    status: item
                                        .get("status")
                                        .and_then(Value::as_str)
                                        .map(Into::into),
                                    delay: item.get("delay").and_then(Value::as_i64).unwrap_or(0),
                                    failed: false,
                                },
                            );
                        }
                    }
                    Err(_) => {
                        for n in &names {
                            let e = s.checks.entry((*n).into()).or_default();
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
        let (cells, verdict) = {
            let s = self.services.lock().unwrap();
            let mut answers = Vec::new();
            let cells: Vec<DashService> = dashboard::SERVICES
                .iter()
                .take(shown_count(wide, columns))
                .map(|(id, label)| {
                    let c = s.checks.get(*id).cloned().unwrap_or_default();
                    answers.push((c.status.clone(), c.failed));
                    let (text, tone) = dashboard::service_status(
                        c.status.as_deref(),
                        c.delay,
                        s.probing,
                        c.failed,
                    );
                    DashService {
                        label: (*label).into(),
                        status: text.into(),
                        tone,
                        dot: dashboard::service_dot(c.status.as_deref(), c.failed),
                    }
                })
                .collect();
            (cells, dashboard::service_verdict(&answers))
        };
        let problems = verdict.ends_with("项异常");
        self.ui(move |ui| {
            ui.set_dash_services(ModelRc::new(VecModel::from(cells)));
            ui.set_dash_service_verdict(verdict.into());
            ui.set_dash_service_warn(problems);
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
