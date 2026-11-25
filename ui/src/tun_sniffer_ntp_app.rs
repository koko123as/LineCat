//! The TUN overrides on the network page and the sniffer's protocol page, on `App`'s side.
//! The sniffer and NTP pages themselves are override pages (settings_app.rs).

use crate::tun_sniffer_ntp::{self as tsn, SNIFFER, TUN};
use crate::{ActionItem, App, PageMeta, SetSection};
use slint::{ModelRc, SharedString, VecModel};
use std::sync::{Arc, Mutex};

/// The protocol the sniff page shows, as `sniffer:sniff.<NAME>`.
static SNIFF: Mutex<String> = Mutex::new(String::new());

fn sniff_path() -> String {
    let key = SNIFF.lock().unwrap().clone();
    key.split_once(':')
        .map(|(_, p)| p.to_string())
        .unwrap_or_default()
}

pub fn page_meta(page: &str) -> Option<PageMeta> {
    (page == "sniff").then(|| PageMeta {
        known: true,
        title: SNIFFER.key(&sniff_path()).map_or("", |k| k.label).into(),
        parent: "sniffer".into(),
        ..Default::default()
    })
}

pub fn sniff_sections(settings: &crate::store::Settings) -> Vec<SetSection> {
    tsn::sniff_sections(&SNIFFER.value(&settings.sniffer_override, &sniff_path()))
}

impl App {
    pub(crate) fn open_sniff(self: &Arc<Self>, key: &str) {
        *SNIFF.lock().unwrap() = key.to_string();
        self.open_page("sniff");
    }

    pub(crate) fn tsn_clicked(self: &Arc<Self>, page: &str, id: &str) -> bool {
        match page {
            "network" => {
                let path = id.strip_prefix("remove:").unwrap_or(id);
                if id != "action" && TUN.key(path).is_none() {
                    return false;
                }
                self.override_clicked_in(TUN.id, "network", id);
            }
            "sniff" => {
                let key = SNIFF.lock().unwrap().clone();
                if id == "ports" {
                    self.open_list(&key, "sniff");
                } else if id == "destination" {
                    self.open_destination();
                }
            }
            _ => return false,
        }
        true
    }

    fn open_destination(self: &Arc<Self>) {
        let state = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .sniffer_override
            .clone();
        let current = tsn::destination(&SNIFFER.value(&state, &sniff_path()));
        let items: Vec<ActionItem> = tsn::DESTINATIONS
            .iter()
            .map(|(id, label, value)| ActionItem {
                id: format!("sniffdest:{id}").into(),
                label: (*label).into(),
                check: true,
                checked: *value == current,
                enabled: true,
                ..Default::default()
            })
            .collect();
        self.ui(move |ui| {
            ui.set_choice_title(SharedString::from("覆盖目标地址"));
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    pub(crate) fn tsn_toggled(self: &Arc<Self>, page: &str, id: &str, on: bool) -> bool {
        if page != "network" || TUN.key(id).is_none() {
            return false;
        }
        self.override_set(TUN.id, id, on.into());
        true
    }

    pub(crate) fn tsn_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        if key != "sniffdest" {
            return false;
        }
        let Some(dest) = tsn::DESTINATIONS.iter().find(|d| d.0 == value) else {
            return true;
        };
        let path = sniff_path();
        let state = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .sniffer_override
            .clone();
        let v = tsn::with_destination(&SNIFFER.value(&state, &path), dest.2);
        self.override_set(SNIFFER.id, &path, v);
        true
    }
}
