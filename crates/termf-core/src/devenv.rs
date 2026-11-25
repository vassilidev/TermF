//! The development environment a machine is made of: what detection finds installed (package managers, language
//! versions, binaries and apps installed by hand), what the user keeps of it in `env.toml`, and the POSIX sh script
//! that installs it anywhere, skipping what is there. Detection only reads: each command has a deadline, and only
//! the managers present are asked.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{process, shell};

/// The manifest, in the config folder: synced with the config.
pub const FILE: &str = "env.toml";
/// The named scripts, beside it: `env/<name>.sh`.
pub const SCRIPTS: &str = "env";

const QUICK: Duration = Duration::from_secs(20);
/// `brew bundle dump` reads every formula installed.
const SLOW: Duration = Duration::from_secs(90);

/// In the install script's order: Homebrew, the commands of your own, languages, then their packages.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Manager {
    Tap,
    Brew,
    Cask,
    Mas,
    Vscode,
    Shell,
    Mise,
    Asdf,
    Fnm,
    Nvm,
    Volta,
    Rustup,
    Npm,
    Pnpm,
    Bun,
    Cargo,
    Pipx,
    Uv,
    Gh,
    Composer,
    Manual,
}

impl Manager {
    fn group(self) -> Group {
        match self {
            Manager::Cask | Manager::Mas => Group::Apps,
            Manager::Vscode => Group::Editors,
            Manager::Mise | Manager::Asdf | Manager::Fnm | Manager::Nvm | Manager::Rustup => {
                Group::Languages
            }
            _ => Group::Cli,
        }
    }

    /// A language's version is what it is for; a package's is whatever is current.
    fn versioned(self) -> bool {
        self.group() == Group::Languages
    }

    fn brewfile(self) -> Option<&'static str> {
        Some(match self {
            Manager::Tap => "tap",
            Manager::Brew => "brew",
            Manager::Cask => "cask",
            Manager::Mas => "mas",
            Manager::Vscode => "vscode",
            _ => return None,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "lowercase")]
pub enum Group {
    Languages,
    #[default]
    Cli,
    Apps,
    Editors,
}

impl Group {
    pub const ALL: [Group; 4] = [Group::Languages, Group::Cli, Group::Apps, Group::Editors];

    fn key(self) -> &'static str {
        match self {
            Group::Languages => "languages",
            Group::Cli => "cli",
            Group::Apps => "apps",
            Group::Editors => "editors",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Group::Languages => "Languages and their versions",
            Group::Cli => "Command-line tools",
            Group::Apps => "Apps",
            Group::Editors => "Editors and their extensions",
        }
    }
}

const fn yes() -> bool {
    true
}

/// One thing the environment is made of.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// The file's table it is in.
    #[serde(default)]
    pub group: Group,
    pub manager: Manager,
    pub name: String,
    /// Installed as is; none: whatever is current.
    #[serde(default)]
    pub version: Option<String>,
    /// The App Store id of a `mas` app.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// The command that installs a `shell` entry, or a `manual` one once known.
    #[serde(default)]
    pub install: Option<String>,
    /// Where a `shell` or `manual` entry lands (`~/…`): there means installed. Else its name on the PATH.
    #[serde(default)]
    pub path: Option<String>,
    /// Off: kept in the file, left out of the script.
    #[serde(default = "yes")]
    pub active: bool,
}

/// An entry's identity in the manifest.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub manager: Manager,
    pub name: String,
}

impl Entry {
    pub fn new(manager: Manager, name: &str) -> Entry {
        Entry {
            group: manager.group(),
            manager,
            name: name.into(),
            version: None,
            id: None,
            description: None,
            install: None,
            path: None,
            active: true,
        }
    }

    pub fn key(&self) -> Key {
        Key {
            manager: self.manager,
            name: self.name.clone(),
        }
    }

    /// What goes into the script and the Brewfile is checked here: each value is quoted there, and none can end
    /// a quote.
    pub fn check(&self) -> Result<()> {
        let name = &self.name;
        let spaced = matches!(
            self.manager,
            Manager::Mas | Manager::Manual | Manager::Shell
        );
        ensure!(
            !name.is_empty()
                && name.len() <= 200
                && !name.starts_with('-')
                && name.chars().all(|c| c.is_ascii_alphanumeric()
                    || "@._+/:-".contains(c)
                    || (spaced && c == ' ')),
            "{name:?} is not a name {} can install",
            serde_json::to_value(self.manager)?
                .as_str()
                .unwrap_or_default()
        );
        if matches!(self.manager, Manager::Tap | Manager::Gh) {
            ensure!(name.contains('/'), "{name}: owner/repository");
        }
        if let Some(v) = &self.version {
            ensure!(
                !v.is_empty()
                    && v.len() <= 64
                    && v.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._+-/*@".contains(c)),
                "{v:?} is not a version"
            );
        }
        match (&self.id, self.manager) {
            (Some(id), Manager::Mas) => ensure!(
                !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()),
                "a mas id is a number"
            ),
            (None, Manager::Mas) => bail!("{name}: an App Store app needs its id"),
            _ => {}
        }
        if self.manager == Manager::Shell {
            ensure!(
                self.install
                    .as_deref()
                    .is_some_and(|c| !c.trim().is_empty()),
                "{name}: a command of your own needs the command that installs it"
            );
        }
        if let Some(c) = &self.install {
            ensure!(
                c.len() <= 4000 && !c.contains('\0'),
                "{name}: install command too long"
            );
        }
        if let Some(p) = &self.path {
            ensure!(
                (p.starts_with("~/") || p.starts_with('/'))
                    && !p.chars().any(|c| "\"$`\\\n\r".contains(c)),
                "{p:?}: a path starts with ~/ or /"
            );
        }
        if let Some(d) = &self.description {
            ensure!(
                d.len() <= 300 && !d.contains('\n'),
                "{name}: description on one short line"
            );
        }
        Ok(())
    }
}

fn manifest_file(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE)
}

/// The manifest's entries, group by group; none without a file.
pub fn load(config_dir: &Path) -> Result<Vec<Entry>> {
    let file = manifest_file(config_dir);
    let Ok(text) = fs::read_to_string(&file) else {
        return Ok(Vec::new());
    };
    let tables: BTreeMap<String, Vec<Entry>> =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    let mut entries = Vec::new();
    for (key, list) in tables {
        let group = Group::ALL
            .into_iter()
            .find(|g| g.key() == key)
            .with_context(|| format!("{}: unknown group [[{key}]]", file.display()))?;
        entries.extend(list.into_iter().map(|e| Entry { group, ..e }));
    }
    entries.sort_by(|a, b| (a.group, a.manager, &a.name).cmp(&(b.group, b.manager, &b.name)));
    Ok(entries)
}

fn quoted(text: &str) -> String {
    toml::Value::String(text.into()).to_string()
}

/// Written by hand so the file reads well: each group under its title, an entry's fields in a fixed order.
// shortcut: comments added by hand to the file are not kept; keep them in `description` to keep them.
pub fn save(config_dir: &Path, entries: &[Entry]) -> Result<()> {
    for e in entries {
        e.check()?;
    }
    let mut text = String::from(
        "# What this machine's development environment is made of, kept by TermF (My environment) and synced with\n\
         # its config. Each entry: its manager, its name, its version when it counts and a few words on what it is;\n\
         # active = false keeps it here but out of the install script. `termf env install` installs what is missing.\n",
    );
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| (a.group, a.manager, &a.name).cmp(&(b.group, b.manager, &b.name)));
    for group in Group::ALL {
        let list: Vec<&&Entry> = sorted.iter().filter(|e| e.group == group).collect();
        if list.is_empty() {
            continue;
        }
        let _ = writeln!(text, "\n# {}", group.title());
        for e in list {
            let manager = serde_json::to_value(e.manager)?;
            let _ = writeln!(text, "\n[[{}]]", group.key());
            let _ = writeln!(
                text,
                "manager = {}",
                quoted(manager.as_str().unwrap_or_default())
            );
            let _ = writeln!(text, "name = {}", quoted(&e.name));
            for (key, value) in [
                ("version", &e.version),
                ("id", &e.id),
                ("description", &e.description),
                ("install", &e.install),
                ("path", &e.path),
            ] {
                if let Some(v) = value {
                    let _ = writeln!(text, "{key} = {}", quoted(v));
                }
            }
            if !e.active {
                text.push_str("active = false\n");
            }
        }
    }
    fs::create_dir_all(config_dir)?;
    crate::write_atomic(&manifest_file(config_dir), &text)
}

/// Adds the entries, or replaces those already there (same manager and name).
pub fn add(config_dir: &Path, new: Vec<Entry>) -> Result<Vec<Entry>> {
    let mut entries = load(config_dir)?;
    for e in new {
        e.check()?;
        match entries.iter_mut().find(|x| x.key() == e.key()) {
            Some(x) => *x = e,
            None => entries.push(e),
        }
    }
    save(config_dir, &entries)?;
    load(config_dir)
}

/// Replaces `previous` by `entry`, which may have another name or manager.
pub fn update(config_dir: &Path, previous: &Key, entry: Entry) -> Result<Vec<Entry>> {
    entry.check()?;
    let mut entries = load(config_dir)?;
    let at = entries
        .iter()
        .position(|e| &e.key() == previous)
        .with_context(|| format!("no entry {} in {FILE}", previous.name))?;
    ensure!(
        entry.key() == *previous || !entries.iter().any(|e| e.key() == entry.key()),
        "{} is already in {FILE}",
        entry.name
    );
    entries[at] = entry;
    save(config_dir, &entries)?;
    load(config_dir)
}

pub fn remove(config_dir: &Path, keys: &[Key]) -> Result<Vec<Entry>> {
    let mut entries = load(config_dir)?;
    let before = entries.len();
    entries.retain(|e| !keys.contains(&e.key()));
    ensure!(entries.len() < before, "none of these entries is in {FILE}");
    save(config_dir, &entries)?;
    load(config_dir)
}

fn script_file(config_dir: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.is_empty()
            && name.len() <= 40
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !name.starts_with('-'),
        "{name:?}: a script's name is lowercase letters, digits and dashes"
    );
    Ok(config_dir.join(SCRIPTS).join(format!("{name}.sh")))
}

/// The named scripts kept beside the manifest.
pub fn scripts(config_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(config_dir.join(SCRIPTS))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".sh")
                .map(str::to_owned)
        })
        .filter(|n| script_file(config_dir, n).is_ok())
        .collect();
    names.sort();
    names
}

pub fn read_script(config_dir: &Path, name: &str) -> Result<String> {
    let file = script_file(config_dir, name)?;
    fs::read_to_string(&file).with_context(|| format!("no script {name}"))
}

pub fn save_script(config_dir: &Path, name: &str, text: &str) -> Result<()> {
    let file = script_file(config_dir, name)?;
    ensure!(
        !text.trim().is_empty() && text.len() <= 256 << 10,
        "a script of some text"
    );
    fs::create_dir_all(config_dir.join(SCRIPTS))?;
    crate::write_atomic(&file, text)
}

pub fn delete_script(config_dir: &Path, name: &str) -> Result<()> {
    let file = script_file(config_dir, name)?;
    fs::remove_file(&file).with_context(|| format!("no script {name}"))
}

// ------------------------------------------------------------------------------------------------------ detection

/// What detection found: an entry as the manifest would keep it, and the version installed.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    #[serde(flatten)]
    pub entry: Entry,
    pub installed: Option<String>,
}

/// One source's detection, once over.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Scanned {
    pub source: String,
    pub found: Vec<Found>,
    pub error: Option<String>,
}

/// Where detection looks.
pub struct Machine {
    pub home: PathBuf,
    pub path: OsString,
    /// `/Applications` on macOS.
    pub apps: Option<PathBuf>,
}

impl Machine {
    pub fn current() -> Machine {
        Machine {
            home: crate::expand_tilde("~"),
            path: shell::login_path()
                .map(OsStr::to_os_string)
                .or_else(|| std::env::var_os("PATH"))
                .unwrap_or_default(),
            apps: cfg!(target_os = "macos").then(|| PathBuf::from("/Applications")),
        }
    }

    fn has(&self, program: &str) -> bool {
        shell::which_in(&self.path, program).is_some()
    }

    /// `~/…` under this machine's home.
    fn portable(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
            Err(_) => path.to_string_lossy().into_owned(),
        }
    }

    fn expand(&self, path: &str) -> PathBuf {
        match path.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None => PathBuf::from(path),
        }
    }

    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<(bool, String)> {
        let exe = shell::which_in(&self.path, program).with_context(|| format!("no {program}"))?;
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .current_dir(&self.home)
            .env("PATH", &self.path)
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("HOMEBREW_NO_ANALYTICS", "1")
            .env("HOMEBREW_NO_ENV_HINTS", "1");
        let (status, out, err) = process::run(cmd, b"", timeout)
            .with_context(|| format!("{program} {}", args.join(" ")))?;
        let out = String::from_utf8_lossy(&out).into_owned();
        if !status.success() && out.trim().is_empty() {
            bail!(
                "{program} {}: {}",
                args.join(" "),
                process::excerpt(&String::from_utf8_lossy(&err))
            );
        }
        Ok((status.success(), out))
    }

    /// Its output when it succeeded.
    fn output(&self, program: &str, args: &[&str], timeout: Duration) -> Result<String> {
        match self.run(program, args, timeout)? {
            (true, out) => Ok(out),
            (false, _) => bail!("{program} {} failed", args.join(" ")),
        }
    }
}

/// What a source found, and the binaries or apps it installed: those are not "installed by hand".
#[derive(Default)]
struct Out {
    found: Vec<Found>,
    owns: Vec<String>,
}

fn item(manager: Manager, name: &str, installed: Option<&str>) -> Found {
    let installed = installed
        .map(|v| v.trim().trim_start_matches('v').to_owned())
        .filter(|v| !v.is_empty());
    let mut entry = Entry::new(manager, name);
    if manager.versioned() {
        entry.version = installed.clone();
    }
    Found { entry, installed }
}

type Detector = fn(&Machine) -> Result<Out>;

/// Each source, and whether this machine has it.
fn detectors(m: &Machine) -> Vec<(&'static str, Detector)> {
    let all: [(&str, bool, Detector); 15] = [
        ("brew", m.has("brew"), brew),
        ("mise", m.has("mise"), mise),
        ("asdf", m.has("asdf"), asdf),
        ("fnm", m.has("fnm"), fnm),
        ("nvm", nvm_dir(m).join("nvm.sh").is_file(), nvm),
        ("volta", m.has("volta"), volta),
        ("rustup", m.has("rustup"), rustup),
        ("npm", m.has("npm"), npm),
        ("pnpm", m.has("pnpm"), pnpm),
        ("bun", m.has("bun"), bun),
        ("cargo", m.has("cargo"), cargo),
        ("pipx", m.has("pipx"), pipx),
        ("uv", m.has("uv"), uv),
        ("gh", m.has("gh"), gh),
        ("composer", m.has("composer"), composer),
    ];
    all.into_iter()
        .filter(|(_, present, _)| *present)
        .map(|(name, _, run)| (name, run))
        .collect()
}

/// What detection says as it goes: a source starts, a source is over.
pub enum Event<'a> {
    Started(&'a [String]),
    Scanned(&'a Scanned),
}

/// Asks each manager present, all at once, then looks for what was installed by hand and checks the manifest's
/// own commands; `on` hears each source as it ends.
pub fn scan(m: &Machine, manifest: &[Entry], on: &(dyn Fn(Event) + Sync)) -> Vec<Found> {
    let managers = detectors(m);
    let mut sources: Vec<String> = managers.iter().map(|(n, _)| (*n).to_owned()).collect();
    sources.push("bins".into());
    if m.apps.as_deref().is_some_and(Path::is_dir) {
        sources.push("apps".into());
    }
    let own = |e: &&Entry| matches!(e.manager, Manager::Shell | Manager::Manual);
    if manifest.iter().any(|e| own(&e)) {
        sources.push("checks".into());
    }
    on(Event::Started(&sources));
    let report = |source: &str, result: Result<Vec<Found>>| -> Vec<Found> {
        let (found, error) = match result {
            Ok(found) => (found, None),
            Err(e) => (Vec::new(), Some(format!("{e:#}"))),
        };
        let scanned = Scanned {
            source: source.into(),
            found,
            error,
        };
        on(Event::Scanned(&scanned));
        scanned.found
    };
    let report = &report;
    let outs: Vec<Out> = std::thread::scope(|s| {
        let running: Vec<_> = managers
            .iter()
            .map(|(name, run)| {
                s.spawn(move || {
                    let (owns, result) = match run(m) {
                        Ok(o) => (o.owns, Ok(o.found)),
                        Err(e) => (Vec::new(), Err(e)),
                    };
                    Out {
                        found: report(name, result),
                        owns,
                    }
                })
            })
            .collect();
        running
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let mut all: Vec<Found> = outs.iter().flat_map(|o| o.found.clone()).collect();
    let mut owned: BTreeSet<String> = outs.iter().flat_map(|o| o.owns.clone()).collect();
    owned.extend(RUSTUP_BINS.iter().map(|b| (*b).to_owned()));
    owned.extend(all.iter().map(|f| f.entry.name.clone()));
    owned.extend(manifest.iter().filter(own).map(|e| e.name.clone()));
    all.extend(report("bins", Ok(bins(m, &owned))));
    if sources.iter().any(|s| s == "apps") {
        all.extend(report("apps", Ok(apps(m, &owned))));
    }
    if sources.iter().any(|s| s == "checks") {
        let present = manifest
            .iter()
            .filter(own)
            .filter(|e| present(m, e))
            .map(|e| Found {
                entry: e.clone(),
                installed: None,
            })
            .collect();
        all.extend(report("checks", Ok(present)));
    }
    all
}

/// A command of your own or an install by hand is there: its path, else its name on the PATH or in a usual folder.
fn present(m: &Machine, e: &Entry) -> bool {
    match &e.path {
        Some(p) => m.expand(p).exists(),
        None => {
            let usual = std::env::join_paths(user_dirs(m)).unwrap_or_default();
            m.has(&e.name) || shell::which_in(&usual, &e.name).is_some()
        }
    }
}

fn brew(m: &Machine) -> Result<Out> {
    let dump = m.output("brew", &["bundle", "dump", "--describe", "--file=-"], SLOW)?;
    let mut out = Out {
        found: brewfile(&dump),
        owns: Vec::new(),
    };
    // Which apps of /Applications came from a cask; without it, they only show as installed by hand.
    if let Ok(info) = m.output(
        "brew",
        &["info", "--cask", "--json=v2", "--installed"],
        SLOW,
    ) && let Ok(json) = serde_json::from_str::<Value>(&info)
    {
        for cask in json["casks"].as_array().into_iter().flatten() {
            for artifact in cask["artifacts"].as_array().into_iter().flatten() {
                for app in artifact["app"].as_array().into_iter().flatten() {
                    if let Some(app) = app.as_str() {
                        out.owns.push(app.trim_end_matches(".app").to_owned());
                    }
                }
            }
        }
    }
    Ok(out)
}

/// A Brewfile's taps, formulae, casks, App Store apps and VS Code extensions, each with the description
/// `--describe` writes above it. What other managers list there (cargo, go…) is detected apart.
// shortcut: a line's options (`link: false`, `args: […]`) are not kept; add them to a script of your own.
fn brewfile(text: &str) -> Vec<Found> {
    let mut found = Vec::new();
    let mut said: Option<String> = None;
    for line in text.lines().map(str::trim) {
        if let Some(comment) = line.strip_prefix('#') {
            said = Some(comment.trim().to_owned()).filter(|c| !c.is_empty());
            continue;
        }
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        let manager = match kind {
            "tap" => Manager::Tap,
            "brew" => Manager::Brew,
            "cask" => Manager::Cask,
            "mas" => Manager::Mas,
            "vscode" => Manager::Vscode,
            _ => {
                said = None;
                continue;
            }
        };
        let Some(name) = rest
            .strip_prefix('"')
            .and_then(|r| r.split_once('"'))
            .map(|(n, _)| n)
        else {
            continue;
        };
        let mut f = item(manager, name, None);
        f.entry.description = said.take().filter(|d| d.len() <= 300);
        if manager == Manager::Mas {
            f.entry.id = rest
                .split_once("id:")
                .map(|(_, id)| id.trim().chars().take_while(char::is_ascii_digit).collect())
                .filter(|id: &String| !id.is_empty());
        }
        if f.entry.check().is_ok() {
            found.push(f);
        }
    }
    found
}

/// `{"dependencies": {name: {"version": …}}}`, as npm and pnpm print it.
fn dependencies(manager: Manager, json: &Value) -> Vec<Found> {
    json["dependencies"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(name, _)| !matches!(name.as_str(), "npm" | "corepack" | "pnpm"))
        .map(|(name, d)| item(manager, name, d["version"].as_str()))
        .collect()
}

fn npm(m: &Machine) -> Result<Out> {
    // npm fails on a peer it finds missing, yet prints the list.
    let (_, text) = m.run("npm", &["ls", "-g", "--depth=0", "--json"], QUICK)?;
    let json: Value = serde_json::from_str(&text).context("npm ls")?;
    Ok(Out {
        found: dependencies(Manager::Npm, &json),
        owns: Vec::new(),
    })
}

fn pnpm(m: &Machine) -> Result<Out> {
    let (_, text) = m.run("pnpm", &["ls", "-g", "--depth=0", "--json"], QUICK)?;
    let json: Value = serde_json::from_str(&text).context("pnpm ls")?;
    let found = match &json {
        Value::Array(list) => list
            .iter()
            .flat_map(|j| dependencies(Manager::Pnpm, j))
            .collect(),
        other => dependencies(Manager::Pnpm, other),
    };
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

/// `├── typescript@5.4.5` lines of `bun pm ls -g`.
fn bun(m: &Machine) -> Result<Out> {
    let text = m.output("bun", &["pm", "ls", "-g"], QUICK)?;
    let found = text
        .lines()
        .filter_map(|l| l.split_once("── ").map(|(_, p)| p.trim()))
        .filter_map(|p| {
            let at = p.rfind('@').filter(|&i| i > 0)?;
            Some(item(Manager::Bun, &p[..at], Some(&p[at + 1..])))
        })
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

/// `ripgrep v14.1.0:` then its binaries, indented.
fn cargo(m: &Machine) -> Result<Out> {
    let text = m.output("cargo", &["install", "--list"], QUICK)?;
    let mut out = Out::default();
    for line in text.lines() {
        if line.starts_with(char::is_whitespace) {
            out.owns.push(line.trim().to_owned());
        } else if let Some(head) = line.trim().strip_suffix(':') {
            let mut words = head.split_whitespace();
            if let Some(name) = words.next() {
                out.found.push(item(Manager::Cargo, name, words.next()));
            }
        }
    }
    Ok(out)
}

/// rustup's proxies, in `~/.cargo/bin`.
const RUSTUP_BINS: &[&str] = &[
    "cargo",
    "cargo-clippy",
    "cargo-fmt",
    "cargo-miri",
    "clippy-driver",
    "rls",
    "rust-analyzer",
    "rust-gdb",
    "rust-gdbgui",
    "rust-lldb",
    "rustc",
    "rustdoc",
    "rustfmt",
    "rustup",
];

/// `stable-aarch64-apple-darwin (default)` → `stable`.
fn rustup(m: &Machine) -> Result<Out> {
    let text = m.output("rustup", &["toolchain", "list"], QUICK)?;
    const ARCHES: &[&str] = &[
        "x86_64",
        "aarch64",
        "i686",
        "i586",
        "arm",
        "armv7",
        "riscv64gc",
        "powerpc64le",
        "s390x",
        "loongarch64",
    ];
    let found = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .filter(|t| !t.starts_with("no"))
        .map(|toolchain| {
            let parts: Vec<&str> = toolchain.split('-').collect();
            let end = parts
                .iter()
                .position(|p| ARCHES.contains(p))
                .unwrap_or(parts.len());
            let mut f = item(Manager::Rustup, &parts[..end.max(1)].join("-"), None);
            f.entry.description = Some("Rust toolchain".into());
            f
        })
        .collect();
    Ok(Out {
        found,
        owns: RUSTUP_BINS.iter().map(|b| (*b).to_owned()).collect(),
    })
}

fn pipx(m: &Machine) -> Result<Out> {
    let text = m.output("pipx", &["list", "--json"], QUICK)?;
    let json: Value = serde_json::from_str(&text).context("pipx list")?;
    let mut out = Out::default();
    for (name, venv) in json["venvs"].as_object().into_iter().flatten() {
        let main = &venv["metadata"]["main_package"];
        out.found
            .push(item(Manager::Pipx, name, main["package_version"].as_str()));
        out.owns.extend(
            main["apps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|a| a.as_str().map(str::to_owned)),
        );
    }
    Ok(out)
}

/// `ruff v0.6.9` then `- ruff`, its binaries.
fn uv(m: &Machine) -> Result<Out> {
    let text = m.output("uv", &["tool", "list"], QUICK)?;
    let mut out = Out::default();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(bin) = line.strip_prefix("- ") {
            out.owns.push(bin.trim().to_owned());
        } else {
            let mut words = line.split_whitespace();
            if let Some(name) = words.next().filter(|n| !n.starts_with("No")) {
                out.found.push(item(Manager::Uv, name, words.next()));
            }
        }
    }
    Ok(out)
}

/// `gh dash  dlvhdr/gh-dash  v4.7.1`: the extension's repository, then its version.
fn gh(m: &Machine) -> Result<Out> {
    let text = m.output("gh", &["extension", "list"], QUICK)?;
    let found = text
        .lines()
        .filter_map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            let at = words.iter().position(|w| w.contains('/'))?;
            Some(item(Manager::Gh, words[at], words.get(at + 1).copied()))
        })
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

fn composer(m: &Machine) -> Result<Out> {
    let text = m.output(
        "composer",
        &["global", "show", "--direct", "--format=json"],
        QUICK,
    )?;
    let json: Value = serde_json::from_str(&text).context("composer global show")?;
    let found = json["installed"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let mut f = item(
                Manager::Composer,
                p["name"].as_str()?,
                p["version"].as_str(),
            );
            f.entry.description = p["description"].as_str().map(|d| {
                d.lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(300)
                    .collect()
            });
            Some(f)
        })
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

/// The versions the global config asks for: `{tool: [{version, requested_version}]}`.
fn mise(m: &Machine) -> Result<Out> {
    let text = m.output("mise", &["ls", "--global", "--json"], QUICK)?;
    let json: Value = serde_json::from_str(&text).context("mise ls")?;
    let found = json
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(tool, versions)| {
            let v = versions.as_array()?.first()?;
            let installed = v["version"].as_str();
            let mut f = item(Manager::Mise, tool, installed);
            f.entry.version = v["requested_version"]
                .as_str()
                .or(installed)
                .map(str::to_owned);
            Some(f)
        })
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

/// asdf's global versions are those of `~/.tool-versions`.
fn asdf(m: &Machine) -> Result<Out> {
    let text = fs::read_to_string(m.home.join(".tool-versions")).unwrap_or_default();
    let found = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or_default())
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            Some(item(Manager::Asdf, words.next()?, words.next()))
        })
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

/// `* v20.11.0 default`: the version fnm uses by default.
fn fnm(m: &Machine) -> Result<Out> {
    let text = m.output("fnm", &["list"], QUICK)?;
    let found = text
        .lines()
        .filter(|l| l.split_whitespace().any(|w| w == "default"))
        .filter_map(|l| l.split_whitespace().find(|w| w.starts_with('v')))
        .map(|v| item(Manager::Fnm, "node", Some(v)))
        .take(1)
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

fn nvm_dir(m: &Machine) -> PathBuf {
    m.home.join(".nvm")
}

/// nvm is a shell function: its default version is the file it keeps.
fn nvm(m: &Machine) -> Result<Out> {
    let default = fs::read_to_string(nvm_dir(m).join("alias").join("default")).unwrap_or_default();
    let found = Some(default.trim())
        .filter(|v| !v.is_empty())
        .map(|v| item(Manager::Nvm, "node", Some(v)))
        .into_iter()
        .collect();
    Ok(Out {
        found,
        owns: Vec::new(),
    })
}

/// `runtime node@20.11.0 (default)`, `package typescript@5.3.3 / tsc, tsserver / node@20.11.0 (default)`.
fn volta(m: &Machine) -> Result<Out> {
    let text = m.output("volta", &["list", "--format", "plain"], QUICK)?;
    let mut out = Out::default();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let (Some(kind), Some(tool)) = (words.next(), words.next()) else {
            continue;
        };
        let Some(at) = tool.rfind('@').filter(|&i| i > 0) else {
            continue;
        };
        let (name, version) = (&tool[..at], &tool[at + 1..]);
        let mut f = item(Manager::Volta, name, Some(version));
        if kind == "package" {
            f.entry.version = None;
            if let Some(bins) = line.split(" / ").nth(1) {
                out.owns
                    .extend(bins.split(',').map(|b| b.trim().to_owned()));
            }
        } else {
            f.entry.group = Group::Languages;
            f.entry.version = Some(version.to_owned());
        }
        out.found.push(f);
    }
    Ok(out)
}

/// Where binaries installed by hand land, and the folders of the PATH under the home that no manager keeps.
fn user_dirs(m: &Machine) -> Vec<PathBuf> {
    // Folders a manager keeps: what they hold is that manager's, found above or reinstalled with it.
    const KEPT: &[&str] = &[
        ".nvm",
        ".volta",
        ".asdf",
        "mise",
        ".fnm",
        "fnm_multishells",
        ".pyenv",
        ".rbenv",
        ".sdkman",
        "pnpm",
        ".npm-global",
        "node_modules",
        "vendor",
        ".rustup",
        ".nix-profile",
        ".orbstack",
        ".docker",
    ];
    let mut dirs: Vec<PathBuf> = [".local/bin", "bin", ".cargo/bin", ".bun/bin", ".deno/bin"]
        .iter()
        .map(|d| m.home.join(d))
        .collect();
    for dir in std::env::split_paths(&m.path) {
        let kept = dir
            .components()
            .any(|c| KEPT.contains(&c.as_os_str().to_string_lossy().as_ref()));
        if dir.starts_with(&m.home) && dir != m.home && !kept && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// Programs in the user's own folders that no manager installed.
fn bins(m: &Machine, owned: &BTreeSet<String>) -> Vec<Found> {
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for dir in user_dirs(m) {
        let Ok(list) = fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = list.flatten().map(|e| e.path()).collect();
        files.sort();
        for file in files {
            let Some(name) = file.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            let name = name.strip_suffix(".exe").unwrap_or(name).to_owned();
            if name.starts_with('.')
                || !file.is_file()
                || !shell::executable(&file)
                || owned.contains(&name)
                || !seen.insert(name.clone())
            {
                continue;
            }
            let mut f = item(Manager::Manual, &name, None);
            f.entry.path = Some(m.portable(&file));
            if f.entry.check().is_ok() {
                found.push(f);
            }
        }
    }
    found
}

/// Apps of `/Applications` no cask installed: those of macOS and the App Store's that `mas` lists are left out.
fn apps(m: &Machine, owned: &BTreeSet<String>) -> Vec<Found> {
    const SYSTEM: &[&str] = &["Safari"];
    let Some(dir) = m.apps.as_deref() else {
        return Vec::new();
    };
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_suffix(".app")
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .filter(|n| !owned.contains(n) && !SYSTEM.contains(&n.as_str()) && !n.starts_with('.'))
        .filter_map(|name| {
            let mut f = item(Manager::Manual, &name, None);
            f.entry.group = Group::Apps;
            f.entry.path = Some(m.portable(&dir.join(format!("{name}.app"))));
            f.entry.check().is_ok().then_some(f)
        })
        .collect()
}

// --------------------------------------------------------------------------------------------------------- script

fn sq(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `"$HOME/…"` for `~/…`; checked paths hold no character that would end the double quotes.
fn path_sh(path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => format!("\"$HOME/{rest}\""),
        None => format!("\"{path}\""),
    }
}

fn label(e: &Entry) -> String {
    match &e.version {
        Some(v) => format!("{} {v}", e.name),
        None => e.name.clone(),
    }
}

const HEAD: &str = r#"#!/bin/sh
# Installs what TermF's env.toml lists, on macOS or Linux, and skips what is already there: run it again at will.
# Written by `termf env script`: change env.toml (or My environment in TermF), not this file.

step() { printf '\n==> %s\n' "$1"; }
fail() { printf '\nTermF: %s failed, so the script stopped there. Fix it, then run the script again.\n' "$1" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }
need() { have "$1" || { printf '\nTermF: %s is not installed, and the next entries need it. Add it to your environment, then run the script again.\n' "$1" >&2; exit 1; }; }

export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.bun/bin:$HOME/.deno/bin:$HOME/.volta/bin:$HOME/.local/share/mise/shims:$HOME/.asdf/shims:$PATH"
"#;

/// The script that installs the active entries, in an order that works: Homebrew and its Brewfile, the commands of
/// your own, languages, then their packages. Each step skips what is there; the first one that fails stops it.
pub fn script(entries: &[Entry]) -> String {
    let active: Vec<&Entry> = {
        let mut list: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.active && e.check().is_ok())
            .collect();
        list.sort_by(|a, b| (a.manager, &a.name).cmp(&(b.manager, &b.name)));
        list
    };
    let mut s = String::from(HEAD);
    let of = |m: Manager| active.iter().filter(move |e| e.manager == m).copied();

    let brewed: Vec<&Entry> = active
        .iter()
        .filter(|e| e.manager.brewfile().is_some())
        .copied()
        .collect();
    if !brewed.is_empty() {
        s.push_str(
            r#"
# Homebrew, installed first on macOS, then its Brewfile: taps, formulae, casks, App Store apps, VS Code extensions.
if [ "$(uname -s)" = Darwin ] && ! have brew; then
  step "Homebrew"
  /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" || fail "Installing Homebrew"
fi
for brew in /opt/homebrew/bin/brew /usr/local/bin/brew /home/linuxbrew/.linuxbrew/bin/brew; do
  if [ -x "$brew" ]; then eval "$("$brew" shellenv)"; break; fi
done
if have brew; then
  step "Homebrew: what the Brewfile lists"
  brew bundle --no-upgrade --file=- <<'BREWFILE' || fail "brew bundle"
"#,
        );
        for e in &brewed {
            if let Some(d) = &e.description {
                let _ = writeln!(s, "# {d}");
            }
            let kind = e.manager.brewfile().unwrap_or_default();
            match &e.id {
                Some(id) if e.manager == Manager::Mas => {
                    let _ = writeln!(s, "{kind} \"{}\", id: {id}", e.name);
                }
                _ => {
                    let _ = writeln!(s, "{kind} \"{}\"", e.name);
                }
            }
        }
        let _ = write!(
            s,
            "BREWFILE\nelse\n  printf 'Homebrew is not installed here: its {} entries are skipped.\\n'\nfi\n",
            brewed.len()
        );
    }

    let own = |s: &mut String, e: &Entry| {
        let Some(command) = e.install.as_deref().filter(|c| !c.trim().is_empty()) else {
            return;
        };
        let there = match &e.path {
            Some(p) => format!("[ -e {} ]", path_sh(p)),
            None => format!("have {}", sq(&e.name)),
        };
        if let Some(d) = &e.description {
            let _ = writeln!(s, "# {d}");
        }
        let _ = writeln!(
            s,
            "if ! {there}; then\n  step {name}\n  ( {command}\n  ) || fail {name}\nfi",
            name = sq(&e.name)
        );
    };
    let shells: Vec<&Entry> = of(Manager::Shell).collect();
    if !shells.is_empty() {
        s.push_str("\n# Your own install commands.\n");
        for e in shells {
            own(&mut s, e);
        }
    }

    let section = |s: &mut String,
                   m: Manager,
                   title: &str,
                   need: Option<&str>,
                   line: &dyn Fn(&Entry) -> String| {
        let list: Vec<&Entry> = of(m).collect();
        if list.is_empty() {
            return;
        }
        let _ = writeln!(s, "\n# {title}");
        if let Some(program) = need {
            let _ = writeln!(s, "need {program}");
        }
        for e in list {
            let _ = writeln!(s, "{}", line(e));
        }
    };
    let version = |e: &Entry, sep: &str| match &e.version {
        Some(v) => sq(&format!("{}{sep}{v}", e.name)),
        None => sq(&e.name),
    };
    let step = |e: &Entry, what: &str, run: String| {
        let said = sq(&format!("{what}: {}", label(e)));
        format!("step {said}\n{run} || fail {said}")
    };
    let skip = |e: &Entry, what: &str, listed: String, run: String| {
        let said = sq(&format!("{what}: {}", label(e)));
        format!("{listed} || {{ step {said}; {run} || fail {said}; }}")
    };

    section(
        &mut s,
        Manager::Mise,
        "Languages: mise, global versions",
        Some("mise"),
        &|e| step(e, "mise", format!("mise use --global {}", version(e, "@"))),
    );
    section(
        &mut s,
        Manager::Asdf,
        "Languages: asdf, global versions",
        Some("asdf"),
        &|e| {
            let (n, v) = (sq(&e.name), sq(e.version.as_deref().unwrap_or("latest")));
            step(
                e,
                "asdf",
                format!(
                    "{{ asdf plugin add {n} >/dev/null 2>&1; asdf install {n} {v} && {{ asdf set -u {n} {v} 2>/dev/null || asdf global {n} {v}; }}; }}"
                ),
            )
        },
    );
    section(
        &mut s,
        Manager::Fnm,
        "Languages: fnm, the default Node.js",
        Some("fnm"),
        &|e| {
            let v = e.version.as_deref().unwrap_or("lts-latest");
            let q = sq(v);
            step(
                e,
                "fnm",
                format!("{{ fnm list | grep -q \"v{v}\" || fnm install {q}; }} && fnm default {q}"),
            )
        },
    );
    if of(Manager::Nvm).next().is_some() {
        s.push_str(
            "\n# Languages: nvm, the default Node.js\nexport NVM_DIR=\"${NVM_DIR:-$HOME/.nvm}\"\n[ -s \"$NVM_DIR/nvm.sh\" ] || need nvm\n. \"$NVM_DIR/nvm.sh\"\n",
        );
        for e in of(Manager::Nvm) {
            let v = sq(e.version.as_deref().unwrap_or("--lts"));
            let _ = writeln!(
                s,
                "{}",
                step(
                    e,
                    "nvm",
                    format!("{{ nvm install {v} && nvm alias default {v}; }}")
                )
            );
        }
    }
    section(
        &mut s,
        Manager::Volta,
        "Languages and tools: Volta",
        Some("volta"),
        &|e| step(e, "volta", format!("volta install {}", version(e, "@"))),
    );
    section(
        &mut s,
        Manager::Rustup,
        "Languages: Rust toolchains",
        Some("rustup"),
        &|e| {
            step(
                e,
                "rustup",
                format!("rustup toolchain install {}", sq(&e.name)),
            )
        },
    );
    section(
        &mut s,
        Manager::Npm,
        "Packages: npm, global",
        Some("npm"),
        &|e| {
            skip(
                e,
                "npm",
                format!("npm ls -g --depth=0 {} >/dev/null 2>&1", sq(&e.name)),
                format!("npm install -g {}", version(e, "@")),
            )
        },
    );
    section(
        &mut s,
        Manager::Pnpm,
        "Packages: pnpm, global",
        Some("pnpm"),
        &|e| {
            skip(
                e,
                "pnpm",
                format!(
                    "pnpm ls -g --depth=0 --parseable 2>/dev/null | grep -q \"/{}$\"",
                    e.name
                ),
                format!("pnpm add -g {}", version(e, "@")),
            )
        },
    );
    section(
        &mut s,
        Manager::Bun,
        "Packages: bun, global",
        Some("bun"),
        &|e| {
            skip(
                e,
                "bun",
                format!("bun pm ls -g 2>/dev/null | grep -q \" {}@\"", e.name),
                format!("bun add -g {}", version(e, "@")),
            )
        },
    );
    section(
        &mut s,
        Manager::Cargo,
        "Packages: cargo install",
        Some("cargo"),
        &|e| {
            let pin = e
                .version
                .as_ref()
                .map(|v| format!(" --version {}", sq(v)))
                .unwrap_or_default();
            skip(
                e,
                "cargo",
                format!("cargo install --list | grep -q \"^{} v\"", e.name),
                format!("cargo install {}{pin}", sq(&e.name)),
            )
        },
    );
    section(
        &mut s,
        Manager::Pipx,
        "Packages: pipx",
        Some("pipx"),
        &|e| {
            skip(
                e,
                "pipx",
                format!("pipx list --short 2>/dev/null | grep -q \"^{} \"", e.name),
                format!("pipx install {}", version(e, "==")),
            )
        },
    );
    section(&mut s, Manager::Uv, "Packages: uv tool", Some("uv"), &|e| {
        skip(
            e,
            "uv",
            format!("uv tool list 2>/dev/null | grep -q \"^{} \"", e.name),
            format!("uv tool install {}", version(e, "==")),
        )
    });
    section(
        &mut s,
        Manager::Gh,
        "GitHub CLI extensions",
        Some("gh"),
        &|e| {
            skip(
                e,
                "gh",
                format!("gh extension list 2>/dev/null | grep -qF {}", sq(&e.name)),
                format!("gh extension install {}", sq(&e.name)),
            )
        },
    );
    section(
        &mut s,
        Manager::Composer,
        "Packages: composer, global",
        Some("composer"),
        &|e| {
            skip(
                e,
                "composer",
                format!("composer global show {} >/dev/null 2>&1", sq(&e.name)),
                format!("composer global require {}", version(e, ":")),
            )
        },
    );

    let manual: Vec<&Entry> = of(Manager::Manual).collect();
    let (known, unknown): (Vec<&Entry>, Vec<&Entry>) = manual
        .into_iter()
        .partition(|e| e.install.as_deref().is_some_and(|c| !c.trim().is_empty()));
    if !known.is_empty() {
        s.push_str("\n# Installed by hand elsewhere, with the command that installs each.\n");
        for e in known {
            own(&mut s, e);
        }
    }
    if !unknown.is_empty() {
        s.push_str("\n# Installed by hand, with no install command yet: only said when missing.\nmissing=''\n");
        for e in unknown {
            let there = match &e.path {
                Some(p) => format!("[ -e {} ]", path_sh(p)),
                None => format!("have {}", sq(&e.name)),
            };
            let _ = writeln!(s, "{there} || missing=\"$missing, \"{}", sq(&e.name));
        }
        s.push_str(
            "[ -z \"$missing\" ] || printf '\\nInstalled by hand elsewhere, with no install command yet, so missing here: %s\\n' \"${missing#, }\"\n",
        );
    }
    s.push_str("\nprintf '\\nDone: what env.toml lists is installed.\\n'\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("termf-devenv-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(manager: Manager, name: &str) -> Entry {
        Entry::new(manager, name)
    }

    #[test]
    fn the_manifest_reads_by_group_and_keeps_each_field() {
        let dir = temp("manifest");
        assert!(load(&dir).unwrap().is_empty());
        let mut node = entry(Manager::Mise, "node");
        node.version = Some("22".into());
        node.description = Some("JavaScript runtime".into());
        let mut claude = entry(Manager::Shell, "claude");
        claude.install = Some("curl -fsSL https://claude.ai/install.sh | bash".into());
        let mut xcode = entry(Manager::Mas, "Xcode");
        xcode.id = Some("497799835".into());
        let mut off = entry(Manager::Npm, "left-pad");
        off.active = false;
        let entries = add(&dir, vec![node.clone(), claude.clone(), xcode, off]).unwrap();
        assert_eq!(entries.len(), 4);
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.contains("# Languages and their versions\n\n[[languages]]\nmanager = \"mise\"\nname = \"node\"\nversion = \"22\""));
        assert!(text.contains("[[apps]]\nmanager = \"mas\"\nname = \"Xcode\"\nid = \"497799835\""));
        assert!(text.contains("active = false"));
        assert_eq!(load(&dir).unwrap(), entries);

        let mut moved = claude.clone();
        moved.group = Group::Editors;
        let entries = update(&dir, &claude.key(), moved).unwrap();
        assert_eq!(
            entries.iter().find(|e| e.name == "claude").unwrap().group,
            Group::Editors
        );
        assert!(
            update(&dir, &claude.key(), node.clone()).is_err(),
            "node is already there"
        );
        let entries = remove(&dir, &[node.key()]).unwrap();
        assert_eq!(entries.len(), 3);
        assert!(remove(&dir, &[node.key()]).is_err());

        let mut bad = entry(Manager::Npm, "x\"; rm -rf ~");
        assert!(add(&dir, vec![bad.clone()]).is_err());
        bad.name = "typescript".into();
        bad.version = Some("5$(id)".into());
        assert!(bad.check().is_err());
        assert!(
            entry(Manager::Shell, "x").check().is_err(),
            "a command of your own needs it"
        );
        assert!(entry(Manager::Gh, "dash").check().is_err(), "owner/repo");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn named_scripts_live_beside_the_manifest() {
        let dir = temp("scripts");
        assert!(scripts(&dir).is_empty());
        save_script(&dir, "laptop", "#!/bin/sh\necho hi\n").unwrap();
        assert_eq!(scripts(&dir), ["laptop"]);
        assert_eq!(read_script(&dir, "laptop").unwrap(), "#!/bin/sh\necho hi\n");
        for name in ["../x", "Laptop", ".hidden", "", "a/b"] {
            assert!(save_script(&dir, name, "x").is_err(), "{name}");
        }
        delete_script(&dir, "laptop").unwrap();
        assert!(read_script(&dir, "laptop").is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_brewfile_gives_its_entries_and_their_descriptions() {
        let dump = "tap \"oven-sh/bun\"\n# Search tool like grep\nbrew \"ripgrep\"\nbrew \"postgresql@16\", restart_service: :changed\n\
                    # Code editor\ncask \"visual-studio-code\"\nmas \"Xcode\", id: 497799835\nvscode \"dbaeumer.vscode-eslint\"\ncargo \"bat\"\n";
        let found = brewfile(dump);
        let names: Vec<_> = found
            .iter()
            .map(|f| (f.entry.manager, f.entry.name.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                (Manager::Tap, "oven-sh/bun"),
                (Manager::Brew, "ripgrep"),
                (Manager::Brew, "postgresql@16"),
                (Manager::Cask, "visual-studio-code"),
                (Manager::Mas, "Xcode"),
                (Manager::Vscode, "dbaeumer.vscode-eslint"),
            ]
        );
        assert_eq!(
            found[1].entry.description.as_deref(),
            Some("Search tool like grep")
        );
        assert_eq!(found[2].entry.description, None);
        assert_eq!(found[3].entry.group, Group::Apps);
        assert_eq!(found[4].entry.id.as_deref(), Some("497799835"));
        assert_eq!(found[5].entry.group, Group::Editors);
    }

    #[test]
    fn the_script_installs_in_order_and_skips_what_is_there() {
        let mut node = entry(Manager::Mise, "node");
        node.version = Some("22".into());
        let mut claude = entry(Manager::Shell, "claude");
        claude.install = Some("curl -fsSL https://claude.ai/install.sh | bash".into());
        let mut ts = entry(Manager::Npm, "typescript");
        ts.version = Some("5.4.5".into());
        let mut off = entry(Manager::Cargo, "bat");
        off.active = false;
        let mut xcode = entry(Manager::Mas, "Xcode");
        xcode.id = Some("497799835".into());
        let mut app = entry(Manager::Manual, "Herd");
        app.path = Some("/Applications/Herd.app".into());
        let entries = [
            ts,
            node,
            claude,
            off,
            xcode,
            entry(Manager::Brew, "ripgrep"),
            entry(Manager::Gh, "dlvhdr/gh-dash"),
            app,
        ];
        let s = script(&entries);
        let at = |needle: &str| s.find(needle).unwrap_or_else(|| panic!("{needle} in\n{s}"));
        assert!(at("install.sh)\" || fail \"Installing Homebrew\"") < at("brew \"ripgrep\""));
        assert!(s.contains("mas \"Xcode\", id: 497799835\n"));
        assert!(at("brew \"ripgrep\"") < at("( curl -fsSL https://claude.ai/install.sh | bash"));
        assert!(at("if ! have 'claude'; then") < at("mise use --global 'node@22'"));
        assert!(at("mise use --global 'node@22'") < at("npm install -g 'typescript@5.4.5'"));
        assert!(s.contains("npm ls -g --depth=0 'typescript' >/dev/null 2>&1 || {"));
        assert!(s.contains("gh extension list 2>/dev/null | grep -qF 'dlvhdr/gh-dash' ||"));
        assert!(s.contains("[ -e \"/Applications/Herd.app\" ] || missing=\"$missing, \"'Herd'\n"));
        assert!(!s.contains("bat"), "an entry turned off is left out");
        assert!(
            script(&[entry(Manager::Npm, "x")]).find("brew").is_none(),
            "no Homebrew without its entries"
        );

        #[cfg(unix)]
        {
            let dir = temp("syntax");
            let file = dir.join("install.sh");
            fs::write(&file, &s).unwrap();
            let ok = Command::new("sh").arg("-n").arg(&file).status().unwrap();
            assert!(ok.success(), "sh -n accepts it");
            fs::remove_dir_all(&dir).unwrap();
        }
    }

    #[cfg(unix)]
    fn program(dir: &Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let file = dir.join(name);
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Fake managers on a throwaway PATH and HOME: what each prints is what the real one does; none installs.
    #[cfg(unix)]
    #[test]
    fn detection_asks_the_managers_present_and_finds_what_was_installed_by_hand() {
        let root = temp("scan");
        let (bin, home, apps) = (
            root.join("bin"),
            root.join("home"),
            root.join("Applications"),
        );
        let local = home.join(".local/bin");
        for d in [&bin, &home, &apps, &local, &home.join(".cargo/bin")] {
            fs::create_dir_all(d).unwrap();
        }
        program(
            &bin,
            "brew",
            r#"case "$1" in
bundle) printf 'tap "oven-sh/bun"\n# Search tool like grep\nbrew "ripgrep"\ncask "visual-studio-code"\nvscode "dbaeumer.vscode-eslint"\n' ;;
info) printf '{"casks":[{"token":"visual-studio-code","artifacts":[{"app":["Visual Studio Code.app"]}]}]}' ;;
*) exit 1 ;;
esac"#,
        );
        program(
            &bin,
            "npm",
            r#"printf '{"dependencies":{"typescript":{"version":"5.4.5"},"npm":{"version":"10.0.0"}}}'; exit 1"#,
        );
        program(
            &bin,
            "cargo",
            "printf 'ripgrep v14.1.0:\\n    rg\\nbat v0.24.0:\\n    bat\\n'",
        );
        program(&bin, "uv", "printf 'ruff v0.6.9\\n- ruff\\n'");
        program(
            &bin,
            "mise",
            r#"printf '{"node":[{"version":"22.11.0","requested_version":"22"}]}'"#,
        );
        program(&local, "ruff", "");
        program(&local, "claude", "");
        program(&local, "mytool", "");
        program(&home.join(".cargo/bin"), "rg", "");
        program(&home.join(".cargo/bin"), "cargo", "");
        fs::write(local.join("notes.txt"), "not a program").unwrap();
        for app in ["Visual Studio Code", "Herd", "Safari"] {
            fs::create_dir_all(apps.join(format!("{app}.app"))).unwrap();
        }
        let m = Machine {
            home: home.clone(),
            path: std::env::join_paths([&bin, &local]).unwrap(),
            apps: Some(apps.clone()),
        };
        let mut claude = entry(Manager::Shell, "claude");
        claude.install = Some("curl -fsSL https://claude.ai/install.sh | bash".into());
        let mut gone = entry(Manager::Manual, "Gone");
        gone.path = Some("~/Applications/Gone.app".into());

        let started = std::sync::Mutex::new(Vec::new());
        let ended = std::sync::Mutex::new(Vec::new());
        let on = |e: Event| match e {
            Event::Started(list) => started.lock().unwrap().extend(list.iter().cloned()),
            Event::Scanned(s) => ended
                .lock()
                .unwrap()
                .push((s.source.clone(), s.error.is_some())),
        };
        let all = scan(&m, &[claude, gone], &on);
        let keys: BTreeSet<(Manager, String)> = all
            .iter()
            .map(|f| (f.entry.manager, f.entry.name.clone()))
            .collect();
        let has = |m: Manager, n: &str| keys.contains(&(m, n.to_owned()));
        assert!(
            has(Manager::Tap, "oven-sh/bun")
                && has(Manager::Brew, "ripgrep")
                && has(Manager::Vscode, "dbaeumer.vscode-eslint")
        );
        assert!(
            has(Manager::Npm, "typescript") && !has(Manager::Npm, "npm"),
            "npm itself is not a package of yours"
        );
        assert!(has(Manager::Cargo, "bat") && has(Manager::Uv, "ruff"));
        let node = all.iter().find(|f| f.entry.name == "node").unwrap();
        assert_eq!(
            (node.entry.version.as_deref(), node.installed.as_deref()),
            (Some("22"), Some("22.11.0"))
        );
        let ts = all.iter().find(|f| f.entry.name == "typescript").unwrap();
        assert_eq!(
            (ts.entry.version.as_deref(), ts.installed.as_deref()),
            (None, Some("5.4.5")),
            "a package's version does not count"
        );
        assert!(has(Manager::Manual, "mytool"), "installed by hand");
        let mytool = all.iter().find(|f| f.entry.name == "mytool").unwrap();
        assert_eq!(mytool.entry.path.as_deref(), Some("~/.local/bin/mytool"));
        for owned in ["ruff", "rg", "cargo", "notes.txt", "claude"] {
            assert!(
                !has(Manager::Manual, owned),
                "{owned} is a manager's, not a file or installed by hand"
            );
        }
        assert!(
            has(Manager::Shell, "claude"),
            "the manifest's own command is there"
        );
        assert!(!has(Manager::Manual, "Gone"));
        assert!(has(Manager::Manual, "Herd"), "an app no cask installed");
        assert!(!has(Manager::Manual, "Visual Studio Code") && !has(Manager::Manual, "Safari"));
        let herd = all.iter().find(|f| f.entry.name == "Herd").unwrap();
        assert_eq!(herd.entry.group, Group::Apps);

        let started = started.into_inner().unwrap();
        assert_eq!(
            started,
            [
                "brew", "mise", "npm", "cargo", "uv", "bins", "apps", "checks"
            ]
        );
        let ended = ended.into_inner().unwrap();
        assert_eq!(ended.len(), started.len());
        assert!(ended.iter().all(|(_, failed)| !failed));
        fs::remove_dir_all(&root).unwrap();
    }

    /// The script itself, run twice against fake managers in a throwaway HOME: it installs, then skips what is
    /// there, and stops on the first failure saying which. No Homebrew entry, so no real `brew` is ever reached.
    #[cfg(unix)]
    #[test]
    fn the_script_installs_then_skips_then_stops_on_a_failure() {
        let root = temp("run");
        let (bin, home) = (root.join("bin"), root.join("home"));
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(home.join(".local/bin")).unwrap();
        let log = root.join("calls.log");
        let log_q = log.display();
        program(
            &bin,
            "npm",
            &format!(
                r#"echo "npm $*" >> '{log_q}'
case "$1" in
ls) [ -f '{root}/npm-installed' ] ;;
install) [ -f '{root}/npm-broken' ] && exit 1; touch '{root}/npm-installed' ;;
esac"#,
                root = root.display()
            ),
        );
        program(&bin, "mise", &format!("echo \"mise $*\" >> '{log_q}'"));
        program(&bin, "cargo", &format!("echo \"cargo $*\" >> '{log_q}'"));
        let mut node = entry(Manager::Mise, "node");
        node.version = Some("22".into());
        let mut tool = entry(Manager::Shell, "mytool");
        tool.install = Some(format!(
            "printf '#!/bin/sh\\n' > \"$HOME/.local/bin/mytool\" && chmod +x \"$HOME/.local/bin/mytool\" && echo mytool >> '{log_q}'"
        ));
        let entries = vec![
            entry(Manager::Npm, "typescript"),
            node,
            tool,
            entry(Manager::Cargo, "bat"),
        ];
        let file = root.join("install.sh");
        fs::write(&file, script(&entries)).unwrap();
        let run = || {
            let out = Command::new("sh")
                .arg(&file)
                .env_clear()
                .env("HOME", &home)
                .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                .output()
                .unwrap();
            let calls = fs::read_to_string(&log).unwrap_or_default();
            let _ = fs::remove_file(&log);
            (
                out.status.success(),
                calls,
                String::from_utf8_lossy(&out.stderr).into_owned(),
            )
        };

        let (ok, calls, _) = run();
        assert!(ok);
        assert!(calls.contains("mytool\n") && calls.contains("mise use --global node@22"));
        assert!(calls.contains("npm install -g typescript") && calls.contains("cargo install bat"));
        assert!(
            calls.find("mytool").unwrap() < calls.find("mise").unwrap(),
            "your own commands, then languages"
        );
        assert!(
            calls.find("mise").unwrap() < calls.find("npm install").unwrap(),
            "languages, then packages"
        );

        let (ok, calls, _) = run();
        assert!(ok);
        assert!(
            !calls.contains("mytool") && !calls.contains("npm install"),
            "what is there is skipped:\n{calls}"
        );

        fs::remove_file(root.join("npm-installed")).unwrap();
        fs::write(root.join("npm-broken"), "").unwrap();
        let (ok, calls, stderr) = run();
        assert!(!ok);
        assert!(stderr.contains("TermF: npm: typescript failed"), "{stderr}");
        assert!(!calls.contains("cargo"), "nothing after the failure runs");

        fs::remove_file(bin.join("mise")).unwrap();
        let (ok, _, stderr) = run();
        assert!(
            !ok && stderr.contains("TermF: mise is not installed"),
            "{stderr}"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_manager_that_fails_says_so_and_the_others_still_report() {
        let root = temp("fail");
        let (bin, home) = (root.join("bin"), root.join("home"));
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&home).unwrap();
        program(&bin, "pipx", "echo 'pipx: broken venv' >&2; exit 1");
        program(
            &bin,
            "composer",
            r#"printf '{"installed":[{"name":"laravel/installer","version":"v5.8.3","description":"Laravel application installer."}]}'"#,
        );
        let m = Machine {
            home,
            path: bin.into_os_string(),
            apps: None,
        };
        let errors = std::sync::Mutex::new(Vec::new());
        let all = scan(&m, &[], &|e| {
            if let Event::Scanned(s) = e
                && let Some(why) = &s.error
            {
                errors.lock().unwrap().push((s.source.clone(), why.clone()));
            }
        });
        let errors = errors.into_inner().unwrap();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "pipx");
        assert!(errors[0].1.contains("broken venv"), "{}", errors[0].1);
        let laravel = all
            .iter()
            .find(|f| f.entry.name == "laravel/installer")
            .unwrap();
        assert_eq!(
            laravel.entry.description.as_deref(),
            Some("Laravel application installer.")
        );
        assert_eq!(laravel.installed.as_deref(), Some("5.8.3"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn nothing_runs_without_its_manager() {
        let m = Machine {
            home: PathBuf::from("/nowhere"),
            path: OsString::new(),
            apps: None,
        };
        assert!(
            m.run("brew", &[], QUICK).is_err(),
            "a manager absent is never run"
        );
        assert_eq!(
            m.portable(Path::new("/nowhere/.local/bin/x")),
            "~/.local/bin/x"
        );
        assert_eq!(path_sh("~/.local/bin/x"), "\"$HOME/.local/bin/x\"");
    }
}
