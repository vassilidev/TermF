//! Property tests: many cases drawn from a fixed seed, so a failure replays.

use termf_core::rng::{AWKWARD, Rng};
use toml::{Table, Value};

use super::*;
use crate::crypto::{self, DataKeys};
use crate::record::{Entry, Record, decode_all, encode};

#[test]
fn sealed_data_opens_only_as_sealed() {
    let mut rng = Rng::new(0x5ea1);
    for _ in 0..500 {
        let keys = DataKeys::new(&crypto::random(), crypto::random());
        let name = rng.text(AWKWARD, 20);
        let len = rng.below(3000);
        let plain = rng.bytes(len);
        let blob = keys.seal_blob(&name, &plain);
        assert_eq!(keys.open_blob(&name, &blob).unwrap().unwrap(), plain);
        let mut flipped = blob.clone();
        let at = rng.below(flipped.len());
        flipped[at] ^= 1 << rng.below(8);
        // A flipped bit in the key generation reads as another generation; anywhere else, as damage.
        assert!(!matches!(keys.open_blob(&name, &flipped), Ok(Some(_))));
        let cut = rng.below(blob.len());
        assert!(!matches!(keys.open_blob(&name, &blob[..cut]), Ok(Some(_))));
        assert!(keys.open_blob(&format!("{name}x"), &blob).is_err());
        assert_eq!(crypto::unhex(&crypto::hex(&plain)).unwrap(), plain);
    }
}

#[test]
fn records_decode_as_encoded() {
    let mut rng = Rng::new(0x2ec);
    for _ in 0..300 {
        let mut out = Vec::new();
        let mut records = BTreeMap::new();
        for _ in 0..rng.below(6) {
            let path = format!("{}{}", rng.text(AWKWARD, 12), records.len());
            let entry = (!rng.chance(4)).then(|| Entry {
                bytes: {
                    let len = rng.below(500);
                    rng.bytes(len)
                },
                exec: rng.chance(2),
            });
            let clock = (0..rng.below(3))
                .map(|_| (rng.text(&['a', 'b', 'é'], 4), rng.next() >> 1))
                .collect();
            let record = Record {
                path: path.clone(),
                entry,
                clock,
            };
            encode(&record, &mut out);
            records.insert(path, record);
        }
        assert_eq!(decode_all(&out).unwrap(), records);
        if !out.is_empty() {
            let cut = rng.below(out.len());
            let _ = decode_all(&out[..cut]);
        }
    }
}

fn key(rng: &mut Rng) -> String {
    rng.pick(&[
        "a",
        "b",
        "c",
        "name",
        "dotted.key",
        "é",
        "with space",
        "\"q\"",
        "1",
    ])
    .to_string()
}

fn leaf(rng: &mut Rng) -> Value {
    match rng.below(4) {
        0 => Value::Integer(rng.below(100) as i64),
        1 => Value::String(rng.text(AWKWARD, 8).replace('\0', "")),
        2 => Value::Boolean(rng.chance(2)),
        _ => Value::Array(vec![Value::Integer(rng.below(9) as i64)]),
    }
}

fn table(rng: &mut Rng, depth: usize) -> Table {
    let mut t = Table::new();
    for _ in 0..rng.below(5) {
        let v = if depth > 0 && rng.chance(3) {
            Value::Table(table(rng, depth - 1))
        } else {
            leaf(rng)
        };
        t.insert(key(rng), v);
    }
    t
}

/// Every path to a value or a table of `t`.
fn paths(t: &Table, prefix: &[String], out: &mut Vec<Vec<String>>) {
    for (k, v) in t {
        let mut p = prefix.to_vec();
        p.push(k.clone());
        out.push(p.clone());
        if let Value::Table(inner) = v {
            paths(inner, &p, out);
        }
    }
}

/// Changes the value at `path` (or adds a key beside it), removes it, or adds a table: one change.
fn change(rng: &mut Rng, t: &mut Table, path: &[String]) {
    let (last, parents) = path.split_last().unwrap();
    let mut at = t;
    for p in parents {
        at = at.get_mut(p).and_then(Value::as_table_mut).unwrap();
    }
    match rng.below(3) {
        0 => _ = at.remove(last),
        1 => _ = at.insert(last.clone(), leaf(rng)),
        _ => _ = at.insert(format!("{last}-new"), Value::Table(table(rng, 1))),
    }
}

fn text(t: &Table, rng: &mut Rng) -> String {
    if rng.chance(2) {
        toml::to_string(t).unwrap()
    } else {
        toml::to_string_pretty(t).unwrap()
    }
}

#[test]
fn changes_to_different_keys_on_two_machines_both_stay() {
    let mut rng = Rng::new(0x7031);
    let mut merged_cases = 0;
    for case in 0..3000 {
        let base = table(&mut rng, 2);
        let mut all = Vec::new();
        paths(&base, &[], &mut all);
        if all.len() < 2 {
            continue;
        }
        let a = all[rng.below(all.len())].clone();
        let b = all[rng.below(all.len())].clone();
        // Disjoint: neither path holds the other.
        if a.starts_with(&b) || b.starts_with(&a) {
            continue;
        }
        let (mut local, mut remote) = (base.clone(), base.clone());
        let mut seed = Rng::new(rng.next());
        change(&mut seed, &mut local, &a);
        let mut again = Rng::new(seed.next());
        change(&mut again, &mut remote, &b);
        // The remote's change applied on top of the local version: what the merge must give.
        let mut expected = local.clone();
        apply(&base, &remote, &b, &mut expected);
        let entry = |t: &Table, rng: &mut Rng| Entry {
            bytes: text(t, rng).into_bytes(),
            exec: false,
        };
        let (be, le, re) = (
            entry(&base, &mut rng),
            entry(&local, &mut rng),
            entry(&remote, &mut rng),
        );
        let merged = merge::merge3("x.toml", Some(&be), &le, &re).unwrap_or_else(|c| {
            panic!(
                "case {case}: conflicts {c:?}\nbase {base:?}\nlocal {local:?}\nremote {remote:?}"
            )
        });
        let text = String::from_utf8(merged.bytes).unwrap();
        let got: Table = toml::from_str(&text)
            .unwrap_or_else(|e| panic!("case {case}: unreadable merge {e}\n{text}"));
        assert_eq!(got, expected, "case {case}\n{text}");
        merged_cases += 1;
    }
    assert!(merged_cases > 500, "{merged_cases}");
}

/// Puts into `out` what `changed` has at `path` (or removes it, and the key added beside it).
fn apply(base: &Table, changed: &Table, path: &[String], out: &mut Table) {
    let (last, parents) = path.split_last().unwrap();
    let (mut from, mut to, mut was) = (changed, out, base);
    for p in parents {
        from = from[p].as_table().unwrap();
        to = to.get_mut(p).and_then(Value::as_table_mut).unwrap();
        was = was[p].as_table().unwrap();
    }
    for k in [last.clone(), format!("{last}-new")] {
        if from.get(&k) == was.get(&k) {
            continue;
        }
        match from.get(&k) {
            Some(v) => _ = to.insert(k, v.clone()),
            None => _ = to.remove(&k),
        }
    }
}
