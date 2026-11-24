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
    pub proxies: ProxiesStyle,
    /// The dashboard's cards in order; none kept means the defaults.
    pub dashboard: Option<Vec<String>>,
}

/// `ProxiesStyleProps` in lib/models/config.dart.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProxiesStyle {
    /// 0 tab, 1 list.
    pub kind: i32,
    /// 0 default, 1 delay, 2 name.
    pub sort: i32,
    /// 0 tight, 1 standard, 2 loose.
    pub layout: i32,
    /// 0 compact, 1 standard, 2 loose.
    pub card: i32,
    /// 0 filled, 1 plain, 2 hidden.
    pub icon: i32,
    pub hide_timeout: bool,
    /// The groups unfolded in the list style (`unfoldSet`).
    pub unfold: Vec<String>,
}

impl Default for ProxiesStyle {
    fn default() -> Self {
        ProxiesStyle { kind: 1, sort: 0, layout: 1, card: 1, icon: 0, hide_timeout: false, unfold: Vec::new() }
    }
}

/// `SubscriptionInfo` in lib/models/profile.dart, from the `subscription-userinfo` header.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Subscription {
    pub upload: i64,
    pub download: i64,
    pub total: i64,
    /// Unix seconds, 0 for none.
    pub expire: i64,
}

impl Subscription {
    /// `SubscriptionInfo.formHString`.
    pub fn parse(header: &str) -> Subscription {
        let mut sub = Subscription::default();
        for field in header.split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim();
            let Some(n) = value.parse::<i64>().ok().or_else(|| {
                value
                    .parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite())
                    .map(|f| f.trunc() as i64)
            }) else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "upload" => sub.upload = n,
                "download" => sub.download = n,
                "total" => sub.total = n,
                "expire" => sub.expire = n,
                _ => {}
            }
        }
        sub
    }
}

/// `defaultUpdateDuration` in lib/common/constant.dart: one day.
pub const DEFAULT_UPDATE_MINUTES: u64 = 24 * 60;

fn yes() -> bool {
    true
}

fn default_interval() -> u64 {
    DEFAULT_UPDATE_MINUTES
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    /// The label as the user or the download named it; empty falls back to `default_label`.
    pub name: String,
    pub url: String,
    /// "url" or "file" (`ProfileType`); empty in files written before it existed, read as
    /// "url" when there is a url.
    pub kind: String,
    /// Unix seconds of the last successful download or save.
    pub updated: u64,
    #[serde(default = "yes")]
    pub auto_update: bool,
    #[serde(default = "default_interval")]
    pub interval_minutes: u64,
    pub age_key: String,
    pub subscription: Option<Subscription>,
    /// The proxy picked in each group, handed to `config.apply` as `selection`.
    pub selection: BTreeMap<String, String>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            id: String::new(),
            name: String::new(),
            url: String::new(),
            kind: String::new(),
            updated: 0,
            auto_update: true,
            interval_minutes: DEFAULT_UPDATE_MINUTES,
            age_key: String::new(),
            subscription: None,
            selection: BTreeMap::new(),
        }
    }
}

impl Profile {
    pub fn is_url(&self) -> bool {
        self.kind == "url" || (self.kind.is_empty() && !self.url.is_empty())
    }

    /// `realLabel`: the label, else the url's host, else the id.
    pub fn label(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        host_of(&self.url).unwrap_or_else(|| self.id.clone())
    }

    /// `exportFileName`.
    pub fn export_name(&self) -> String {
        let label = self.label();
        let lower = label.to_lowercase();
        if lower.ends_with(".yaml") || lower.ends_with(".yml") {
            label
        } else {
            format!("{label}.yaml")
        }
    }

    /// `realAutoUpdate` and the check in `autoUpdateProfiles`.
    pub fn due(&self, now: u64) -> bool {
        self.is_url() && self.auto_update && self.updated + self.interval_minutes.max(1) * 60 <= now
    }
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]"))?
    } else {
        host.split(':').next()?.to_string()
    };
    (!host.is_empty()).then_some(host)
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
        assert!(again.state.profiles[0].auto_update);
        assert_eq!(again.state, store.state);
        assert_eq!(again.active().unwrap().name, "a");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn subscription_header_reads_like_flclash() {
        let s = Subscription::parse("upload=10; download=20.7;total=100; expire=1700000000; x=y");
        assert_eq!(
            s,
            Subscription {
                upload: 10,
                download: 20,
                total: 100,
                expire: 1700000000
            }
        );
    }

    #[test]
    fn older_files_read_as_url_profiles_with_auto_update() {
        let p: Profile =
            serde_json::from_str(r#"{"id":"1","url":"https://a.example.com:8443/x"}"#).unwrap();
        assert!(p.is_url() && p.auto_update);
        assert_eq!(p.interval_minutes, DEFAULT_UPDATE_MINUTES);
        assert_eq!(p.label(), "a.example.com");
        assert_eq!(p.export_name(), "a.example.com.yaml");
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
