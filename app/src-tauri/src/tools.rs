//! What TermF needs on the machine (ARCHITECTURE §16, first launch): git and Claude Code, then the GitHub CLI, the
//! other agent CLIs TermF drives (Codex, Gemini) and a Chrome, all optional. Chrome for Testing is downloaded on
//! demand when no Chrome is found.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::State;
use termf_claude::Env;
use termf_claude::cli::{self, Tool};
use termf_core::claude::{self as agent, Found};
use termf_visual::cft;

use crate::{AppState, CmdResult, err};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tools {
    git: Tool,
    gh: Tool,
    claude: Found,
    codex: Found,
    gemini: Found,
    chrome: Option<Chrome>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Chrome {
    path: String,
    /// Chrome for Testing, downloaded by TermF.
    testing: bool,
    /// Known for Chrome for Testing only: asking a browser would start it.
    version: Option<String>,
}

fn chrome(path: PathBuf, data_dir: &Path) -> Chrome {
    let testing = path.starts_with(cft::root(data_dir));
    Chrome {
        version: testing
            .then(|| cft::installed(data_dir).map(|(v, _)| v))
            .flatten(),
        path: path.to_string_lossy().into_owned(),
        testing,
    }
}

/// `browser`: the Chrome found, looked for apart since it reads the login shell and the usual install folders.
pub fn detect(env: &Env, data_dir: &Path, browser: Option<PathBuf>) -> Tools {
    let mut found = cli::tools_of(env, &["git", "gh"]).into_iter();
    let (Some(git), Some(gh)) = (found.next(), found.next()) else {
        unreachable!("one result per name");
    };
    let path = env.path.clone().unwrap_or_default();
    let other = |program| agent::find_in(program, env.home.as_deref(), &path);
    Tools {
        git,
        gh,
        claude: cli::binary(env),
        codex: other("codex"),
        gemini: other("gemini"),
        chrome: browser.map(|p| chrome(p, data_dir)),
    }
}

fn data_dir() -> CmdResult<PathBuf> {
    termf_core::default_data_dir().map_err(err)
}

#[tauri::command]
pub async fn tools_check(state: State<'_, AppState>) -> CmdResult<Tools> {
    let mut env = crate::claude::env(&state.config_dir, None);
    let data = data_dir()?;
    tauri::async_runtime::spawn_blocking(move || {
        // Codex and Gemini are also looked for where an interactive shell finds them (npm under nvm…).
        let paths = [agent::Tool::Codex, agent::Tool::Gemini].map(agent::search_path_for);
        let all = env
            .path
            .iter()
            .chain(&paths)
            .flat_map(std::env::split_paths);
        env.path = std::env::join_paths(all).ok();
        detect(&env, &data, termf_visual::browser::find().ok())
    })
    .await
    .map_err(|e| e.to_string())
}

static DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// Downloads Chrome for Testing (about 150 MB) into TermF's data folder.
#[tauri::command]
pub async fn tools_chrome_install() -> CmdResult<Chrome> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("Chrome for Testing is already being downloaded".into());
    }
    let data = data_dir();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let data = data?;
        let (_, exe) = cft::install(&data, cft::API).map_err(err)?;
        Ok(chrome(exe, &data))
    })
    .await
    .map_err(|e| e.to_string());
    DOWNLOADING.store(false, Ordering::SeqCst);
    result?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn program(dir: &Path, name: &str, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        let file = dir.join(name);
        fs::write(&file, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(windows)]
    fn program(dir: &Path, name: &str, version: &str) {
        fs::write(
            dir.join(format!("{name}.cmd")),
            format!("@echo {version}\r\n"),
        )
        .unwrap();
    }

    #[test]
    fn finds_the_tools_on_a_given_path() {
        let root = std::env::temp_dir().join(format!("termf-tools-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (bin, home, data) = (root.join("bin"), root.join("home"), root.join("data"));
        for d in [&bin, &home, &data] {
            fs::create_dir_all(d).unwrap();
        }
        let env = |path: &Path| Env {
            path: Some(path.into()),
            home: Some(home.clone()),
            ..Env::current(None, "claude".into())
        };

        let none = detect(&env(&bin), &data, None);
        assert!(none.git.path.is_none() && none.gh.path.is_none());
        assert!(none.claude.install.is_none() && none.chrome.is_none());
        assert!(none.codex.install.is_none() && none.gemini.install.is_none());

        program(&bin, "git", "git version 2.47.1");
        program(&bin, "claude", "2.1.330 (Claude Code)");
        program(&bin, "codex", "codex-cli 0.171.0");
        let some = detect(&env(&bin), &data, None);
        assert_eq!(some.git.version.as_deref(), Some("2.47.1"));
        assert!(some.gh.path.is_none(), "gh is optional and missing");
        let claude = some.claude.install.unwrap();
        assert_eq!(claude.version.as_deref(), Some("2.1.330"));
        assert!(!some.claude.too_old);
        assert_eq!(
            some.codex.install.unwrap().version.as_deref(),
            Some("0.171.0")
        );
        assert!(
            some.gemini.install.is_none(),
            "Gemini CLI is optional and missing"
        );

        let testing = cft::root(&data).join("131.0.6778.85/chrome-linux64/chrome");
        let found = chrome(testing, &data);
        assert!(found.testing);
        assert!(!chrome(PathBuf::from("/usr/bin/chromium"), &data).testing);
        fs::remove_dir_all(&root).unwrap();
    }
}
