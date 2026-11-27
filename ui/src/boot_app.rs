//! The launch guard on `App`'s side: the record in `ui/boot.json`, the decision this
//! launch took and the startup error page (FlClash: lib/bootstrap.dart, lib/pages/error.dart).

use crate::boot_guard::{self as guard, BootRecord, Decision, Recovery};
use crate::store::Store;
use crate::{App, CALL};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

struct Boot {
    path: PathBuf,
    decision: Decision,
    record: BootRecord,
}

fn boot() -> &'static Mutex<Option<Boot>> {
    static B: OnceLock<Mutex<Option<Boot>>> = OnceLock::new();
    B.get_or_init(Default::default)
}

fn write(path: &PathBuf, record: &BootRecord) {
    if let Ok(bytes) = serde_json::to_vec(record) {
        let _ = std::fs::write(path, bytes);
    }
}

/// `BootGuard.evaluate`, before anything reads the selected profile.
pub(crate) fn begin(store: &mut Store) {
    let path = store.dir.join("boot.json");
    let previous: Option<BootRecord> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let decision = guard::decide(previous.as_ref());
    if decision.recovery == Recovery::ClearProfile {
        store.state.active = None;
        store.save();
    }
    let record = guard::starting(
        previous.as_ref(),
        &decision,
        store.state.active.clone(),
        crate::now_secs(),
    );
    write(&path, &record);
    *boot().lock().unwrap() = Some(Boot {
        path,
        decision,
        record,
    });
}

/// `_bootDecision.isDegraded`: this launch leaves out the automatic setup.
pub(crate) fn degraded() -> bool {
    boot()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|b| b.decision.degraded())
}

impl App {
    pub(crate) fn show_boot_tip(self: &Arc<Self>) {
        let decision = match boot().lock().unwrap().as_ref() {
            Some(b) => b.decision.clone(),
            None => return,
        };
        let label = decision
            .failed_profile_id
            .as_deref()
            .map(|id| self.label_of(id))
            .unwrap_or_default();
        if let Some((title, text)) = guard::tip(&decision, &label) {
            self.alert(title, text);
        }
    }

    pub(crate) fn boot_running(&self) {
        if let Some(b) = boot().lock().unwrap().as_mut() {
            if b.record.stage.as_deref() == Some("starting") {
                b.record = guard::running(&b.record, &b.decision);
                write(&b.path, &b.record);
            }
        }
    }

    pub(crate) fn boot_closed(&self) {
        if let Some(b) = boot().lock().unwrap().as_mut() {
            b.record = guard::closed(&b.record);
            write(&b.path, &b.record);
        }
    }

    /// `InitErrorView`: the Core could not start, so the app goes no further.
    pub(crate) fn show_init_error(self: &Arc<Self>, error: String) {
        self.ui(move |ui| {
            ui.set_init_error(error.into());
        });
    }

    pub(crate) fn init_error_act(self: &Arc<Self>, action: &str, error: String) {
        match action {
            "copy" => self.spawn(move |app| {
                let copied = app
                    .bridge()
                    .and_then(|b| b.shell("clipboard.set", json!({"text": error}), CALL));
                if copied.is_ok() {
                    app.toast("已复制");
                }
            }),
            "clear" => self.confirm(
                "清除数据",
                "将删除所有配置、设置和本地数据，随后关闭应用。重新打开即可从头开始。".into(),
                "clear-data",
                String::new(),
            ),
            "exit" => self.spawn(|app| {
                if let Ok(b) = app.bridge() {
                    let _ = b.shell("app.close", json!({"minimize": false}), CALL);
                }
            }),
            _ => {}
        }
    }
}
