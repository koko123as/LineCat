//! Override quick edit on `App`'s side: the bar's compose on the DNS page and on a custom
//! profile's DNS, NTP and sniffer pages opens the picked keys as YAML; saving reads the
//! text back through the Core's `config.read`.

use crate::override_yaml as oy;
use crate::overrides::{OverrideState, Spec};
use crate::settings_app::Effect;
use crate::{profile_ref, App, CALL};
use serde_json::{json, Value};
use slint::SharedString;
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq)]
enum Open {
    /// The DNS page in 工具, else a custom profile's page.
    Global,
    Profile(String),
}

static OPEN: Mutex<Option<(Open, String)>> = Mutex::new(None);

impl App {
    /// Opens the page's quick edit; false when the page has none.
    pub(crate) fn open_override_yaml(self: &Arc<Self>, page: &str, profile: bool) -> bool {
        let found = if profile {
            crate::profile_more_app::spec(page).map(|spec| {
                let o = self.overrides_now();
                (spec, crate::profile_more_app::state_of(&o, page).clone())
            })
        } else {
            let s = self.store.lock().unwrap().state.settings.clone();
            crate::overrides::spec(page).zip(crate::settings_app::override_state(&s, page).cloned())
        };
        let Some((spec, state)) = found.filter(|(spec, _)| oy::section_of(spec).is_some()) else {
            return false;
        };
        let text = oy::override_yaml(spec, &state);
        let open = if profile {
            Open::Profile(page.to_string())
        } else {
            Open::Global
        };
        *OPEN.lock().unwrap() = Some((open, text.clone()));
        if profile {
            let (title, text): (SharedString, SharedString) = (spec.title.into(), text.into());
            self.ui(move |ui| {
                ui.set_editor_title(title);
                ui.set_editor_text(text);
                ui.set_editor_readonly(false);
                ui.set_editor_parent("pmore".into());
                ui.set_profiles_page("editor".into());
            });
        } else {
            self.open_tools_editor(spec.title, &text, page);
        }
        true
    }

    pub(crate) fn override_yaml_closed(&self) {
        *OPEN.lock().unwrap() = None;
    }

    /// The tools page moved off the editor (`global`), or the profile editor was left.
    pub(crate) fn override_yaml_left(&self, global: bool) {
        let mut open = OPEN.lock().unwrap();
        if open
            .as_ref()
            .is_some_and(|(o, _)| (*o == Open::Global) == global)
        {
            *open = None;
        }
    }

    fn read_override(
        &self,
        spec: &Spec,
        state: &OverrideState,
        text: &str,
    ) -> Result<OverrideState, String> {
        let path = self.store.lock().unwrap().dir.join("quick-edit.yaml");
        let parsed = std::fs::write(&path, oy::document(spec, text))
            .map_err(|e| e.to_string())
            .and_then(|_| self.bridge())
            .and_then(|b| b.call("config.read", json!({"profile": profile_ref(&path)}), CALL));
        let _ = std::fs::remove_file(&path);
        let doc = parsed?
            .get("config")
            .and_then(|c| c.get(oy::document_key(spec)))
            .cloned()
            .unwrap_or(Value::Null);
        if doc.is_null() && !text.trim().is_empty() {
            return Err("无法读取内容".into());
        }
        oy::quick_edit(spec, state, &doc)
    }

    /// The editor's save while a quick edit is open; false otherwise.
    pub(crate) fn override_yaml_save(self: &Arc<Self>, text: &str, profile: bool) -> bool {
        let Some((open, original)) = OPEN.lock().unwrap().clone() else {
            return false;
        };
        if matches!(open, Open::Global) == profile {
            return false;
        }
        let text = text.to_string();
        self.spawn(move |app| {
            if text != original {
                if let Err(e) = app.apply_override_yaml(&open, &text) {
                    return app.alert("无法保存", e);
                }
            }
            app.override_yaml_closed();
            match open {
                Open::Global => app.ui(|ui| ui.set_tools_page("dns".into())),
                Open::Profile(_) => {
                    app.publish_pm();
                    app.ui(|ui| {
                        ui.set_editor_parent("".into());
                        ui.set_profiles_page("pmore".into());
                    });
                }
            }
        });
        true
    }

    fn apply_override_yaml(self: &Arc<Self>, open: &Open, text: &str) -> Result<(), String> {
        match open {
            Open::Global => {
                let s = self.store.lock().unwrap().state.settings.clone();
                let spec = &crate::overrides::DNS;
                let state = crate::settings_app::override_state(&s, "dns")
                    .cloned()
                    .unwrap_or_default();
                let next = self.read_override(spec, &state, text)?;
                self.update_settings(Effect::Reapply, move |s| s.dns_override = next);
            }
            Open::Profile(page) => {
                let spec = crate::profile_more_app::spec(page).ok_or_else(String::new)?;
                let o = self.overrides_now();
                let state = crate::profile_more_app::state_of(&o, page).clone();
                let next = self.read_override(spec, &state, text)?;
                self.edit_spec(page, move |_, st| *st = next);
            }
        }
        Ok(())
    }
}
