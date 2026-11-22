//! TermF's own updates (ARCHITECTURE §16), through Tauri's updater and the GitHub releases: the newest release
//! of the channel that carries a signed `latest.json` is the one offered. Stable skips the pre-releases, beta
//! takes them too. Checked at startup at most once a day, never in the way; installed only when the user asks.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use termf_core::Channel;

use crate::{AppState, CmdResult, err};

/// Published releases only: GitHub hides the drafts from a request without a token.
const RELEASES: &str = "https://api.github.com/repos/vassilidev/TermF/releases?per_page=30";
const MANIFEST: &str = "latest.json";
const DAY: u64 = 24 * 60 * 60;

pub struct Updates {
    /// When TermF last looked, kept out of termf.toml, which is synced.
    file: PathBuf,
    found: Mutex<Option<Update>>,
    /// Why the last check failed.
    error: Mutex<Option<String>>,
}

impl Updates {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Updates {
            file: data_dir.join("updates.json"),
            found: Mutex::new(None),
            error: Mutex::new(None),
        }
    }

    fn checked(&self) -> Option<u64> {
        let text = fs::read_to_string(&self.file).ok()?;
        serde_json::from_str::<serde_json::Value>(&text).ok()?["checked"].as_u64()
    }

    fn set_checked(&self, at: u64) {
        let _ = fs::write(&self.file, serde_json::json!({ "checked": at }).to_string());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    version: String,
    channel: Channel,
    check: bool,
    /// Seconds since the epoch.
    checked: Option<u64>,
    available: Option<Available>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    version: String,
    notes: Option<String>,
    /// Seconds since the epoch.
    date: Option<i64>,
}

#[derive(Deserialize)]
pub struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    /// The release's notes as published, which may differ from those built into `latest.json`.
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
pub struct Asset {
    name: String,
    browser_download_url: String,
}

/// The `latest.json` of the newest release `channel` takes. A release without one (built without the signing
/// key) is skipped, as is a tag that is no version.
pub fn manifest(releases: &[Release], channel: Channel) -> Option<&str> {
    releases
        .iter()
        .filter(|r| !r.draft && (channel == Channel::Beta || !r.prerelease))
        .filter_map(|r| {
            let version =
                Version::parse(r.tag_name.strip_prefix('v').unwrap_or(&r.tag_name)).ok()?;
            let asset = r.assets.iter().find(|a| a.name == MANIFEST)?;
            Some((version, asset.browser_download_url.as_str()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, url)| url)
}

/// A day after the last check; a clock set back counts as due.
pub fn due(checked: Option<u64>, now: u64) -> bool {
    checked.is_none_or(|at| at > now || now - at >= DAY)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn state(app: &AppHandle) -> UpdateState {
    let updates = app.state::<Updates>();
    let settings = termf_core::load_settings(&app.state::<AppState>().config_dir)
        .map(|s| s.updates)
        .unwrap_or_default();
    let available = updates.found.lock().unwrap().as_ref().map(|u| Available {
        version: u.version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        date: u.date.map(|d| d.unix_timestamp()),
    });
    UpdateState {
        version: app.package_info().version.to_string(),
        channel: settings.channel,
        check: settings.check,
        checked: updates.checked(),
        available,
        error: updates.error.lock().unwrap().clone(),
    }
}

fn releases() -> Result<Vec<Release>> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-fsSL",
        "-H",
        "Accept: application/vnd.github+json",
        RELEASES,
    ]);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
    let (status, out, stderr) =
        termf_core::process::run(cmd, b"", Duration::from_secs(30)).context("cannot run curl")?;
    if !status.success() {
        bail!(
            "cannot list TermF's releases ({status}): {}",
            termf_core::process::excerpt(&String::from_utf8_lossy(&stderr))
        );
    }
    serde_json::from_slice(&out).context("unreadable list of TermF's releases")
}

async fn find(app: &AppHandle, channel: Channel) -> Result<Option<Update>> {
    let releases = tauri::async_runtime::spawn_blocking(releases).await??;
    let Some(url) = manifest(&releases, channel) else {
        return Ok(None);
    };
    let notes = releases
        .iter()
        .find(|r| r.assets.iter().any(|a| a.browser_download_url == url))
        .and_then(|r| r.body.clone());
    let update = app
        .updater_builder()
        .endpoints(vec![url.parse()?])?
        .timeout(Duration::from_secs(30))
        .build()?
        .check()
        .await?;
    Ok(update.map(|mut u| {
        u.body = notes.or(u.body);
        u
    }))
}

#[tauri::command]
pub fn update_state(app: AppHandle) -> UpdateState {
    state(&app)
}

/// `auto`: the startup check, which only runs when it is on and a day has passed, and keeps its error to itself.
#[tauri::command]
pub async fn update_check(app: AppHandle, auto: bool) -> CmdResult<UpdateState> {
    let settings = termf_core::load_settings(&app.state::<AppState>().config_dir)
        .map(|s| s.updates)
        .map_err(err)?;
    let updates = app.state::<Updates>();
    if auto && (!settings.check || !due(updates.checked(), now())) {
        return Ok(state(&app));
    }
    match find(&app, settings.channel).await {
        Ok(found) => {
            updates.set_checked(now());
            *updates.found.lock().unwrap() = found;
            *updates.error.lock().unwrap() = None;
        }
        Err(e) => {
            *updates.error.lock().unwrap() = Some(err(e));
            if !auto {
                return Err(updates.error.lock().unwrap().clone().unwrap_or_default());
            }
        }
    }
    Ok(state(&app))
}

#[tauri::command]
pub fn update_settings_set(
    app: AppHandle,
    config: State<'_, AppState>,
    channel: Channel,
    check: bool,
) -> CmdResult<UpdateState> {
    let before = termf_core::load_settings(&config.config_dir)
        .map(|s| s.updates.channel)
        .map_err(err)?;
    termf_core::save_updates(&config.config_dir, channel, check).map_err(err)?;
    if before != channel {
        // What was found belongs to the other channel.
        *app.state::<Updates>().found.lock().unwrap() = None;
    }
    Ok(state(&app))
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: Option<u64>,
}

/// Downloads, checks the signature, installs, then restarts TermF: open terminals and agents stop with it.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> CmdResult<()> {
    let update = app
        .state::<Updates>()
        .found
        .lock()
        .unwrap()
        .clone()
        .ok_or("no update to install")?;
    let mut done = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                let _ = app.emit("update-progress", Progress { done, total });
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, manifest: bool) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease,
            body: None,
            assets: manifest
                .then(|| Asset {
                    name: MANIFEST.into(),
                    browser_download_url: format!("https://example.test/{tag}/latest.json"),
                })
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn each_channel_takes_its_newest_release_with_a_manifest() {
        let releases = vec![
            release("v0.3.0-beta.2", true, true),
            release("v0.2.10", false, true),
            release("v0.2.9", false, true),
            release("v0.3.0", false, false),
            release("nightly", true, true),
            Release {
                draft: true,
                ..release("v9.0.0", false, true)
            },
        ];
        let tag = |c| manifest(&releases, c).map(|u| u.split('/').nth(3).unwrap().to_owned());
        // 0.2.10 > 0.2.9 as versions, not as text; 0.3.0 has no manifest; the draft is never taken.
        assert_eq!(tag(Channel::Stable).as_deref(), Some("v0.2.10"));
        assert_eq!(tag(Channel::Beta).as_deref(), Some("v0.3.0-beta.2"));

        let releases = vec![
            release("v0.3.0-beta.2", true, true),
            release("v0.3.0", false, true),
        ];
        assert_eq!(
            manifest(&releases, Channel::Beta),
            Some("https://example.test/v0.3.0/latest.json"),
            "a release is newer than its pre-releases"
        );
        assert_eq!(
            manifest(&[release("v0.1.0", true, true)], Channel::Stable),
            None
        );
    }

    #[test]
    fn checks_at_most_once_a_day() {
        assert!(due(None, 1_000_000));
        assert!(!due(Some(1_000_000), 1_000_000 + DAY - 1));
        assert!(due(Some(1_000_000), 1_000_000 + DAY));
        assert!(due(Some(1_000_000 + 10), 1_000_000), "clock set back");
    }

    #[test]
    fn reads_github_releases() {
        let json = r#"[{"tag_name":"v0.2.0","draft":false,"prerelease":true,"body":"…",
            "assets":[{"name":"latest.json","browser_download_url":"https://github.com/x/latest.json","size":1}]}]"#;
        let releases: Vec<Release> = serde_json::from_str(json).unwrap();
        assert_eq!(
            manifest(&releases, Channel::Beta),
            Some("https://github.com/x/latest.json")
        );
    }
}
