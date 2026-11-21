//! The PATH (and other variables) of the user's login shell, which an app launched from the Finder or a desktop
//! menu lacks.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{OnceLock, mpsc};
use std::time::Duration;

/// Asked once. `printenv` is an external command, so this works whatever the shell's syntax (fish,
/// nushell…). `None` on Windows or when the shell gives nothing usable: the current PATH applies then.
pub fn login_path() -> Option<&'static OsStr> {
    static PATH: OnceLock<Option<OsString>> = OnceLock::new();
    PATH.get_or_init(|| {
        if cfg!(windows) {
            return None;
        }
        login_var("PATH")
    })
    .as_deref()
}

/// A variable as the login shell sets it, so as the sessions TermF opens see it; `None` when it is unset there,
/// on Windows, or without a usable shell. `name` is TermF's own, never the user's: it goes into a command line.
pub fn login_var(name: &str) -> Option<OsString> {
    if cfg!(windows) {
        return None;
    }
    var_from(Path::new(&std::env::var_os("SHELL")?), name)
}

fn var_from(shell: &Path, name: &str) -> Option<OsString> {
    let mut child = Command::new(shell)
        .args(["-l", "-c", &format!("printenv {name}")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    // A profile waiting for input must not block the action that needed the variable.
    let text = rx.recv_timeout(Duration::from_secs(10)).ok();
    let _ = child.kill();
    let _ = child.wait();
    // Profiles may print their own lines first.
    let value = text?
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())?
        .to_owned();
    value.contains('/').then(|| value.into())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn reads_the_login_path_without_shell_syntax() {
        let dir = std::env::temp_dir().join(format!("termf-core-shell-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let shell = |name: &str, body: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let fish = shell(
            "fish",
            "[ \"$1 $2 $3\" = '-l -c printenv PATH' ] || exit 3\necho 'Welcome to fish'\necho /opt/fake/bin:/usr/bin",
        );
        assert_eq!(
            var_from(&fish, "PATH"),
            Some("/opt/fake/bin:/usr/bin".into())
        );
        assert_eq!(var_from(&fish, "HOME"), None);
        assert_eq!(var_from(&shell("broken", "exit 1"), "PATH"), None);
        assert_eq!(var_from(&dir.join("missing"), "PATH"), None);
    }
}
