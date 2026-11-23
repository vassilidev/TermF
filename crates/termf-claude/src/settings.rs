//! Claude Code's settings files by scope, which scope decides each key, and editing them.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::{Env, State, mask, schema};

/// From the lowest priority to the highest: managed settings win over everything (ARCHITECTURE §11.3,
/// CLAUDE-CODE.md; `--settings` only exists for one session).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    User,
    Project,
    Local,
    Managed,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ScopeFile {
    pub scope: Scope,
    pub path: String,
    pub state: State,
    pub error: Option<String>,
    /// Masked.
    pub value: Option<Value>,
}

/// One leaf of the settings once the scopes are merged: a scalar is decided by the highest scope that sets it,
/// a list gathers every scope's items.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Effective {
    pub key: String,
    pub value: Value,
    pub scope: Scope,
    /// The other scopes that set it: overridden for a scalar, merged for a list.
    pub others: Vec<Scope>,
    pub merged: bool,
}

#[derive(Serialize, Debug)]
pub struct Settings {
    pub files: Vec<ScopeFile>,
    pub effective: Vec<Effective>,
}

pub fn file(env: &Env, scope: Scope) -> Option<PathBuf> {
    match scope {
        Scope::User => Some(env.claude_dir.join("settings.json")),
        Scope::Project => Some(env.repo.as_ref()?.join(".claude/settings.json")),
        Scope::Local => Some(env.repo.as_ref()?.join(".claude/settings.local.json")),
        Scope::Managed => Some(env.managed_dir.join("managed-settings.json")),
    }
}

fn read_file(scope: Scope, path: PathBuf) -> ScopeFile {
    let (state, error, value) = match fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (State::Absent, None, None),
        Err(e) => (State::Invalid, Some(e.to_string()), None),
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => (State::Ok, None, Some(Value::Object(map))),
            Ok(_) => (State::Invalid, Some("not a JSON object".into()), None),
            Err(e) => (State::Invalid, Some(e.to_string()), None),
        },
    };
    ScopeFile {
        scope,
        path: path.to_string_lossy().into_owned(),
        state,
        error,
        value,
    }
}

/// Reads every scope (unmasked: `overview` masks the result).
pub fn read(env: &Env) -> Settings {
    let files: Vec<ScopeFile> = [Scope::User, Scope::Project, Scope::Local, Scope::Managed]
        .into_iter()
        .filter_map(|scope| Some(read_file(scope, file(env, scope)?)))
        .collect();
    let mut effective: Vec<Effective> = Vec::new();
    for f in &files {
        if let Some(Value::Object(map)) = &f.value {
            for (key, value) in map {
                // Hooks have their own section; `$schema` only serves editors.
                if key != "hooks" && key != "$schema" {
                    merge(&mut effective, key.clone(), value, f.scope);
                }
            }
        }
    }
    Settings { files, effective }
}

fn merge(effective: &mut Vec<Effective>, key: String, value: &Value, scope: Scope) {
    if let Value::Object(map) = value
        && !map.is_empty()
    {
        for (k, v) in map {
            merge(effective, format!("{key}.{k}"), v, scope);
        }
        return;
    }
    match effective.iter_mut().find(|e| e.key == key) {
        Some(e) => {
            e.others.push(e.scope);
            e.scope = scope;
            match (&mut e.value, value) {
                (Value::Array(items), Value::Array(more)) => {
                    for item in more {
                        if !items.contains(item) {
                            items.push(item.clone());
                        }
                    }
                    e.merged = true;
                }
                (slot, _) => *slot = value.clone(),
            }
        }
        None => effective.push(Effective {
            key,
            value: value.clone(),
            scope,
            others: Vec::new(),
            merged: false,
        }),
    }
}

pub fn masked(mut settings: Settings) -> Settings {
    for f in &mut settings.files {
        if let Some(v) = &mut f.value {
            mask::mask(v);
        }
    }
    for e in &mut settings.effective {
        let leaf = e.key.rsplit('.').next().unwrap_or_default();
        if mask::secret_key(leaf) && !e.value.is_array() {
            e.value = mask::MASK.into();
        } else {
            mask::mask(&mut e.value);
        }
    }
    settings
}

/// One change to a settings file. Lists and hooks change item by item, by position and by what the page shows
/// there (masked), so a file changed meanwhile is never edited at the wrong place; and the page never sends a
/// whole value back unless you revealed it.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Edit {
    /// Sets a dotted key (`model`, `permissions.defaultMode`, `env.NODE_ENV`); `value: null` removes it.
    Set { key: String, value: Option<Value> },
    /// Appends an item to the list at `key`.
    Add { key: String, value: Value },
    /// Removes item `index` of the list at `key`, which must be `value` (as masked) when given.
    Drop {
        key: String,
        index: usize,
        #[serde(default)]
        value: Option<Value>,
    },
    /// Moves item `index` of the list at `key` to the end of the list at `to` (`permissions.allow` → `deny`).
    Move {
        key: String,
        index: usize,
        to: String,
        #[serde(default)]
        value: Option<Value>,
    },
    /// Adds a command hook to `event`, in the group of the same matcher.
    AddHook {
        event: String,
        matcher: Option<String>,
        command: String,
    },
    /// Removes hook `index` of matcher group `group` of `event`, which must run `run` (as masked) when given.
    DropHook {
        event: String,
        group: usize,
        index: usize,
        #[serde(default)]
        run: Option<String>,
    },
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub path: String,
    pub diff: String,
    /// The file as read for this preview: saving refuses a file that changed since.
    pub hash: String,
    /// The whole file once saved: it gets this text exactly.
    #[serde(skip)]
    pub text: String,
}

/// FNV-1a: the same text gives the same value in any process and version.
pub fn fingerprint(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

pub(crate) fn read_text(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
    }
}

/// A file that is not valid JSON is never rewritten: Claude Code would reject it anyway, and its content
/// would be lost.
fn parse_root(text: &str, path: &Path) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => bail!(
            "{} is not a valid JSON object: fix it first",
            path.display()
        ),
    }
}

/// What `edit` would change in `scope`'s file.
pub fn preview(env: &Env, scope: Scope, edit: &Edit) -> Result<Preview> {
    ensure!(scope != Scope::Managed, "managed settings are read-only");
    let path = file(env, scope).context("choose a repository for the project and local scopes")?;
    let before = read_text(&path)?;
    let mut root = parse_root(&before, &path)?;
    if let Some((key, value, item)) = apply(&mut root, edit)? {
        check(scope, &key, &value, item)?;
    }
    let text = pretty(&Value::Object(root), indent(&before))?;
    Ok(Preview {
        path: path.to_string_lossy().into_owned(),
        // Shown in the app and the CLI: the lines around a change may hold secrets.
        diff: crate::native::mask_lines(&diff(&before, &text)),
        hash: fingerprint(&before),
        text,
    })
}

/// Writes what the preview of `hash` showed: refused when the file changed since (`None`: no preview shown).
pub fn save(env: &Env, scope: Scope, edit: &Edit, hash: Option<&str>) -> Result<Preview> {
    if let (Some(hash), Some(path)) = (hash, file(env, scope)) {
        ensure!(
            hash == fingerprint(&read_text(&path)?),
            "{} changed since its preview: refresh and look at the change again",
            path.display()
        );
    }
    let preview = preview(env, scope, edit)?;
    write_atomic(Path::new(&preview.path), &preview.text)?;
    if scope == Scope::Local {
        exclude_local(Path::new(&preview.path))?;
    }
    Ok(preview)
}

/// Your own settings for one repository stay out of git, as when Claude Code creates the file: unless git
/// ignores it already, through `info/exclude`, which is never committed. Outside git, nothing to do.
fn exclude_local(file: &Path) -> Result<()> {
    let dir = file.parent().context("no parent folder")?;
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
    };
    if git(&["check-ignore", "-q", "settings.local.json"])?
        .status
        .success()
    {
        return Ok(());
    }
    let out = git(&["rev-parse", "--show-prefix", "--git-path", "info/exclude"])?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let (true, Some(prefix), Some(exclude)) = (out.status.success(), lines.next(), lines.next())
    else {
        return Ok(());
    };
    // Taken literally: a folder's name may hold what gitignore reads as a pattern.
    let prefix: String = prefix
        .chars()
        .flat_map(|c| ["\\*?[".contains(c).then_some('\\'), Some(c)])
        .flatten()
        .collect();
    let exclude = dir.join(exclude);
    let before = fs::read_to_string(&exclude).unwrap_or_default();
    let newline = if before.is_empty() || before.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    fs::create_dir_all(exclude.parent().context("no parent folder")?)?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude)
        .and_then(|mut f| {
            std::io::Write::write_all(
                &mut f,
                format!("{newline}/{prefix}settings.local.json\n").as_bytes(),
            )
        })
        .with_context(|| format!("cannot write {}", exclude.display()))
}

/// The value of a dotted key in `scope`'s file, unmasked: only for a value you asked to see.
pub fn reveal(env: &Env, scope: Scope, key: &str) -> Result<Value> {
    let path = file(env, scope).context("choose a repository for the project and local scopes")?;
    let root = Value::Object(parse_root(&read_text(&path)?, &path)?);
    Ok(key
        .split('.')
        .try_fold(&root, |v, part| v.get(part))
        .cloned()
        .unwrap_or(Value::Null))
}

/// Applies `edit`; returns what it brings in, to check: the key, the value, and whether it is an item of the
/// list at that key.
fn apply(root: &mut Map<String, Value>, edit: &Edit) -> Result<Option<(String, Value, bool)>> {
    Ok(match edit {
        Edit::Set { key, value } => {
            set(root, key, value.as_ref())?;
            if value.is_none()
                && let Some((parent, _)) = key.rsplit_once('.')
            {
                prune(root, parent);
            }
            value.clone().map(|v| (key.clone(), v, false))
        }
        Edit::Add { key, value } => {
            let list = list(root, key)?;
            ensure!(!list.contains(value), "{value} is already in {key}");
            list.push(value.clone());
            Some((key.clone(), value.clone(), true))
        }
        Edit::Drop { key, index, value } => {
            take(list(root, key)?, *index, value.as_ref(), key)?;
            prune(root, key);
            None
        }
        Edit::Move {
            key,
            index,
            to,
            value,
        } => {
            ensure!(key != to, "{key} is already where it goes");
            let item = take(list(root, key)?, *index, value.as_ref(), key)?;
            let target = list(root, to)?;
            if !target.contains(&item) {
                target.push(item.clone());
            }
            prune(root, key);
            Some((to.clone(), item, true))
        }
        Edit::AddHook {
            event,
            matcher,
            command,
        } => {
            ensure!(
                !event.is_empty() && event.chars().all(|c| c.is_ascii_alphanumeric()),
                "{event} is not a hook event"
            );
            ensure!(!command.trim().is_empty(), "a hook needs a command");
            let key = format!("hooks.{event}");
            let matcher = matcher.as_deref().map(str::trim).filter(|m| !m.is_empty());
            let hook = json!({"type": "command", "command": command});
            // Checked alone in a group of its own: the hooks already there are not this edit's.
            let mut added = Map::new();
            if let Some(m) = matcher {
                added.insert("matcher".into(), m.into());
            }
            added.insert("hooks".into(), json!([hook]));
            let added = Value::Object(added);
            let groups = list(root, &key)?;
            let same = |g: &Value| g["matcher"].as_str().filter(|m| !m.is_empty()) == matcher;
            match groups.iter_mut().find(|g| same(g)) {
                Some(group) => group["hooks"]
                    .as_array_mut()
                    .context("a hook group of this event has no list of hooks")?
                    .push(hook),
                None => groups.push(added.clone()),
            }
            Some((key, added, true))
        }
        Edit::DropHook {
            event,
            group,
            index,
            run,
        } => {
            let key = format!("hooks.{event}");
            let groups = list(root, &key)?;
            let hooks = groups
                .get_mut(*group)
                .and_then(|g| g["hooks"].as_array_mut())
                .with_context(|| {
                    format!("{key} has no hook group {group}: refresh and try again")
                })?;
            if let Some(run) = run {
                ensure!(
                    hooks.get(*index).map(crate::hook_run).as_ref() == Some(run),
                    "{key} has no such hook at {group}/{index}: refresh and try again"
                );
            }
            take(hooks, *index, None, &key)?;
            if hooks.is_empty() {
                groups.remove(*group);
            }
            prune(root, &key);
            None
        }
    })
}

/// The list at a dotted key, created when missing.
fn list<'a>(root: &'a mut Map<String, Value>, key: &str) -> Result<&'a mut Vec<Value>> {
    let mut map = root;
    let mut parts: Vec<&str> = key.split('.').collect();
    let last = parts.pop().unwrap_or_default();
    for part in parts {
        map = map
            .entry(part.to_owned())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .with_context(|| format!("{part} is not an object in this file"))?;
    }
    map.entry(last.to_owned())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .with_context(|| format!("{key} is not a list in this file"))
}

/// Item `index` of `list`, which must be `expected` as the page shows it (masked).
fn take(list: &mut Vec<Value>, index: usize, expected: Option<&Value>, key: &str) -> Result<Value> {
    let found = list.get(index).is_some_and(|item| {
        expected.is_none_or(|e| {
            let mut shown = item.clone();
            mask::mask(&mut shown);
            item == e || &shown == e
        })
    });
    ensure!(
        found,
        "{key} has no such item {index}: refresh and try again"
    );
    Ok(list.remove(index))
}

/// Removes the empty lists and objects a removal leaves along `key`.
fn prune(root: &mut Map<String, Value>, key: &str) {
    let parts: Vec<&str> = key.split('.').collect();
    for depth in (1..=parts.len()).rev() {
        let (path, last) = (&parts[..depth - 1], parts[depth - 1]);
        let Some(map) = path
            .iter()
            .try_fold(&mut *root, |m, p| m.get_mut(*p)?.as_object_mut())
        else {
            return;
        };
        let empty = match map.get(last) {
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::Object(o)) => o.is_empty(),
            _ => false,
        };
        if !empty {
            return;
        }
        map.shift_remove(last);
    }
}

const PERMISSION_LISTS: [&str; 3] = ["allow", "ask", "deny"];

/// What an edit brings in: no value still masked, no secret in the project's shared file, what the schema
/// accepts, permission rules Claude Code reads, and the modes it refuses from a shared file.
fn check(scope: Scope, key: &str, value: &Value, item: bool) -> Result<()> {
    ensure!(
        !value.to_string().contains(mask::MASK),
        "{key} still holds a hidden value: reveal it, or type it again"
    );
    let leaf = key.rsplit('.').next().unwrap_or(key);
    ensure!(
        scope != Scope::Project || !secret(leaf, value),
        "{key} looks like a secret, and .claude/settings.json is shared with everyone who clones the \
         repository: use the local scope"
    );
    let node = if item {
        schema::item(key)
    } else {
        schema::at(key)
    };
    if let Some(node) = node {
        let mut errors = Vec::new();
        schema::check(node, value, key, &mut errors);
        ensure!(errors.is_empty(), "{}", errors.join("; "));
    }
    let mode = match key {
        "permissions" => value.get("defaultMode"),
        "permissions.defaultMode" => Some(value),
        _ => None,
    };
    ensure!(
        scope != Scope::Project
            || !matches!(
                mode.and_then(Value::as_str),
                Some("auto" | "bypassPermissions")
            ),
        "Claude Code ignores {} in a project's shared settings",
        mode.unwrap_or_default()
    );
    let rules: Vec<(&str, &Value)> = match key.strip_prefix("permissions.") {
        Some(list) if PERMISSION_LISTS.contains(&list) && item => vec![(list, value)],
        Some(list) if PERMISSION_LISTS.contains(&list) => value
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| (list, r))
            .collect(),
        None if key == "permissions" => PERMISSION_LISTS
            .iter()
            .flat_map(|list| {
                value[*list]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|r| (*list, r))
            })
            .collect(),
        _ => Vec::new(),
    };
    for (list, r) in rules {
        rule(list, r.as_str().unwrap_or_default())?;
    }
    let names: Vec<&str> = match key.strip_prefix("env.") {
        Some(name) => vec![name],
        None if key == "env" => value
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k.as_str())
            .collect(),
        None => Vec::new(),
    };
    for name in names {
        ensure!(
            name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{name} is not a variable name (letters, digits and _)"
        );
    }
    if key == "model" {
        ensure!(
            value.as_str().is_some_and(|m| !m.trim().is_empty()
                && m.len() <= 200
                && !m.contains(char::is_control)),
            "model takes a model name or alias"
        );
    }
    Ok(())
}

/// Whether `value`, set under `key`, holds what the masking would hide.
pub fn secret(key: &str, value: &Value) -> bool {
    let probe = json!({ key: value });
    let mut masked = probe.clone();
    mask::mask(&mut masked);
    masked != probe
}

/// A permission rule as Claude Code reads it (https://code.claude.com/docs/en/permissions): `Tool`,
/// `Tool(specifier)`, `mcp__server`, `mcp__server__tool` or `mcp__server__*`; a pattern in the tool name only in
/// `deny` and `ask`, or after `mcp__server__`.
pub fn rule(list: &str, rule: &str) -> Result<()> {
    ensure!(
        !rule.trim().is_empty() && rule == rule.trim() && !rule.contains(char::is_control),
        "a rule is a tool name, with an optional specifier in parentheses: Bash(npm run *)"
    );
    let (name, spec) = match rule.split_once('(') {
        Some((name, rest)) => {
            let spec = rest.strip_suffix(')').filter(|s| !s.trim().is_empty());
            let spec = spec.with_context(|| {
                format!("{rule}: the specifier goes between parentheses, as in Bash(npm run *)")
            })?;
            (name, Some(spec))
        }
        None => (rule, None),
    };
    ensure!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-*".contains(c)),
        "{rule}: {name} is not a tool name"
    );
    if let Some(rest) = name.strip_prefix("mcp__") {
        ensure!(
            spec.is_none(),
            "{rule}: Claude Code skips an mcp__ rule with parentheses"
        );
        let server = rest.split_once("__").map_or(rest, |(s, _)| s);
        ensure!(
            !server.is_empty(),
            "{rule}: name the server, as in mcp__github__*"
        );
        ensure!(
            list != "allow" || !server.contains('*'),
            "{rule}: in allow, a pattern only goes after mcp__<server>__, as in mcp__github__*"
        );
        return Ok(());
    }
    if name.contains('*') {
        ensure!(
            list != "allow",
            "{rule}: Claude Code skips a pattern in a tool name in allow"
        );
        return Ok(());
    }
    let tools = schema::tools();
    ensure!(
        tools.is_empty() || tools.iter().any(|t| t == name),
        "{rule}: unknown tool {name} (known: {})",
        tools.join(", ")
    );
    Ok(())
}

fn set(root: &mut Map<String, Value>, key: &str, value: Option<&Value>) -> Result<()> {
    let mut map = root;
    let mut parts: Vec<&str> = key.split('.').collect();
    let last = parts.pop().unwrap_or_default();
    for part in parts {
        if value.is_none() && !map.contains_key(part) {
            return Ok(());
        }
        let slot = map
            .entry(part.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        map = slot
            .as_object_mut()
            .with_context(|| format!("{part} is not an object in this file"))?;
    }
    match value {
        Some(v) => {
            map.insert(last.to_owned(), v.clone());
        }
        None => {
            map.shift_remove(last);
        }
    }
    Ok(())
}

/// The file's own indentation, so saving one key does not re-indent the rest.
pub(crate) fn indent(text: &str) -> String {
    text.lines()
        .find_map(|l| {
            let ws: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            (!ws.is_empty()).then_some(ws)
        })
        .unwrap_or_else(|| "  ".into())
}

pub(crate) fn pretty(value: &Value, indent: String) -> Result<String> {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    value.serialize(&mut ser)?;
    Ok(String::from_utf8(out)? + "\n")
}

/// Replaces `file` through a temporary file in the same folder. A symlinked file (dotfiles) is written through,
/// so the link stays, and it keeps its permissions.
pub fn write_atomic(file: &Path, text: &str) -> Result<()> {
    let file = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let dir = file.parent().context("no parent folder")?;
    fs::create_dir_all(dir)?;
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.termf-tmp"));
    fs::write(&tmp, text).with_context(|| format!("cannot write {}", tmp.display()))?;
    if let Ok(meta) = fs::metadata(&file) {
        fs::set_permissions(&tmp, meta.permissions())?;
    }
    fs::rename(&tmp, &file).with_context(|| format!("cannot write {}", file.display()))
}

/// A line diff (`-` removed, `+` added, ` ` context) of the changed part; empty when nothing changes.
pub fn diff(before: &str, after: &str) -> String {
    const CONTEXT: usize = 3;
    let (a, b): (Vec<&str>, Vec<&str>) = (before.lines().collect(), after.lines().collect());
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    if am.is_empty() && bm.is_empty() {
        return String::new();
    }
    let mut out: Vec<String> = a[prefix.saturating_sub(CONTEXT)..prefix]
        .iter()
        .map(|l| format!(" {l}"))
        .collect();
    // A settings edit changes a few lines: removed then added is enough, no alignment needed.
    out.extend(am.iter().map(|l| format!("-{l}")));
    out.extend(bm.iter().map(|l| format!("+{l}")));
    let tail = a.len() - suffix;
    out.extend(
        a[tail..(tail + CONTEXT).min(a.len())]
            .iter()
            .map(|l| format!(" {l}")),
    );
    out.join("\n") + "\n"
}
