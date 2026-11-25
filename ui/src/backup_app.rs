//! The backup and restore page on `App`'s side (lib/views/backup_and_restore.dart): its
//! sections, the WebDAV form, and backing up or restoring locally or on the server.

use crate::backup::{self, Dav, DavClient, RestoreOption};
use crate::{ActionItem, App, AppWindow, SetRow, SetSection};
use serde_json::{json, Value};
use slint::{ModelRc, VecModel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// The server's answer to the last ping (`DAVConnectionController`): None while checking.
fn connection() -> &'static Mutex<(Option<Dav>, Option<bool>)> {
    static C: OnceLock<Mutex<(Option<Dav>, Option<bool>)>> = OnceLock::new();
    C.get_or_init(|| Mutex::new((None, None)))
}

/// Where the restore being chosen reads from: true the server.
fn restore_remote() -> &'static Mutex<bool> {
    static R: OnceLock<Mutex<bool>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(false))
}

fn row(id: &str, kind: &str, title: &str, subtitle: &str) -> SetRow {
    SetRow {
        id: id.into(),
        kind: kind.into(),
        glyph: "".into(),
        title: title.into(),
        subtitle: subtitle.into(),
        on: false,
        enabled: true,
        lead: "".into(),
        chip: "".into(),
        trailing: "".into(),
        image: "".into(),
    }
}

fn section(title: &str, rows: Vec<SetRow>) -> SetSection {
    SetSection {
        title: title.into(),
        footer: "".into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

/// `BackupAndRestoreView`'s sections; `connected` is the last ping.
pub fn sections(settings: &crate::store::Settings, connected: Option<bool>) -> Vec<SetSection> {
    let remote = match &settings.dav {
        None => vec![row("dav-bind", "open", "暂无信息", "请绑定WebDAV")],
        Some(dav) => vec![
            row(
                "dav-edit",
                "open",
                &dav.user,
                match connected {
                    None => "连通性：检测中…",
                    Some(true) => "连通性：正常",
                    Some(false) => "连通性：失败",
                },
            ),
            row("dav-file", "value", "文件", &dav.file_name),
            row("dav-backup", "action", "备份", ""),
            row("dav-restore", "action", "恢复", ""),
        ],
    };
    vec![
        section("远程", remote),
        section(
            "本地",
            vec![
                row("local-backup", "action", "备份", ""),
                row("local-restore", "action", "恢复", ""),
            ],
        ),
        section(
            "选项",
            vec![row(
                "restore-strategy",
                "value",
                "恢复策略",
                if settings.restore_override {
                    "覆盖"
                } else {
                    "兼容"
                },
            )],
        ),
    ]
}

/// `WebDAVForm._validate`: the first problem, or "".
pub fn check_dav(uri: &str, user: &str, pass: &str) -> &'static str {
    if !backup::valid_uri(uri.trim()) {
        "请输入有效的WebDAV地址"
    } else if user.trim().is_empty() {
        "账号不能为空"
    } else if pass.is_empty() {
        "密码不能为空"
    } else {
        ""
    }
}

impl App {
    pub(crate) fn bind_backup(self: &Arc<Self>, ui: &AppWindow) {
        ui.on_dav_check(|u, a, p| check_dav(&u, &a, &p).into());
        let a = self.clone();
        ui.on_dav_submit(move |uri, user, pass| {
            a.update_dav(|d| {
                let mut dav = d.clone().unwrap_or_default();
                dav.uri = uri.trim().to_string();
                dav.user = user.trim().to_string();
                dav.password = pass.to_string();
                *d = Some(dav);
            })
        });
        let a = self.clone();
        ui.on_dav_delete(move || a.update_dav(|d| *d = None));
    }

    fn update_dav(self: &Arc<Self>, f: impl FnOnce(&mut Option<Dav>)) {
        {
            let mut store = self.store.lock().unwrap();
            f(&mut store.state.settings.dav);
            store.save();
        }
        self.ping_dav();
    }

    /// `DAVConnectionController.update`: a ping when the credentials changed.
    pub(crate) fn ping_dav(self: &Arc<Self>) {
        let (dav, check) = {
            let s = &self.store.lock().unwrap().state.settings;
            (s.dav.clone(), s.general.check_certificate)
        };
        let same = {
            let mut c = connection().lock().unwrap();
            let strip = |d: &Option<Dav>| {
                d.clone().map(|d| Dav {
                    file_name: String::new(),
                    ..d
                })
            };
            let same = strip(&c.0) == strip(&dav) && c.1.is_some();
            if !same {
                *c = (dav.clone(), None);
            }
            same
        };
        self.publish_settings();
        if same {
            return;
        }
        if let Some(dav) = dav {
            self.spawn(move |app| {
                let ok = DavClient::new(&dav, check).ping();
                let mut c = connection().lock().unwrap();
                if c.0.as_ref().map(|d| (&d.uri, &d.user, &d.password))
                    == Some((&dav.uri, &dav.user, &dav.password))
                {
                    c.1 = Some(ok);
                }
                drop(c);
                app.publish_settings();
            });
        }
    }

    pub(crate) fn backup_sections(&self, settings: &crate::store::Settings) -> Vec<SetSection> {
        sections(settings, connection().lock().unwrap().1)
    }

    pub(crate) fn backup_clicked(self: &Arc<Self>, id: &str) {
        match id {
            "dav-bind" | "dav-edit" => {
                let dav = self.store.lock().unwrap().state.settings.dav.clone();
                self.ui(move |ui| {
                    ui.set_dav_bound(dav.is_some());
                    let d = dav.unwrap_or_default();
                    ui.set_dav_uri(d.uri.into());
                    ui.set_dav_user(d.user.into());
                    ui.set_dav_pass(d.password.into());
                    ui.set_dav_open(true);
                });
            }
            "dav-file" => {
                let name = self
                    .store
                    .lock()
                    .unwrap()
                    .state
                    .settings
                    .dav
                    .as_ref()
                    .map(|d| d.file_name.clone())
                    .unwrap_or_default();
                self.open_input("dav-file", "文件", "", &name);
            }
            "dav-backup" => self.run_backup(true),
            "local-backup" => self.run_backup(false),
            "dav-restore" | "local-restore" => {
                *restore_remote().lock().unwrap() = id == "dav-restore";
                let items = vec![
                    ActionItem {
                        id: "restore-option:profiles".into(),
                        label: "仅恢复配置文件".into(),
                        enabled: true,
                        ..Default::default()
                    },
                    ActionItem {
                        id: "restore-option:all".into(),
                        label: "恢复所有数据".into(),
                        enabled: true,
                        ..Default::default()
                    },
                ];
                self.ui(move |ui| {
                    ui.set_choice_title("恢复".into());
                    ui.set_choice_items(ModelRc::new(VecModel::from(items)));
                    ui.set_choice_open(true);
                });
            }
            "restore-strategy" => {
                let over = self.store.lock().unwrap().state.settings.restore_override;
                let item = |id: &str, label: &str, checked: bool| ActionItem {
                    id: format!("restore-strategy:{id}").into(),
                    label: label.into(),
                    check: true,
                    checked,
                    enabled: true,
                    ..Default::default()
                };
                let items = vec![
                    item("compatible", "兼容", !over),
                    item("override", "覆盖", over),
                ];
                self.ui(move |ui| {
                    ui.set_choice_title("恢复策略".into());
                    ui.set_choice_items(ModelRc::new(VecModel::from(items)));
                    ui.set_choice_open(true);
                });
            }
            _ => {}
        }
    }

    pub(crate) fn set_dav_file(self: &Arc<Self>, name: &str) {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        self.update_dav(|d| {
            if let Some(d) = d {
                d.file_name = name;
            }
        });
    }

    pub(crate) fn set_restore_strategy(self: &Arc<Self>, over: bool) {
        {
            let mut store = self.store.lock().unwrap();
            store.state.settings.restore_override = over;
            store.save();
        }
        self.publish_settings();
    }

    fn dav_client(&self) -> Result<DavClient, String> {
        let s = &self.store.lock().unwrap().state.settings;
        s.dav
            .as_ref()
            .map(|d| DavClient::new(d, s.general.check_certificate))
            .ok_or_else(|| "请绑定WebDAV".to_string())
    }

    /// `_backupOnWebDAV` / `_backupOnLocal`.
    fn run_backup(self: &Arc<Self>, remote: bool) {
        self.spawn(move |app| {
            let result = (|| -> Result<bool, String> {
                let bytes = backup::create(&app.store.lock().unwrap())?;
                if remote {
                    app.dav_client()?.upload(bytes)?;
                    return Ok(true);
                }
                let path = app.store.lock().unwrap().dir.join("backup.part.zip");
                std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
                let name = format!(
                    "SeasonCat_backup_{}.zip",
                    crate::date(crate::now_secs() as i64)
                );
                let r = app.bridge().and_then(|b| {
                    b.shell(
                        "file.save",
                        json!({"src": path.to_string_lossy(), "name": name}),
                        Duration::from_secs(3600),
                    )
                });
                let _ = std::fs::remove_file(&path);
                Ok(r?.get("cancelled").and_then(Value::as_bool) != Some(true))
            })();
            match result {
                Ok(true) => app.toast("备份成功"),
                Ok(false) => {}
                Err(e) => app.alert("备份", e),
            }
        });
    }

    /// `_restoreOnWebDAV` / `_restoreOnLocal` once the option is picked.
    pub(crate) fn run_restore(self: &Arc<Self>, option: RestoreOption) {
        let remote = *restore_remote().lock().unwrap();
        self.spawn(move |app| {
            let result = (|| -> Result<bool, String> {
                let bytes = if remote {
                    app.dav_client()?.download()?
                } else {
                    let path = app.store.lock().unwrap().dir.join("restore.part.zip");
                    let r = app.bridge()?.shell(
                        "file.open",
                        json!({"dest": path.to_string_lossy()}),
                        Duration::from_secs(3600),
                    )?;
                    if r.get("cancelled").and_then(Value::as_bool) == Some(true) {
                        return Ok(false);
                    }
                    let bytes = std::fs::read(&path).map_err(|e| e.to_string());
                    let _ = std::fs::remove_file(&path);
                    bytes?
                };
                let restored = backup::read(&bytes)?;
                let mut store = app.store.lock().unwrap();
                for (id, data) in &restored.files {
                    std::fs::write(store.profile_path(id), data).map_err(|e| e.to_string())?;
                }
                let over = store.state.settings.restore_override;
                backup::apply(&mut store.state, restored.state, option, over);
                store.save();
                Ok(true)
            })();
            match result {
                Ok(true) => {
                    app.toast("恢复成功");
                    app.after_restore();
                }
                Ok(false) => {}
                Err(e) => app.alert("恢复", e),
            }
        });
    }

    fn after_restore(self: &Arc<Self>) {
        let settings = self.store.lock().unwrap().state.settings.clone();
        self.refresh_profiles();
        self.publish_settings();
        self.ping_dav();
        self.ui(move |ui| {
            crate::apply_theme(&ui, &settings);
            ui.set_mode(settings.mode);
        });
        self.reapply();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dav_form_checks_each_field() {
        assert_eq!(check_dav("dav", "u", "p"), "请输入有效的WebDAV地址");
        assert_eq!(check_dav("https://d.example", " ", "p"), "账号不能为空");
        assert_eq!(check_dav("https://d.example", "u", ""), "密码不能为空");
        assert_eq!(check_dav("https://d.example", "u", "p"), "");
    }

    #[test]
    fn sections_follow_the_binding() {
        let mut s = crate::store::Settings::default();
        let ids = |v: &Vec<SetSection>| -> Vec<String> {
            use slint::Model;
            v.iter()
                .flat_map(|s| s.rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>())
                .collect()
        };
        assert_eq!(
            ids(&sections(&s, None)),
            [
                "dav-bind",
                "local-backup",
                "local-restore",
                "restore-strategy"
            ]
        );
        s.dav = Some(Dav::default());
        let v = sections(&s, Some(false));
        assert_eq!(
            ids(&v)[..4],
            ["dav-edit", "dav-file", "dav-backup", "dav-restore"]
        );
    }
}
