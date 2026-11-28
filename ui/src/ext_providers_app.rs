//! The profile sheet's 外部资源 on `App`'s side: the rows, update one or all, upload, preview
//! and the subscription (FlClash: lib/views/proxies/providers.dart, `ProxiesAction`).

use crate::ext_providers::{self as ep, ExtProvider, ProviderFile};
use crate::{ActionItem, App, ProviderRow, CALL};
use serde_json::{json, Value};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[derive(Default)]
struct Shown {
    providers: Vec<ExtProvider>,
    files: HashMap<String, ProviderFile>,
    /// Names whose file is one of the app's remote rule sets, with that set's id.
    app_sets: HashMap<String, String>,
    busy: HashSet<String>,
    updating: bool,
    editing: Option<ExtProvider>,
}

const EDIT_PARENT: &str = "provider";

fn shown() -> &'static Mutex<Shown> {
    static S: OnceLock<Mutex<Shown>> = OnceLock::new();
    S.get_or_init(Default::default)
}

fn head(path: &str) -> Option<Vec<u8>> {
    if path.is_empty() {
        return Some(vec![0]);
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0; ep::PROBE_LENGTH];
    let n = file.read(&mut buf).unwrap_or(0);
    buf.truncate(n);
    Some(buf)
}

fn row(p: &ExtProvider, s: &Shown) -> ProviderRow {
    let usage = p.usage();
    let sub = p.subscription.unwrap_or_default();
    let file = s
        .files
        .get(&p.name)
        .copied()
        .unwrap_or(ProviderFile::Binary);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    ProviderRow {
        name: p.name.as_str().into(),
        subtitle: p.count_label().into(),
        chip: if usage.is_some() {
            crate::icon_sets::compact(p.count.max(0) as usize).into()
        } else {
            Default::default()
        },
        expire: if sub.expire > 0 {
            crate::date(sub.expire).into()
        } else {
            Default::default()
        },
        expired: sub.expire > 0 && sub.expire < now,
        meter: usage.is_some(),
        traffic: format!(
            "{} / {}",
            crate::size(sub.upload + sub.download),
            crate::size(sub.total)
        )
        .into(),
        ratio: usage.unwrap_or(0.0),
        updated: if p.updated_at > 0 {
            crate::ago_short((p.updated_at / 1000) as u64).into()
        } else {
            Default::default()
        },
        busy: s.busy.contains(&p.name),
        previewable: file != ProviderFile::Binary,
    }
}

impl App {
    /// The rows from a `provider.list` answer; reads each file's head, so not on the UI thread.
    pub(crate) fn set_ext_providers(self: &Arc<Self>, list: &[Value]) {
        let providers: Vec<ExtProvider> = list.iter().filter_map(ExtProvider::parse).collect();
        let mut files = HashMap::new();
        let mut app_sets = HashMap::new();
        for p in &providers {
            files.insert(p.name.clone(), ep::probe(head(&p.path).as_deref()));
            if p.rule && p.vehicle.eq_ignore_ascii_case("file") {
                if let Some(id) = self.remote_rule_set_at(&p.path) {
                    app_sets.insert(p.name.clone(), id);
                }
            }
        }
        {
            let mut s = shown().lock().unwrap();
            s.providers = providers;
            s.files = files;
            s.app_sets = app_sets;
        }
        self.publish_ext_providers();
    }

    fn publish_ext_providers(self: &Arc<Self>) {
        let (rows, updating) = {
            let s = shown().lock().unwrap();
            let rows: Vec<ProviderRow> = s.providers.iter().map(|p| row(p, &s)).collect();
            (rows, s.updating)
        };
        self.ui(move |ui| {
            ui.set_profile_providers(ModelRc::new(VecModel::from(rows)));
            ui.set_providers_updating(updating);
        });
    }

    fn ext_provider(&self, name: &str) -> Option<ExtProvider> {
        let s = shown().lock().unwrap();
        s.providers.iter().find(|p| p.name == name).cloned()
    }

    /// The sheet's ⟳, a row's tap and its long press, and the menu's picks.
    pub(crate) fn provider_act(self: &Arc<Self>, action: &str, name: &str) {
        match action {
            "update-all" => self.update_all_providers(),
            "menu" => self.provider_menu(name),
            "preview" => self.open_provider_file(name, false),
            "edit" => self.open_provider_file(name, true),
            "upload" => self.upload_provider(name),
            "sync" => self.sync_provider(name),
            "subscription" => self.show_provider_subscription(name),
            _ => {}
        }
    }

    fn provider_menu(self: &Arc<Self>, name: &str) {
        let (file, syncable, subscribed) = {
            let s = shown().lock().unwrap();
            let Some(p) = s.providers.iter().find(|p| p.name == name) else {
                return;
            };
            if s.busy.contains(name) {
                return;
            }
            let file = s.files.get(name).copied().unwrap_or(ProviderFile::Binary);
            let subscribed = p.subscription.is_some_and(|x| x.total > 0);
            (file, p.syncable(s.app_sets.contains_key(name)), subscribed)
        };
        let items: Vec<ActionItem> = ep::menu(file, syncable, subscribed)
            .into_iter()
            .map(|(id, glyph, label)| ActionItem {
                id: id.into(),
                glyph: glyph.into(),
                label: label.into(),
                enabled: true,
                ..Default::default()
            })
            .collect();
        let name: slint::SharedString = name.into();
        self.ui(move |ui| {
            let anchor = ui.global::<crate::Anchor>();
            ui.set_ctx_names(ModelRc::default());
            ui.set_ctx_items(ModelRc::new(VecModel::from(items)));
            ui.set_ctx_a(name);
            ui.set_ctx_x(anchor.get_x());
            ui.set_ctx_y(anchor.get_y());
            ui.set_ctx_w(anchor.get_w());
            ui.set_ctx_h(anchor.get_h());
            ui.set_ctx_kind("provider".into());
        });
    }

    fn set_provider_busy(self: &Arc<Self>, name: &str, busy: bool) {
        {
            let mut s = shown().lock().unwrap();
            if busy {
                s.busy.insert(name.to_string());
            } else {
                s.busy.remove(name);
            }
        }
        self.publish_ext_providers();
    }

    /// `provider.get` after a change, so the row shows the new count and time.
    fn refresh_provider(self: &Arc<Self>, name: &str) {
        let Ok(info) = self
            .bridge()
            .and_then(|b| b.call("provider.get", json!({"name": name}), CALL))
        else {
            return;
        };
        if let Some(next) = ExtProvider::parse(&info) {
            let mut s = shown().lock().unwrap();
            if let Some(p) = s.providers.iter_mut().find(|p| p.name == name) {
                *p = next;
            }
        }
    }

    /// `ProxiesAction.updateProvider`: an app rule set downloads again through its own page,
    /// anything else is the Core's `provider.update`.
    fn update_provider(self: &Arc<Self>, name: &str) -> Result<(), String> {
        let app_set = shown().lock().unwrap().app_sets.get(name).cloned();
        match app_set {
            Some(id) => self.update_rule_set(&id)?,
            None => {
                self.bridge()?
                    .call("provider.update", json!({"name": name}), CALL)?;
            }
        }
        self.refresh_provider(name);
        Ok(())
    }

    fn sync_provider(self: &Arc<Self>, name: &str) {
        let name = name.to_string();
        self.set_provider_busy(&name, true);
        self.spawn(move |app| {
            if let Err(e) = app.update_provider(&name) {
                app.toast(e);
            }
            app.set_provider_busy(&name, false);
            app.refresh_groups();
        });
    }

    /// `_updateProviders`: every provider at once, the failures listed after.
    fn update_all_providers(self: &Arc<Self>) {
        let names: Vec<String> = {
            let mut s = shown().lock().unwrap();
            if s.updating {
                return;
            }
            s.updating = true;
            s.providers.iter().map(|p| p.name.clone()).collect()
        };
        for name in &names {
            shown().lock().unwrap().busy.insert(name.clone());
        }
        self.publish_ext_providers();
        self.spawn(move |app| {
            let failures = Arc::new(Mutex::new(Vec::new()));
            let workers: Vec<_> = names
                .into_iter()
                .map(|name| {
                    let (app, failures) = (app.clone(), failures.clone());
                    std::thread::spawn(move || {
                        if let Err(e) = app.update_provider(&name) {
                            failures.lock().unwrap().push((name.clone(), e));
                        }
                        app.set_provider_busy(&name, false);
                    })
                })
                .collect();
            for w in workers {
                let _ = w.join();
            }
            shown().lock().unwrap().updating = false;
            app.publish_ext_providers();
            app.refresh_groups();
            let failures = std::mem::take(&mut *failures.lock().unwrap());
            app.show_failures(failures);
        });
    }

    /// `_handleSideLoadProvider`: the picked file goes to the Core, then over the provider's.
    fn upload_provider(self: &Arc<Self>, name: &str) {
        let Some(p) = self.ext_provider(name) else {
            return;
        };
        self.spawn(move |app| {
            let picked = app.temp_for("provider-upload");
            let answer = app.bridge().and_then(|b| {
                b.shell(
                    "file.open",
                    json!({"dest": picked.to_string_lossy()}),
                    Duration::from_secs(3600),
                )
            });
            match answer {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => return,
                Ok(_) => {}
                Err(e) => return app.toast(e),
            }
            let bytes = std::fs::read(&picked).unwrap_or_default();
            let _ = std::fs::remove_file(&picked);
            app.set_provider_busy(&p.name, true);
            let result = ep::decode(&bytes).map(str::to_string).and_then(|data| {
                app.bridge()?.call(
                    "provider.sideLoad",
                    json!({"name": p.name, "data": data}),
                    CALL,
                )?;
                if !p.path.is_empty() {
                    std::fs::write(&p.path, &bytes).map_err(|e| e.to_string())?;
                }
                Ok(())
            });
            app.refresh_provider(&p.name);
            app.set_provider_busy(&p.name, false);
            match result {
                Ok(()) => app.refresh_groups(),
                Err(e) => app.toast(e),
            }
        });
    }

    /// `_handlePreviewProvider` (an `.mrs` set dumped to text), or `_EditProviderView`.
    fn open_provider_file(self: &Arc<Self>, name: &str, editable: bool) {
        let Some(p) = self.ext_provider(name) else {
            return;
        };
        let file = shown().lock().unwrap().files.get(name).copied();
        if p.path.is_empty()
            || file == Some(ProviderFile::Binary)
            || (editable && file != Some(ProviderFile::Text))
        {
            return;
        }
        shown().lock().unwrap().editing = editable.then(|| p.clone());
        let title: slint::SharedString = p.name.as_str().into();
        let parent = if editable { EDIT_PARENT } else { "" };
        self.ui(move |ui| {
            ui.set_profile_sheet_open(false);
            ui.set_editor_title(title);
            ui.set_editor_text(Default::default());
            ui.set_editor_readonly(!editable);
            ui.set_editor_parent(parent.into());
            ui.set_editor_schema("provider".into());
            ui.set_editor_error(Default::default());
            ui.set_editor_loading(true);
            ui.set_profiles_page("preview".into());
        });
        let load = crate::items_app::editor_load();
        self.spawn(move |app| {
            let text = if file == Some(ProviderFile::RuleSet) {
                app.bridge()
                    .and_then(|b| {
                        b.call("ruleSet.dump", json!({"profile": {"path": p.path}}), CALL)
                    })
                    .map(|r| {
                        r.get("text")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string()
                    })
            } else {
                std::fs::read(&p.path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| ep::decode(&b).map(str::to_string))
            };
            app.profile_editor_loaded(load, text);
        });
    }

    pub(crate) fn provider_editor_save(self: &Arc<Self>, parent: &str, text: &str) -> bool {
        if parent != EDIT_PARENT {
            return false;
        }
        let Some(p) = shown().lock().unwrap().editing.clone() else {
            return false;
        };
        let text = text.to_string();
        self.spawn(move |app| {
            app.set_provider_busy(&p.name, true);
            let result = app
                .bridge()
                .and_then(|b| {
                    b.call(
                        "provider.sideLoad",
                        json!({"name": p.name, "data": text}),
                        CALL,
                    )
                })
                .and_then(|_| std::fs::write(&p.path, text.as_bytes()).map_err(|e| e.to_string()));
            app.refresh_provider(&p.name);
            app.set_provider_busy(&p.name, false);
            match result {
                Ok(()) => {
                    app.refresh_groups();
                    app.leave_provider_editor();
                }
                Err(e) => app.toast(e),
            }
        });
        true
    }

    /// `_handlePop`: unchanged text leaves, else 是否保存更改？ under the provider's name.
    pub(crate) fn provider_editor_back(self: &Arc<Self>, text: String) {
        if text == crate::items_app::profile_editor_opened() {
            return self.leave_provider_editor();
        }
        let Some(name) = shown()
            .lock()
            .unwrap()
            .editing
            .as_ref()
            .map(|p| p.name.clone())
        else {
            return self.leave_provider_editor();
        };
        self.confirm(&name, "是否保存更改？".into(), "provider-save", text);
    }

    pub(crate) fn provider_save_answer(self: &Arc<Self>, text: String, yes: bool) {
        if !yes {
            return self.leave_provider_editor();
        }
        self.provider_editor_save(EDIT_PARENT, &text);
    }

    fn leave_provider_editor(self: &Arc<Self>) {
        shown().lock().unwrap().editing = None;
        self.ui(|ui| {
            ui.set_editor_parent("".into());
            ui.set_profiles_page("".into());
            ui.set_profile_sheet_open(true);
        });
    }

    fn show_provider_subscription(self: &Arc<Self>, name: &str) {
        let Some(s) = self.ext_provider(name).and_then(|p| p.subscription) else {
            return;
        };
        let text = ep::subscription_text(s, &crate::date(s.expire));
        self.alert("订阅信息", text);
    }
}
