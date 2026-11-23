//! Which Claude Code TermF runs. A bare `claude` is looked for where Claude Code's installer puts it, then on the
//! login shell's PATH, then where an interactive shell finds it (`.zshrc` may add it: npm under nvm…): the
//! newest found wins. A path set in `[agents]` is always the one run. Codex CLI and Gemini CLI are found the
//! same way, without a native install folder.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::{OsStr, OsString};
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
/// The releases whose documentation TermF's options were checked against (docs/AGENTS-CLI.md, 2026-11-22): `exec
/// --json` with `thread.started`, `exec resume`, `developer_instructions`, `notify`, MCP `env_vars`.
pub const CODEX_MIN_VERSION: &str = "0.171.0";
/// The same for Gemini CLI: `--output-format stream-json`, `--session-id`, `--resume`, `--policy`, hooks.
pub const GEMINI_MIN_VERSION: &str = "0.70.0";

const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// An agent CLI TermF finds, checks and drives (docs/AGENTS-CLI.md).
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Claude,
    Codex,
    Gemini,
}

impl Tool {
    pub const ALL: [Tool; 3] = [Tool::Claude, Tool::Codex, Tool::Gemini];

    /// The tool an `[agents]` command runs, by its program's name: `codex`, `/opt/bin/gemini --yolo`…
    pub fn of(command: &str) -> Option<Tool> {
        let program = command.split_whitespace().next()?;
        let stem = Path::new(program).file_stem()?;
        Tool::ALL.into_iter().find(|t| stem == t.program())
    }

    pub fn program(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Gemini => "gemini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Claude => "Claude Code",
            Tool::Codex => "Codex CLI",
            Tool::Gemini => "Gemini CLI",
        }
    }

    /// The oldest version with everything TermF runs.
    pub fn minimum(self) -> &'static str {
        match self {
            Tool::Claude => MIN_VERSION,
            Tool::Codex => CODEX_MIN_VERSION,
            Tool::Gemini => GEMINI_MIN_VERSION,
        }
    }

    fn update(self) -> &'static str {
        match self {
            Tool::Claude => "claude update",
            Tool::Codex => "npm install -g @openai/codex@latest",
            Tool::Gemini => "npm install -g @google/gemini-cli@latest",
        }
    }

    /// Where the tool's own installer puts it, before the PATH.
    fn native(self, home: &Path) -> Vec<PathBuf> {
        match self {
            Tool::Claude => vec![
                home.join(".local").join("bin"),
                home.join(".claude").join("local"),
            ],
            Tool::Codex | Tool::Gemini => Vec::new(),
        }
    }
}

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
    #[serde(skip)]
    pub tool: Tool,
}

impl Default for Found {
    fn default() -> Self {
        Found::new(Tool::Claude, None, Vec::new())
    }
}

impl Found {
    fn new(tool: Tool, install: Option<Install>, others: Vec<Install>) -> Self {
        let too_old = install
            .as_ref()
            .and_then(|i| i.version.as_deref())
            .is_some_and(|v| numbers(v) < numbers(tool.minimum()));
        Found {
            install,
            others,
            minimum: tool.minimum(),
            too_old,
            tool,
        }
    }

    /// The install to run, refused when too old. None found: `program` itself, so that running it says so.
    pub fn usable(self, program: &str) -> Result<Install> {
        let tool = self.tool;
        if let Some(old) = self.install.as_ref().filter(|_| self.too_old) {
            let newer = self
                .others
                .iter()
                .filter(|o| rank(tool, o).0 == 2)
                .max_by_key(|o| rank(tool, o));
            let unknown = self.others.iter().find(|o| o.version.is_none());
            let such = match (newer, unknown) {
                (Some(n), _) => format!(", such as {n}"),
                (None, Some(u)) => format!(", such as the one {u}, whose version is unknown"),
                (None, None) => String::new(),
            };
            bail!(
                "{} {old} is too old: TermF needs {} or later. Update it (`{}`), or set `{}` in termf.toml [agents] \
                 to the path of a recent install{such}",
                tool.label(),
                tool.minimum(),
                tool.update(),
                tool.program(),
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
    let tool = Tool::of(program).unwrap_or(Tool::Claude);
    find_in(program, dirs::home_dir().as_deref(), &search_path_for(tool))
}

/// The login shell's PATH, then the folder of the `claude` an interactive login shell runs, as a tile's.
pub fn search_path() -> OsString {
    search_path_for(Tool::Claude)
}

/// [`search_path`] with the folder of the `tool` an interactive login shell runs.
pub fn search_path_for(tool: Tool) -> OsString {
    let path = crate::shell::login_path()
        .map(OsStr::to_os_string)
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    let Some(dir) = interactive(tool).and_then(|c| c.parent().map(Path::to_path_buf)) else {
        return path;
    };
    std::env::join_paths(std::env::split_paths(&path).chain([dir])).unwrap_or(path)
}

/// Asked again until a shell answered in time.
fn interactive(tool: Tool) -> Option<PathBuf> {
    static FOUND: Mutex<Option<HashMap<Tool, Option<PathBuf>>>> = Mutex::new(None);
    if let Some(found) = FOUND.lock().unwrap().as_ref().and_then(|f| f.get(&tool)) {
        return found.clone();
    }
    let shell = std::env::var_os("SHELL").filter(|_| cfg!(unix))?;
    let found = interactive_in(Path::new(&shell), tool).ok()?;
    let mut cache = FOUND.lock().unwrap();
    cache.get_or_insert_default().insert(tool, found.clone());
    found
}

/// The `tool` the interactive login shell `shell` runs; `Err` when it did not answer in time.
fn interactive_in(shell: &Path, tool: Tool) -> Result<Option<PathBuf>> {
    let mut cmd = Command::new(shell);
    cmd.args(["-l", "-i", "-c", &format!("command -v {}", tool.program())]);
    let (_, out, _) = crate::process::run(cmd, b"", PROBE_TIMEOUT)?;
    // Profiles may print their own lines first; an alias or a function is no path.
    Ok(String::from_utf8_lossy(&out)
        .lines()
        .map(str::trim)
        .rfind(|l| l.starts_with('/'))
        .map(PathBuf::from)
        .filter(|p| p.is_file()))
}

/// [`find`] in the home folder `home` and the folders of `path`. A program other than `claude` is taken as is.
pub fn find_in(program: &str, home: Option<&Path>, path: &OsStr) -> Found {
    search(program, home, path, true)
}

/// `others`: also look for the installs a path set in `[agents]` leaves aside.
fn search(program: &str, home: Option<&Path>, path: &OsStr, others: bool) -> Found {
    let Some(tool) = Tool::of(program) else {
        let install = Install {
            path: program.into(),
            version: None,
            origin: Origin::Setting,
        };
        return Found::new(Tool::Claude, Some(install), Vec::new());
    };
    let set = (Path::new(program).components().count() > 1).then(|| crate::expand_tilde(program));
    let exe = format!("{}{}", tool.program(), std::env::consts::EXE_SUFFIX);
    let native = home
        .into_iter()
        .flat_map(|h| tool.native(h))
        .map(|dir| (dir.join(&exe), Origin::Native));
    // npm puts a `claude.cmd` on Windows.
    let names: Vec<String> = match cfg!(windows) {
        true => [".exe", ".cmd"]
            .map(|ext| format!("{}{ext}", tool.program()))
            .to_vec(),
        false => vec![tool.program().to_owned()],
    };
    let on_path = std::env::split_paths(path).flat_map(|dir| {
        names
            .iter()
            .map(move |n| (dir.join(n), Origin::Path))
            .collect::<Vec<_>>()
    });
    let look = others || set.is_none();
    let mut seen = HashSet::new();
    let mut all: Vec<Install> = set
        .map(|p| (p, Origin::Setting))
        .into_iter()
        .chain(native.chain(on_path).filter(|(p, _)| look && p.is_file()))
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
        // The first of the best ranked.
        (0..all.len()).min_by_key(|&i| Reverse(rank(tool, &all[i])))
    };
    let install = chosen.map(|i| all.remove(i));
    Found::new(tool, install, all)
}

/// The newest of those recent enough, then one whose version could not be read (it may be recent), then the
/// newest of the older ones.
fn rank(tool: Tool, install: &Install) -> (u8, Vec<u64>) {
    match install.version.as_deref().map(numbers) {
        Some(v) if v >= numbers(tool.minimum()) => (2, v),
        None => (1, Vec::new()),
        Some(v) => (0, v),
    }
}

/// What `program` runs, refused before it runs when it is too old (see [`Found::usable`]).
pub fn binary(program: &str) -> Result<Install> {
    let tool = Tool::of(program).unwrap_or(Tool::Claude);
    resolve(program, dirs::home_dir().as_deref(), &search_path_for(tool))
}

/// A path set is run without asking the other installs their version, unless it is too old: then a newer one is
/// named.
fn resolve(program: &str, home: Option<&Path>, path: &OsStr) -> Result<Install> {
    let found = search(program, home, path, false);
    let found = match found.too_old {
        true => search(program, home, path, true),
        false => found,
    };
    found.usable(program)
}

/// `command`, an `[agents]` command run by a shell, with a bare `claude` replaced by [`binary`]'s path, quoted by
/// `quote` for that shell. A path written there stays as is, for the shell to expand, and is checked the same.
pub fn in_command(command: &str, quote: impl Fn(&str) -> String) -> Result<(String, Install)> {
    replace(command, quote, binary)
}

fn replace(
    command: &str,
    quote: impl Fn(&str) -> String,
    binary: impl Fn(&str) -> Result<Install>,
) -> Result<(String, Install)> {
    let trimmed = command.trim_start();
    let end = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
    let program = &trimmed[..end];
    let install = binary(program)?;
    if install.origin == Origin::Setting || install.path == Path::new(program) {
        return Ok((command.to_owned(), install));
    }
    let start = start(&install.path.to_string_lossy(), quote);
    Ok((format!("{start}{}", &trimmed[end..]), install))
}

/// How a command run by `termf_term::shell_command`'s shell starts the program at `path`, quoted by `quote`.
pub fn start(path: &str, quote: impl Fn(&str) -> String) -> String {
    starting(path, quote, cfg!(windows))
}

fn starting(path: &str, quote: impl Fn(&str) -> String, windows: bool) -> String {
    let plain = path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/\\._-:+@".contains(c));
    match path.rsplit_once(['\\', '/']) {
        _ if plain => path.to_owned(),
        // cmd.exe drops the first and last quotes of a command that starts with one, and the command reaches it
        // quoted with `\"` inside: no quote at all, its folder first on the PATH instead.
        Some((dir, file)) if windows => format!("set PATH={dir};%PATH%&& {file}"),
        _ => quote(path),
    }
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

/// `<path> --version`, asked again only once the file changed, or when it did not answer in time.
fn version(path: &Path, search: &OsStr) -> Option<String> {
    version_within(path, search, PROBE_TIMEOUT)
}

fn version_within(path: &Path, search: &OsStr, timeout: Duration) -> Option<String> {
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
    let probed = crate::process::run(cmd, b"", timeout);
    let version = probed
        .as_ref()
        .ok()
        .filter(|(status, ..)| status.success())
        .and_then(|(_, out, _)| read_version(&String::from_utf8_lossy(out)));
    if probed.is_ok() {
        CACHE
            .lock()
            .unwrap()
            .insert(key, (modified, version.clone()));
    }
    version
}

/// The first word of the first line that is a version: `2.1.330 (Claude Code)`, `codex-cli 0.98.0`, `v0.30.0`.
fn read_version(text: &str) -> Option<String> {
    text.lines()
        .next()?
        .split_whitespace()
        .map(|w| w.strip_prefix('v').unwrap_or(w))
        .find(|w| w.starts_with(|c: char| c.is_ascii_digit()))
        .map(str::to_owned)
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
            let count = self.root.join(rel).with_extension("count");
            self.script(
                rel,
                &format!(
                    "if [ \"$1\" != --version ]; then\n{body}\nfi\necho x >> '{}'\necho '{version} (Claude Code)'",
                    count.display()
                ),
            )
        }

        /// A shell script, run once without arguments.
        fn script(&self, rel: &str, body: &str) -> PathBuf {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
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
            find_in(program, Some(&self.home()), &self.path(path))
        }

        fn resolve(&self, program: &str, path: &[&str]) -> Result<Install> {
            resolve(program, Some(&self.home()), &self.path(path))
        }

        fn path(&self, dirs: &[&str]) -> OsString {
            std::env::join_paths(dirs.iter().map(|p| self.root.join(p))).unwrap()
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

        // A version that cannot be read is tried before an older one, which would be refused.
        let broken = b.script("odd/bin/claude", "exit 1");
        b.claude("brew/bin/claude", "2.0.1");
        fs::rename(&native, b.root.join("native.bak")).unwrap();
        let found = b.find("claude", &["brew/bin", "odd/bin"]);
        assert_eq!(picked(&found), (broken.as_path(), None, Origin::Path));
        assert!(!found.too_old);
        fs::rename(b.root.join("native.bak"), &native).unwrap();

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

        // An install whose version cannot be read is named.
        let odd = b.script("odd/bin/claude", "exit 1");
        let error = b
            .find(&old.to_string_lossy(), &["odd/bin"])
            .usable("x")
            .unwrap_err()
            .to_string();
        assert!(
            error.ends_with(&format!(
                "such as the one at {}, whose version is unknown",
                odd.display()
            )),
            "{error}"
        );

        let set = b.claude("opt/claude", "2.1.288");
        let found = b.find(&set.to_string_lossy(), &["brew/bin"]);
        assert_eq!(
            found.usable("x").unwrap().path,
            set,
            "the minimum itself is enough"
        );
        let unseen = b.claude("brew2/bin/claude", "2.1.500");
        let run = b.resolve(&set.to_string_lossy(), &["brew2/bin"]).unwrap();
        assert_eq!(run.path, set);
        assert!(
            !unseen.with_extension("count").exists(),
            "a recent path set: the other installs are never run"
        );
        let error = b
            .resolve(&old.to_string_lossy(), &["brew2/bin"])
            .unwrap_err()
            .to_string();
        assert!(
            error.ends_with(&format!("such as 2.1.500 at {}", unseen.display())),
            "{error}"
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

        // Too slow once: asked again the next time.
        let mark = b.root.join("asked");
        let slow = b.script(
            "slow/claude",
            &format!(
                "[ \"$1\" = --version ] || exit 0\n[ -e '{0}' ] || {{ touch '{0}'; sleep 5; }}\necho '2.1.330 (Claude Code)'",
                mark.display()
            ),
        );
        let (quick, path) = (Duration::from_secs(1), std::env::var_os("PATH").unwrap());
        assert_eq!(version_within(&slow, &path, quick), None);
        assert_eq!(
            version_within(&slow, &path, quick).as_deref(),
            Some("2.1.330")
        );
    }

    #[test]
    fn finds_the_claude_an_interactive_shell_runs() {
        let b = Sandbox::new("interactive");
        let nvm = b.claude("nvm/bin/claude", "2.1.400");
        let shell = b.script(
            "shell",
            &format!(
                "[ \"$*\" = '-l -i -c command -v claude' ] || exit 1\necho 'Welcome back'\necho '{}'",
                nvm.display()
            ),
        );
        assert_eq!(
            interactive_in(&shell, Tool::Claude).unwrap(),
            Some(nvm.clone())
        );
        let shell = b.script("shell", "echo 'claude: aliased to npx claude'");
        assert_eq!(
            interactive_in(&shell, Tool::Claude).unwrap(),
            None,
            "an alias is no path"
        );
        // Its folder after the login PATH: recent there, it is run rather than an older one refused.
        b.claude("brew/bin/claude", "2.0.1");
        let found = b.find("claude", &["brew/bin", "nvm/bin"]);
        assert_eq!(
            picked(&found),
            (nvm.as_path(), Some("2.1.400"), Origin::Path)
        );
    }

    #[test]
    fn replaces_only_a_bare_claude_in_a_command() {
        let b = Sandbox::new("command");
        let native = b.claude("home/.local/bin/claude", "2.1.330");
        let quote = |s: &str| format!("'{s}'");
        let find = |p: &str| b.resolve(p, &[]);
        let (command, install) = replace("  claude --model opus | tee log", quote, find).unwrap();
        assert_eq!(
            command,
            format!("{} --model opus | tee log", native.display())
        );
        assert_eq!(install.origin, Origin::Native);
        let spaced = b.claude("my tools/claude", "2.1.400");
        let (command, _) =
            replace("claude -c", quote, |p: &str| b.resolve(p, &["my tools"])).unwrap();
        assert_eq!(
            command,
            format!("'{}' -c", spaced.display()),
            "quoted only when it must be"
        );
        fs::remove_file(&spaced).unwrap();
        // cmd.exe: never a leading quote.
        let quote = |s: &str| format!("\"{s}\"");
        assert_eq!(
            starting(r"C:\Users\Jérôme\.local\bin\claude.exe", quote, true),
            r"set PATH=C:\Users\Jérôme\.local\bin;%PATH%&& claude.exe"
        );
        assert_eq!(
            starting(r"C:\Users\jo\.local\bin\claude.exe", quote, true),
            r"C:\Users\jo\.local\bin\claude.exe"
        );
        assert_eq!(
            starting("/opt/my tools/claude", quote, false),
            "\"/opt/my tools/claude\""
        );
        let quote = |s: &str| format!("'{s}'");
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
    fn a_stream_that_dies_after_its_init_gives_its_error_not_the_init() {
        let b = Sandbox::new("stream-crash");
        let body = "echo '{\"type\":\"system\",\"subtype\":\"init\"}'\necho 'Killed: out of memory' >&2\nexit 137";
        b.claude_running("brew/bin/claude", "2.1.330", body);
        let claude = b.find("claude", &["brew/bin"]).usable("claude").unwrap();
        let mut cmd = Command::new(&claude.path);
        cmd.args(["-p", "--output-format", "stream-json"]);
        let (status, out, err) = crate::process::run(cmd, b"hi", Duration::from_secs(10)).unwrap();
        let error = super::super::structured_output(&claude, status, &out, &err)
            .unwrap_err()
            .to_string();
        assert!(error.contains("Killed: out of memory"), "{error}");
        assert!(!error.contains("Claude Code: init"), "{error}");
    }

    #[test]
    fn compares_versions_by_number() {
        assert!(numbers("2.1.330") > numbers("2.1.288"));
        assert!(numbers("2.1.288") > numbers("2.0.99"));
        assert!(numbers("2.10.0") > numbers("2.9.400"));
        assert_eq!(numbers("2.1.330-beta"), numbers("2.1.330"));
    }
}
