//! The launch guard and the start failure page on `App`'s side (FlClash: lib/bootstrap.dart,
//! lib/pages/error.dart `InitErrorScreen`).

use crate::boot_guard::{self, Recovery, Stage};
use crate::bridge::Bridge;
use crate::{App, AppWindow, CALL};
use serde_json::json;
use slint::ComponentHandle;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

impl App {
    fn boot_path(&self) -> PathBuf {
        self.store.lock().unwrap().dir.join("boot.json")
    }

    /// An unfinished last launch waits for `hello`'s exit info; any other is recorded now.
    pub(crate) fn begin_boot(self: &Arc<Self>) {
        let path = self.boot_path();
        let record = boot_guard::read(&path);
        let now = crate::requests_app::now_ms();
        if record.as_ref().and_then(|r| r.stage) != Some(Stage::Starting) {
            let active = self.store.lock().unwrap().state.active.clone();
            let decision = boot_guard::Decision::default();
            boot_guard::write(
                &path,
                &boot_guard::starting(record.as_ref(), &decision, active, now, None),
            );
        }
        *self.boot_pending.lock().unwrap() = Some((record, now));
    }

    pub(crate) fn settle_boot(self: &Arc<Self>, exit: Option<boot_guard::ExitInfo>) {
        let Some((record, now)) = self.boot_pending.lock().unwrap().take() else {
            return;
        };
        let decision = boot_guard::resolve(record.as_ref(), exit.as_ref());
        let clear = decision.recovery == Recovery::ClearProfile;
        let active = {
            let mut store = self.store.lock().unwrap();
            if clear {
                store.state.active = None;
                store.save();
            }
            store.state.active.clone()
        };
        if clear {
            self.refresh_profiles();
        }
        boot_guard::write(
            &self.boot_path(),
            &boot_guard::starting(record.as_ref(), &decision, active, now, exit.as_ref()),
        );
        *self.boot.lock().unwrap() = decision;
    }

    /// The first connection skips applying the profile and auto run after an unfinished launch.
    pub(crate) fn boot_degraded(&self) -> bool {
        self.boot.lock().unwrap().is_degraded()
    }

    /// `markRunning`, once the first connection reached the Core.
    pub(crate) fn finish_boot(&self) {
        let path = self.boot_path();
        let Some(record) = boot_guard::read(&path) else {
            return;
        };
        if record.stage == Some(Stage::Starting) {
            boot_guard::write(
                &path,
                &boot_guard::running(&record, &self.boot.lock().unwrap()),
            );
        }
    }

    /// `_showCrashRecoveryTip`.
    pub(crate) fn show_boot_tip(self: &Arc<Self>) {
        let decision = self.boot.lock().unwrap().clone();
        let label = decision
            .failed_profile_id
            .as_deref()
            .map(|id| {
                let store = self.store.lock().unwrap();
                store
                    .state
                    .profiles
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.label())
                    .unwrap_or_else(|| id.to_string())
            })
            .unwrap_or_default();
        if let Some((title, text)) = decision.tip(&label) {
            self.alert(title, text);
        }
    }

    /// `initFailureApp`: the first connection failed before the Core answered.
    pub(crate) fn show_init_error(self: &Arc<Self>, error: String, bridge: Bridge) {
        *self.init_bridge.lock().unwrap() = Some(bridge);
        self.ui(move |ui| {
            ui.set_init_exiting(false);
            ui.set_init_error(error.into());
        });
    }

    pub(crate) fn hide_init_error(self: &Arc<Self>) {
        self.init_bridge.lock().unwrap().take();
        self.ui(|ui| ui.set_init_error("".into()));
    }

    fn init_notice(self: &Arc<Self>, text: String) {
        let shown: slint::SharedString = text.into();
        let kept = shown.clone();
        self.ui(move |ui| ui.set_init_notice(shown));
        self.spawn(move |app| {
            std::thread::sleep(Duration::from_secs(4));
            app.ui(move |ui| {
                if ui.get_init_notice() == kept {
                    ui.set_init_notice("".into());
                }
            });
        });
    }

    fn init_copy(self: &Arc<Self>, error: String) {
        self.spawn(move |app| {
            let text = format!("=== ERROR ===\n{error}");
            let copied = app
                .init_bridge
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| "未连接".to_string())
                .and_then(|b| b.shell("clipboard.set", json!({"text": text}), CALL));
            match copied {
                Ok(_) => app.init_notice("复制成功".into()),
                Err(e) => app.init_notice(e),
            }
        });
    }

    /// `clearAppData`: the settings, the profiles and the launch record, then exit.
    fn init_clear(self: &Arc<Self>) {
        self.ui(|ui| ui.set_init_exiting(true));
        let dir = self.store.lock().unwrap().dir.clone();
        self.spawn(move |app| match std::fs::remove_dir_all(&dir) {
            Ok(()) => app.init_exit(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => app.init_exit(),
            Err(e) => {
                app.ui(|ui| ui.set_init_exiting(false));
                app.init_notice(format!("清除数据失败：{e}"));
            }
        });
    }

    fn init_exit(self: &Arc<Self>) {
        self.ui(|ui| ui.set_init_exiting(true));
        let bridge = self.init_bridge.lock().unwrap().clone();
        self.spawn(move |_| {
            let closed = bridge
                .map(|b| b.shell("app.close", json!({"minimize": false}), CALL))
                .is_some_and(|r| r.is_ok());
            if !closed {
                let _ = slint::quit_event_loop();
            }
        });
    }

    pub(crate) fn bind_init_error(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_init_copy(move |error| a.init_copy(error.into()));
        let a = self.clone();
        ui.on_init_exit(move || a.init_exit());
        let a = self.clone();
        let w = ui.as_weak();
        ui.on_init_clear(move || {
            let Some(ui) = w.upgrade() else { return };
            ui.set_init_clear_open(false);
            a.init_clear();
        });
    }
}
