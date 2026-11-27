//! Profile scripts on `App`'s side. FlClash: lib/providers/actions/setup.dart,
//! lib/providers/state/profile.dart.

use crate::bridge::Bridge;
use crate::{script, App, CALL};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

impl App {
    pub(crate) fn profile_script(&self, profile_id: &str) -> Option<String> {
        let id = {
            let store = self.store.lock().unwrap();
            let p = store.state.profiles.iter().find(|p| p.id == profile_id)?;
            if p.is_custom() {
                return None;
            }
            let id = p.script_id.clone()?;
            store
                .state
                .settings
                .scripts
                .iter()
                .any(|s| s.id == id)
                .then_some(id)?
        };
        let content = std::fs::read_to_string(self.script_path(&id)).ok()?;
        (!content.trim().is_empty()).then_some(content)
    }

    /// The scripted mapping, written beside the profile for the Core; None without a script.
    pub(crate) fn scripted_profile(
        &self,
        bridge: &Bridge,
        profile_id: &str,
        path: &Path,
    ) -> Result<Option<(PathBuf, Value)>, String> {
        let Some(content) = self.profile_script(profile_id) else {
            return Ok(None);
        };
        let read = bridge.call(
            "config.read",
            json!({"profile": crate::profile_ref(path)}),
            CALL,
        )?;
        let config = read.get("config").cloned().unwrap_or(Value::Null);
        let scripted = script::handle_evaluate(&content, &config)?;
        let out = path.with_file_name(format!("{profile_id}.scripted.yaml"));
        std::fs::write(&out, serde_json::to_vec(&scripted).unwrap_or_default())
            .map_err(|e| e.to_string())?;
        Ok(Some((out, scripted)))
    }
}
