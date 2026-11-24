//! Settings and profiles, kept as one JSON file under the app's internal data directory.
//! Profile files live beside it in `ui/profiles/<id>.yaml`, inside the Core's home, since
//! `config.apply` only reads files there.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Index into `theme::ACCENTS`.
    pub accent: usize,
    /// 0 follow system, 1 light, 2 dark.
    pub theme_mode: i32,
    /// 0 rule, 1 global, 2 direct.
    pub mode: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub url: String,
    /// Unix seconds of the last successful download.
    pub updated: u64,
    /// The proxy picked in each group, handed to `config.apply` as `selection`.
    pub selection: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub active: Option<String>,
}

pub struct Store {
    pub dir: PathBuf,
    pub state: State,
}

pub fn mode_name(mode: i32) -> &'static str {
    match mode {
        1 => "global",
        2 => "direct",
        _ => "rule",
    }
}

impl Store {
    pub fn open(home: &Path) -> Store {
        let dir = home.join("ui");
        let _ = std::fs::create_dir_all(dir.join("profiles"));
        let state = std::fs::read(dir.join("state.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Store { dir, state }
    }

    pub fn save(&self) {
        let Ok(bytes) = serde_json::to_vec_pretty(&self.state) else {
            return;
        };
        let tmp = self.dir.join("state.json.part");
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, self.dir.join("state.json"));
        }
    }

    pub fn profile_path(&self, id: &str) -> PathBuf {
        self.dir.join("profiles").join(format!("{id}.yaml"))
    }

    pub fn temp_path(&self) -> PathBuf {
        self.dir.join("download.part.yaml")
    }

    pub fn active(&self) -> Option<&Profile> {
        let id = self.state.active.as_ref()?;
        self.state.profiles.iter().find(|p| &p.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut Profile> {
        self.state.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn remove(&mut self, id: &str) {
        self.state.profiles.retain(|p| p.id != id);
        let _ = std::fs::remove_file(self.profile_path(id));
        if self.state.active.as_deref() == Some(id) {
            self.state.active = self.state.profiles.first().map(|p| p.id.clone());
        }
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_survives_a_round_trip() {
        let home = std::env::temp_dir().join(format!("sc-store-{}", std::process::id()));
        let mut store = Store::open(&home);
        store.state.settings.accent = 3;
        store.state.profiles.push(Profile {
            id: "1".into(),
            name: "a".into(),
            ..Default::default()
        });
        store.state.active = Some("1".into());
        store.save();
        let again = Store::open(&home);
        assert_eq!(again.state, store.state);
        assert_eq!(again.active().unwrap().name, "a");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn removing_the_active_profile_moves_on() {
        let home = std::env::temp_dir().join(format!("sc-store-rm-{}", std::process::id()));
        let mut store = Store::open(&home);
        for id in ["1", "2"] {
            store.state.profiles.push(Profile {
                id: id.into(),
                ..Default::default()
            });
        }
        store.state.active = Some("1".into());
        store.remove("1");
        assert_eq!(store.state.active.as_deref(), Some("2"));
        let _ = std::fs::remove_dir_all(home);
    }
}
