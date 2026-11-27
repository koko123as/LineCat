//! Whether this Android device trusts the MITM CA, and the replies to installing or
//! removing it (lib/common/mitm_trust.dart `AndroidMitmTrustStore`, the dialogs of
//! lib/views/config/mitm.dart).

use serde_json::Value;

pub const READY: &str = "已信任";
pub const NOT_INSTALLED: &str = "未安装";
pub const CHECKING: &str = "正在检查…";
pub const INSTALL: &str = "安装到系统";
pub const INSTALLED: &str = "证书已安装";
pub const REMOVE: &str = "移除证书";
pub const REMOVED: &str = "已移除证书";
pub const OPEN_SETTINGS: &str = "打开设置";
pub const ANDROID_READY_TIP: &str = "只有信任用户证书的应用（多为浏览器）会被解密。";
pub const SYSTEM_STORE_FAILED: &str = "无法把证书加入系统信任的根证书";
pub const ROOT_UNTIL_REBOOT: &str =
    "未找到可保留证书的 Magisk、KernelSU 或 APatch，重启手机后需要重新安装。";
pub const ROOT_PENDING_REBOOT: &str = "证书将在重启手机后生效。";
pub const INSTALL_SYSTEM_TIP: &str = "检测到 root，证书将以 Magisk、KernelSU 或 APatch 模块“SeasonCat MITM CA”加入系统信任的根证书。之后本机所有应用都会信任它，持有其私钥的人可以解密本机的 HTTPS 流量。如需撤销，可重新生成证书，或在 root 管理器中移除该模块并重启。若拒绝 root 授权，将改为安装用户证书。";
pub const INSTALL_USER_TIP: &str = "未检测到 root，证书只能安装为用户证书。只有信任用户证书的应用（多为浏览器）会被解密，其他应用访问列出的主机会失败，请不要列出它们的主机。Android 11 起需在系统设置中手动完成安装。";
pub const SYSTEM_STORE_REMOVE_FAILED: &str = "无法把证书从系统信任的根证书中移除";
pub const REMOVE_IN_SETTINGS: &str = "在系统设置中移除";
pub const CANNOT_REMOVE: &str = "无法移除";
pub const REMOVE_SYSTEM_TIP: &str = "证书由 root 模块“SeasonCat MITM CA”加入系统。请在 root 管理器中允许 SeasonCat 使用 root 后再试，或在其中移除该模块并重启手机。";

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrustCheck {
    pub user: bool,
    pub system: bool,
    pub root: bool,
}

impl TrustCheck {
    pub fn of(answer: &Value) -> Self {
        let flag = |k: &str| answer.get(k).and_then(Value::as_bool) == Some(true);
        TrustCheck {
            user: flag("user"),
            system: flag("system"),
            root: flag("root"),
        }
    }

    pub fn ready(&self) -> bool {
        self.user || self.system
    }

    /// Tapping the row installs: not trusted yet, or only as a user CA while su could
    /// put it in the system store.
    pub fn installable(&self) -> bool {
        !self.ready() || (!self.system && self.root)
    }
}

/// The base64 body of a PEM block, which is the DER the platform takes.
pub fn der_base64(pem: &str) -> String {
    pem.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("-----"))
        .collect()
}

/// `mitmSavedFileName`: a renewed CA saved beside the old one is told apart.
pub fn saved_file_name(subject: &str) -> String {
    format!("{subject}.crt")
}

pub fn label(trust: &Option<Result<TrustCheck, String>>) -> String {
    match trust {
        Some(Ok(c)) if c.ready() => READY.into(),
        Some(Ok(_)) => NOT_INSTALLED.into(),
        Some(Err(e)) => e.clone(),
        None => CHECKING.into(),
    }
}

pub fn footer(trust: &Option<Result<TrustCheck, String>>) -> &'static str {
    match trust {
        Some(Ok(c)) if c.system => "",
        _ => ANDROID_READY_TIP,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Installed {
    Installed,
    UntilReboot,
    Prompted,
    Saved,
}

impl Installed {
    /// `installCaCertificateWithRoot`'s answer; None when su was denied.
    pub fn of_root(outcome: &str) -> Option<Self> {
        match outcome {
            "installed" => Some(Installed::Installed),
            "untilReboot" => Some(Installed::UntilReboot),
            _ => None,
        }
    }

    /// `installCaCertificate`'s answer; None when no installer took it.
    pub fn of_user(outcome: &str) -> Option<Self> {
        match outcome {
            "prompted" => Some(Installed::Prompted),
            "saved" => Some(Installed::Saved),
            _ => None,
        }
    }

    pub fn rechecks(&self) -> bool {
        matches!(self, Installed::Installed | Installed::UntilReboot)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    None,
    Alert(String),
    TitledAlert(&'static str, String),
    Toast(String),
    Settings {
        title: String,
        text: String,
        user: bool,
    },
}

pub fn saved_tip(file: &str) -> String {
    format!(
        "已将 {file} 保存到“下载”。请在系统设置的“安全 › 加密与凭据 › 安装证书 › CA 证书”中安装。"
    )
}

/// `_install`'s switch, given the check made after installing.
pub fn install_reply(outcome: Installed, after: &TrustCheck, subject: &str) -> Reply {
    match outcome {
        Installed::Installed if !after.system => Reply::Alert(ROOT_PENDING_REBOOT.into()),
        Installed::UntilReboot => Reply::Alert(ROOT_UNTIL_REBOOT.into()),
        Installed::Installed if after.ready() => Reply::Toast(INSTALLED.into()),
        Installed::Saved => Reply::Settings {
            title: INSTALL.into(),
            text: saved_tip(&saved_file_name(subject)),
            user: false,
        },
        _ => Reply::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Removed {
    Removed,
    UserStore,
    NeedsRoot,
    Failed,
}

/// `AndroidMitmTrustStore.remove`: su takes a system CA out; an app cannot remove a
/// user CA.
pub fn remove_outcome(found: &TrustCheck, remove_root: impl FnOnce() -> bool) -> Removed {
    if found.system && (!found.root || !remove_root()) {
        return Removed::NeedsRoot;
    }
    if found.user {
        Removed::UserStore
    } else {
        Removed::Removed
    }
}

pub fn remove_reply(outcome: Removed, subject: &str) -> Reply {
    match outcome {
        Removed::Removed => Reply::Toast(REMOVED.into()),
        Removed::UserStore => Reply::Settings {
            title: REMOVE_IN_SETTINGS.into(),
            text: format!(
                "Android 不允许应用移除用户证书。请在“安全 › 加密与凭据 › 信任的凭据 › 用户”中移除 {subject}。"
            ),
            user: true,
        },
        Removed::NeedsRoot => Reply::TitledAlert(CANNOT_REMOVE, REMOVE_SYSTEM_TIP.into()),
        Removed::Failed => Reply::Toast(SYSTEM_STORE_REMOVE_FAILED.into()),
    }
}

/// `_regenerate` when the old CA stayed in a store.
pub fn old_left_reply(system: bool, root_access: bool, subject: &str) -> Reply {
    if system && !root_access {
        return Reply::Alert(format!(
            "旧证书 {subject} 仍在系统信任的证书中。请在 root 管理器中允许 SeasonCat 使用 root 后重新生成，或在其中移除“SeasonCat MITM CA”模块并重启。"
        ));
    }
    Reply::Settings {
        title: String::new(),
        text: format!(
            "请在系统设置的“安全 › 加密与凭据 › 信任的凭据 › 用户”中移除旧证书 {subject}，再安装新证书。"
        ),
        user: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checks_and_labels() {
        let user = TrustCheck::of(&json!({"user": true, "system": false, "root": true}));
        assert!(user.ready() && user.installable());
        let system = TrustCheck {
            system: true,
            root: true,
            ..Default::default()
        };
        assert!(!system.installable());
        assert!(TrustCheck::default().installable());
        assert_eq!(label(&Some(Ok(user))), READY);
        assert_eq!(label(&Some(Ok(TrustCheck::default()))), NOT_INSTALLED);
        assert_eq!(label(&None), CHECKING);
        assert_eq!(footer(&Some(Ok(system))), "");
        assert_eq!(footer(&Some(Ok(user))), ANDROID_READY_TIP);
    }

    #[test]
    fn der_is_the_pem_body() {
        let pem = "-----BEGIN CERTIFICATE-----\nMIIB\r\nAAEC\n-----END CERTIFICATE-----\n";
        assert_eq!(der_base64(pem), "MIIBAAEC");
    }

    #[test]
    fn install_replies() {
        let none = TrustCheck::default();
        let system = TrustCheck {
            system: true,
            ..Default::default()
        };
        assert_eq!(
            install_reply(Installed::Installed, &none, "CA"),
            Reply::Alert(ROOT_PENDING_REBOOT.into())
        );
        assert_eq!(
            install_reply(Installed::Installed, &system, "CA"),
            Reply::Toast(INSTALLED.into())
        );
        assert_eq!(install_reply(Installed::Prompted, &none, "CA"), Reply::None);
        assert!(matches!(
            install_reply(Installed::Saved, &none, "CA"),
            Reply::Settings { user: false, ref text, .. } if text.contains("CA.crt")
        ));
        assert_eq!(Installed::of_root("denied"), None);
        assert_eq!(Installed::of_user("saved"), Some(Installed::Saved));
    }

    #[test]
    fn removing() {
        let system = TrustCheck {
            system: true,
            root: true,
            ..Default::default()
        };
        assert_eq!(remove_outcome(&system, || false), Removed::NeedsRoot);
        assert_eq!(remove_outcome(&system, || true), Removed::Removed);
        let user = TrustCheck {
            user: true,
            ..Default::default()
        };
        assert_eq!(remove_outcome(&user, || unreachable!()), Removed::UserStore);
        assert_eq!(
            remove_reply(Removed::NeedsRoot, "CA"),
            Reply::TitledAlert("无法移除", REMOVE_SYSTEM_TIP.into())
        );
        assert!(matches!(old_left_reply(true, false, "CA"), Reply::Alert(_)));
        assert!(matches!(
            old_left_reply(false, false, "CA"),
            Reply::Settings { user: true, .. }
        ));
    }
}
