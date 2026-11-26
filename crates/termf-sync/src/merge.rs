//! Three-way merge of one file against the version last synced: TOML and JSON key by key, `~/.ssh/config` host
//! by host and `~/.gitconfig` section by section, each option on its own (`lines.rs`).
//!
//! What only makes sense on the machine it was set on (an absolute path out of the home folder, a keychain of
//! one system) never travels: [`pin`] leaves it out of what is synced, [`unpin`] keeps this machine's own.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use toml::Value as Toml;
use toml_edit::{DocumentMut, Item, TableLike};

use crate::lines::{self, Syntax};
use crate::record::Entry;

fn is_toml(path: &str) -> bool {
    path.ends_with(".toml")
}

fn is_json(path: &str) -> bool {
    path.ends_with(".json")
}

fn parse(entry: &Entry) -> Option<toml::Table> {
    toml::from_str(std::str::from_utf8(&entry.bytes).ok()?).ok()
}

fn json(entry: &Entry) -> Option<Value> {
    serde_json::from_slice(&entry.bytes).ok()
}

/// Equal content; for TOML and JSON, equal values whatever the formatting.
pub fn same(path: &str, a: Option<&Entry>, b: Option<&Entry>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.exec == b.exec
                && (a.bytes == b.bytes
                    || is_toml(path) && parse(a).is_some_and(|ta| parse(b) == Some(ta))
                    || is_json(path) && json(a).is_some_and(|ja| json(b) == Some(ja)))
        }
        _ => false,
    }
}

/// Both sides changed `base`. Keeps every key only one side changed; the keys both changed differently are
/// returned as conflicts (empty: the whole file conflicts).
pub fn merge3(
    path: &str,
    base: Option<&Entry>,
    local: &Entry,
    remote: &Entry,
) -> Result<Entry, Vec<String>> {
    merge(path, base, local, remote, None)
}

/// [`merge3`] where the keys both sides changed take `remote`'s value, or `local`'s: what keeping one side of a
/// conflict writes, the other side's changes to the other keys kept. `None`: the file cannot be merged.
pub fn settle(
    path: &str,
    base: Option<&Entry>,
    local: &Entry,
    remote: &Entry,
    keep_remote: bool,
) -> Option<Entry> {
    merge(path, base, local, remote, Some(keep_remote)).ok()
}

fn merge(
    path: &str,
    base: Option<&Entry>,
    local: &Entry,
    remote: &Entry,
    keep: Option<bool>,
) -> Result<Entry, Vec<String>> {
    if path.ends_with(".jsonl") {
        return appended(base, local, remote).ok_or_else(Vec::new);
    }
    let text = |e: &Entry| String::from_utf8(e.bytes.clone()).ok();
    let (Some(l), Some(r)) = (text(local), text(remote)) else {
        return Err(Vec::new());
    };
    let b = base.and_then(text);
    let mut conflicts = Vec::new();
    let merged = if is_toml(path) {
        merge_toml(b.as_deref(), &l, &r, keep, &mut conflicts)
    } else if is_json(path) {
        merge_json(b.as_deref(), &l, &r, keep, &mut conflicts)
    } else {
        Syntax::of(path).map(|s| lines::merge(s, b.as_deref(), &l, &r, keep, &mut conflicts))
    };
    let merged = merged.ok_or_else(Vec::new)?;
    if !conflicts.is_empty() {
        return Err(conflicts);
    }
    let exec = if base.is_none_or(|b| b.exec == local.exec) {
        remote.exec
    } else {
        local.exec
    };
    Ok(Entry {
        bytes: merged.into_bytes(),
        exec,
    })
}

fn merge_toml(
    base: Option<&str>,
    local: &str,
    remote: &str,
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
) -> Option<String> {
    let lt: toml::Table = toml::from_str(local).ok()?;
    let rt: toml::Table = toml::from_str(remote).ok()?;
    let bt: toml::Table = base
        .and_then(|b| toml::from_str(b).ok())
        .unwrap_or_default();
    let mut mine: DocumentMut = local.parse().ok()?;
    let theirs: DocumentMut = remote.parse().ok()?;
    merge_tables(
        Some(&bt),
        &lt,
        &rt,
        mine.as_table_mut(),
        theirs.as_table(),
        "",
        keep,
        conflicts,
    );
    Some(mine.to_string())
}

/// A journal both machines only added lines to: this machine's lines, then the other's new ones.
fn appended(base: Option<&Entry>, local: &Entry, remote: &Entry) -> Option<Entry> {
    let base = base.map_or(&[][..], |b| &b.bytes);
    let (l, r) = (&local.bytes, &remote.bytes);
    if !l.starts_with(base) || !r.starts_with(base) || !l.ends_with(b"\n") {
        return None;
    }
    // Without a common history the lines both already had count once.
    let same = l.iter().zip(r).take_while(|(a, b)| a == b).count();
    let common = l[..same]
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1)
        .max(base.len());
    Some(Entry {
        bytes: [&l[..], &r[common..]].concat(),
        exec: local.exec,
    })
}

fn take(mine: &mut dyn TableLike, theirs: &dyn TableLike, key: &str) {
    match (mine.get_mut(key), theirs.get(key)) {
        // In place: the comments above a key belong to the key, and stay.
        (Some(slot), Some(item)) => *slot = item.clone(),
        (None, Some(item)) => _ = mine.insert(key, item.clone()),
        (_, None) => _ = mine.remove(key),
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_tables(
    base: Option<&toml::Table>,
    local: &toml::Table,
    remote: &toml::Table,
    mine: &mut dyn TableLike,
    theirs: &dyn TableLike,
    prefix: &str,
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
) {
    let keys: BTreeSet<&String> = local
        .keys()
        .chain(remote.keys())
        .chain(base.into_iter().flat_map(|b| b.keys()))
        .collect();
    for key in keys {
        let (b, l, r) = (
            base.and_then(|t| t.get(key)),
            local.get(key),
            remote.get(key),
        );
        if l == r || r == b {
            continue;
        }
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if l == b {
            take(mine, theirs, key);
            continue;
        }
        if let (Some(Toml::Table(l)), Some(Toml::Table(r))) = (l, r)
            && let (Some(m), Some(t)) = (
                mine.get_mut(key).and_then(Item::as_table_like_mut),
                theirs.get(key).and_then(Item::as_table_like),
            )
        {
            let b = match b {
                Some(Toml::Table(b)) => Some(b),
                _ => None,
            };
            merge_tables(b, l, r, m, t, &path, keep, conflicts);
            continue;
        }
        match keep {
            Some(true) => take(mine, theirs, key),
            Some(false) => {}
            None => conflicts.push(path),
        }
    }
}

fn object(text: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str(text).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// Written as Claude Code writes its files: two spaces, a final newline when the file had one.
fn pretty(map: Map<String, Value>, like: &str) -> Option<String> {
    let mut text = serde_json::to_string_pretty(&Value::Object(map)).ok()?;
    if like.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

fn merge_json(
    base: Option<&str>,
    local: &str,
    remote: &str,
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
) -> Option<String> {
    let (l, r) = (object(local)?, object(remote)?);
    let b = base.and_then(object);
    pretty(
        merge_objects(b.as_ref(), &l, &r, "", keep, conflicts),
        local,
    )
}

/// As [`merge_tables`]; this machine's keys keep their order, the other's new ones come after.
fn merge_objects(
    base: Option<&Map<String, Value>>,
    local: &Map<String, Value>,
    remote: &Map<String, Value>,
    prefix: &str,
    keep: Option<bool>,
    conflicts: &mut Vec<String>,
) -> Map<String, Value> {
    let mut out = Map::new();
    let keys = local
        .keys()
        .chain(remote.keys().filter(|k| !local.contains_key(*k)));
    for key in keys {
        let (b, l, r) = (
            base.and_then(|t| t.get(key)),
            local.get(key),
            remote.get(key),
        );
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let value = if l == r || r == b {
            l.cloned()
        } else if l == b {
            r.cloned()
        } else if let (Some(Value::Object(lo)), Some(Value::Object(ro))) = (l, r) {
            let bo = b.and_then(Value::as_object);
            Some(Value::Object(merge_objects(
                bo, lo, ro, &path, keep, conflicts,
            )))
        } else {
            if keep.is_none() {
                conflicts.push(path);
            }
            if keep == Some(true) {
                r.cloned()
            } else {
                l.cloned()
            }
        };
        if let Some(value) = value {
            out.insert(key.clone(), value);
        }
    }
    out
}

/// A path only this machine may have: absolute and out of the home folder, which travels as `$TERMF_HOME` or `~`.
pub fn machine_path(value: &str) -> bool {
    let v = value.trim().trim_start_matches(['"', '\'']).as_bytes();
    v.starts_with(b"/")
        || v.starts_with(b"\\\\")
        || v.len() > 2 && v[0].is_ascii_alphabetic() && v[1] == b':' && matches!(v[2], b'\\' | b'/')
}

/// The synced files that can hold what only makes sense here.
fn pinned_file(path: &str) -> bool {
    matches!(
        path,
        "termf.toml"
            | "ssh/config"
            | "dev/gitconfig"
            | "dev/claude.json"
            | "dev/claude/settings.json"
    )
}

/// `current` as it is synced, when it holds what only makes sense on this machine: those values become `base`'s
/// (the version last synced), or leave when it has none, so they are never sent nor taken as a change here.
pub fn pin(path: &str, current: &Entry, base: Option<&Entry>) -> Option<Entry> {
    if !pinned_file(path) {
        return None;
    }
    let text = std::str::from_utf8(&current.bytes).ok()?;
    let base = base.and_then(|b| std::str::from_utf8(&b.bytes).ok());
    let pinned = if is_toml(path) {
        pin_toml(text, base)
    } else if is_json(path) {
        pin_json(text, base)
    } else {
        lines::pin(Syntax::of(path)?, text, base)
    }?;
    Some(Entry {
        bytes: pinned.into_bytes(),
        exec: current.exec,
    })
}

/// What another machine sent, as it is written here: without what only made sense there, with what only makes
/// sense here (`current`, this machine's file as synced before [`pin`]) kept.
pub fn unpin(path: &str, incoming: &Entry, current: Option<&Entry>) -> Entry {
    let incoming = pin(path, incoming, None).unwrap_or_else(|| incoming.clone());
    let Some(current) = current else {
        return incoming;
    };
    // Against that base, this machine's own values are its only changes, and everything else is the other's: what
    // came is the text written, this machine's values put back into it.
    let Some(base) = pin(path, current, Some(&incoming)) else {
        return incoming;
    };
    settle(path, Some(&base), &incoming, current, false).unwrap_or(incoming)
}

fn pin_toml(current: &str, base: Option<&str>) -> Option<String> {
    let mut doc: DocumentMut = current.parse().ok()?;
    let base = base.and_then(|b| b.parse::<DocumentMut>().ok());
    let mut changed = false;
    pin_table(
        doc.as_table_mut(),
        base.as_ref().map(|b| b.as_table() as &dyn TableLike),
        &mut changed,
    );
    changed.then(|| doc.to_string())
}

fn pin_table(table: &mut dyn TableLike, base: Option<&dyn TableLike>, changed: &mut bool) {
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_owned()).collect();
    for key in keys {
        let b = base.and_then(|b| b.get(&key));
        let Some(item) = table.get_mut(&key) else {
            continue;
        };
        if let Some(inner) = item.as_table_like_mut() {
            pin_table(inner, b.and_then(Item::as_table_like), changed);
        } else if item.as_str().is_some_and(machine_path) {
            *changed = true;
            match b.filter(|b| b.is_value()) {
                Some(b) => *item = b.clone(),
                None => _ = table.remove(&key),
            }
        }
    }
}

fn pin_json(current: &str, base: Option<&str>) -> Option<String> {
    let mut doc = object(current)?;
    let base = base.and_then(object);
    let mut changed = false;
    pin_object(&mut doc, base.as_ref(), &mut changed);
    changed.then(|| pretty(doc, current)).flatten()
}

/// A string that is such a path, or an object run by one (an MCP server, a status line): the whole object then,
/// as it means nothing without its command.
fn machine_value(value: &Value) -> bool {
    match value {
        Value::String(s) => machine_path(s),
        Value::Object(o) => o
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(machine_path),
        _ => false,
    }
}

fn pin_object(map: &mut Map<String, Value>, base: Option<&Map<String, Value>>, changed: &mut bool) {
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let b = base.and_then(|b| b.get(&key));
        if machine_value(&map[&key]) {
            *changed = true;
            match b {
                Some(b) => _ = map.insert(key, b.clone()),
                None => _ = map.shift_remove(&key),
            }
        } else if let Some(Value::Object(inner)) = map.get_mut(&key) {
            pin_object(inner, b.and_then(Value::as_object), changed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> Entry {
        Entry {
            bytes: text.into(),
            exec: false,
        }
    }

    fn merged(base: Option<&str>, local: &str, remote: &str) -> Result<String, Vec<String>> {
        merged_as("x.toml", base, local, remote)
    }

    fn merged_as(
        path: &str,
        base: Option<&str>,
        local: &str,
        remote: &str,
    ) -> Result<String, Vec<String>> {
        merge3(
            path,
            base.map(entry).as_ref(),
            &entry(local),
            &entry(remote),
        )
        .map(|e| String::from_utf8(e.bytes).unwrap())
    }

    #[test]
    fn merges_keys_changed_on_one_side() {
        let base = "# mine\na = 1\nb = 1\n[t]\nx = 1\ny = 1\n";
        let local = "# mine\na = 2 # changed here\nb = 1\n[t]\nx = 2\ny = 1\n";
        let remote = "a = 1\nb = 3\nc = 4\n[t]\nx = 1\ny = 5\n";
        let text = merged(Some(base), local, remote).unwrap();
        assert!(text.starts_with("# mine\na = 2 # changed here\n"), "{text}");
        let table: toml::Table = toml::from_str(&text).unwrap();
        let expected: toml::Table =
            toml::from_str("a = 2\nb = 3\nc = 4\n[t]\nx = 2\ny = 5\n").unwrap();
        assert_eq!(table, expected);

        let deleted = merged(Some("a = 1\nb = 1\n"), "a = 2\nb = 1\n", "a = 1\n").unwrap();
        assert_eq!(
            toml::from_str::<toml::Table>(&deleted).unwrap(),
            toml::from_str("a = 2").unwrap()
        );
    }

    #[test]
    fn reports_keys_changed_on_both_sides() {
        assert_eq!(
            merged(
                Some("a = 1\n[t]\nx = 1\n"),
                "a = 2\n[t]\nx = 2\n",
                "a = 3\n[t]\nx = 3\n"
            ),
            Err(vec!["a".into(), "t.x".into()])
        );
        assert_eq!(
            merged(None, "a = 1\nb = 1\n", "a = 2\nc = 1\n"),
            Err(vec!["a".into()])
        );
        assert_eq!(merged(None, "a = 1", "not toml ["), Err(vec![]));
        assert_eq!(
            merge3("notes.md", None, &entry("a"), &entry("b")).map(|_| ()),
            Err(vec![])
        );
    }

    #[test]
    fn keeping_one_side_keeps_the_others_other_changes() {
        let settle = |path: &str, keep_remote| {
            settle(
                path,
                Some(&entry("a = 1\nb = 1\n")),
                &entry("a = 2\nb = 1\n"),
                &entry("a = 3\nb = 1\nc = 1\n"),
                keep_remote,
            )
            .map(|e| toml::from_str::<toml::Table>(&String::from_utf8(e.bytes).unwrap()).unwrap())
        };
        assert_eq!(
            settle("x.toml", false),
            Some(toml::from_str("a = 2\nb = 1\nc = 1").unwrap())
        );
        assert_eq!(
            settle("x.toml", true),
            Some(toml::from_str("a = 3\nb = 1\nc = 1").unwrap())
        );
        assert_eq!(settle("x.md", true), None);
    }

    #[test]
    fn json_merges_key_by_key_in_this_machines_order() {
        let base = r#"{"model": "opus", "mcpServers": {"db": {"command": "db"}}}"#;
        let local = r#"{"theme": "dark", "model": "opus", "mcpServers": {"db": {"command": "db"}, "web": {"command": "web"}}}"#;
        let remote =
            r#"{"model": "sonnet", "mcpServers": {"db": {"command": "db2"}}, "env": {"X": "1"}}"#;
        let text = merged_as("dev/claude/settings.json", Some(base), local, remote).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"theme": "dark", "model": "sonnet",
                "mcpServers": {"db": {"command": "db2"}, "web": {"command": "web"}}, "env": {"X": "1"}})
        );
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["theme", "model", "mcpServers", "env"]);

        // Without a common history: what each side has alone stays, the same server set differently conflicts.
        assert_eq!(
            merged_as(
                "dev/claude.json",
                None,
                r#"{"mcpServers": {"db": {"command": "a"}, "x": {}}}"#,
                r#"{"mcpServers": {"db": {"command": "b"}, "y": {}}}"#
            ),
            Err(vec!["mcpServers.db.command".into()])
        );
        assert_eq!(merged_as("a.json", None, "[1]", "[2]"), Err(vec![]));
        assert!(same(
            "a.json",
            Some(&entry("{\"a\": 1}")),
            Some(&entry("{\n  \"a\":1\n}\n"))
        ));
    }

    #[test]
    fn journals_keep_the_lines_both_sides_added() {
        let merged = |base: Option<&str>, local: &str, remote: &str| {
            merged_as("scout/journal.jsonl", base, local, remote)
        };
        assert_eq!(
            merged(Some("a\n"), "a\nb\n", "a\nc\n").unwrap(),
            "a\nb\nc\n"
        );
        assert_eq!(merged(None, "a\nb\n", "a\nc\n").unwrap(), "a\nb\nc\n");
        assert_eq!(merged(None, "a\n", "ab\n").unwrap(), "a\nab\n");
        // A line rewritten instead of added: the whole file conflicts.
        assert_eq!(merged(Some("a\n"), "x\n", "a\nc\n"), Err(vec![]));
    }

    #[test]
    fn compares_toml_by_value() {
        let (a, b) = (entry("a = 1 # note\n"), entry("a=1"));
        assert!(same("x.toml", Some(&a), Some(&b)));
        assert!(!same("x.md", Some(&a), Some(&b)));
        assert!(!same("x.toml", Some(&a), None));
        let exec = Entry {
            exec: true,
            ..a.clone()
        };
        assert!(!same("x.toml", Some(&a), Some(&exec)));
    }

    #[test]
    fn what_only_makes_sense_here_stays_here() {
        assert!(machine_path("/opt/homebrew/bin/gpg"));
        assert!(machine_path("\"C:\\Program Files\\Git\\bin\\ssh.exe\" -T"));
        assert!(!machine_path("$TERMF_HOME/.gitignore"));
        assert!(!machine_path("~/.ssh/id_work"));
        assert!(!machine_path("code --wait"));

        let text = |e: Entry| String::from_utf8(e.bytes).unwrap();
        let a = entry(
            "editor = \"/usr/local/bin/subl -w\"\nscan_depth = 3\n[agents]\nclaude = \"claude\"\n",
        );
        // Never sent: not in what was synced, it leaves; in it, it stays as synced.
        assert_eq!(
            text(pin("termf.toml", &a, None).unwrap()),
            "scan_depth = 3\n[agents]\nclaude = \"claude\"\n"
        );
        let base = entry("editor = \"zed\"\nscan_depth = 3\n");
        assert!(
            text(pin("termf.toml", &a, Some(&base)).unwrap()).starts_with("editor = \"zed\"\n")
        );
        assert!(pin("projects/acme.toml", &a, None).is_none());

        // Never taken: what arrives keeps this machine's own value and drops the other's.
        let incoming = entry("editor = \"/opt/other/ed\"\nscan_depth = 5\n");
        let written = text(unpin("termf.toml", &incoming, Some(&a)));
        let table: toml::Table = toml::from_str(&written).unwrap();
        assert_eq!(
            table,
            toml::from_str("editor = \"/usr/local/bin/subl -w\"\nscan_depth = 5\n").unwrap()
        );
        assert_eq!(
            text(unpin("termf.toml", &incoming, None)),
            "scan_depth = 5\n"
        );

        let servers = entry(
            r#"{"mcpServers": {"db": {"command": "/opt/homebrew/bin/db"}, "web": {"command": "npx", "args": ["web"]}}}"#,
        );
        let synced: Value =
            serde_json::from_slice(&pin("dev/claude.json", &servers, None).unwrap().bytes).unwrap();
        assert_eq!(
            synced,
            serde_json::json!({"mcpServers": {"web": {"command": "npx", "args": ["web"]}}})
        );
    }
}
