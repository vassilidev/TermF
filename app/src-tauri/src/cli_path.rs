//! The `termf` CLI shipped with TermF, put on the PATH from the settings (ARCHITECTURE §16). macOS and Linux: a
//! link in `~/.local/bin`, a folder of the user's (no password asked, nothing written outside the home folder;
//! Claude Code's installer uses it too). Windows: the app's folder added to the user's PATH.

use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use crate::CmdResult;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CliPath {
    /// The CLI shipped with TermF; `None` in a build without it.
    bundled: Option<String>,
    /// The link TermF makes, or on Windows the folder it adds to the PATH.
    target: String,
    installed: bool,
    /// Something else is already at the link, which TermF leaves alone.
    taken: bool,
    /// The link's folder is on the login shell's PATH.
    on_path: bool,
    /// `package`: a Linux package already put it on the PATH; `appimage`: its folder moves at each launch.
    managed: Option<&'static str>,
    version: &'static str,
}

fn bundled() -> Option<PathBuf> {
    crate::api::termf_binary()
}

#[cfg(unix)]
mod sys {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::{Path, PathBuf};

    use anyhow::{Result, bail, ensure};

    use super::CliPath;

    pub fn link(home: &Path) -> PathBuf {
        home.join(".local").join("bin").join("termf")
    }

    pub fn status(bundled: Option<&Path>, home: &Path, path: &OsStr, appimage: bool) -> CliPath {
        let link = link(home);
        let installed = bundled.is_some_and(|b| fs::read_link(&link).ok().as_deref() == Some(b));
        let dir = link.parent().unwrap_or(home);
        CliPath {
            bundled: bundled.map(|b| b.to_string_lossy().into_owned()),
            target: link.to_string_lossy().into_owned(),
            installed,
            taken: !installed && fs::symlink_metadata(&link).is_ok(),
            on_path: std::env::split_paths(path).any(|d| d == dir),
            managed: if appimage {
                Some("appimage")
            } else if bundled.is_some_and(|b| b.starts_with("/usr")) {
                Some("package")
            } else {
                None
            },
            version: env!("CARGO_PKG_VERSION"),
        }
    }

    pub fn install(bundled: &Path, home: &Path) -> Result<()> {
        let link = link(home);
        if fs::read_link(&link).ok().as_deref() == Some(bundled) {
            return Ok(());
        }
        if fs::symlink_metadata(&link).is_ok() {
            bail!("{} already exists: TermF leaves it alone", link.display());
        }
        fs::create_dir_all(link.parent().unwrap_or(home))?;
        std::os::unix::fs::symlink(bundled, &link)?;
        Ok(())
    }

    pub fn uninstall(bundled: &Path, home: &Path) -> Result<()> {
        let link = link(home);
        ensure!(
            fs::read_link(&link).ok().as_deref() == Some(bundled),
            "{} is not TermF's link: TermF leaves it alone",
            link.display()
        );
        fs::remove_file(link)?;
        Ok(())
    }
}

/// `dir` added at the end of the PATH list `path`, unless it is there already.
#[cfg(any(windows, test))]
fn path_with(path: &str, dir: &str) -> String {
    if path_has(path, dir) {
        return path.to_owned();
    }
    let path = path.trim_end_matches(';');
    if path.is_empty() {
        dir.to_owned()
    } else {
        format!("{path};{dir}")
    }
}

#[cfg(any(windows, test))]
fn path_without(path: &str, dir: &str) -> String {
    path.split(';')
        .filter(|p| !p.is_empty() && !same_dir(p, dir))
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(any(windows, test))]
fn path_has(path: &str, dir: &str) -> bool {
    path.split(';').any(|p| same_dir(p, dir))
}

/// Windows paths: case aside, with or without a trailing backslash.
#[cfg(any(windows, test))]
fn same_dir(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.trim().trim_end_matches(['\\', '/']).to_lowercase();
    norm(a) == norm(b)
}

#[cfg(windows)]
mod sys {
    use anyhow::{Context, Result};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };
    use windows::core::w;
    use windows_registry::CURRENT_USER;

    /// The user's own PATH, unexpanded (`%USERPROFILE%\…` stays as written).
    pub fn user_path() -> Result<String> {
        let key = CURRENT_USER.create("Environment")?;
        Ok(key
            .get_hstring("Path")
            .map(|p| p.to_string_lossy())
            .unwrap_or_default())
    }

    /// Written back as an expandable string, then announced, so that programs started afterwards see it.
    pub fn set_user_path(path: &str) -> Result<()> {
        let key = CURRENT_USER.create("Environment")?;
        key.set_expand_string("Path", path)
            .context("cannot write the user's PATH")?;
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(w!("Environment").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                5000,
                None,
            );
        }
        Ok(())
    }
}

fn state() -> Result<CliPath> {
    let bundled = bundled();
    #[cfg(unix)]
    {
        let home = termf_core::expand_tilde("~");
        let path = termf_core::shell::login_path()
            .map(|p| p.to_os_string())
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        Ok(sys::status(
            bundled.as_deref(),
            &home,
            &path,
            std::env::var_os("APPIMAGE").is_some(),
        ))
    }
    #[cfg(windows)]
    {
        let dir = bundled
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(|d| d.to_string_lossy().into_owned());
        let installed = dir
            .as_deref()
            .is_some_and(|d| sys::user_path().is_ok_and(|p| path_has(&p, d)));
        Ok(CliPath {
            bundled: bundled.map(|b| b.to_string_lossy().into_owned()),
            target: dir.unwrap_or_default(),
            installed,
            taken: false,
            on_path: installed,
            managed: None,
            version: env!("CARGO_PKG_VERSION"),
        })
    }
}

fn change(install: bool) -> Result<CliPath> {
    let bundled =
        bundled().ok_or_else(|| anyhow::anyhow!("this build of TermF has no termf CLI"))?;
    #[cfg(unix)]
    {
        let home = termf_core::expand_tilde("~");
        if install {
            sys::install(&bundled, &home)?;
        } else {
            sys::uninstall(&bundled, &home)?;
        }
    }
    #[cfg(windows)]
    {
        let dir = bundled
            .parent()
            .unwrap_or(&bundled)
            .to_string_lossy()
            .into_owned();
        let path = sys::user_path()?;
        sys::set_user_path(&if install {
            path_with(&path, &dir)
        } else {
            path_without(&path, &dir)
        })?;
    }
    state()
}

async fn blocking<T: Send + 'static>(
    run: impl FnOnce() -> Result<T> + Send + 'static,
) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(run)
        .await
        .map_err(|e| e.to_string())?
        .map_err(crate::err)
}

#[tauri::command]
pub async fn cli_state() -> CmdResult<CliPath> {
    blocking(state).await
}

#[tauri::command]
pub async fn cli_install() -> CmdResult<CliPath> {
    blocking(|| change(true)).await
}

#[tauri::command]
pub async fn cli_uninstall() -> CmdResult<CliPath> {
    blocking(|| change(false)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::path::Path;

    #[test]
    fn edits_the_windows_path_list() {
        let dir = r"C:\Users\alex\AppData\Local\TermF";
        assert_eq!(path_with("", dir), dir);
        assert_eq!(
            path_with(r"%USERPROFILE%\bin;", dir),
            format!(r"%USERPROFILE%\bin;{dir}")
        );
        let with = path_with(r"C:\a;c:\users\ALEX\appdata\local\termf\", dir);
        assert_eq!(
            with, r"C:\a;c:\users\ALEX\appdata\local\termf\",
            "already there, whatever the case"
        );
        assert_eq!(
            path_without(&format!(r"C:\a;{dir}\;;C:\b"), dir),
            r"C:\a;C:\b"
        );
    }

    #[cfg(unix)]
    #[test]
    fn links_the_cli_in_the_home_folder() {
        use std::fs;
        let root = std::env::temp_dir().join(format!("termf-cli-path-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let app = root.join("TermF.app/Contents/MacOS");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&home).unwrap();
        let cli = app.join("termf");
        fs::write(&cli, "").unwrap();
        let bin = home.join(".local/bin");
        let path = std::env::join_paths([Path::new("/usr/bin"), &bin]).unwrap();

        let before = sys::status(Some(&cli), &home, &path, false);
        assert!(!before.installed && !before.taken && before.on_path && before.managed.is_none());
        assert!(!sys::status(Some(&cli), &home, std::ffi::OsStr::new("/usr/bin"), false).on_path);

        sys::install(&cli, &home).unwrap();
        sys::install(&cli, &home).unwrap();
        assert_eq!(fs::read_link(bin.join("termf")).unwrap(), cli);
        assert!(sys::status(Some(&cli), &home, &path, false).installed);

        sys::uninstall(&cli, &home).unwrap();
        assert!(fs::symlink_metadata(bin.join("termf")).is_err());
        assert!(sys::uninstall(&cli, &home).is_err());

        // Someone else's termf stays, whatever is asked.
        fs::write(bin.join("termf"), "mine").unwrap();
        let taken = sys::status(Some(&cli), &home, &path, false);
        assert!(taken.taken && !taken.installed);
        assert!(sys::install(&cli, &home).is_err());
        assert!(sys::uninstall(&cli, &home).is_err());
        assert_eq!(fs::read_to_string(bin.join("termf")).unwrap(), "mine");

        let package = sys::status(Some(Path::new("/usr/bin/termf")), &home, &path, false);
        assert_eq!(package.managed, Some("package"));
        assert_eq!(
            sys::status(Some(&cli), &home, &path, true).managed,
            Some("appimage")
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
