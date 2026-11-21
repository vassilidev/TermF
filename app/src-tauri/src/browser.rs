//! Opens a web address (a ticket, a pull request) in the system's browser.

use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};

use crate::{CmdResult, err};

/// Only `http://` and `https://` addresses with a host, and nothing a launcher could read as something else:
/// no control character, no space, no quote.
fn check(url: &str) -> Result<()> {
    let rest = ["https://", "http://"]
        .iter()
        .find_map(|scheme| {
            url.get(..scheme.len())
                .filter(|s| s.eq_ignore_ascii_case(scheme))
                .map(|s| &url[s.len()..])
        })
        .context("only http and https addresses open in the browser")?;
    ensure!(
        !rest.is_empty()
            && !rest.starts_with('/')
            && !url
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || c == '"'),
        "{url:?} is not a web address"
    );
    Ok(())
}

#[tauri::command]
pub fn open_url(url: String) -> CmdResult<()> {
    check(&url).map_err(err)?;
    // No shell reads the address: each launcher takes it as one argument. On Windows, `cmd /c start` would
    // run whatever follows a `&` in it, `explorer` does not.
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    let mut child = Command::new(program)
        .arg(&url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot run {program}"))
        .map_err(err)?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn opens_web_addresses_only() {
        for good in [
            "https://github.com/acme/front/pull/7",
            "HTTP://acme.atlassian.net/browse/ACME-1?focusedCommentId=1&x=%20y",
            "https://linear.app/acme/issue/ENG-7/fix-the-login",
        ] {
            assert!(check(good).is_ok(), "{good}");
        }
        for bad in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ssh://git@github.com/acme/front",
            "https:/github.com",
            "https://",
            "https:///etc",
            "github.com/acme",
            "https://x.test/a b",
            "https://x.test/\"&calc",
            "https://x.test/\nrm",
            "https://x.test/\u{7}",
            "https://x.test/\u{85}",
            " https://x.test",
            "",
        ] {
            assert!(check(bad).is_err(), "{bad:?}");
        }
    }
}
