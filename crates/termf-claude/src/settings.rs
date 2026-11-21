//! Claude Code's settings files by scope, which scope decides each key, and editing a simple key.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Env, State, mask};

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

/// The keys TermF edits, with the values the official schema accepts
/// (https://json.schemastore.org/claude-code-settings.json).
const EDITABLE: &[&str] = &[
    "model",
    "effortLevel",
    "permissions.defaultMode",
    "autoMemoryEnabled",
    "cleanupPeriodDays",
    "autoUpdatesChannel",
];

fn check(scope: Scope, key: &str, value: &Value) -> Result<()> {
    let one_of = |allowed: &[&str]| {
        ensure!(
            value.as_str().is_some_and(|v| allowed.contains(&v)),
            "{key} takes one of: {}",
            allowed.join(", ")
        );
        Ok(())
    };
    match key {
        "model" => ensure!(
            value.as_str().is_some_and(|m| !m.trim().is_empty()
                && m.len() <= 200
                && !m.contains(char::is_control)),
            "model takes a model name or alias"
        ),
        "effortLevel" => one_of(&["low", "medium", "high", "xhigh"])?,
        "permissions.defaultMode" => {
            one_of(&[
                "default",
                "acceptEdits",
                "plan",
                "auto",
                "dontAsk",
                "bypassPermissions",
            ])?;
            ensure!(
                scope != Scope::Project
                    || !matches!(value.as_str(), Some("auto" | "bypassPermissions")),
                "Claude Code ignores {} in a project's shared settings",
                value
            );
        }
        "autoMemoryEnabled" => ensure!(value.is_boolean(), "autoMemoryEnabled takes true or false"),
        "cleanupPeriodDays" => ensure!(
            value.as_u64().is_some_and(|d| d >= 1),
            "cleanupPeriodDays takes a whole number of days, at least 1"
        ),
        "autoUpdatesChannel" => one_of(&["stable", "latest"])?,
        _ => bail!("TermF does not edit {key}"),
    }
    Ok(())
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub path: String,
    pub diff: String,
    /// The whole file once saved: it gets this text exactly.
    #[serde(skip)]
    pub text: String,
}

/// What setting `key` (`value: None` removes it) in `scope`'s file would change. A file that is not valid JSON
/// is never rewritten: Claude Code would reject it anyway, and its content would be lost.
pub fn preview(env: &Env, scope: Scope, key: &str, value: Option<&Value>) -> Result<Preview> {
    ensure!(EDITABLE.contains(&key), "TermF does not edit {key}");
    ensure!(scope != Scope::Managed, "managed settings are read-only");
    if let Some(value) = value {
        check(scope, key, value)?;
    }
    let path = file(env, scope).context("choose a repository for the project and local scopes")?;
    let before = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("cannot read {}", path.display())),
    };
    let mut root = if before.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str(&before) {
            Ok(Value::Object(map)) => map,
            _ => bail!(
                "{} is not a valid JSON object: fix it first",
                path.display()
            ),
        }
    };
    set(&mut root, key, value)?;
    let text = pretty(&Value::Object(root), indent(&before))?;
    Ok(Preview {
        path: path.to_string_lossy().into_owned(),
        diff: diff(&before, &text),
        text,
    })
}

pub fn save(env: &Env, scope: Scope, key: &str, value: Option<&Value>) -> Result<Preview> {
    let preview = preview(env, scope, key, value)?;
    write_atomic(Path::new(&preview.path), &preview.text)?;
    Ok(preview)
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
fn indent(text: &str) -> String {
    text.lines()
        .find_map(|l| {
            let ws: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            (!ws.is_empty()).then_some(ws)
        })
        .unwrap_or_else(|| "  ".into())
}

fn pretty(value: &Value, indent: String) -> Result<String> {
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
