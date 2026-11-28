//! 工具 → 崩溃日志 on `App`'s side.

use crate::crash_log;
use crate::App;
use std::path::PathBuf;
use std::sync::Arc;

impl App {
    fn crash_path(&self) -> PathBuf {
        self.home.join("crash.txt")
    }

    pub(crate) fn open_crash_log(self: &Arc<Self>) {
        self.open_page("crash-log");
        self.publish_crash_log();
    }

    /// Reads `crash.txt` off the UI thread; the exit is the one `hello` gave at connect.
    pub(crate) fn publish_crash_log(self: &Arc<Self>) {
        self.spawn(|app| {
            let exit = app.last_exit.lock().unwrap().clone();
            let offset = exit
                .as_ref()
                .map(|e| crate::requests_app::utc_offset(e.timestamp))
                .unwrap_or(0);
            let exit = crash_log::exit_view(exit.as_ref(), offset);
            let log = std::fs::read_to_string(app.crash_path())
                .ok()
                .and_then(|t| crash_log::log_view(&t));
            app.ui(move |ui| {
                let exit = exit.unwrap_or_default();
                ui.set_crash_exit_has(!exit.reason.is_empty());
                ui.set_crash_exit_reason(exit.reason.into());
                ui.set_crash_exit_time(exit.time.into());
                ui.set_crash_exit_desc(exit.description.into());
                ui.set_crash_exit_crashed(exit.crashed);
                let log = log.unwrap_or_default();
                ui.set_crash_log_note(log.note().into());
                ui.set_crash_log_text(log.text.into());
            });
        });
    }

    pub(crate) fn crash_log_clicked(self: &Arc<Self>, id: &str) {
        match id {
            "copy" => self.spawn(|app| match std::fs::read_to_string(app.crash_path()) {
                Ok(text) if !text.trim().is_empty() => app.copy_to_clipboard(text),
                _ => app.toast(crash_log::NO_LOG),
            }),
            "clear" => self.confirm(
                "提示",
                "确定清除崩溃日志吗？".into(),
                "crash-clear",
                String::new(),
            ),
            _ => {}
        }
    }

    pub(crate) fn clear_crash_log(self: &Arc<Self>) {
        self.spawn(|app| {
            match std::fs::remove_file(app.crash_path()) {
                Ok(()) => app.toast("已清除"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => app.toast(e.to_string()),
            }
            app.publish_crash_log();
        });
    }
}
