//! The library: skills, commands, roles, preferences, rules and workflows, read from layers that go from the
//! defaults shipped with TermF to a repository's `.termf/library/`. The most specific layer wins.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use include_dir::{Dir, DirEntry, include_dir};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use toml_edit::{Array, DocumentMut, Table};
use yaml_rust2::{Yaml, YamlLoader};

pub mod archive;
mod claude;

pub use claude::{Imported, Outcome, import_claude, materialize};

static SHIPPED: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../library");

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Skill,
    Command,
    Role,
    Preference,
    Rule,
    Workflow,
}

impl Kind {
    pub const ALL: [Kind; 6] = [
        Kind::Skill,
        Kind::Command,
        Kind::Role,
        Kind::Preference,
        Kind::Rule,
        Kind::Workflow,
    ];

    /// The folder of this kind in a library, which is also the key of its activation list in `[library]`.
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Skill => "skills",
            Kind::Command => "commands",
            Kind::Role => "roles",
            Kind::Preference => "preferences",
            Kind::Rule => "rules",
            Kind::Workflow => "workflows",
        }
    }

    /// `skill` or `skills`.
    pub fn parse(text: &str) -> Option<Kind> {
        Kind::ALL
            .into_iter()
            .find(|k| text == k.dir() || text == k.to_string())
    }

    /// Items of these kinds are folders, read through this file.
    fn main_file(self) -> Option<&'static str> {
        match self {
            Kind::Skill => Some("SKILL.md"),
            Kind::Rule => Some("rule.md"),
            _ => None,
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(self.dir().trim_end_matches('s'))
    }
}

/// From the least to the most specific.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// Built into TermF, from the `library/` folder of its repository.
    Shipped,
    /// `<config>/library/`, switched on and off in `termf.toml`.
    Global,
    /// `<config>/projects/<id>/library/`, switched on and off in `projects/<id>.toml`.
    Project,
    /// `<repo>/.termf/library/`, versioned with the repository.
    Repo,
    /// Switches for one launch.
    Run,
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(match self {
            Layer::Shipped => "shipped",
            Layer::Global => "global",
            Layer::Project => "project",
            Layer::Repo => "repo",
            Layer::Run => "run",
        })
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: Kind,
    /// The file or folder name, which identifies the item across layers.
    pub name: String,
    pub description: Option<String>,
    /// Roles the item is meant for.
    pub applies_to: Vec<String>,
    pub tags: Vec<String>,
    /// The agent CLI it is written for; `None` means any.
    pub runtime: Option<String>,
    /// The whole frontmatter (the whole file for a workflow), kind-specific keys included.
    pub meta: Map<String, Value>,
    /// What follows the frontmatter, verbatim.
    pub body: String,
    /// Why the frontmatter could not be read; the item is still used as it is.
    pub error: Option<String>,
    /// The most specific layer that has an item of this kind and name.
    pub layer: Layer,
    /// Its file or folder; `None` for a shipped item, which is built into TermF.
    pub path: Option<PathBuf>,
    /// Less specific layers whose item of the same name this one hides.
    pub shadows: Vec<Layer>,
    /// The `+name` (true) or `-name` (false) of each layer that has one.
    pub toggles: BTreeMap<Layer, bool>,
    /// Shipped items start off and the others on, then the most specific toggle decides.
    pub active: bool,
    /// Path inside its layer, to copy a shipped item.
    #[serde(skip)]
    rel: String,
}

/// Activation lists by kind folder, as in `[library]`: `skills = ["+name", "-other"]`.
pub type Toggles = BTreeMap<String, Vec<String>>;

/// The layers that apply besides the shipped and global ones.
#[derive(Default, Clone, Debug)]
pub struct Scope {
    pub project: Option<String>,
    pub repo: Option<PathBuf>,
    pub overrides: Toggles,
}

impl Scope {
    /// The scope of `path`: the git repository that contains it, and unless `project` is given, the first
    /// project that lists it.
    pub fn new(config_dir: &Path, path: Option<&Path>, project: Option<String>) -> Result<Scope> {
        let Some(path) = path else {
            return Ok(Scope {
                project,
                ..Scope::default()
            });
        };
        let repo = path
            .ancestors()
            .find(|d| d.join(".git").exists())
            .unwrap_or(path);
        let project = match project {
            Some(id) => Some(id),
            // shortcut: a repository listed in several projects gets the first one, until sessions know their project.
            None => termf_core::load_projects(config_dir)?
                .into_iter()
                .find(|p| p.repos.iter().any(|r| path.starts_with(&r.path)))
                .map(|p| p.id),
        };
        Ok(Scope {
            project,
            repo: Some(repo.to_path_buf()),
            overrides: Toggles::new(),
        })
    }
}

/// `<config>/library`, or a project's `<config>/projects/<id>/library`.
pub fn library_dir(config_dir: &Path, project: Option<&str>) -> Result<PathBuf> {
    Ok(match project {
        Some(id) => {
            termf_core::project_file(config_dir, id)?;
            config_dir.join("projects").join(id).join("library")
        }
        None => config_dir.join("library"),
    })
}

/// Every item visible in `scope`, each in its most specific layer's version, with its state.
pub fn resolve(config_dir: &Path, scope: &Scope) -> Result<Vec<Item>> {
    let global = config_dir.join("library");
    let project = scope
        .project
        .as_deref()
        .map(|id| library_dir(config_dir, Some(id)))
        .transpose()?;
    let repo = scope
        .repo
        .as_ref()
        .map(|r| r.join(".termf").join("library"));
    let sources = [
        (Layer::Shipped, Some(Source::Shipped)),
        (Layer::Global, Some(Source::Disk(&global))),
        (Layer::Project, project.as_deref().map(Source::Disk)),
        (Layer::Repo, repo.as_deref().map(Source::Disk)),
    ];
    let mut items: BTreeMap<(Kind, String), Item> = BTreeMap::new();
    for (layer, source) in sources {
        for mut item in source.map(|s| s.scan(layer)).unwrap_or_default() {
            if let Some(hidden) = items.remove(&(item.kind, item.name.clone())) {
                item.shadows = hidden.shadows;
                item.shadows.push(hidden.layer);
            }
            items.insert((item.kind, item.name.clone()), item);
        }
    }

    let mut toggles = vec![(Layer::Global, read_toggles(&config_dir.join("termf.toml"))?)];
    if let Some(id) = &scope.project {
        let file = termf_core::project_file(config_dir, id)?;
        toggles.push((Layer::Project, read_toggles(&file)?));
    }
    toggles.push((Layer::Run, scope.overrides.clone()));
    for item in items.values_mut() {
        item.active = item.layer != Layer::Shipped;
        for (layer, list) in &toggles {
            if let Some(on) = toggle(list, item.kind, &item.name) {
                item.toggles.insert(*layer, on);
                item.active = on;
            }
        }
    }
    Ok(items.into_values().collect())
}

/// Which layer provides an item, what it hides, and which toggle (if any) decides its state.
pub fn explain(config_dir: &Path, scope: &Scope, kind: Kind, name: &str) -> Result<Item> {
    resolve(config_dir, scope)?
        .into_iter()
        .find(|i| i.kind == kind && i.name == name)
        .with_context(|| format!("no {kind} named {name}"))
}

/// Writes `+name` (`Some(true)`) or `-name` (`Some(false)`) in the activation list of `termf.toml`, or of the
/// project's file, or removes the toggle (`None`) so the item inherits. Comments and the rest of the file stay.
pub fn set_toggle(
    config_dir: &Path,
    project: Option<&str>,
    kind: Kind,
    name: &str,
    enabled: Option<bool>,
) -> Result<()> {
    ensure!(
        !name.is_empty() && !name.starts_with(['+', '-']),
        "invalid name {name:?}"
    );
    let file = match project {
        Some(id) => termf_core::project_file(config_dir, id)?,
        None => {
            termf_core::load_settings(config_dir)?;
            config_dir.join("termf.toml")
        }
    };
    let mut doc: DocumentMut = fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    let library = doc
        .entry("library")
        .or_insert(toml_edit::Item::Table(Table::new()))
        .as_table_like_mut()
        .with_context(|| format!("`library` in {} must be a table", file.display()))?;
    let list = library
        .entry(kind.dir())
        .or_insert(toml_edit::value(Array::new()))
        .as_array_mut()
        .with_context(|| format!("`library.{}` must be a list", kind.dir()))?;
    list.retain(|t| {
        t.as_str()
            .is_none_or(|t| t.trim_start_matches(['+', '-']) != name)
    });
    if let Some(on) = enabled {
        list.push(format!("{}{name}", if on { '+' } else { '-' }));
    }
    termf_core::write_atomic(&file, &doc.to_string())
}

fn read_toggles(file: &Path) -> Result<Toggles> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default)]
        library: Toggles,
    }
    if !file.exists() {
        return Ok(Toggles::new());
    }
    let text = fs::read_to_string(file)?;
    let parsed: File =
        toml::from_str(&text).with_context(|| format!("invalid {}", file.display()))?;
    Ok(parsed.library)
}

/// The last mention of `name` in the kind's list: `-name` is off, `+name` or a bare `name` on.
fn toggle(toggles: &Toggles, kind: Kind, name: &str) -> Option<bool> {
    toggles
        .get(kind.dir())?
        .iter()
        .rev()
        .find_map(|t| match t.strip_prefix('-') {
            Some(n) => (n == name).then_some(false),
            None => (t.strip_prefix('+').unwrap_or(t) == name).then_some(true),
        })
}

/// Where a layer's items are read from.
enum Source<'a> {
    Shipped,
    Disk(&'a Path),
}

impl Source<'_> {
    fn scan(&self, layer: Layer) -> Vec<Item> {
        let mut items = Vec::new();
        for kind in Kind::ALL {
            for (entry, is_dir) in self.entries(kind.dir()) {
                let rel = format!("{}/{entry}", kind.dir());
                let extension = if kind == Kind::Workflow {
                    ".toml"
                } else {
                    ".md"
                };
                let (name, main) = match kind.main_file() {
                    Some(main) if is_dir => (entry.as_str(), format!("{rel}/{main}")),
                    None if !is_dir => match entry.strip_suffix(extension) {
                        Some(name) => (name, rel.clone()),
                        None => continue,
                    },
                    _ => continue,
                };
                if name.is_empty() || name.starts_with('.') {
                    continue;
                }
                // A folder without its main file is not an item (yet).
                let Some(text) = self.read(&main) else {
                    continue;
                };
                let path = match self {
                    Source::Shipped => None,
                    Source::Disk(root) => Some(root.join(kind.dir()).join(&entry)),
                };
                items.push(parse(kind, name.into(), &text, layer, path, rel));
            }
        }
        items
    }

    /// Names in a folder, each with whether it is a folder.
    fn entries(&self, rel: &str) -> Vec<(String, bool)> {
        match self {
            Source::Shipped => SHIPPED.get_dir(rel).map_or_else(Vec::new, |dir| {
                dir.entries()
                    .iter()
                    .filter_map(|e| {
                        Some((e.path().file_name()?.to_str()?.into(), e.as_dir().is_some()))
                    })
                    .collect()
            }),
            Source::Disk(root) => fs::read_dir(root.join(rel)).map_or_else(
                |_| Vec::new(),
                |entries| {
                    entries
                        .flatten()
                        .map(|e| {
                            (
                                e.file_name().to_string_lossy().into_owned(),
                                e.path().is_dir(),
                            )
                        })
                        .collect()
                },
            ),
        }
    }

    fn read(&self, rel: &str) -> Option<String> {
        match self {
            Source::Shipped => SHIPPED.get_file(rel)?.contents_utf8().map(str::to_owned),
            Source::Disk(root) => fs::read_to_string(root.join(rel)).ok(),
        }
    }
}

fn parse(
    kind: Kind,
    name: String,
    text: &str,
    layer: Layer,
    path: Option<PathBuf>,
    rel: String,
) -> Item {
    let (meta, error) = match metadata(kind, text) {
        Ok(meta) => (meta, None),
        Err(e) => (Map::new(), Some(format!("{e:#}"))),
    };
    let text_of = |key: &str| meta.get(key).and_then(Value::as_str).map(str::to_owned);
    // Claude Code accepts a comma-separated string where a list is expected.
    let list = |key: &str| match meta.get(key) {
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(Into::into)
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(Into::into)
            .collect(),
        _ => Vec::new(),
    };
    Item {
        kind,
        name,
        description: text_of("description"),
        applies_to: list("applies_to"),
        tags: list("tags"),
        runtime: text_of("runtime"),
        body: match kind {
            Kind::Workflow => text.into(),
            _ => frontmatter(text).1.into(),
        },
        meta,
        error,
        layer,
        path,
        shadows: Vec::new(),
        toggles: BTreeMap::new(),
        active: false,
        rel,
    }
}

/// Splits a `---` YAML frontmatter from the Markdown body.
fn frontmatter(text: &str) -> (Option<&str>, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, text);
    };
    let mut end = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return (Some(&rest[..end]), &rest[end + line.len()..]);
        }
        end += line.len();
    }
    (None, text)
}

fn metadata(kind: Kind, text: &str) -> Result<Map<String, Value>> {
    let value = match (kind, frontmatter(text).0) {
        (Kind::Workflow, _) => serde_json::to_value(toml::from_str::<toml::Table>(text)?)?,
        (_, Some(yaml)) => YamlLoader::load_from_str(yaml)?
            .into_iter()
            .next()
            .map_or(Value::Null, json),
        (_, None) => Value::Null,
    };
    match value {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        _ => bail!("the frontmatter is not a set of keys"),
    }
}

fn json(yaml: Yaml) -> Value {
    match yaml {
        Yaml::Real(real) => real
            .parse()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map_or(Value::String(real), Value::Number),
        Yaml::Integer(i) => i.into(),
        Yaml::String(s) => s.into(),
        Yaml::Boolean(b) => b.into(),
        Yaml::Array(items) => items.into_iter().map(json).collect(),
        Yaml::Hash(map) => map
            .into_iter()
            .map(|(k, v)| {
                let key = match json(k) {
                    Value::String(s) => s,
                    k => k.to_string(),
                };
                (key, json(v))
            })
            .collect::<Map<_, _>>()
            .into(),
        _ => Value::Null,
    }
}

/// Copies an item's file or folder to `dest`.
fn copy_item(item: &Item, dest: &Path) -> Result<()> {
    match &item.path {
        Some(path) => copy_path(path, dest),
        None => match SHIPPED.get_entry(&item.rel) {
            Some(DirEntry::File(file)) => write_file(dest, file.contents()),
            Some(DirEntry::Dir(dir)) => extract(dir, dir.path(), dest),
            None => bail!("{} is not shipped", item.rel),
        },
    }
}

fn extract(dir: &Dir, base: &Path, dest: &Path) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(sub) => extract(sub, base, dest)?,
            DirEntry::File(file) => {
                write_file(&dest.join(file.path().strip_prefix(base)?), file.contents())?
            }
        }
    }
    Ok(())
}

/// Copies a file or a folder, following links.
fn copy_path(src: &Path, dest: &Path) -> Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_path(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest).with_context(|| format!("cannot copy {}", src.display()))?;
    }
    Ok(())
}

fn write_file(dest: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dest, bytes).with_context(|| format!("cannot write {}", dest.display()))
}

#[cfg(test)]
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termf-library-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
fn put(path: &Path, text: &str) {
    write_file(path, text.as_bytes()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(items: &'a [Item], kind: Kind, name: &str) -> &'a Item {
        items
            .iter()
            .find(|i| i.kind == kind && i.name == name)
            .unwrap_or_else(|| panic!("no {kind} {name}"))
    }

    #[test]
    fn parses_frontmatter_and_keeps_the_body() {
        let text = "---\nname: x\ndescription: \"Does: things, well\"\napplies_to: [front, back]\ntags: a, b\nruntime: claude-code\nlevel: block\ncheck: { run: check.sh }\n---\n# Title\n\nBody ---\n";
        let item = parse(
            Kind::Rule,
            "x".into(),
            text,
            Layer::Global,
            None,
            String::new(),
        );
        assert_eq!(item.error, None);
        assert_eq!(item.description.as_deref(), Some("Does: things, well"));
        assert_eq!(item.applies_to, ["front", "back"]);
        assert_eq!(item.tags, ["a", "b"], "comma-separated lists too");
        assert_eq!(item.runtime.as_deref(), Some("claude-code"));
        assert_eq!(item.meta["level"], "block");
        assert_eq!(item.meta["check"]["run"], "check.sh");
        assert_eq!(item.body, "# Title\n\nBody ---\n");

        let crlf = parse(
            Kind::Command,
            "c".into(),
            "---\r\ndescription: d\r\n---\r\nhi\r\n",
            Layer::Global,
            None,
            String::new(),
        );
        assert_eq!(crlf.description.as_deref(), Some("d"));
        assert_eq!(crlf.body, "hi\r\n");

        let plain = parse(
            Kind::Command,
            "c".into(),
            "just text\n---\n",
            Layer::Global,
            None,
            String::new(),
        );
        assert!(plain.meta.is_empty() && plain.error.is_none());
        assert_eq!(plain.body, "just text\n---\n");

        let broken = parse(
            Kind::Skill,
            "s".into(),
            "---\ndescription: [oops\n---\nbody\n",
            Layer::Global,
            None,
            String::new(),
        );
        assert!(broken.error.is_some());
        assert_eq!(broken.body, "body\n", "a broken frontmatter keeps the body");

        let workflow = parse(
            Kind::Workflow,
            "w".into(),
            "description = \"flow\"\n[steps.a]\ntype = \"gate\"\n",
            Layer::Global,
            None,
            String::new(),
        );
        assert_eq!(workflow.description.as_deref(), Some("flow"));
        assert_eq!(workflow.meta["steps"]["a"]["type"], "gate");
    }

    #[test]
    fn ships_roles_rules_and_preferences_switched_off() {
        let dir = temp_dir("shipped");
        let items = resolve(&dir, &Scope::default()).unwrap();
        for (kind, names) in [
            (
                Kind::Role,
                &["orchestrator", "po", "front", "back", "devops", "secops"][..],
            ),
            (
                Kind::Rule,
                &["scope-strict", "no-text-changes", "visual-review"],
            ),
            (
                Kind::Preference,
                &["canary", "conventional-commits", "front-conventions"],
            ),
            (Kind::Command, &["handoff"]),
        ] {
            for name in names {
                let item = find(&items, kind, name);
                assert_eq!(item.layer, Layer::Shipped);
                assert!(!item.active, "{name} is off until enabled");
                assert!(item.error.is_none() && item.description.is_some(), "{name}");
                assert!(!item.body.trim().is_empty());
            }
        }
        let rule = find(&items, Kind::Rule, "no-text-changes");
        assert_eq!(rule.applies_to, ["front", "back", "devops"]);
        assert_eq!(rule.meta["level"], "block");
        assert_eq!(find(&items, Kind::Role, "secops").meta["effort"], "xhigh");
    }

    #[test]
    fn the_most_specific_layer_and_toggle_win() {
        let dir = temp_dir("layers");
        let repo = dir.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(dir.join("projects")).unwrap();
        put(
            &dir.join("projects/acme.toml"),
            &format!(
                "name = \"Acme\"\n[[repos]]\npath = {:?}\n",
                repo.to_string_lossy()
            ),
        );
        put(
            &dir.join("library/skills/x/SKILL.md"),
            "---\ndescription: global\n---\n",
        );
        put(&dir.join("library/commands/handoff.md"), "mine\n");
        put(
            &dir.join("projects/acme/library/skills/x/SKILL.md"),
            "---\ndescription: project\n---\n",
        );
        put(
            &repo.join(".termf/library/skills/x/SKILL.md"),
            "---\ndescription: repo\n---\n",
        );
        put(
            &repo.join(".termf/library/skills/empty/notes.md"),
            "no SKILL.md",
        );

        let scope = Scope::new(&dir, Some(&repo.join("src")), None).unwrap();
        assert_eq!(scope.project.as_deref(), Some("acme"));
        assert_eq!(scope.repo.as_deref(), Some(repo.as_path()));
        let items = resolve(&dir, &scope).unwrap();
        let x = find(&items, Kind::Skill, "x");
        assert_eq!(x.description.as_deref(), Some("repo"));
        assert_eq!(x.layer, Layer::Repo);
        assert_eq!(x.shadows, [Layer::Global, Layer::Project]);
        assert!(x.active, "an item the user added starts on");
        assert_eq!(
            x.path.as_deref(),
            Some(repo.join(".termf/library/skills/x").as_path())
        );
        assert!(!items.iter().any(|i| i.name == "empty"));
        let handoff = find(&items, Kind::Command, "handoff");
        assert_eq!((handoff.layer, handoff.active), (Layer::Global, true));
        assert_eq!(handoff.shadows, [Layer::Shipped]);
        assert_eq!(handoff.body, "mine\n");

        set_toggle(&dir, None, Kind::Skill, "x", Some(false)).unwrap();
        set_toggle(&dir, None, Kind::Preference, "canary", Some(true)).unwrap();
        let global_only = resolve(&dir, &Scope::default()).unwrap();
        let x = find(&global_only, Kind::Skill, "x");
        assert_eq!((x.layer, x.active), (Layer::Global, false));
        assert!(find(&global_only, Kind::Preference, "canary").active);

        set_toggle(&dir, Some("acme"), Kind::Skill, "x", Some(true)).unwrap();
        let x = explain(&dir, &scope, Kind::Skill, "x").unwrap();
        assert!(x.active);
        assert_eq!(
            x.toggles,
            BTreeMap::from([(Layer::Global, false), (Layer::Project, true)])
        );

        let mut run = scope.clone();
        run.overrides = Toggles::from([("skills".into(), vec!["-x".into()])]);
        assert!(
            !explain(&dir, &run, Kind::Skill, "x").unwrap().active,
            "a launch override is the most specific"
        );
        assert!(explain(&dir, &scope, Kind::Skill, "nope").is_err());
    }

    #[test]
    fn toggles_round_trip_and_keep_comments() {
        let dir = temp_dir("toggles");
        put(
            &dir.join("termf.toml"),
            "# mine\nroots = [\"~\"]\n\n[library]\n# my skills\nskills = [\"+a\", \"-b\"] # inline\n",
        );
        fs::create_dir_all(dir.join("projects")).unwrap();
        put(&dir.join("projects/acme.toml"), "# Acme\nname = \"Acme\"\n");

        set_toggle(&dir, None, Kind::Skill, "b", Some(true)).unwrap();
        set_toggle(&dir, None, Kind::Skill, "a", None).unwrap();
        set_toggle(&dir, None, Kind::Role, "front", Some(false)).unwrap();
        let text = fs::read_to_string(dir.join("termf.toml")).unwrap();
        for kept in ["# mine\n", "# my skills\n", "# inline"] {
            assert!(text.contains(kept), "lost {kept:?} in:\n{text}");
        }
        let toggles = read_toggles(&dir.join("termf.toml")).unwrap();
        assert_eq!(toggles["skills"], ["+b"]);
        assert_eq!(toggles["roles"], ["-front"]);
        assert_eq!(toggle(&toggles, Kind::Skill, "b"), Some(true));
        assert_eq!(toggle(&toggles, Kind::Skill, "a"), None);

        set_toggle(&dir, Some("acme"), Kind::Rule, "scope-strict", Some(true)).unwrap();
        let text = fs::read_to_string(dir.join("projects/acme.toml")).unwrap();
        assert!(text.starts_with("# Acme\n"), "{text}");
        assert_eq!(
            read_toggles(&dir.join("projects/acme.toml")).unwrap()["rules"],
            ["+scope-strict"]
        );
        assert_eq!(termf_core::load_projects(&dir).unwrap()[0].name, "Acme");
        assert!(
            termf_core::load_settings(&dir).is_ok(),
            "termf.toml stays valid"
        );

        assert!(set_toggle(&dir, Some("missing"), Kind::Skill, "x", Some(true)).is_err());
        assert!(set_toggle(&dir, Some("../x"), Kind::Skill, "x", Some(true)).is_err());
        assert!(set_toggle(&dir, None, Kind::Skill, "-x", Some(true)).is_err());
        assert_eq!(Kind::parse("skills"), Some(Kind::Skill));
        assert_eq!(Kind::parse("preference"), Some(Kind::Preference));
        assert_eq!(Kind::parse("nope"), None);
    }
}
