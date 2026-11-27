//! The on-demand page and the suspend on `App`'s side: the shell's SSID, location
//! permission and battery exemption (UiBridge's `wifi.*` and `battery.*`), the excluded
//! SSIDs, and a run parked while the Wi-Fi in use is one of them (`syncSuspend`).

use crate::on_demand::{self, Permission};
use crate::{App, PageMeta, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A permission prompt or a settings page waits on the user.
const ASK: Duration = Duration::from_secs(600);

#[derive(Default)]
struct State {
    on_wifi: bool,
    ssid: Option<String>,
    ssid_seq: u64,
    permission: Permission,
    auto_requested: bool,
    requesting: bool,
    /// None until the shell answers, and while a request is open.
    battery: Option<bool>,
    /// The run the service reports, before a park is laid over it.
    service: String,
    /// Started by the user, its service stopped while on an excluded Wi-Fi.
    parked: bool,
    resuming: bool,
    selected: BTreeSet<String>,
}

fn state() -> &'static Mutex<State> {
    static S: std::sync::OnceLock<Mutex<State>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(State {
            service: "OFFLINE".into(),
            ..Default::default()
        })
    })
}

pub fn page_meta(page: &str) -> Option<PageMeta> {
    (page == "on-demand").then(|| PageMeta {
        known: true,
        title: "按需运行".into(),
        parent: "general".into(),
        ..Default::default()
    })
}

/// Whether traffic leaves through the Core: started and not parked (`findProxy`'s DIRECT
/// while suspended).
pub fn parked() -> bool {
    state().lock().unwrap().parked
}

impl App {
    fn excluded(&self) -> Vec<String> {
        self.store
            .lock()
            .unwrap()
            .state
            .settings
            .exclude_ssids
            .clone()
    }

    /// The run state to show for what the service reports.
    pub(crate) fn shown_run_state(&self, service: &str) -> String {
        let mut st = state().lock().unwrap();
        st.service = service.to_string();
        on_demand::shown_state(service, st.parked).to_string()
    }

    pub(crate) fn on_demand_connected(self: &Arc<Self>, hello: &Value) {
        state().lock().unwrap().on_wifi = hello.get("wifi").and_then(Value::as_bool) == Some(true);
        self.refresh_location();
        self.update_ssid();
    }

    pub(crate) fn on_network(self: &Arc<Self>, wifi: bool) {
        state().lock().unwrap().on_wifi = wifi;
        self.update_ssid();
    }

    /// `didChangeAppLifecycleState(resumed)`, and the battery stream's next answer.
    pub(crate) fn on_resume(self: &Arc<Self>) {
        self.refresh_location();
        self.refresh_battery();
    }

    /// `LocationPermissions.refresh`, asking once by itself when a network is excluded.
    fn refresh_location(self: &Arc<Self>) {
        self.spawn(|app| {
            let Ok(bridge) = app.bridge() else { return };
            let Ok(answer) = bridge.shell("wifi.permission", json!({}), CALL) else {
                return;
            };
            let checked = Permission::decode(answer["permission"].as_i64().unwrap_or(1));
            let excluded = app.excluded();
            let ask = {
                let mut st = state().lock().unwrap();
                st.permission = st.permission.after_check(checked);
                if excluded.is_empty() {
                    st.auto_requested = false;
                }
                let ask =
                    !excluded.is_empty() && checked == Permission::Denied && !st.auto_requested;
                if ask {
                    st.auto_requested = true;
                }
                ask
            };
            app.publish_on_demand();
            app.update_ssid();
            if ask {
                app.request_location(false);
            }
        });
    }

    /// `LocationPermissions.request`, then `_handleRequestLocationPermission`'s follow-up
    /// when the page's button asked.
    fn request_location(self: &Arc<Self>, from_page: bool) {
        {
            let mut st = state().lock().unwrap();
            if st.requesting {
                return;
            }
            st.requesting = true;
        }
        self.spawn(move |app| {
            let answer = app
                .bridge()
                .and_then(|b| b.shell("wifi.request", json!({}), ASK));
            state().lock().unwrap().requesting = false;
            let Ok(answer) = answer else { return };
            let p = Permission::decode(answer["permission"].as_i64().unwrap_or(1));
            state().lock().unwrap().permission = p;
            app.publish_on_demand();
            app.update_ssid();
            if from_page {
                match p {
                    Permission::Granted => {}
                    Permission::PermanentlyDenied => app.open_app_settings(),
                    Permission::Denied => app.confirm(
                        "需要位置权限",
                        "位置权限已被拒绝，无法获取当前 Wi-Fi 名称。请前往系统设置手动开启位置权限。"
                            .into(),
                        "location-settings",
                        String::new(),
                    ),
                }
            }
        });
    }

    fn refresh_battery(self: &Arc<Self>) {
        self.spawn(|app| {
            let Ok(answer) = app
                .bridge()
                .and_then(|b| b.shell("battery.ignored", json!({}), CALL))
            else {
                return;
            };
            state().lock().unwrap().battery = answer["ignored"].as_bool();
            app.publish_on_demand();
        });
    }

    fn request_battery(self: &Arc<Self>) {
        {
            let mut st = state().lock().unwrap();
            if st.battery != Some(false) {
                return;
            }
            st.battery = None;
        }
        self.publish_on_demand();
        self.spawn(|app| {
            let answer = app
                .bridge()
                .and_then(|b| b.shell("battery.request", json!({}), ASK));
            state().lock().unwrap().battery =
                Some(answer.ok().and_then(|a| a["ignored"].as_bool()) == Some(true));
            app.publish_on_demand();
        });
    }

    /// `ConnectivityManager._updateSsid`: the newest read wins.
    pub(crate) fn update_ssid(self: &Arc<Self>) {
        let excluded = self.excluded();
        let (seq, reads) = {
            let mut st = state().lock().unwrap();
            st.ssid_seq += 1;
            (st.ssid_seq, on_demand::reads_ssid(st.on_wifi, &excluded))
        };
        if !reads {
            self.publish_ssid(seq, None);
            return;
        }
        self.spawn(move |app| {
            let ssid = app
                .bridge()
                .and_then(|b| b.shell("wifi.ssid", json!({}), CALL))
                .ok()
                .and_then(|a| a["ssid"].as_str().map(str::to_string));
            app.publish_ssid(seq, ssid);
        });
    }

    fn publish_ssid(self: &Arc<Self>, seq: u64, ssid: Option<String>) {
        {
            let mut st = state().lock().unwrap();
            if st.ssid_seq != seq {
                return;
            }
            st.ssid = ssid;
        }
        self.sync_suspend();
    }

    /// `syncSuspend`: an excluded Wi-Fi parks a run without ending it, and leaving it
    /// starts the service again.
    pub(crate) fn sync_suspend(self: &Arc<Self>) {
        let excluded = self.excluded();
        let (suspend, park, resume) = {
            let mut st = state().lock().unwrap();
            let suspend = on_demand::suspended(st.ssid.as_deref(), &excluded);
            let park = suspend && !st.parked && st.service == "STARTED";
            let resume = !suspend && st.parked && !st.resuming;
            if park {
                st.parked = true;
            }
            if resume {
                st.resuming = true;
            }
            (suspend, park, resume)
        };
        self.ui(move |ui| ui.global::<crate::Suspend>().set_suspended(suspend));
        if park {
            self.spawn(|app| {
                if let Err(e) = app
                    .bridge()
                    .and_then(|b| b.shell("vpn.stop", json!({}), CALL))
                {
                    state().lock().unwrap().parked = false;
                    app.toast(e);
                }
            });
        }
        if resume {
            self.spawn(|app| {
                let result = app.start_vpn();
                let service = {
                    let mut st = state().lock().unwrap();
                    st.resuming = false;
                    st.parked = false;
                    st.service.clone()
                };
                app.set_run_state(&service);
                if let Err(e) = result {
                    app.toast(e);
                }
            });
        }
    }

    /// A start on an excluded Wi-Fi parks at once instead of starting the service.
    pub(crate) fn park_instead_of_start(self: &Arc<Self>) -> bool {
        let excluded = self.excluded();
        let park = {
            let mut st = state().lock().unwrap();
            let park = on_demand::suspended(st.ssid.as_deref(), &excluded);
            if park {
                st.parked = true;
            }
            park
        };
        if park {
            let service = state().lock().unwrap().service.clone();
            self.set_run_state(&service);
        }
        park
    }

    /// A stop while parked only lets the park go: the service is already stopped.
    pub(crate) fn unpark_instead_of_stop(self: &Arc<Self>) -> bool {
        let service = {
            let mut st = state().lock().unwrap();
            if !st.parked {
                return false;
            }
            st.parked = false;
            st.service.clone()
        };
        if service == "STARTED" {
            return false;
        }
        self.set_run_state(&service);
        true
    }

    pub(crate) fn open_on_demand(self: &Arc<Self>) {
        state().lock().unwrap().selected.clear();
        self.publish_on_demand();
        self.refresh_battery();
        self.refresh_location();
    }

    pub(crate) fn publish_on_demand(self: &Arc<Self>) {
        let list = self.excluded();
        let (battery, location, selected, selecting) = {
            let mut st = state().lock().unwrap();
            st.selected.retain(|s| list.contains(s));
            let battery = match st.battery {
                None => 0,
                Some(false) => 1,
                Some(true) => 2,
            };
            let selected: Vec<bool> = list.iter().map(|s| st.selected.contains(s)).collect();
            let selecting = !st.selected.is_empty();
            (
                battery,
                st.permission == Permission::Granted,
                selected,
                selecting,
            )
        };
        let ssids: Vec<SharedString> = list.iter().map(|s| s.as_str().into()).collect();
        self.ui(move |ui| {
            ui.set_od_battery(battery);
            ui.set_od_location(location);
            ui.set_od_ssids(ModelRc::new(VecModel::from(ssids)));
            ui.set_od_selected(ModelRc::new(VecModel::from(selected)));
            ui.set_od_selecting(selecting);
        });
    }

    fn update_excluded(self: &Arc<Self>, f: impl FnOnce(&[String]) -> Vec<String>) {
        let was_empty = {
            let mut store = self.store.lock().unwrap();
            let s = &mut store.state.settings;
            let was_empty = s.exclude_ssids.is_empty();
            s.exclude_ssids = f(&s.exclude_ssids);
            store.save();
            was_empty
        };
        self.publish_on_demand();
        if was_empty != self.excluded().is_empty() {
            self.refresh_location();
        } else {
            self.sync_suspend();
        }
    }

    /// A tap on the on-demand page.
    pub(crate) fn on_demand_clicked(self: &Arc<Self>, id: &str) {
        let list = self.excluded();
        let index = |s: &str| s.parse::<usize>().ok().filter(|i| *i < list.len());
        match id {
            "battery" => self.request_battery(),
            "location" => {
                let p = state().lock().unwrap().permission;
                match p {
                    Permission::Granted => {}
                    Permission::PermanentlyDenied => self.open_app_settings(),
                    Permission::Denied => self.request_location(true),
                }
            }
            "add" => self.open_input("ssid-add", "添加SSID", "SSID", ""),
            "select-all" => {
                {
                    let mut st = state().lock().unwrap();
                    st.selected = on_demand::select_all(&st.selected, &list);
                }
                self.publish_on_demand();
            }
            "delete" => {
                let selected = std::mem::take(&mut state().lock().unwrap().selected);
                self.update_excluded(|l| on_demand::without(l, &selected));
            }
            _ => {
                if let Some(i) = id.strip_prefix("edit:").and_then(index) {
                    let key = format!("ssid-edit:{i}");
                    self.open_input(&key, "编辑SSID", "SSID", &list[i]);
                } else if let Some(i) = id.strip_prefix("select:").and_then(index) {
                    {
                        let mut st = state().lock().unwrap();
                        if !st.selected.remove(&list[i]) {
                            st.selected.insert(list[i].clone());
                        }
                    }
                    self.publish_on_demand();
                } else if let Some((from, to)) = id
                    .strip_prefix("move:")
                    .and_then(|m| m.split_once(':'))
                    .and_then(|(f, t)| Some((index(f)?, index(t)?)))
                {
                    self.update_excluded(|l| {
                        let mut l = l.to_vec();
                        crate::reorder::move_item(&mut l, from, to);
                        l
                    });
                }
            }
        }
    }

    /// The SSID dialog's check; None for a key it does not own.
    pub(crate) fn on_demand_check(&self, key: &str, value: &str) -> Option<String> {
        let list = self.excluded();
        let editing = match key {
            "ssid-add" => None,
            _ => {
                let i = key.strip_prefix("ssid-edit:")?.parse::<usize>().ok()?;
                Some(list.get(i)?.clone())
            }
        };
        Some(on_demand::ssid_issue(&list, editing.as_deref(), value).unwrap_or_default())
    }

    /// The SSID dialog's answer; false for a key it does not own.
    pub(crate) fn on_demand_input(self: &Arc<Self>, key: &str, value: &str) -> bool {
        let list = self.excluded();
        let old = match key {
            "ssid-add" => None,
            _ => match key
                .strip_prefix("ssid-edit:")
                .and_then(|i| i.parse::<usize>().ok())
            {
                Some(i) => list.get(i).cloned(),
                None => return false,
            },
        };
        if on_demand::ssid_issue(&list, old.as_deref(), value).is_some() {
            return true;
        }
        if old.as_deref() == Some(on_demand::clip(value).as_str()) {
            return true;
        }
        self.update_excluded(|l| on_demand::put(l, old.as_deref(), value));
        true
    }
}
