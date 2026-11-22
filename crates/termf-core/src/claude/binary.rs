//! Which Claude Code TermF runs. A bare `claude` is looked for where Claude Code's installer puts it, then on the
//! login shell's PATH, which lacks what only an interactive shell adds (`~/.local/bin` set in `.zshrc`): the
//! newest found wins. A path set in `[agents]` is always the one run.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use anyhow::{Result, bail};
use serde::Serialize;

/// The oldest Claude Code with everything TermF runs, from its changelog: `claude purge` (2.1.288),
/// `claude plugin install --accept-command` (2.1.271), `--json` on `claude plugin install…` (2.1.268),
/// `--restricted` (2.1.248).
pub const MIN_VERSION: &str = "2.1.288";

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// A path set in `termf.toml` `[agents]`, or another program than `claude`.
    Setting,
    /// Where Claude Code's installer puts it: `~/.local/bin`, or the older `~/.claude/local`.
    Native,
    /// The login shell's PATH.
    Path,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Install {
    pub path: PathBuf,
    /// What `--version` printed; `None` when it printed nothing readable.
    pub version: Option<String>,
    pub origin: Origin,
}

/// `2.1.330 at /home/alex/.local/bin/claude`, for messages that start with "Claude Code".
impl fmt::Display for Install {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.version {
            Some(v) => write!(f, "{v} at {}", self.path.display()),
            None => write!(f, "at {}", self.path.display()),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    /// What TermF runs; `None` when no Claude Code was found.
    pub install: Option<Install>,
    /// The other installs found, which TermF does not run.
    pub others: Vec<Install>,
    pub minimum: &'static str,
    /// The one TermF runs is older than [`MIN_VERSION`].
    pub too_old: bool,
}

impl Default for Found {
    fn default() -> Self {
        Found::new(None, Vec::new())
    }
}

impl Found {
    fn new(install: Option<Install>, others: Vec<Install>) -> Self {
        let too_old = install
            .as_ref()
            .and_then(|i| i.version.as_deref())
            .is_some_and(|v| numbers(v) < numbers(MIN_VERSION));
        Found {
            install,
            others,
            minimum: MIN_VERSION,
            too_old,
        }
    }

    /// The install to run, refused when too old. None found: `program` itself, so that running it says so.
    pub fn usable(self, program: &str) -> Result<Install> {
        if let Some(old) = self.install.as_ref().filter(|_| self.too_old) {
            let newer = self
                .others
                .iter()
                .filter(|o| {
                    o.version
                        .as_deref()
                        .is_some_and(|v| numbers(v) >= numbers(MIN_VERSION))
                })
                .max_by_key(|o| o.version.as_deref().map(numbers));
            let such = newer.map_or_else(String::new, |n| format!(", such as {n}"));
            bail!(
                "Claude Code {old} is too old: TermF needs {MIN_VERSION} or later. Update it (`claude update`), or set \
                 `claude` in termf.toml [agents] to the path of a recent install{such}"
            );
        }
        Ok(self.install.unwrap_or_else(|| Install {
            path: program.into(),
            version: None,
            origin: Origin::Path,
        }))
    }
}

/// What `program`, the first word of an `[agents]` command, runs on this machine.
pub fn find(program: &str) -> Found {
    let path = crate::shell::login_path()
        .map(OsStr::to_os_string)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    find_in(program, dirs::home_dir().as_deref(), &path)
}

/// [`find`] in the home folder `home` and the folders of `path`. A program other than `claude` is taken as is.
pub fn find_in(program: &str, home: Option<&Path>, path: &OsStr) -> Found {
    if !super::is_claude(program) {
        let install = Install {
            path: program.into(),
            version: None,
            origin: Origin::Setting,
        };
        return Found::new(Some(install), Vec::new());
    }
    let set = (Path::new(program).components().count() > 1).then(|| crate::expand_tilde(program));
    let exe = format!("claude{}", std::env::consts::EXE_SUFFIX);
    let native = home
        .into_iter()
        .flat_map(|h| {
            [
                h.join(".local").join("bin"),
                h.join(".claude").join("local"),
            ]
        })
        .map(|dir| (dir.join(&exe), Origin::Native));
    // npm puts a `claude.cmd` on Windows.
    let names: &[&str] = if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    };
    let on_path = std::env::split_paths(path)
        .flat_map(|dir| names.iter().map(move |n| (dir.join(n), Origin::Path)));
    let mut seen = HashSet::new();
    let mut all: Vec<Install> = set
        .map(|p| (p, Origin::Setting))
        .into_iter()
        .chain(native.chain(on_path).filter(|(p, _)| p.is_file()))
        .filter(|(p, _)| seen.insert(p.canonicalize().unwrap_or_else(|_| p.clone())))
        .map(|(p, origin)| Install {
            version: version(&p, path),
            path: p,
            origin,
        })
        .collect();
    let chosen = if all.first().is_some_and(|i| i.origin == Origin::Setting) {
        Some(0)
    } else {
        // The first of the newest; a version that cannot be read comes last.
        (0..all.len()).min_by_key(|&i| Reverse(all[i].version.as_deref().map(numbers)))
    };
    let install = chosen.map(|i| all.remove(i));
    Found::new(install, all)
}

/// What `program` runs, refused before it runs when it is too old (see [`Found::usable`]).
pub fn binary(program: &str) -> Result<Install> {
    find(program).usable(program)
}

/// `command`, an `[agents]` command run by a shell, with a bare `claude` replaced by [`binary`]'s path, quoted by
/// `quote` for that shell. A path written there stays as is, for the shell to expand, and is checked the same.
pub fn in_command(command: &str, quote: impl Fn(&str) -> String) -> Result<(String, Install)> {
    replace(command, quote, find)
}

fn replace(
    command: &str,
    quote: impl Fn(&str) -> String,
    find: impl Fn(&str) -> Found,
) -> Result<(String, Install)> {
    let trimmed = command.trim_start();
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let program = &trimmed[..end];
    let install = find(program).usable(program)?;
    if install.origin == Origin::Setting || install.path == Path::new(program) {
        return Ok((command.to_owned(), install));
    }
    let path = install.path.to_string_lossy();
    // Unquoted when it can be: cmd.exe drops the first and last quotes of a command that starts with one.
    let plain = path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/\\._-:+@".contains(c));
    let path = if plain {
        path.into_owned()
    } else {
        quote(&path)
    };
    Ok((format!("{path}{}", &trimmed[end..]), install))
}

/// A headless run without its answer: which Claude Code it was, then what it printed on stderr.
pub fn failure(claude: &Install, status: ExitStatus, stderr: &[u8]) -> String {
    let said = crate::process::excerpt(&String::from_utf8_lossy(stderr));
    format!("Claude Code {claude} gave no answer ({status}): {said}")
}

/// `2.1.330 (Claude Code)` → `[2, 1, 330]`.
fn numbers(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

/// Each file's modification time and version.
type Versions = BTreeMap<PathBuf, (Option<SystemTime>, Option<String>)>;

/// `<path> --version`, asked again only once the file changed.
fn version(path: &Path, search: &OsStr) -> Option<String> {
    static CACHE: Mutex<Versions> = Mutex::new(BTreeMap::new());
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let modified = std::fs::metadata(&key).and_then(|m| m.modified()).ok();
    if let Some((when, version)) = CACHE.lock().unwrap().get(&key)
        && *when == modified
    {
        return version.clone();
    }
    #[allow(unused_mut)]
    let mut cmd = Command::new(path);
    cmd.arg("--version").env("PATH", search);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
    let version = crate::process::run(cmd, b"", PROBE_TIMEOUT)
        .ok()
        .filter(|(status, ..)| status.success())
        .and_then(|(_, out, _)| {
            let text = String::from_utf8_lossy(&out);
            let word = text.lines().next()?.split_whitespace().next()?;
            let word = word.strip_prefix('v').unwrap_or(word);
            word.starts_with(|c: char| c.is_ascii_digit())
                .then(|| word.to_owned())
        });
    CACHE
        .lock()
        .unwrap()
        .insert(key, (modified, version.clone()));
    version
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Sandbox {
            let root = std::env::temp_dir()
                .join(format!("termf-claude-bin-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("home")).unwrap();
            Sandbox { root }
        }

        fn home(&self) -> PathBuf {
            self.root.join("home")
        }

        /// A `claude` that prints `version` for `--version` and counts how often it was asked.
        fn claude(&self, rel: &str, version: &str) -> PathBuf {
            self.claude_running(rel, version, "exit 0")
        }

        /// The same, which runs `body` for anything else.
        fn claude_running(&self, rel: &str, version: &str, body: &str) -> PathBuf {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let count = path.with_extension("count");
            fs::write(
                &path,
                format!(
                    "#!/bin/sh\nif [ \"$1\" != --version ]; then\n{body}\nfi\necho x >> '{}'\necho '{version} (Claude Code)'\n",
                    count.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            // A process another test forks while the file is open for writing keeps it busy (ETXTBSY on Linux)
            // until it execs: wait for one run to start.
            for _ in 0..100 {
                let started = Command::new(&path).output();
                if !matches!(started, Err(e) if e.raw_os_error() == Some(26)) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            path
        }

        fn find(&self, program: &str, path: &[&str]) -> Found {
            let path = std::env::join_paths(path.iter().map(|p| self.root.join(p))).unwrap();
            find_in(program, Some(&self.home()), &path)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn picked(found: &Found) -> (&Path, Option<&str>, Origin) {
        let i = found.install.as_ref().unwrap();
        (&i.path, i.version.as_deref(), i.origin)
    }

    #[test]
    fn finds_the_native_install_the_login_path_misses() {
        let b = Sandbox::new("native");
        let native = b.claude("home/.local/bin/claude", "2.1.330");
        let npm = b.claude("brew/bin/claude", "2.0.1");
        let found = b.find("claude", &["brew/bin", "usr/bin"]);
        assert_eq!(
            picked(&found),
            (native.as_path(), Some("2.1.330"), Origin::Native)
        );
        assert_eq!(found.others.len(), 1);
        assert_eq!(
            (&found.others[0].path, found.others[0].origin),
            (&npm, Origin::Path)
        );
        assert!(!found.too_old);
        assert_eq!(found.minimum, MIN_VERSION);

        // The newest wins, wherever it is; among equals, the native one.
        let newer = b.claude("brew/bin/claude", "2.1.400");
        let found = b.find("claude", &["brew/bin"]);
        assert_eq!(
            picked(&found),
            (newer.as_path(), Some("2.1.400"), Origin::Path)
        );
        b.claude("brew/bin/claude", "2.1.330");
        assert_eq!(picked(&b.find("claude", &["brew/bin"])).2, Origin::Native);

        // The older local install, and the same file twice on PATH, counted once.
        fs::remove_file(&native).unwrap();
        let legacy = b.claude("home/.claude/local/claude", "2.1.300");
        let found = b.find("claude", &[]);
        assert_eq!(
            picked(&found),
            (legacy.as_path(), Some("2.1.300"), Origin::Native)
        );
        let found = b.find("claude", &["brew/bin", "brew/bin"]);
        assert_eq!(found.others.len(), 1);

        fs::remove_file(&legacy).unwrap();
        let none = b.find("claude", &["usr/bin"]);
        assert!(none.install.is_none() && !none.too_old);
        assert_eq!(none.usable("claude").unwrap().path, Path::new("claude"));
    }

    #[test]
    fn a_path_set_always_wins_and_an_old_one_is_refused_naming_a_newer_install() {
        let b = Sandbox::new("setting");
        let native = b.claude("home/.local/bin/claude", "2.1.330");
        let old = b.claude("brew/bin/claude", "2.0.1");
        let found = b.find(&old.to_string_lossy(), &["brew/bin"]);
        assert_eq!(
            picked(&found),
            (old.as_path(), Some("2.0.1"), Origin::Setting)
        );
        assert!(found.too_old);
        assert_eq!(
            found.others.len(),
            1,
            "the same file on PATH is not another install"
        );
        let error = found.usable("x").unwrap_err().to_string();
        assert!(
            error.starts_with(&format!(
                "Claude Code 2.0.1 at {} is too old: TermF needs {MIN_VERSION} or later",
                old.display()
            )),
            "{error}"
        );
        assert!(error.contains("claude update"), "{error}");
        assert!(
            error.ends_with(&format!("such as 2.1.330 at {}", native.display())),
            "{error}"
        );

        // Old everywhere: nothing newer to name.
        b.claude("home/.local/bin/claude", "1.0.0");
        let error = b
            .find("claude", &["brew/bin"])
            .usable("claude")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("Claude Code 2.0.1 at "), "{error}");
        assert!(!error.contains("such as"), "{error}");

        let set = b.claude("opt/claude", "2.1.288");
        let found = b.find(&set.to_string_lossy(), &["brew/bin"]);
        assert_eq!(
            found.usable("x").unwrap().path,
            set,
            "the minimum itself is enough"
        );

        let other = b.find("/bin/sh", &["brew/bin"]);
        assert_eq!(
            picked(&other),
            (Path::new("/bin/sh"), None, Origin::Setting)
        );
        assert!(
            other.others.is_empty(),
            "not Claude Code: nothing looked for"
        );
    }

    #[test]
    fn asks_the_version_again_only_once_the_file_changed() {
        let b = Sandbox::new("cache");
        let native = b.claude("home/.local/bin/claude", "2.1.330");
        let count = || {
            fs::read_to_string(native.with_extension("count"))
                .unwrap()
                .lines()
                .count()
        };
        b.find("claude", &[]);
        b.find("claude", &[]);
        assert_eq!(count(), 1);
        std::thread::sleep(Duration::from_millis(20));
        b.claude("home/.local/bin/claude", "2.1.331");
        assert_eq!(picked(&b.find("claude", &[])).1, Some("2.1.331"));
    }

    #[test]
    fn replaces_only_a_bare_claude_in_a_command() {
        let b = Sandbox::new("command");
        let native = b.claude("home/.local/bin/claude", "2.1.330");
        let quote = |s: &str| format!("'{s}'");
        let find = |p: &str| b.find(p, &[]);
        let (command, install) = replace("  claude --model opus | tee log", quote, find).unwrap();
        assert_eq!(
            command,
            format!("{} --model opus | tee log", native.display())
        );
        assert_eq!(install.origin, Origin::Native);
        let spaced = b.claude("my tools/claude", "2.1.400");
        let (command, _) = replace("claude -c", quote, |p: &str| b.find(p, &["my tools"])).unwrap();
        assert_eq!(
            command,
            format!("'{}' -c", spaced.display()),
            "quoted only when it must be"
        );
        fs::remove_file(&spaced).unwrap();
        for same in [
            "~/bin/claude -c",
            "/bin/echo claude",
            "\"/opt/my tools/claude\" -c",
        ] {
            assert_eq!(replace(same, quote, find).unwrap().0, same);
        }
        b.claude("home/.local/bin/claude", "2.0.1");
        let error = replace("claude", quote, find).unwrap_err().to_string();
        assert!(error.contains("is too old"), "{error}");
        fs::remove_file(&native).unwrap();
        assert_eq!(
            replace("claude -c", quote, find).unwrap().0,
            "claude -c",
            "none found: as typed"
        );
    }

    #[test]
    fn a_crash_says_which_claude_and_its_error_line() {
        let b = Sandbox::new("crash");
        let trace = "cat >&2 <<'EOF'\nfile:///usr/lib/node_modules/@anthropic-ai/claude-code/cli.js:388\nvar a=1;\n    ^\n\n\
                     TypeError: Cannot read properties of undefined (reading 'prototype')\n    at file:///usr/lib/x.js:1:2\n\n\
                     Node.js v26.0.0\nEOF\nexit 1";
        let path = b.claude_running("brew/bin/claude", "2.1.300", trace);
        let claude = b.find("claude", &["brew/bin"]).usable("claude").unwrap();
        let mut cmd = Command::new(&claude.path);
        cmd.args(["-p", "--output-format", "json"]);
        let (status, out, err) = crate::process::run(cmd, b"hi", Duration::from_secs(10)).unwrap();
        let error = super::super::structured_output(&claude, status, &out, &err)
            .unwrap_err()
            .to_string();
        assert!(
            error.starts_with(&format!(
                "Claude Code 2.1.300 at {} gave no answer (exit status: 1): \
                 file:///usr/lib/node_modules/@anthropic-ai/claude-code/cli.js:388\n",
                path.display()
            )),
            "{error}"
        );
        assert!(
            error.contains(
                "\nTypeError: Cannot read properties of undefined (reading 'prototype')\n"
            ),
            "{error}"
        );
        assert!(error.ends_with("Node.js v26.0.0"), "{error}");
        assert_eq!(failure(&claude, status, &err), error);
    }

    #[test]
    fn compares_versions_by_number() {
        assert!(numbers("2.1.330") > numbers("2.1.288"));
        assert!(numbers("2.1.288") > numbers("2.0.99"));
        assert!(numbers("2.10.0") > numbers("2.9.400"));
        assert_eq!(numbers("2.1.330-beta"), numbers("2.1.330"));
    }
}
