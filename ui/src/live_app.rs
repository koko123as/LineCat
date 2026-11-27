//! The dashboard's live status button and deck: the Core's link, the config's warnings,
//! the update download and the MITM modules awaiting consent, as `live_status.rs` orders
//! them, plus the developer page's preview of every kind.

use crate::live_status::{self, Core, Status, Warning};
use crate::{App, AppWindow, LiveCard, LivePill, LiveWarning};
use serde_json::json;
use slint::{ModelRc, VecModel};
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Live {
    /// None until the app first reaches the Core.
    pub core: Option<Core>,
    pub reason: Option<String>,
    pub warnings: Vec<Warning>,
    pub update: Option<Status>,
    pub consent: Vec<String>,
    /// The developer page's deck, whose actions do nothing.
    pub preview: bool,
    pub open: bool,
    pub front: usize,
    pub asking: bool,
    pub expanded: bool,
}

impl Live {
    fn list(&self) -> Vec<Status> {
        if self.preview {
            return live_status::preview();
        }
        live_status::statuses(&live_status::Sources {
            core: self.core.clone(),
            separate_core: false,
            update: self.update.clone(),
            consent: self.consent.clone(),
            warnings: self.warnings.clone(),
        })
    }
}

impl App {
    pub(crate) fn bind_live(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_live_show(move || {
            a.with_live(|l| {
                l.preview = false;
                l.open = true;
                l.front = 0;
                l.asking = false;
                l.expanded = false;
            })
        });
        let a = self.clone();
        ui.on_live_close(move || {
            a.with_live(|l| {
                l.open = false;
                l.preview = false;
            })
        });
        let a = self.clone();
        ui.on_live_flip(move |dy| {
            a.with_live(|l| {
                let n = l.list().len();
                if l.asking || l.expanded {
                    return;
                }
                l.front = live_status::flip(l.front, n, dy, 0.0);
            })
        });
        let a = self.clone();
        ui.on_live_unask(move || a.with_live(|l| l.asking = false));
        let a = self.clone();
        ui.on_live_action(move || a.live_action());
        let a = self.clone();
        ui.on_live_confirm(move || {
            let (preview, front) = {
                let mut l = a.live.lock().unwrap();
                l.asking = false;
                (l.preview, l.list().get(l.front).cloned())
            };
            match front {
                Some(Status::Core(Core::Running)) if !preview => a.restart_core(),
                Some(Status::Downloading { .. }) if !preview => a.cancel_update_download(),
                _ => {}
            }
            a.publish_live();
        });
        let a = self.clone();
        ui.on_live_discard(move || {
            if !a.live.lock().unwrap().preview {
                a.discard_update();
            }
        });
    }

    fn with_live(self: &Arc<Self>, f: impl FnOnce(&mut Live)) {
        f(&mut self.live.lock().unwrap());
        self.publish_live();
    }

    /// The front card's button: restart, ask first, or open the warnings.
    fn live_action(self: &Arc<Self>) {
        let (front, preview) = {
            let l = self.live.lock().unwrap();
            (l.list().get(l.front).cloned(), l.preview)
        };
        match front {
            Some(Status::Warnings(_)) => self.with_live(|l| l.expanded = !l.expanded),
            Some(Status::Core(Core::Running)) | Some(Status::Downloading { .. }) => {
                self.with_live(|l| l.asking = true)
            }
            Some(Status::Core(Core::Stopped)) if !preview => self.restart_core(),
            Some(Status::Consent(_)) if !preview => self.review_modules(),
            Some(Status::Ready { .. }) if !preview => {
                self.with_live(|l| l.open = false);
                self.install_update();
            }
            _ => {}
        }
    }

    /// `restartCore` for a Core in the app's process: drop the link, which the connect
    /// loop opens and sets up again.
    fn restart_core(self: &Arc<Self>) {
        if let Some(b) = self.bridge.lock().unwrap().as_ref() {
            b.close();
        }
    }

    /// The link to the Core came up, failed with `reason`, or went down.
    pub(crate) fn set_core(self: &Arc<Self>, core: Core, reason: Option<String>) {
        self.with_live(|l| {
            if core == Core::Stopped && l.core.is_none() && reason.is_none() {
                return;
            }
            if core != Core::Running {
                l.warnings.clear();
            }
            l.core = Some(core);
            l.reason = reason;
        });
    }

    /// `_loadConfigWarnings` after a config is set up, kept only while 日志捕获 is on.
    pub(crate) fn load_warnings(self: &Arc<Self>) {
        let open_logs = self.store.lock().unwrap().state.settings.general.open_logs;
        let warnings = if open_logs {
            self.bridge()
                .and_then(|b| b.call("core.lastWarnings", json!({}), Duration::from_secs(10)))
                .map(|r| live_status::warnings_of(&r))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.with_live(|l| l.warnings = warnings);
    }

    /// The developer page's 实时状态卡片测试.
    pub(crate) fn live_preview(self: &Arc<Self>) {
        self.with_live(|l| {
            l.preview = true;
            l.open = true;
            l.front = 0;
            l.asking = false;
            l.expanded = false;
        });
    }

    pub(crate) fn publish_live(self: &Arc<Self>) {
        let installed = self.app_version.lock().unwrap().clone();
        let (pill, cards, open, front, asking, expanded) = {
            let mut l = self.live.lock().unwrap();
            let live = Live {
                preview: false,
                core: l.core.clone(),
                update: l.update.clone(),
                consent: l.consent.clone(),
                warnings: l.warnings.clone(),
                ..Live::default()
            };
            let pill = live_status::pill(&live.list()).unwrap_or_default();
            let list = l.list();
            if list.is_empty() {
                l.open = false;
            }
            l.front = l.front.min(list.len().saturating_sub(1));
            let reason = if l.preview { None } else { l.reason.clone() };
            let cards: Vec<live_status::Card> = list
                .iter()
                .map(|s| live_status::card(s, reason.as_deref(), &installed))
                .collect();
            (pill, cards, l.open, l.front, l.asking, l.expanded)
        };
        self.ui(move |ui| {
            ui.set_live_pill(LivePill {
                kind: pill.kind.into(),
                tone: pill.tone,
                text: pill.text.into(),
                progress: pill.progress,
            });
            let cards: Vec<LiveCard> = cards.into_iter().map(card_model).collect();
            ui.set_live_cards(ModelRc::new(VecModel::from(cards)));
            ui.set_live_front(front as i32);
            ui.set_live_asking(asking);
            ui.set_live_expanded(expanded);
            ui.set_live_open(open);
        });
    }
}

/// A card as the deck draws it.
pub fn card_model(c: live_status::Card) -> LiveCard {
    let (on, off) = if c.kind == "downloading" {
        live_status::ticks(c.progress)
    } else {
        Default::default()
    };
    LiveCard {
        kind: c.kind.into(),
        tint: c.tint,
        title: c.title.into(),
        body: c.body.into(),
        action: c.action.into(),
        action_tone: c.action_tone,
        ask_title: c.ask_title.into(),
        ask_body: c.ask_body.into(),
        ask_label: c.ask_label.into(),
        big: c.big.into(),
        tag: c.tag.into(),
        ticks_on: on.into(),
        ticks_off: off.into(),
        chips: ModelRc::new(VecModel::from(
            c.chips
                .into_iter()
                .map(slint::SharedString::from)
                .collect::<Vec<_>>(),
        )),
        warnings: ModelRc::new(VecModel::from(
            c.warnings
                .into_iter()
                .map(|w| LiveWarning {
                    path: w.path.into(),
                    message: w.message.into(),
                })
                .collect::<Vec<_>>(),
        )),
    }
}
