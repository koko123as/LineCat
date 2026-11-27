//! The custom profile page and the profile rename dialog (`renameProfile`), on `App`'s side.
//! FlClash: lib/views/profiles/custom/custom.dart.

use crate::custom_profile as cp;
use crate::{now_secs, App, AppWindow, CustomRow};
use slint::{ModelRc, VecModel};
use std::sync::{Arc, Mutex};

/// The open page: which profile, whether it was just added, its label then and what it
/// held when the page opened (`_initialProfile`, `_loadedInputs`).
struct Page {
    id: String,
    is_new: bool,
    initial_label: String,
    inputs: serde_json::Value,
}

static PAGE: Mutex<Option<Page>> = Mutex::new(None);

const RENAME: &str = "profile-rename:";

/// The profile the page shows, while it is open.
pub(crate) fn page_id() -> Option<String> {
    PAGE.lock().unwrap().as_ref().map(|p| p.id.clone())
}

impl App {
    pub(crate) fn bind_custom_profile(self: &Arc<Self>, ui: &AppWindow) {
        let a = self.clone();
        ui.on_add_custom(move || a.add_custom_profile());
        let a = self.clone();
        ui.on_custom_back(move || a.leave_custom());
        let a = self.clone();
        ui.on_custom_action(move |action| {
            let Some(id) = PAGE.lock().unwrap().as_ref().map(|p| p.id.clone()) else {
                return;
            };
            match action.as_str() {
                "rename" => a.ask_rename(&id),
                "preview" => a.preview(id),
                "import-url" => a.open_input(
                    crate::custom_import_app::URL_INPUT,
                    "通过URL导入",
                    "URL",
                    "",
                ),
                "import-file" => a.custom_import_from_file(id),
                _ => {}
            }
        });
        let a = self.clone();
        ui.on_rename_profile(move |id| a.ask_rename(&id));
        let a = self.clone();
        ui.on_custom_row(move |id| {
            a.open_overview_row(&id);
        });
        let a = self.clone();
        ui.on_custom_group(move |action| a.custom_group(&action));
        let a = self.clone();
        ui.on_pg_clicked(move |id| a.pg_clicked(&id));
        let a = self.clone();
        ui.on_pg_toggled(move |id, on| a.pg_toggled(&id, on));
        let a = self.clone();
        ui.on_pg_back(move || a.pg_back());
        let a = self.clone();
        ui.on_group_yaml_closed(move || a.group_yaml_closed());
        let a = self.clone();
        ui.on_pg_member(move |action| a.pg_member(&action));
        let a = self.clone();
        ui.on_pg_pick(move |action| a.pg_pick(&action));
        let a = self.clone();
        ui.on_icon_pick(move |action| a.icon_pick(&action));
        self.bind_profile_rules(ui);
    }

    /// `_toAddCustom`: an unnamed custom profile, opened as new.
    fn add_custom_profile(self: &Arc<Self>) {
        let id = self.new_id();
        self.put_new(cp::new_profile(id.clone(), "未命名", now_secs()));
        self.open_custom(id, true);
    }

    pub(crate) fn open_custom(self: &Arc<Self>, id: String, is_new: bool) {
        let page = {
            let store = self.store.lock().unwrap();
            let Some(p) = store.state.profiles.iter().find(|p| p.id == id) else {
                return;
            };
            Page {
                initial_label: p.name.clone(),
                inputs: cp::snapshot(&store.state, &id),
                id,
                is_new,
            }
        };
        *PAGE.lock().unwrap() = Some(page);
        self.reset_groups_section();
        self.reset_rules_page();
        self.reset_pm();
        self.publish_custom();
        self.ui(|ui| {
            ui.set_custom_open(true);
            ui.set_profiles_over(ui.get_tab() != 2);
            ui.set_picker_open(false);
            ui.set_profiles_page("custom".into());
        });
    }

    pub(crate) fn publish_custom(self: &Arc<Self>) {
        let Some(id) = PAGE.lock().unwrap().as_ref().map(|p| p.id.clone()) else {
            return;
        };
        let others = {
            let store = self.store.lock().unwrap();
            store
                .state
                .profiles
                .iter()
                .find(|p| p.id == id)
                .map_or(0, |p| cp::other_issue_count(&store.state, p))
        };
        let issues =
            (self.publish_custom_groups(&id) + self.profile_rule_issues(&id) + others) as i32;
        let (title, rows, groups) = {
            let store = self.store.lock().unwrap();
            let Some(p) = store.state.profiles.iter().find(|p| p.id == id) else {
                return;
            };
            let rows: Vec<CustomRow> = cp::overview(&store.state, p)
                .into_iter()
                .map(|(id, glyph, title, state, tone)| CustomRow {
                    id: id.into(),
                    glyph: glyph.into(),
                    title: title.into(),
                    state: state.into(),
                    tone,
                })
                .collect();
            (p.label(), rows, cp::groups_count(&store.state, &id) as i32)
        };
        self.ui(move |ui| {
            ui.set_custom_title(title.into());
            ui.set_custom_rows(ModelRc::new(VecModel::from(rows)));
            ui.set_custom_issues(issues);
            ui.set_custom_group_count(groups);
        });
    }

    /// `PopGuard` with `_nameBeforeLeaving` and `_discardIfUntouched`, then `dispose`: a new
    /// profile left as it was created goes again; one edited but still on its first label
    /// asks for a name once the page is gone; an edited one is stamped and applied.
    fn leave_custom(self: &Arc<Self>) {
        let Some(page) = PAGE.lock().unwrap().take() else {
            return;
        };
        self.reset_groups_section();
        self.reset_rules_page();
        self.reset_pm();
        self.ui(|ui| {
            ui.set_custom_open(false);
            ui.set_custom_menu_open(false);
            ui.set_custom_import_open(false);
            let page = ui.get_profiles_page();
            if matches!(
                page.as_str(),
                "custom" | "group" | "group-members" | "rules" | "rule" | "rule-presets" | "pmore"
            ) || (page == "editor"
                && matches!(ui.get_editor_parent().as_str(), "group" | "rules"))
            {
                ui.set_profiles_page("".into());
            }
            ui.set_editor_parent("".into());
        });
        let (untouched, label, edited) = {
            let store = self.store.lock().unwrap();
            let label = store
                .state
                .profiles
                .iter()
                .find(|p| p.id == page.id)
                .map(|p| p.name.clone());
            (
                cp::is_untouched(&store.state, &page.id, &page.initial_label),
                label,
                cp::snapshot(&store.state, &page.id) != page.inputs,
            )
        };
        let Some(label) = label else {
            return;
        };
        if page.is_new && untouched {
            self.delete_profile(page.id);
            return;
        }
        if page.is_new && label == page.initial_label {
            self.ask_rename(&page.id);
        }
        if edited {
            self.touch_profile(&page.id);
        }
    }

    /// `touchProfile` and `autoApplyProfile`.
    fn touch_profile(self: &Arc<Self>, id: &str) {
        let active = {
            let mut store = self.store.lock().unwrap();
            if let Some(p) = store.profile_mut(id) {
                p.updated = now_secs();
            }
            store.save();
            store.state.active.as_deref() == Some(id)
        };
        self.refresh_profiles();
        if active {
            self.reapply();
        }
    }

    fn ask_rename(self: &Arc<Self>, id: &str) {
        let Some(label) = self
            .store
            .lock()
            .unwrap()
            .state
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.clone())
        else {
            return;
        };
        self.open_input(&format!("{RENAME}{id}"), "重命名", "名称", &label);
    }

    /// `validateProfileName`.
    pub(crate) fn custom_profile_check(&self, key: &str, value: &str) -> Option<String> {
        if key == crate::custom_import_app::URL_INPUT {
            return Some(self.custom_import_url_check(value).unwrap_or_default());
        }
        key.starts_with(RENAME)
            .then(|| {
                value
                    .trim()
                    .is_empty()
                    .then(|| "请输入配置名称".to_string())
            })
            .flatten()
    }

    pub(crate) fn custom_profile_input(self: &Arc<Self>, key: &str, value: &str) -> bool {
        if key == crate::custom_import_app::URL_INPUT {
            if let Some(id) = page_id() {
                self.custom_import_from_url(id, value.trim().to_string());
            }
            return true;
        }
        let Some(id) = key.strip_prefix(RENAME) else {
            return false;
        };
        let label: String = value.trim().chars().take(64).collect();
        if label.is_empty() {
            return true;
        }
        {
            let mut store = self.store.lock().unwrap();
            if let Some(p) = store.profile_mut(id) {
                p.name = label;
            }
            store.save();
        }
        self.refresh_profiles();
        self.publish_custom();
        true
    }
}
