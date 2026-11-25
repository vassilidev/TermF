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

/// Variables as the login shell sets them, whatever they hold (tokens too), in one run and never cached: a
/// session token may change. Those unset there are left out; nothing on Windows. `names` are TermF's own.
pub fn login_vars(names: &[&str]) -> Vec<(String, OsString)> {
    if cfg!(windows) || names.is_empty() {
        return Vec::new();
    }
    std::env::var_os("SHELL").map_or_else(Vec::new, |shell| vars_from(Path::new(&shell), names))
}

/// The program `path`, a `PATH`-like list of folders, resolves.
pub fn which_in(path: &OsStr, program: &str) -> Option<std::path::PathBuf> {
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".cmd", ".bat", ""]
    } else {
        &[""]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{program}{ext}")))
            .find(|p| p.is_file() && executable(p))
    })
}

#[cfg(unix)]
pub fn executable(file: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    file.metadata()
        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
pub fn executable(_: &Path) -> bool {
    true
}

/// A line the profiles cannot print, before each value: what follows it up to the next one is that value.
const MARK: &str = "TERMF_LOGIN_VARS_MARK";

fn vars_from(shell: &Path, names: &[&str]) -> Vec<(String, OsString)> {
    let script: Vec<String> = names
        .iter()
        .flat_map(|n| [format!("printenv {MARK}"), format!("printenv {n}")])
        .collect();
    let Ok(mut child) = Command::new(shell)
        .args(["-l", "-c", &script.join("; ")])
        .env(MARK, MARK)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Vec::new();
    };
    let Some(mut stdout) = child.stdout.take() else {
        return Vec::new();
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let text = rx.recv_timeout(Duration::from_secs(10)).unwrap_or_default();
    let _ = child.kill();
    let _ = child.wait();
    // Whatever the profiles print comes before the first mark.
    let mark = format!("{MARK}\n");
    let values = text.split(&mark).skip(1);
    names
        .iter()
        .zip(values)
        .filter_map(|(name, value)| {
            let value = value.strip_suffix('\n').unwrap_or(value);
            (!value.is_empty()).then(|| (name.to_string(), value.into()))
        })
        .collect()
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
            // A process another test forks while the file is open for writing keeps it busy (ETXTBSY on Linux)
            // until it execs: wait for one run to start before the calls under test.
            for _ in 0..100 {
                let started = Command::new(&path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if !matches!(started, Err(e) if e.raw_os_error() == Some(26)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
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

        let login = shell(
            "login",
            "echo 'Welcome back'\nexport VAULT_TOKEN=hvs.s3cr3t VAULT_ADDR=https://vault.acme.test\nexec /bin/sh -c \"$3\"",
        );
        assert_eq!(
            vars_from(&login, &["VAULT_ADDR", "TERMF_UNSET_VAR", "VAULT_TOKEN"]),
            [
                ("VAULT_ADDR".to_owned(), "https://vault.acme.test".into()),
                ("VAULT_TOKEN".to_owned(), "hvs.s3cr3t".into())
            ]
        );
        assert!(vars_from(&shell("broken2", "exit 1"), &["HOME"]).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
