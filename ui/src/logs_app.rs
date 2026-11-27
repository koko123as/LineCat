//! The logs page wired to the window: the records, search, level filters, the menu and a
//! record's details. FlClash: lib/views/logs.dart.

use crate::logs::{self, Payload, Record};
use crate::settings_app::Effect;
use crate::{ActionItem, App, AppWindow, LogItem};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

const PAUSED_MAX_LOGS: usize = 2 * logs::MAX_LOGS;

#[derive(Default)]
pub(crate) struct LogsState {
    pub records: Vec<Record>,
    pub query: String,
    pub levels: Vec<String>,
    pub paused: bool,
}

/// A record as the list draws it; `id` is its index in the records.
pub fn log_item(id: usize, r: &Record) -> LogItem {
    let p = Payload::parse(&r.payload);
    let source = p.source_text();
    let route = p.route.unwrap_or_default();
    LogItem {
        id: id as i32,
        level: r.level.as_str().into(),
        time: r.date_time.as_str().into(),
        text: r.payload.as_str().into(),
        tag: p.tag.into(),
        message: p.message.into(),
        has_route: !route.proxy.is_empty(),
        dest: route.destination.into(),
        error: route.error.into(),
        rule: route.rule.into(),
        proxy: route.proxy.into(),
        source: source.into(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl App {
    pub(crate) fn bind_logs(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_logs_search(move |q| {
            a.logs.lock().unwrap().query = q.to_string();
            a.publish_logs();
        });
        let a = self.clone();
        ui.on_logs_filter(move |level| {
            {
                let mut s = a.logs.lock().unwrap();
                if !s.levels.iter().any(|l| *l == level.as_str()) {
                    s.levels.push(level.to_string());
                }
            }
            a.publish_logs();
        });
        let a = self.clone();
        ui.on_logs_unfilter(move |level| {
            a.logs
                .lock()
                .unwrap()
                .levels
                .retain(|l| *l != level.as_str());
            a.publish_logs();
        });
        let a = self.clone();
        ui.on_logs_following(move |on| {
            let resumed = {
                let mut s = a.logs.lock().unwrap();
                let resumed = s.paused && on;
                s.paused = !on;
                resumed
            };
            if resumed {
                a.publish_logs();
            }
        });
        let a = self.clone();
        ui.on_logs_open_filters(move || a.open_log_filters());
        let a = self.clone();
        ui.on_logs_open_menu(move || a.open_log_menu());
        let a = self.clone();
        let w = ui.as_weak();
        ui.on_logs_open(move |id| {
            let Some(ui) = w.upgrade() else { return };
            let s = a.logs.lock().unwrap();
            if let Some(r) = s.records.get(id as usize) {
                ui.set_log_detail(log_item(id as usize, r));
                ui.set_log_detail_open(true);
            }
        });
        let a = self.clone();
        ui.on_logs_copy(move |id| {
            let text = a
                .logs
                .lock()
                .unwrap()
                .records
                .get(id as usize)
                .map(logs::copy_text);
            if let Some(text) = text {
                a.copy_to_clipboard(text);
            }
        });
    }

    pub(crate) fn copy_to_clipboard(self: &Arc<Self>, text: String) {
        self.spawn(move |app| {
            match app
                .bridge()
                .and_then(|b| b.shell("clipboard.set", json!({"text": text}), crate::CALL))
            {
                Ok(_) => app.toast("复制成功"),
                Err(e) => app.toast(e),
            }
        });
    }

    /// One Core log entry: {"level", "time" (ms), "text"}.
    pub(crate) fn push_log(self: &Arc<Self>, entry: &Value) {
        let ms = entry.get("time").and_then(Value::as_i64).unwrap_or(0);
        let record = Record {
            level: entry
                .get("level")
                .and_then(Value::as_str)
                .unwrap_or("info")
                .into(),
            date_time: crate::full_time(if ms > 0 { ms } else { now_ms() }),
            payload: entry
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
        };
        {
            let mut pending = self.pending_logs.lock().unwrap();
            pending.push(record);
            logs::trim(&mut pending);
        }
        if self.logs_scheduled.swap(true, Ordering::SeqCst) {
            return;
        }
        let app = self.clone();
        // One UI update per burst: the Core may log hundreds of lines a second.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            app.logs_scheduled.store(false, Ordering::SeqCst);
            let batch: Vec<Record> = std::mem::take(&mut *app.pending_logs.lock().unwrap());
            let paused = {
                let mut s = app.logs.lock().unwrap();
                s.records.extend(batch);
                if !s.paused || s.records.len() > PAUSED_MAX_LOGS {
                    logs::trim(&mut s.records);
                }
                s.paused
            };
            if !paused {
                app.publish_logs();
            }
        });
    }

    /// The list newest first (the reversed `RecordFeedList`), the offered levels and the
    /// picked ones.
    pub(crate) fn publish_logs(self: &Arc<Self>) {
        let (items, offered, picked) = {
            let s = self.logs.lock().unwrap();
            let shown = logs::visible(&s.records, &s.query, &s.levels);
            let items: Vec<LogItem> = shown
                .into_iter()
                .rev()
                .map(|i| log_item(i, &s.records[i]))
                .collect();
            let offered: Vec<SharedString> = logs::offered(&s.records, &s.levels)
                .into_iter()
                .map(Into::into)
                .collect();
            let picked: Vec<SharedString> = s.levels.iter().map(|l| l.as_str().into()).collect();
            (items, offered, picked)
        };
        self.ui(move |ui| {
            ui.set_logs(ModelRc::new(VecModel::from(items)));
            ui.set_log_offered(ModelRc::new(VecModel::from(offered)));
            ui.set_log_filters(ModelRc::new(VecModel::from(picked)));
        });
    }

    /// `optionMenuItems`: each offered level with a check, kept open while toggling.
    fn open_log_filters(self: &Arc<Self>) {
        let items: Vec<ActionItem> = {
            let s = self.logs.lock().unwrap();
            logs::offered(&s.records, &s.levels)
                .into_iter()
                .enumerate()
                .map(|(i, l)| ActionItem {
                    id: format!("log-filter:{l}").into(),
                    heading: if i == 0 {
                        "筛选".into()
                    } else {
                        SharedString::new()
                    },
                    label: l.into(),
                    check: true,
                    checked: s.levels.iter().any(|p| p == l),
                    enabled: true,
                    ..Default::default()
                })
                .collect()
        };
        self.show_choice("", items);
    }

    /// `_buildMenuItems`: export, then the log level with its choices.
    fn open_log_menu(self: &Arc<Self>) {
        let level = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .general
            .log_level
            .clone();
        let items = vec![
            ActionItem {
                id: "log-menu:export".into(),
                glyph: "export".into(),
                label: "导出日志".into(),
                enabled: true,
                ..Default::default()
            },
            ActionItem {
                id: "log-menu:level".into(),
                glyph: "sliders".into(),
                label: "日志等级".into(),
                subtitle: level.into(),
                enabled: true,
                ..Default::default()
            },
        ];
        self.show_choice("", items);
    }

    fn open_log_levels(self: &Arc<Self>) {
        let level = self
            .store
            .lock()
            .unwrap()
            .state
            .settings
            .general
            .log_level
            .clone();
        let items = logs::LEVELS
            .iter()
            .map(|l| ActionItem {
                id: format!("log-level:{l}").into(),
                label: (*l).into(),
                check: true,
                checked: *l == level,
                enabled: true,
                ..Default::default()
            })
            .collect();
        self.show_choice("日志等级", items);
    }

    pub(crate) fn show_choice(self: &Arc<Self>, title: &str, items: Vec<ActionItem>) {
        let title: SharedString = title.into();
        self.ui(move |ui| {
            ui.set_choice_title(title);
            ui.set_choice_items(ModelRc::new(VecModel::from(items)));
            ui.set_choice_open(true);
        });
    }

    pub(crate) fn logs_picked(self: &Arc<Self>, key: &str, value: &str) -> bool {
        match (key, value) {
            ("log-filter", level) => {
                logs::toggle(&mut self.logs.lock().unwrap().levels, level);
                self.publish_logs();
                self.open_log_filters();
            }
            ("log-menu", "export") => self.export_logs(),
            ("log-menu", "level") => self.open_log_levels(),
            ("log-level", level) => {
                let level = level.to_string();
                self.update_general(Effect::Reapply, move |g| g.log_level = level);
            }
            _ => return false,
        }
        true
    }

    /// `Logs.exportLogs`: every record, written to a file the user saves.
    fn export_logs(self: &Arc<Self>) {
        let text = logs::export_text(&self.logs.lock().unwrap().records);
        let date = crate::full_time(now_ms());
        let name = logs::file_name("SeasonCat", &date[..10.min(date.len())]);
        let path = self.store.lock().unwrap().dir.join("logs.export.log");
        self.spawn(move |app| {
            if let Err(e) = std::fs::write(&path, text) {
                app.alert("导出日志", e.to_string());
                return;
            }
            let r = app.bridge().and_then(|b| {
                b.shell(
                    "file.save",
                    json!({"src": path.to_string_lossy(), "name": name}),
                    Duration::from_secs(3600),
                )
            });
            let _ = std::fs::remove_file(&path);
            match r {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => {}
                Ok(_) => app.alert("提示", "导出成功"),
                Err(e) => app.alert("导出日志", e),
            }
        });
    }
}
