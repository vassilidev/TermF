//! Chrome for Testing, downloaded on demand into TermF's data folder when no Chrome is installed: the captures
//! and the agents' browser then use it. The official JSON API gives the stable version and its zip; it publishes
//! no checksum, so the download relies on HTTPS and the zip's own CRCs, checked when it is extracted.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

pub const API: &str = "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const API_TIMEOUT: Duration = Duration::from_secs(30);

/// The API's name for this system; Windows on ARM runs the x64 build.
pub fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("mac-arm64"),
        ("macos", "x86_64") => Some("mac-x64"),
        ("linux", "x86_64") => Some("linux64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("windows", "x86_64" | "aarch64") => Some("win64"),
        _ => None,
    }
}

/// Where the browser lies in the extracted zip of `platform`.
fn executable(platform: &str) -> PathBuf {
    let dir = PathBuf::from(format!("chrome-{platform}"));
    if platform.starts_with("mac") {
        dir.join("Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing")
    } else if platform.starts_with("win") {
        dir.join("chrome.exe")
    } else {
        dir.join("chrome")
    }
}

pub fn root(data_dir: &Path) -> PathBuf {
    data_dir.join("chrome-for-testing")
}

/// The newest version downloaded, and its browser.
pub fn installed(data_dir: &Path) -> Option<(String, PathBuf)> {
    let exe = executable(platform()?);
    fs::read_dir(root(data_dir))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let version = e.file_name().to_str()?.to_owned();
            let numbers = numbers(&version)?;
            let path = e.path().join(&exe);
            path.is_file().then_some((numbers, version, path))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v, p)| (v, p))
}

fn numbers(version: &str) -> Option<Vec<u32>> {
    version.split('.').map(|n| n.parse().ok()).collect()
}

/// Downloads the stable Chrome for Testing that `api` lists for this system, replacing any older one.
pub fn install(data_dir: &Path, api: &str) -> Result<(String, PathBuf)> {
    let platform = platform().context("Chrome for Testing has no build for this system")?;
    let listing: Value = serde_json::from_slice(&curl(&["-fsSL", api], API_TIMEOUT)?)
        .context("unreadable Chrome for Testing listing")?;
    let stable = &listing["channels"]["Stable"];
    let version = stable["version"]
        .as_str()
        .context("no stable Chrome for Testing version")?;
    ensure!(
        numbers(version).is_some(),
        "unexpected Chrome for Testing version {version:?}"
    );
    let url = stable["downloads"]["chrome"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|d| d["platform"] == platform)
        .and_then(|d| d["url"].as_str())
        .with_context(|| format!("no Chrome for Testing {version} for {platform}"))?;

    let root = root(data_dir);
    fs::create_dir_all(&root)?;
    let zip = root.join(format!("{version}.zip"));
    let part = root.join(format!("{version}.part"));
    let result = (|| {
        curl(
            &["-fsSL", "-o", &zip.to_string_lossy(), url],
            DOWNLOAD_TIMEOUT,
        )?;
        let _ = fs::remove_dir_all(&part);
        fs::create_dir_all(&part)?;
        extract(&zip, &part)?;
        let exe = executable(platform);
        ensure!(
            part.join(&exe).is_file(),
            "the Chrome for Testing zip has no {}",
            exe.display()
        );
        let dest = root.join(version);
        let _ = fs::remove_dir_all(&dest);
        fs::rename(&part, &dest)?;
        Ok((version.to_owned(), dest.join(exe)))
    })();
    let _ = fs::remove_file(&zip);
    let _ = fs::remove_dir_all(&part);
    let (version, exe) = result?;
    for entry in fs::read_dir(&root)?.flatten() {
        if entry.file_name() != version.as_str() {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    Ok((version, exe))
}

fn curl(args: &[&str], timeout: Duration) -> Result<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(args);
    let (status, out, err) = run(cmd, timeout).context("cannot run curl")?;
    if !status.success() {
        bail!(
            "curl failed ({status}): {}",
            termf_core::process::excerpt(&String::from_utf8_lossy(&err))
        );
    }
    Ok(out)
}

/// The system's own unzip, which keeps the macOS bundle's symlinks and the executable bits.
fn extract(zip: &Path, dest: &Path) -> Result<()> {
    let cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("ditto");
        c.arg("-x").arg("-k").arg(zip).arg(dest);
        c
    } else if cfg!(windows) {
        // The bsdtar that ships with Windows reads zips; a GNU tar earlier on the PATH (Git's) would not.
        let system = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let mut c = Command::new(Path::new(&system).join(r"System32\tar.exe"));
        c.arg("-xf").arg(zip).arg("-C").arg(dest);
        c
    } else {
        let mut c = Command::new("unzip");
        c.arg("-q").arg(zip).arg("-d").arg(dest);
        c
    };
    let (status, _, err) = run(cmd, DOWNLOAD_TIMEOUT)
        .context("cannot extract Chrome for Testing (is unzip installed?)")?;
    ensure!(
        status.success(),
        "cannot extract Chrome for Testing ({status}): {}",
        termf_core::process::excerpt(&String::from_utf8_lossy(&err))
    );
    Ok(())
}

fn run(cmd: Command, timeout: Duration) -> Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)> {
    #[cfg(windows)]
    let cmd = {
        let mut cmd = cmd;
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
        cmd
    };
    termf_core::process::run(cmd, b"", timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// One folder per platform, each with its browser (mode 755).
    const ZIP: &[u8] = include_bytes!("../tests/cft.zip");

    /// Serves the listing, then the zip, then refuses: nothing else is asked.
    fn server(version: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let host = base.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(2) {
                let mut stream = stream.unwrap();
                let mut line = String::new();
                BufReader::new(&stream).read_line(&mut line).unwrap();
                let body = if line.contains("/listing.json") {
                    let downloads: Vec<Value> = ["mac-arm64", "mac-x64", "linux64", "linux-arm64", "win64"]
                        .iter()
                        .map(|p| serde_json::json!({"platform": p, "url": format!("{host}/chrome-{p}.zip")}))
                        .collect();
                    serde_json::json!({"channels": {"Stable": {"version": version, "downloads": {"chrome": downloads}}}})
                        .to_string()
                        .into_bytes()
                } else {
                    ZIP.to_vec()
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        format!("{base}/listing.json")
    }

    #[test]
    fn downloads_the_stable_build_and_replaces_the_older_one() {
        let data = std::env::temp_dir().join(format!("termf-cft-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        assert!(installed(&data).is_none());

        let (version, exe) = install(&data, &server("131.0.6778.85")).unwrap();
        assert_eq!(version, "131.0.6778.85");
        assert!(exe.is_file(), "{}", exe.display());
        assert_eq!(installed(&data), Some((version, exe.clone())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(&exe).unwrap().permissions().mode() & 0o111,
                0,
                "executable"
            );
        }

        let (newer, exe) = install(&data, &server("132.0.6834.6")).unwrap();
        assert_eq!(installed(&data), Some((newer, exe)));
        let left: Vec<_> = fs::read_dir(root(&data))
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            left,
            ["132.0.6834.6"],
            "the older version and the download are gone"
        );
        fs::remove_dir_all(&data).unwrap();
    }

    #[test]
    fn a_failed_download_leaves_nothing() {
        let data = std::env::temp_dir().join(format!("termf-cft-fail-{}", std::process::id()));
        let _ = fs::remove_dir_all(&data);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/listing.json", listener.local_addr().unwrap());
        drop(listener);
        assert!(install(&data, &url).is_err());
        assert!(installed(&data).is_none());
        let _ = fs::remove_dir_all(&data);
    }
}
