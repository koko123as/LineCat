//! The MITM CA's trust store on `App`'s side: `MitmTrustState` and the page's install,
//! remove and regenerate flows, through the shell's `ca.*` ops.

use crate::ca_trust::{self, Installed, Removed, Reply, TrustCheck};
use crate::mitm::Certificate;
use crate::settings_app::Effect;
use crate::{App, CALL};
use serde_json::json;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

#[derive(Default)]
struct State {
    trust: Option<Result<TrustCheck, String>>,
    shown: bool,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// The root module and the user installer can both wait on a prompt.
const PROMPT: Duration = Duration::from_secs(600);

impl App {
    /// `mitmTrustStoreProvider`: only Android has a store to ask.
    pub(crate) fn can_trust(&self) -> bool {
        cfg!(target_os = "android") && self.bridge().is_ok()
    }

    pub(crate) fn trust(&self) -> Option<Result<TrustCheck, String>> {
        state().lock().unwrap().trust.clone()
    }

    pub(crate) fn trust_shown(&self, shown: bool) {
        state().lock().unwrap().shown = shown;
    }

    /// The user may come back from installing the certificate in Settings.
    pub(crate) fn trust_resumed(self: &Arc<Self>) {
        if state().lock().unwrap().shown {
            self.spawn(|app| app.refresh_trust());
        }
    }

    fn locate(&self, c: &Certificate) -> Result<TrustCheck, String> {
        let der = ca_trust::der_base64(&c.pem);
        self.bridge()
            .and_then(|b| b.shell("ca.locate", json!({ "der": der }), CALL))
            .map(|r| TrustCheck::of(&r))
    }

    /// `MitmTrustState._check`, recording the CA found trusted for the Core.
    pub(crate) fn refresh_trust(self: &Arc<Self>) {
        if !self.can_trust() {
            return;
        }
        let Some(c) = self.current_certificate() else {
            state().lock().unwrap().trust = None;
            return self.mitm_publish();
        };
        let check = self.locate(&c);
        let trusted = match &check {
            Ok(t) if t.ready() => Some(c.fingerprint.clone()),
            _ => None,
        };
        state().lock().unwrap().trust = Some(check);
        if self.mitm_settings().mitm.trusted_fingerprint != trusted {
            self.update_settings(Effect::Reapply, |s| s.mitm.trusted_fingerprint = trusted);
        }
        self.mitm_publish();
    }

    fn reply(self: &Arc<Self>, reply: Reply) {
        match reply {
            Reply::None => {}
            Reply::Alert(text) => self.alert("", text),
            Reply::TitledAlert(title, text) => self.alert(title, text),
            Reply::Toast(text) => self.toast(text),
            Reply::Settings { title, text, user } => self.confirm_labeled(
                &title,
                text,
                "mitm-ca-settings",
                user.to_string(),
                (ca_trust::OPEN_SETTINGS, "", false),
            ),
        }
    }

    /// The certificate row: install while it is installable, else its details.
    pub(crate) fn certificate_tapped(self: &Arc<Self>) {
        let Some(c) = self.current_certificate() else {
            return;
        };
        let trust = self.trust();
        if let Some(Ok(t)) = trust {
            if t.installable() {
                return self.spawn(|app| app.confirm_install());
            }
        }
        let text = format!(
            "有效期至 {}\n\nSHA-256\n{}",
            crate::date(c.not_after / 1000),
            c.fingerprint
        );
        if matches!(trust, Some(Ok(t)) if t.ready()) {
            self.confirm_labeled(
                &c.subject,
                text,
                "mitm-ca-remove",
                String::new(),
                (ca_trust::REMOVE, "关闭", true),
            );
        } else {
            self.alert(c.subject, text);
        }
    }

    /// `_confirmAndroidInstall`.
    fn confirm_install(self: &Arc<Self>) {
        self.refresh_trust();
        let root = matches!(self.trust(), Some(Ok(t)) if t.root);
        let text = if root {
            ca_trust::INSTALL_SYSTEM_TIP
        } else {
            ca_trust::INSTALL_USER_TIP
        };
        self.confirm_labeled(
            ca_trust::INSTALL,
            text.into(),
            "mitm-ca-install",
            String::new(),
            (ca_trust::INSTALL, "", false),
        );
    }

    /// `AndroidMitmTrustStore.install`: su first, the user store when su is denied.
    fn install_outcome(self: &Arc<Self>, c: &Certificate) -> Result<Installed, String> {
        let bridge = self.bridge()?;
        let der = ca_trust::der_base64(&c.pem);
        if self.locate(c)?.root {
            let answer = bridge
                .shell("ca.installRoot", json!({ "der": der }), PROMPT)
                .map_err(|e| {
                    self.push_log(&json!({
                        "level": "warning",
                        "time": crate::now_secs() as i64 * 1000,
                        "text": format!("Installing the MITM certificate with root failed: {e}"),
                    }));
                    ca_trust::SYSTEM_STORE_FAILED.to_string()
                })?;
            if let Some(o) = Installed::of_root(answer["outcome"].as_str().unwrap_or("")) {
                return Ok(o);
            }
        }
        let answer = bridge.shell(
            "ca.install",
            json!({
                "der": der,
                "name": c.subject,
                "fileName": ca_trust::saved_file_name(&c.subject),
            }),
            PROMPT,
        )?;
        Installed::of_user(answer["outcome"].as_str().unwrap_or(""))
            .ok_or_else(|| ca_trust::SYSTEM_STORE_FAILED.to_string())
    }

    pub(crate) fn install_certificate(self: &Arc<Self>) {
        let Some(c) = self.current_certificate() else {
            return;
        };
        self.mitm_loading(ca_trust::INSTALL);
        let outcome = self.install_outcome(&c);
        if outcome.as_ref().map_or(true, Installed::rechecks) {
            self.refresh_trust();
        }
        self.mitm_loading("");
        match outcome {
            Ok(o) => {
                let after = match self.trust() {
                    Some(Ok(t)) => t,
                    _ => TrustCheck::default(),
                };
                self.reply(ca_trust::install_reply(o, &after, &c.subject));
            }
            Err(e) => self.toast(e),
        }
    }

    fn remove_outcome(&self, c: &Certificate) -> Removed {
        let Ok(found) = self.locate(c) else {
            return Removed::Failed;
        };
        ca_trust::remove_outcome(&found, || {
            let der = ca_trust::der_base64(&c.pem);
            self.bridge()
                .and_then(|b| b.shell("ca.removeRoot", json!({ "der": der }), PROMPT))
                .map(|r| r["removed"].as_bool() == Some(true))
                .unwrap_or(false)
        })
    }

    fn remove_certificate(self: &Arc<Self>) {
        let Some(c) = self.current_certificate() else {
            return;
        };
        self.mitm_loading(ca_trust::REMOVE);
        let outcome = self.remove_outcome(&c);
        self.refresh_trust();
        self.mitm_loading("");
        self.reply(ca_trust::remove_reply(outcome, &c.subject));
    }

    fn root_access(&self) -> bool {
        self.bridge()
            .and_then(|b| b.shell("ca.rootAccess", json!({}), CALL))
            .ok()
            .and_then(|r| r["granted"].as_bool())
            .unwrap_or(false)
    }

    /// `MitmTrustState.regenerate` and `_regenerate`: the old CA leaves the store first.
    pub(crate) fn regenerate_certificate(self: &Arc<Self>) {
        let old = self.current_certificate();
        let check = match self.trust() {
            Some(Ok(t)) if self.can_trust() => Some(t),
            _ => None,
        };
        let mut removed = true;
        if let (Some(old), Some(t)) = (&old, check) {
            if t.ready() {
                self.mitm_loading("重新生成证书");
                removed = self.remove_outcome(old) == Removed::Removed;
            }
        }
        if !self.generate_certificate("重新生成证书") {
            return;
        }
        self.refresh_trust();
        let system = check.is_some_and(|t| t.system);
        if !removed {
            let subject = old.map(|c| c.subject).unwrap_or_default();
            let root = system && self.root_access();
            return self.reply(ca_trust::old_left_reply(system, root, &subject));
        }
        if system {
            self.install_certificate();
        }
    }

    pub(crate) fn trust_confirmed(self: &Arc<Self>, action: &str, arg: &str, yes: bool) -> bool {
        match action {
            "mitm-ca-install" if yes => self.spawn(|app| app.install_certificate()),
            "mitm-ca-remove" if yes => self.spawn(|app| app.remove_certificate()),
            "mitm-ca-settings" if yes => {
                let user = arg == "true";
                self.spawn(move |app| {
                    if let Ok(b) = app.bridge() {
                        let _ = b.shell("ca.settings", json!({ "userCredentials": user }), CALL);
                    }
                });
            }
            "mitm-ca-install" | "mitm-ca-remove" | "mitm-ca-settings" => {}
            _ => return false,
        }
        true
    }
}
