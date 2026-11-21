//! Three-way merge of one file against the version last synced; TOML is merged key by key.

use std::collections::BTreeSet;

use toml::Value;
use toml_edit::{DocumentMut, Item, TableLike};

use crate::record::Entry;

fn is_toml(path: &str) -> bool {
    path.ends_with(".toml")
}

fn parse(entry: &Entry) -> Option<toml::Table> {
    toml::from_str(std::str::from_utf8(&entry.bytes).ok()?).ok()
}

/// Equal content; for TOML, equal values whatever the formatting.
pub fn same(path: &str, a: Option<&Entry>, b: Option<&Entry>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.exec == b.exec
                && (a.bytes == b.bytes
                    || is_toml(path) && parse(a).is_some_and(|ta| parse(b) == Some(ta)))
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
    if !is_toml(path) {
        return Err(Vec::new());
    }
    let (Some(lt), Some(rt)) = (parse(local), parse(remote)) else {
        return Err(Vec::new());
    };
    let bt = base.and_then(parse).unwrap_or_default();
    let text = |e: &Entry| String::from_utf8_lossy(&e.bytes).parse::<DocumentMut>();
    let (Ok(mut mine), Ok(theirs)) = (text(local), text(remote)) else {
        return Err(Vec::new());
    };
    let mut conflicts = Vec::new();
    merge_tables(
        Some(&bt),
        &lt,
        &rt,
        mine.as_table_mut(),
        theirs.as_table(),
        "",
        &mut conflicts,
    );
    if !conflicts.is_empty() {
        return Err(conflicts);
    }
    let exec = if base.is_none_or(|b| b.exec == local.exec) {
        remote.exec
    } else {
        local.exec
    };
    Ok(Entry {
        bytes: mine.to_string().into_bytes(),
        exec,
    })
}

fn merge_tables(
    base: Option<&toml::Table>,
    local: &toml::Table,
    remote: &toml::Table,
    mine: &mut dyn TableLike,
    theirs: &dyn TableLike,
    prefix: &str,
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
            match (mine.get_mut(key), theirs.get(key)) {
                // In place: the comments above a key belong to the key, and stay.
                (Some(slot), Some(item)) => *slot = item.clone(),
                (None, Some(item)) => _ = mine.insert(key, item.clone()),
                (_, None) => _ = mine.remove(key),
            }
            continue;
        }
        if let (Some(Value::Table(l)), Some(Value::Table(r))) = (l, r)
            && let (Some(m), Some(t)) = (
                mine.get_mut(key).and_then(Item::as_table_like_mut),
                theirs.get(key).and_then(Item::as_table_like),
            )
        {
            let b = match b {
                Some(Value::Table(b)) => Some(b),
                _ => None,
            };
            merge_tables(b, l, r, m, t, &path, conflicts);
            continue;
        }
        conflicts.push(path);
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
        merge3(
            "x.toml",
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
}
