//! 导入 on the custom profile page (`_handleImportFromUrl`, `_handleImportFromFile`,
//! `readCustomImport` and `importCustomProfile`), on `App`'s side: the source is read by
//! the Core's `config.read`, mapped by `custom_import`, and replaces what the profile holds
//! once agreed to.

use crate::custom_import::{self as ci, AppSide, Import};
use crate::custom_profile::ProxyDialer;
use crate::{App, PendingKey, CALL};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) const URL_INPUT: &str = "custom-import-url";
const TITLE: &str = "导入";

/// The import read and waiting on 替换, with the profile it goes into.
static PENDING: Mutex<Option<(String, Import)>> = Mutex::new(None);

fn new_ids() -> impl FnMut() -> u64 {
    let mut next = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or_default();
    move || {
        next += 1;
        next
    }
}

impl App {
    fn import_path(&self) -> PathBuf {
        self.store.lock().unwrap().dir.join("custom-import.yaml")
    }

    fn set_importing(self: &Arc<Self>, on: bool) {
        self.ui(move |ui| ui.set_custom_importing(on));
    }

    pub(crate) fn custom_import_url_check(&self, value: &str) -> Option<String> {
        let url = value.trim();
        if url.is_empty() {
            Some("URL不能为空".into())
        } else if !crate::profile::is_url(url) {
            Some("URL必须为URL".into())
        } else {
            None
        }
    }

    pub(crate) fn custom_import_from_url(self: &Arc<Self>, profile_id: String, url: String) {
        self.set_importing(true);
        self.spawn(move |app| {
            let check = app
                .store
                .lock()
                .unwrap()
                .state
                .settings
                .general
                .check_certificate;
            let path = app.import_path();
            let staged = crate::profile::download(url.trim(), check)
                .and_then(|d| std::fs::write(&path, d.bytes).map_err(|e| e.to_string()));
            match staged {
                Ok(()) => app.custom_import_staged(profile_id),
                Err(e) => app.import_failed(e),
            }
        });
    }

    pub(crate) fn custom_import_from_file(self: &Arc<Self>, profile_id: String) {
        self.spawn(move |app| {
            let path = app.import_path();
            let picked = app.bridge().and_then(|b| {
                b.shell(
                    "file.open",
                    json!({"dest": path.to_string_lossy()}),
                    Duration::from_secs(3600),
                )
            });
            match picked {
                Ok(r) if r.get("cancelled").and_then(Value::as_bool) == Some(true) => {}
                Ok(_) => {
                    app.set_importing(true);
                    app.custom_import_staged(profile_id);
                }
                Err(e) => app.toast(e),
            }
        });
    }

    fn import_failed(self: &Arc<Self>, e: String) {
        let _ = std::fs::remove_file(self.import_path());
        self.set_importing(false);
        self.alert(TITLE, e);
    }

    /// An age-encrypted source asks for its key, as `_withAgeSecretKey` does for profiles.
    fn custom_import_staged(self: &Arc<Self>, profile_id: String) {
        let path = self.import_path();
        let encrypted = std::fs::read(&path)
            .map(|b| b.starts_with(crate::profile::AGE_ARMOR_HEADER))
            .unwrap_or(false);
        if encrypted {
            *self.pending_key.lock().unwrap() = Some(PendingKey::CustomImport { profile_id });
            self.ui(|ui| ui.set_age_open(true));
            return;
        }
        self.custom_import_read(profile_id);
    }

    pub(crate) fn custom_import_key(self: &Arc<Self>, profile_id: String, key: String) {
        if key.is_empty() {
            let _ = std::fs::remove_file(self.import_path());
            self.set_importing(false);
            self.toast("该配置已用 age 加密，需要 age 私钥才能解密");
            return;
        }
        let path = self.import_path();
        let plain = self.bridge().and_then(|b| {
            b.call(
                "config.decrypt",
                json!({"profile": crate::profile_ref(&path), "key": key}),
                CALL,
            )
        });
        let written = plain.and_then(|r| {
            let text = r.get("text").and_then(Value::as_str).unwrap_or("");
            std::fs::write(&path, text).map_err(|e| e.to_string())
        });
        match written {
            Ok(()) => self.custom_import_read(profile_id),
            Err(e) => self.import_failed(e),
        }
    }

    fn import_app_side(&self, profile_id: &str) -> AppSide {
        let store = self.store.lock().unwrap();
        let s = &store.state;
        AppSide {
            profile_labels: s
                .profiles
                .iter()
                .flat_map(|p| [p.name.clone(), p.label()])
                .collect(),
            profile_sources: s
                .profiles
                .iter()
                .filter(|p| p.is_url())
                .map(|p| ((p.url.clone(), p.age_key.clone()), p.label()))
                .collect(),
            rule_set_labels: s
                .settings
                .rule_sets
                .iter()
                .map(|r| r.label.clone())
                .collect(),
            rule_set_sources: s
                .settings
                .rule_sets
                .iter()
                .filter(|r| r.is_remote())
                .map(|r| ((r.url.clone(), String::new()), r.label.clone()))
                .collect(),
            proxies: s
                .settings
                .custom_proxies
                .iter()
                .map(|p| p.definition.clone())
                .collect(),
            group_names: s
                .proxy_groups
                .iter()
                .filter(|g| g.profile_id != profile_id)
                .map(|g| g.name().to_string())
                .collect(),
        }
    }

    /// `readCustomImport`, then `_import`'s replace confirm when the profile holds anything.
    fn custom_import_read(self: &Arc<Self>, profile_id: String) {
        let path = self.import_path();
        let read = self.bridge().and_then(|b| {
            b.call(
                "config.read",
                json!({"profile": crate::profile_ref(&path)}),
                CALL,
            )
        });
        let _ = std::fs::remove_file(&path);
        let document = match read {
            Ok(r) => r.get("config").cloned().unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };
        let import = match ci::read(&document, &self.import_app_side(&profile_id)) {
            Ok(import) => import,
            Err(e) => return self.import_failed(e.message()),
        };
        let empty = {
            let store = self.store.lock().unwrap();
            crate::custom_profile::is_empty(&store.state, &profile_id)
        };
        *PENDING.lock().unwrap() = Some((profile_id, import));
        if empty {
            self.custom_import_replace(true);
        } else {
            self.confirm(
                "提示",
                "导入会替换此配置现有的策略组、规则和设置".into(),
                "ci-replace",
                String::new(),
            );
        }
    }

    pub(crate) fn custom_import_replace(self: &Arc<Self>, yes: bool) {
        let pending = PENDING.lock().unwrap().take();
        let Some((profile_id, import)) = pending.filter(|_| yes) else {
            self.set_importing(false);
            return;
        };
        let dropped = import.dropped_sub_rules;
        let loads = self.import_into(&profile_id, import);
        self.publish_custom();
        self.publish_settings();
        self.refresh_profiles();
        if dropped > 0 {
            self.alert(TITLE, ci::dropped_tip(dropped));
        }
        self.spawn(move |app| {
            for load in loads {
                load(&app);
            }
            app.set_importing(false);
            app.publish_custom();
        });
    }

    /// `importCustomProfile`: the providers are kept before the groups and rules naming
    /// them; what loads them is handed back to run afterwards.
    #[allow(clippy::type_complexity)]
    fn import_into(
        self: &Arc<Self>,
        profile_id: &str,
        import: Import,
    ) -> Vec<Box<dyn FnOnce(&Arc<App>) + Send>> {
        let mut loads: Vec<Box<dyn FnOnce(&Arc<App>) + Send>> = Vec::new();
        for p in import.proxy_providers {
            let id = self.new_id();
            let profile = crate::store::Profile {
                id: id.clone(),
                name: p.label.clone(),
                url: p.url.clone(),
                kind: if p.url.is_empty() { "file" } else { "url" }.into(),
                auto_update: !p.url.is_empty(),
                age_key: p.age_key.clone(),
                ..Default::default()
            };
            self.put_new(profile);
            loads.push(Box::new(move |app: &Arc<App>| {
                let result = if p.url.is_empty() {
                    let tmp = app.temp_for(&id);
                    std::fs::write(&tmp, p.content.as_bytes())
                        .map_err(|e| e.to_string())
                        .and_then(|_| app.save_file(&tmp, &id, "").map_err(crate::Ingest::text))
                        .map(|_| app.stamp_profile(&id))
                } else {
                    app.update_one(&id)
                };
                if let Err(e) = result {
                    app.toast(format!("{}：{e}", p.label));
                }
            }));
        }
        for p in import.rule_providers {
            loads.push(Box::new(move |app: &Arc<App>| {
                let content = p.url.is_empty().then(|| p.content.into_bytes());
                app.import_rule_set(&p.label, &p.url, content);
            }));
        }
        let mut next = new_ids();
        {
            let mut store = self.store.lock().unwrap();
            let s = &mut store.state;
            let mut proxy_ids = std::collections::HashMap::new();
            for d in import.proxies {
                let proxy = crate::custom_proxies::CustomProxy::new(next().to_string(), d);
                s.settings.custom_proxies.push(proxy);
            }
            for p in &s.settings.custom_proxies {
                proxy_ids.insert(p.name(), p.id.clone());
            }
            s.proxy_groups.retain(|g| g.profile_id != profile_id);
            let mut order = Vec::new();
            for def in import.groups {
                let id = next();
                order.push(id);
                s.proxy_groups.push(crate::custom_profile::ProxyGroupRow {
                    id,
                    profile_id: profile_id.into(),
                    definition: def,
                    order: None,
                });
            }
            crate::proxy_groups::renumber(&mut s.proxy_groups, profile_id, &order);
            let rules = import
                .rules
                .into_iter()
                .map(|mut r| {
                    r.id = next();
                    r
                })
                .collect();
            crate::profile_rules::set_all(s, profile_id, rules);
            s.proxy_dialers.retain(|d| d.profile_id != profile_id);
            for (name, target) in import.dialers {
                if let Some(proxy_id) = proxy_ids.get(&name) {
                    s.proxy_dialers.push(ProxyDialer {
                        profile_id: profile_id.into(),
                        proxy_id: proxy_id.clone(),
                        target,
                    });
                }
            }
            if let Some(p) = store.profile_mut(profile_id) {
                p.overrides = import.overrides;
            }
            store.save();
        }
        loads
    }

    fn stamp_profile(&self, id: &str) {
        let mut store = self.store.lock().unwrap();
        if let Some(p) = store.profile_mut(id) {
            p.updated = crate::now_secs();
        }
        store.save();
    }
}
