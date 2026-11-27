//! The self update (FlClash: lib/providers/actions/common.dart): `checkUpdateResultHandle`,
//! `testUpdateDownload`, `_downloadUpdate`, `cancelUpdateDownload`, `discardUpdate` and
//! `installUpdate`, with the download's state shown on the live status deck.

use crate::live_status::Status;
use crate::settings_app::Effect;
use crate::update::{self, Failure, Version};
use crate::{profile, App};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// `_releasesPerPage` and `_releasePages`.
const PER_PAGE: u32 = 100;
const PAGES: u32 = 5;
/// The installer waits on the user's confirmation.
const INSTALL: Duration = Duration::from_secs(600);

#[derive(Default)]
pub(crate) struct Download {
    cancel: Option<Arc<AtomicBool>>,
    ready: Option<(String, PathBuf)>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent
        .get(url)
        .header("User-Agent", profile::USER_AGENT)
        .call()
        .map_err(|e| e.to_string())
}

/// `newestOfAllReleases`, falling back to the release feed once the API cannot answer.
fn newest_release() -> Result<(String, String), String> {
    match newest_listed() {
        Err(Listing::Unanswered(e)) => newest_in_feed().map_err(|f| format!("{e}；{f}")),
        Err(Listing::Missing(e)) => Err(e),
        Ok(found) => Ok(found),
    }
}

enum Listing {
    /// The request failed or was refused (`DioException`).
    Unanswered(String),
    /// The list answered but holds no release of this line.
    Missing(String),
}

fn newest_in_feed() -> Result<(String, String), String> {
    let agent = agent();
    let url = format!("https://github.com/{}/releases.atom", update::REPOSITORY);
    let mut response = get(&agent, &url).map_err(|e| format!("检查更新失败：{e}"))?;
    if response.status() != 200 {
        return Err(format!("检查更新失败：HTTP {}", response.status().as_u16()));
    }
    let feed = response
        .body_mut()
        .with_config()
        .limit(4 << 20)
        .read_to_string()
        .map_err(|e| format!("检查更新失败：{e}"))?;
    let head: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into();
    for tag in update::feed_candidates(&update::release_tags_of_feed(&feed)) {
        let sums = update::asset_url(&tag, &update::checksums_name(&tag));
        let status = head
            .head(&sums)
            .header("User-Agent", profile::USER_AGENT)
            .call()
            .map_err(|e| format!("检查更新失败：{e}"))?
            .status()
            .as_u16();
        match status {
            404 => continue,
            s if s < 400 => return Ok((tag, String::new())),
            s => return Err(format!("检查更新失败：HTTP {s}")),
        }
    }
    Err("检查更新失败：没有可用的版本".into())
}

fn newest_listed() -> Result<(String, String), Listing> {
    let agent = agent();
    for page in 1..=PAGES {
        let url = format!(
            "https://api.github.com/repos/{}/releases?per_page={PER_PAGE}&page={page}",
            update::REPOSITORY
        );
        let mut response =
            get(&agent, &url).map_err(|e| Listing::Unanswered(format!("检查更新失败：{e}")))?;
        if response.status() != 200 {
            return Err(Listing::Unanswered(format!(
                "检查更新失败：HTTP {}",
                response.status().as_u16()
            )));
        }
        let list: Value = response
            .body_mut()
            .with_config()
            .limit(16 << 20)
            .read_to_string()
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .ok_or_else(|| Listing::Unanswered("检查更新失败".to_string()))?;
        if let Some(tag) = update::newest_of(&list) {
            let notes = list
                .as_array()
                .into_iter()
                .flatten()
                .find(|r| r.get("tag_name").and_then(Value::as_str) == Some(tag.as_str()))
                .and_then(|r| r.get("body")?.as_str())
                .unwrap_or("")
                .to_string();
            return Ok((tag, notes));
        }
        if list.as_array().map_or(0, Vec::len) < PER_PAGE as usize {
            break;
        }
    }
    Err(Listing::Missing("检查更新失败：没有可用的版本".into()))
}

/// `downloadVerifiedRelease`: the checksums first, then the file, checked against them.
fn download_verified(
    tag: &str,
    name: &str,
    dir: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, Failure> {
    let failed = |e: String| Failure::Failed(e);
    let agent = agent();
    let mut sums = get(
        &agent,
        &update::asset_url(tag, &update::checksums_name(tag)),
    )
    .map_err(failed)?;
    match sums.status().as_u16() {
        404 => return Err(Failure::Missing),
        200 => {}
        code => return Err(Failure::Failed(format!("HTTP {code}"))),
    }
    let sums = sums
        .body_mut()
        .read_to_string()
        .map_err(|e| failed(e.to_string()))?;
    let expected = update::checksum(&sums, name).ok_or(Failure::Missing)?;
    if cancel.load(Ordering::SeqCst) {
        return Err(Failure::Cancelled);
    }
    let response = get(&agent, &update::asset_url(tag, name)).map_err(failed)?;
    match response.status().as_u16() {
        404 => return Err(Failure::Missing),
        200 => {}
        code => return Err(Failure::Failed(format!("HTTP {code}"))),
    }
    let total = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok()?.parse::<u64>().ok())
        .unwrap_or(0);
    let path = dir.join(name);
    let mut file = std::fs::File::create(&path).map_err(|e| failed(e.to_string()))?;
    let mut reader = response.into_body().into_reader();
    let mut buf = vec![0u8; 1 << 16];
    let (mut received, mut shown) = (0u64, 0u64);
    let step = update::progress_step(total);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(Failure::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| failed(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| failed(e.to_string()))?;
        received += n as u64;
        if received - shown >= step || received == total {
            shown = received;
            progress(received, total);
        }
    }
    file.sync_all().map_err(|e| failed(e.to_string()))?;
    drop(file);
    let bytes = std::fs::read(&path).map_err(|e| failed(e.to_string()))?;
    if update::hex(&crate::mitm::sha256(&bytes)) != expected {
        return Err(Failure::NotVerified);
    }
    Ok(path)
}

impl App {
    fn update_dir(&self) -> PathBuf {
        self.store.lock().unwrap().dir.join("update")
    }

    /// `clearUpdateDownloads`.
    pub(crate) fn clear_update_downloads(&self) {
        let _ = std::fs::remove_dir_all(self.update_dir());
    }

    fn set_update(self: &Arc<Self>, status: Option<Status>) {
        self.live.lock().unwrap().update = status;
        self.publish_live();
    }

    /// `checkForUpdate` then `checkUpdateResultHandle`.
    pub(crate) fn check_update(self: &Arc<Self>, by_user: bool) {
        self.ui(|ui| ui.set_update_checking(true));
        self.spawn(move |app| {
            let installed = app.app_version.lock().unwrap().clone();
            let newest = newest_release();
            app.ui(|ui| ui.set_update_checking(false));
            match newest {
                Ok((tag, notes)) => {
                    let installed = Version::parse(&installed)
                        .unwrap_or_else(|| Version::parse("0.0.0").unwrap());
                    let newer = Version::parse(&tag).is_some_and(|v| v > installed);
                    if newer {
                        app.offer_update(&tag, &notes, by_user);
                    } else if by_user {
                        app.alert("检查更新", "当前应用已经是最新版了");
                    }
                }
                Err(e) if by_user => app.toast(e),
                Err(e) => app.log_warning(format!("checkForUpdate failed: {e}")),
            }
        });
    }

    fn offer_update(self: &Arc<Self>, tag: &str, notes: &str, by_user: bool) {
        let o = update::offer(tag, notes, update::self_update_arch().is_some(), by_user);
        let arg = format!("{}{tag}", if by_user { "user:" } else { "auto:" });
        self.confirm_labeled(
            "发现新版本",
            o.text,
            "update-offer",
            arg,
            (o.yes, o.no, false),
        );
    }

    pub(crate) fn update_confirmed(self: &Arc<Self>, action: &str, arg: &str, yes: bool) {
        match (action, yes) {
            ("update-offer", _) => {
                let (by_user, tag) = match arg.split_once(':') {
                    Some((who, tag)) => (who == "user", tag.to_string()),
                    None => return,
                };
                if yes {
                    match update::self_update_arch() {
                        Some(arch) => {
                            self.download_update(tag.clone(), update::asset_name(&tag, arch))
                        }
                        None => self.open_release_page(&tag),
                    }
                } else if !by_user {
                    self.update_settings(Effect::None, |s| s.auto_check_update = false);
                }
            }
            ("update-failed", true) => self.open_release_page(arg),
            _ => {}
        }
    }

    fn open_release_page(self: &Arc<Self>, tag: &str) {
        let url = update::release_page(tag);
        self.spawn(move |app| {
            if let Err(e) = app
                .bridge()
                .and_then(|b| b.shell("url.open", json!({"url": url}), crate::CALL))
            {
                app.toast(e);
            }
        });
    }

    /// `testUpdateDownload`: the newest release even when it is not newer.
    pub(crate) fn test_update_download(self: &Arc<Self>) {
        let Some(arch) = update::self_update_arch() else {
            return;
        };
        if self.live.lock().unwrap().update.is_some() {
            return;
        }
        self.ui(|ui| ui.set_update_checking(true));
        self.spawn(move |app| {
            let newest = newest_release();
            app.ui(|ui| ui.set_update_checking(false));
            match newest {
                Ok((tag, _)) => app.download_update(tag.clone(), update::asset_name(&tag, arch)),
                Err(e) => app.toast(e),
            }
        });
    }

    /// `_downloadUpdate`.
    fn download_update(self: &Arc<Self>, tag: String, name: String) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut live = self.live.lock().unwrap();
            if live.update.is_some() {
                return;
            }
            live.update = Some(Status::Downloading {
                tag: tag.clone(),
                received: 0,
                total: 0,
            });
        }
        *self.update.lock().unwrap() = Download {
            cancel: Some(cancel.clone()),
            ready: None,
        };
        self.publish_live();
        self.toast(update::STARTED);
        self.spawn(move |app| {
            let dir = app.update_dir();
            app.clear_update_downloads();
            let result = std::fs::create_dir_all(&dir)
                .map_err(|e| Failure::Failed(e.to_string()))
                .and_then(|_| {
                    download_verified(&tag, &name, &dir, &cancel, |received, total| {
                        app.set_update(Some(Status::Downloading {
                            tag: tag.clone(),
                            received: received as i64,
                            total: total as i64,
                        }));
                    })
                });
            {
                let mut d = app.update.lock().unwrap();
                if d.cancel.as_ref().is_some_and(|c| Arc::ptr_eq(c, &cancel)) {
                    d.cancel = None;
                }
            }
            match result {
                Ok(path) => {
                    app.update.lock().unwrap().ready = Some((tag.clone(), path));
                    app.set_update(Some(Status::Ready { tag }));
                    app.toast(update::READY);
                }
                Err(failure) => {
                    app.set_update(None);
                    app.clear_update_downloads();
                    match failure {
                        Failure::Cancelled => {}
                        Failure::Missing => {
                            app.open_release_page(&tag);
                            app.toast(update::FILE_MISSING);
                        }
                        ref other => {
                            if let Failure::Failed(e) = other {
                                app.log_warning(format!("Update download failed: {e}"));
                            }
                            app.show_update_failure(&tag, other.message().unwrap_or_default());
                        }
                    }
                }
            }
        });
    }

    /// `cancelUpdateDownload`.
    pub(crate) fn cancel_update_download(&self) {
        if let Some(c) = &self.update.lock().unwrap().cancel {
            c.store(true, Ordering::SeqCst);
        }
    }

    /// `discardUpdate`.
    pub(crate) fn discard_update(self: &Arc<Self>) {
        if self.update.lock().unwrap().ready.take().is_none() {
            return;
        }
        self.set_update(None);
        self.clear_update_downloads();
    }

    /// `installUpdate` on Android: the shell commits a PackageInstaller session.
    pub(crate) fn install_update(self: &Arc<Self>) {
        let Some((tag, path)) = self.update.lock().unwrap().ready.take() else {
            return;
        };
        self.set_update(None);
        self.spawn(move |app| {
            let status = app.bridge().and_then(|b| {
                b.shell(
                    "update.install",
                    json!({"path": path.to_string_lossy()}),
                    INSTALL,
                )
            });
            app.clear_update_downloads();
            let failed = match status {
                Ok(r) => r
                    .get("status")
                    .and_then(Value::as_i64)
                    .is_some_and(update::install_failed),
                Err(e) => {
                    app.log_warning(format!("Package install failed: {e}"));
                    true
                }
            };
            if failed {
                app.show_update_failure(&tag, update::INSTALL_FAILED);
            }
        });
    }

    fn show_update_failure(self: &Arc<Self>, tag: &str, message: &str) {
        self.confirm_labeled(
            update::FAILED,
            message.to_string(),
            "update-failed",
            tag.to_string(),
            (update::GO_DOWNLOAD, "", false),
        );
    }

    fn log_warning(self: &Arc<Self>, text: String) {
        self.push_log(&json!({
            "level": "warning",
            "time": 0,
            "text": text,
        }));
    }
}
