//! Backup and restore (lib/views/backup_and_restore.dart, lib/providers/actions/backup.dart,
//! lib/common/dav_client.dart): the settings and profiles packed into one zip, kept on the
//! device or on a WebDAV server.
//!
//! FlClash's archive carries its database; this app keeps its state in `ui/state.json`, so
//! the archive holds that file and the profiles beside it.

use crate::mitm;
use crate::store::{State, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// `sideBySideName`: the folder the backup goes in on the server.
pub const DAV_ROOT: &str = "SeasonCat";
/// `defaultDavFileName`.
pub const DAV_FILE_NAME: &str = "backup.zip";
const STATE_ENTRY: &str = "seasoncat.json";
const ARCHIVE_VERSION: i64 = 1;
const MAX_ARCHIVE_BYTES: u64 = 256 << 20;
const MODULES_DIR: &str = "modules";

/// `DAVProps`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Dav {
    pub uri: String,
    pub user: String,
    pub password: String,
    pub file_name: String,
}

impl Default for Dav {
    fn default() -> Self {
        Dav {
            uri: String::new(),
            user: String::new(),
            password: String::new(),
            file_name: DAV_FILE_NAME.into(),
        }
    }
}

/// `RestoreOption`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreOption {
    OnlyProfiles,
    All,
}

// ---------------------------------------------------------------- zip

/// A zip of the given entries, each deflated.
pub fn zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    use std::io::Write;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let mut enc =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        let _ = enc.write_all(data);
        let packed = enc.finish().unwrap_or_default();
        let offset = out.len() as u32;
        let head = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0x0800u16.to_le_bytes()); // UTF-8 names
            h.extend(8u16.to_le_bytes());
            h.extend(0u16.to_le_bytes());
            h.extend(0x21u16.to_le_bytes());
            h.extend(crc.sum().to_le_bytes());
            h.extend((packed.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central {
                h.extend([0u8; 10]);
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(head(0x0403_4b50, false));
        out.extend(&packed);
        central.extend(head(0x0201_4b50, true));
    }
    let at = out.len() as u32;
    let n = entries.len() as u16;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(n.to_le_bytes());
    out.extend(n.to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

fn u16_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}

/// The entries of a zip (stored or deflated); None when it is not one.
pub fn unzip(b: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    let end = (0..=b.len().checked_sub(22)?)
        .rev()
        .find(|&i| b[i..i + 4] == 0x0605_4b50u32.to_le_bytes())?;
    let count = u16_at(b, end + 10)?;
    let mut at = u32_at(b, end + 16)?;
    let mut out = Vec::new();
    for _ in 0..count {
        if u32_at(b, at)? != 0x0201_4b50 {
            return None;
        }
        let method = u16_at(b, at + 10)?;
        let crc = u32_at(b, at + 16)? as u32;
        let packed = u32_at(b, at + 20)?;
        let size = u32_at(b, at + 24)?;
        let (nl, el, cl) = (
            u16_at(b, at + 28)?,
            u16_at(b, at + 30)?,
            u16_at(b, at + 32)?,
        );
        let local = u32_at(b, at + 42)?;
        let name = String::from_utf8_lossy(b.get(at + 46..at + 46 + nl)?).into_owned();
        at += 46 + nl + el + cl;
        if name.ends_with('/') {
            continue;
        }
        let start = local + 30 + u16_at(b, local + 26)? + u16_at(b, local + 28)?;
        let raw = b.get(start..start + packed)?;
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut v = Vec::with_capacity(size);
                flate2::read::DeflateDecoder::new(raw)
                    .take(MAX_ARCHIVE_BYTES)
                    .read_to_end(&mut v)
                    .ok()?;
                v
            }
            _ => return None,
        };
        let mut sum = flate2::Crc::new();
        sum.update(&data);
        if sum.sum() != crc {
            return None;
        }
        out.push((name, data));
    }
    Some(out)
}

// ---------------------------------------------------------------- archive

#[derive(Serialize, Deserialize)]
struct Archive {
    version: i64,
    state: State,
}

/// `_createArchive`: the state, every profile's file, and each local module with its
/// attachments from `mitm_dir` (a remote module downloads again, `BackupFiles.of`).
pub fn create(store: &Store, mitm_dir: &Path) -> Result<Vec<u8>, String> {
    let archive = Archive {
        version: ARCHIVE_VERSION,
        state: store.state.clone(),
    };
    let mut entries = vec![(
        STATE_ENTRY.to_string(),
        serde_json::to_vec_pretty(&archive).map_err(|e| e.to_string())?,
    )];
    for p in &store.state.profiles {
        if let Ok(bytes) = std::fs::read(store.profile_path(&p.id)) {
            entries.push((format!("profiles/{}.yaml", p.id), bytes));
        }
    }
    let local = store
        .state
        .settings
        .mitm_modules
        .iter()
        .filter(|m| !m.is_remote());
    let mut attachments = BTreeSet::new();
    for m in local {
        let path = format!("{MODULES_DIR}/{}", m.file_name());
        if let Ok(bytes) = std::fs::read(mitm_dir.join(&path)) {
            entries.push((format!("mitm/{path}"), bytes));
        }
        attachments.extend(m.attachments().into_values());
    }
    for name in attachments
        .into_iter()
        .filter(|n| mitm::is_resource_name(n))
    {
        let path = format!("{}/{name}", mitm::RESOURCES_DIR);
        if let Ok(bytes) = std::fs::read(mitm_dir.join(&path)) {
            entries.push((format!("mitm/{path}"), bytes));
        }
    }
    Ok(zip(&entries))
}

/// What a restore brings back: the state it carried and its profile files by id.
pub struct Restored {
    pub state: State,
    pub files: Vec<(String, Vec<u8>)>,
    /// Module files and attachments by their path under the Core's `mitm` directory.
    pub mitm: Vec<(String, Vec<u8>)>,
}

/// `restoreTask`: the archive read back; the errors are `BackupFailure`'s messages.
pub fn read(bytes: &[u8]) -> Result<Restored, String> {
    let entries = unzip(bytes).ok_or("无效备份文件")?;
    let state = entries
        .iter()
        .find(|(n, _)| n == STATE_ENTRY)
        .ok_or("无效备份文件")?;
    let mut archive: Archive = serde_json::from_slice(&state.1).map_err(|_| "无效备份文件")?;
    archive.state.settings.migrate();
    if archive.version > ARCHIVE_VERSION {
        return Err("备份来自更新的版本".into());
    }
    let mitm = entries
        .iter()
        .filter_map(|(n, d)| {
            let (dir, name) = n.strip_prefix("mitm/")?.split_once('/')?;
            let known = dir == MODULES_DIR || dir == mitm::RESOURCES_DIR;
            (known && mitm::is_resource_name(name)).then(|| (format!("{dir}/{name}"), d.clone()))
        })
        .collect();
    let files = entries
        .into_iter()
        .filter_map(|(n, d)| {
            let id = n.strip_prefix("profiles/")?.strip_suffix(".yaml")?;
            (!id.is_empty() && !id.contains(['/', '\\', '.'])).then(|| (id.to_string(), d))
        })
        .collect();
    Ok(Restored {
        state: archive.state,
        files,
        mitm,
    })
}

/// `applyRestore`: the profiles join (compatible) or replace (override) the ones there are;
/// with `All` the settings come back too, keeping this device's WebDAV binding when the
/// backup has none and its app list when the backup's is empty.
pub fn apply(current: &mut State, restored: State, option: RestoreOption, override_all: bool) {
    let mut profiles = if override_all {
        Vec::new()
    } else {
        current.profiles.clone()
    };
    if override_all {
        current.proxy_groups.clear();
        current.profile_rules.clear();
        current.proxy_dialers.clear();
        current.disabled_rules.clear();
    }
    for p in &restored.profiles {
        crate::custom_profile::remove_profile(current, &p.id);
    }
    current.proxy_groups.extend(restored.proxy_groups);
    current.profile_rules.extend(restored.profile_rules);
    current.proxy_dialers.extend(restored.proxy_dialers);
    current.disabled_rules.extend(restored.disabled_rules);
    // The icon records ride along as FlClash's database does; the later use of a URL wins.
    for r in restored.icon_records {
        let newer = current
            .icon_records
            .iter()
            .find(|c| c.url == r.url)
            .is_none_or(|c| c.last_accessed_ms < r.last_accessed_ms);
        if newer {
            crate::icon_picker::put_record(&mut current.icon_records, &r.url, r.last_accessed_ms);
        }
    }
    for p in restored.profiles {
        match profiles.iter_mut().find(|q| q.id == p.id) {
            Some(q) => *q = p,
            None => profiles.push(p),
        }
    }
    current.profiles = profiles;
    if restored
        .active
        .as_ref()
        .is_some_and(|a| current.profiles.iter().any(|p| &p.id == a))
        || current.active.is_none()
    {
        current.active = restored.active.clone();
    }
    if current
        .active
        .as_ref()
        .is_some_and(|a| !current.profiles.iter().any(|p| &p.id == a))
    {
        current.active = current.profiles.first().map(|p| p.id.clone());
    }
    if option == RestoreOption::All {
        let mut settings = restored.settings;
        if settings.dav.is_none() {
            settings.dav = current.settings.dav.clone();
        }
        let ac = &settings.access_control;
        if ac.accept_list.is_empty() && ac.reject_list.is_empty() {
            let keep = &current.settings.access_control;
            settings.access_control.accept_list = keep.accept_list.clone();
            settings.access_control.reject_list = keep.reject_list.clone();
        }
        current.settings = settings;
    }
}

// ---------------------------------------------------------------- WebDAV

/// Standard base64, for the Basic authorization.
pub fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in input.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `addressTip`: an http(s) address.
pub fn valid_uri(uri: &str) -> bool {
    let rest = uri
        .strip_prefix("https://")
        .or_else(|| uri.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty() && !r.starts_with('/'))
}

/// `DAVClient`: `<uri>/SeasonCat/<fileName>`.
pub struct DavClient {
    dav: Dav,
    agent: ureq::Agent,
}

impl DavClient {
    pub fn new(dav: &Dav, check_certificate: bool) -> DavClient {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .disable_verification(!check_certificate)
                    .build(),
            )
            .build()
            .into();
        DavClient {
            dav: dav.clone(),
            agent,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.dav.uri.trim_end_matches('/'), path)
    }

    fn auth(&self) -> String {
        format!(
            "Basic {}",
            base64(format!("{}:{}", self.dav.user, self.dav.password).as_bytes())
        )
    }

    fn send(&self, method: &str, path: &str, body: Vec<u8>) -> Result<(u16, Vec<u8>), String> {
        let request = ureq::http::Request::builder()
            .method(method)
            .uri(self.url(path))
            .header("Authorization", self.auth())
            .body(body)
            .map_err(|e| e.to_string())?;
        let mut response = self.agent.run(request).map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_ARCHIVE_BYTES)
            .read_to_vec()
            .unwrap_or_default();
        Ok((status, bytes))
    }

    fn ok(&self, method: &str, path: &str, body: Vec<u8>) -> Result<Vec<u8>, String> {
        match self.send(method, path, body)? {
            (200..=299, bytes) => Ok(bytes),
            (code, _) => Err(format!("HTTP {code}")),
        }
    }

    /// `ping`: OPTIONS on the root answers.
    pub fn ping(&self) -> bool {
        self.ok("OPTIONS", "/", Vec::new()).is_ok()
    }

    fn file(&self) -> String {
        format!("/{DAV_ROOT}/{}", self.dav.file_name)
    }

    pub fn upload(&self, bytes: Vec<u8>) -> Result<(), String> {
        // The folder may be there already (405).
        let _ = self.send("MKCOL", &format!("/{DAV_ROOT}"), Vec::new());
        self.ok("PUT", &self.file(), bytes).map(|_| ())
    }

    pub fn download(&self) -> Result<Vec<u8>, String> {
        self.ok("GET", &self.file(), Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Profile, Settings};

    #[test]
    fn zip_round_trips() {
        let entries = vec![
            ("a.json".to_string(), b"{\"x\":1}".to_vec()),
            (
                "profiles/p1.yaml".to_string(),
                "代理: 1\n".repeat(300).into_bytes(),
            ),
            ("empty".to_string(), Vec::new()),
        ];
        let z = zip(&entries);
        assert_eq!(unzip(&z).unwrap(), entries);
        assert!(unzip(b"not a zip at all, not at all").is_none());
        let mut bad = z.clone();
        bad[40] ^= 0xff;
        assert!(unzip(&bad).is_none() || unzip(&bad).unwrap() != entries);
    }

    #[test]
    fn base64_and_uri() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"user:pass"), "dXNlcjpwYXNz");
        assert!(valid_uri("https://dav.example.com/remote.php"));
        assert!(valid_uri("http://10.0.0.2:5005"));
        assert!(!valid_uri("dav.example.com"));
        assert!(!valid_uri("https://"));
    }

    fn profile(id: &str, name: &str) -> Profile {
        Profile {
            id: id.into(),
            name: name.into(),
            ..Default::default()
        }
    }

    #[test]
    fn archive_reads_back_and_applies() {
        let dir = std::env::temp_dir().join(format!("sc-backup-{}", std::process::id()));
        let mut store = Store::open(&dir);
        store.state.profiles = vec![profile("p1", "one")];
        store.state.active = Some("p1".into());
        store.state.settings.general.mixed_port = 7999;
        std::fs::write(store.profile_path("p1"), b"proxies: []\n").unwrap();
        let mitm_dir = dir.join("mitm");
        std::fs::create_dir_all(mitm_dir.join("modules")).unwrap();
        std::fs::create_dir_all(mitm_dir.join("resources")).unwrap();
        let mut local = crate::mitm::MitmModule {
            id: "m1".into(),
            ..Default::default()
        };
        local.resources.insert("a.js".into(), "h.js".into());
        let remote = crate::mitm::MitmModule {
            id: "m2".into(),
            url: "https://x/m".into(),
            ..Default::default()
        };
        std::fs::write(mitm_dir.join("modules/m1.txt"), b"#!name=a").unwrap();
        std::fs::write(mitm_dir.join("modules/m2.txt"), b"#!name=b").unwrap();
        std::fs::write(mitm_dir.join("resources/h.js"), b"1").unwrap();
        store.state.settings.mitm_modules = vec![local, remote];
        let bytes = create(&store, &mitm_dir).unwrap();
        let restored = read(&bytes).unwrap();
        assert_eq!(
            restored.mitm,
            [
                ("modules/m1.txt".to_string(), b"#!name=a".to_vec()),
                ("resources/h.js".to_string(), b"1".to_vec())
            ]
        );
        assert_eq!(
            restored.files,
            [("p1".to_string(), b"proxies: []\n".to_vec())]
        );

        let mut here = State {
            profiles: vec![profile("p0", "zero"), profile("p1", "old")],
            active: Some("p0".into()),
            ..Default::default()
        };
        here.settings.dav = Some(Dav {
            uri: "https://d".into(),
            ..Default::default()
        });
        here.settings.access_control.reject_list = vec!["com.a".into()];
        apply(
            &mut here,
            restored.state.clone(),
            RestoreOption::OnlyProfiles,
            false,
        );
        assert_eq!(here.profiles.len(), 2);
        assert_eq!(here.profiles[1].name, "one");
        assert_eq!(here.active.as_deref(), Some("p1"));
        assert_eq!(
            here.settings.general.mixed_port,
            Settings::default().general.mixed_port
        );
        apply(&mut here, restored.state, RestoreOption::All, true);
        assert_eq!(here.profiles.len(), 1);
        assert_eq!(here.settings.general.mixed_port, 7999);
        assert_eq!(here.settings.dav.as_ref().unwrap().uri, "https://d");
        assert_eq!(here.settings.access_control.reject_list, ["com.a"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read(&zip(&[])).err().as_deref(), Some("无效备份文件"));
    }

    #[test]
    fn icon_records_restore_with_the_later_use() {
        use crate::icon_picker::IconRecord;
        let rec = |url: &str, ms: u64| IconRecord {
            url: url.into(),
            last_accessed_ms: ms,
        };
        let mut here = State {
            icon_records: vec![rec("a", 5), rec("b", 9)],
            ..Default::default()
        };
        let backup = State {
            icon_records: vec![rec("a", 7), rec("b", 1), rec("c", 3)],
            ..Default::default()
        };
        apply(&mut here, backup, RestoreOption::OnlyProfiles, false);
        let mut got: Vec<(String, u64)> = here
            .icon_records
            .into_iter()
            .map(|r| (r.url, r.last_accessed_ms))
            .collect();
        got.sort();
        assert_eq!(got, [("a".into(), 7), ("b".into(), 9), ("c".into(), 3)]);
    }

    #[test]
    fn restored_profiles_bring_their_groups() {
        use crate::custom_profile::ProxyGroupRow;
        let row = |id: u64, profile: &str| ProxyGroupRow {
            id,
            profile_id: profile.into(),
            ..Default::default()
        };
        let mut here = State {
            profiles: vec![profile("p0", "zero"), profile("p1", "old")],
            proxy_groups: vec![row(1, "p0"), row(2, "p1")],
            ..Default::default()
        };
        let restored = State {
            profiles: vec![profile("p1", "one")],
            proxy_groups: vec![row(3, "p1")],
            ..Default::default()
        };
        apply(
            &mut here,
            restored.clone(),
            RestoreOption::OnlyProfiles,
            false,
        );
        let ids: Vec<u64> = here.proxy_groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, [1, 3]);
        apply(&mut here, restored, RestoreOption::OnlyProfiles, true);
        let ids: Vec<u64> = here.proxy_groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, [3]);
    }
}
