use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub bytes: Vec<u8>,
    pub exec: bool,
}

/// How many times each device changed a file. A version that does not count every change of the version a
/// machine last saw was written without it: that machine's version must be merged, never overwritten.
pub type Clock = BTreeMap<String, u64>;

pub fn descends(newer: &Clock, older: &Clock) -> bool {
    older
        .iter()
        .all(|(device, n)| newer.get(device).is_some_and(|m| m >= n))
}

/// Counts both histories, plus one change by `device`.
pub fn next(a: &Clock, b: &Clock, device: &str) -> Clock {
    let mut clock = a.clone();
    for (d, n) in b {
        let m = clock.entry(d.clone()).or_default();
        *m = (*m).max(*n);
    }
    *clock.entry(device.into()).or_default() += 1;
    clock
}

/// A file as synced. `entry: None` is a deletion, kept so other machines delete it too.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub path: String,
    pub entry: Option<Entry>,
    pub clock: Clock,
}

#[derive(Serialize, Deserialize)]
struct Header {
    path: String,
    #[serde(default)]
    exec: bool,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    clock: Clock,
}

pub fn encode(record: &Record, out: &mut Vec<u8>) {
    let header = serde_json::to_vec(&Header {
        path: record.path.clone(),
        exec: record.entry.as_ref().is_some_and(|e| e.exec),
        deleted: record.entry.is_none(),
        clock: record.clock.clone(),
    })
    .expect("a header always serializes");
    let bytes = record.entry.as_ref().map_or(&[][..], |e| &e.bytes);
    out.extend((header.len() as u32).to_le_bytes());
    out.extend(header);
    out.extend((bytes.len() as u64).to_le_bytes());
    out.extend(bytes);
}

fn take<'a>(data: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    ensure!(data.len() >= n, "truncated sync record");
    let (head, rest) = data.split_at(n);
    *data = rest;
    Ok(head)
}

/// Reads the record at the start of `data` and moves past it.
pub fn decode(data: &mut &[u8]) -> Result<Record> {
    let len = u32::from_le_bytes(take(data, 4)?.try_into()?) as usize;
    let header: Header = serde_json::from_slice(take(data, len)?).context("invalid sync record")?;
    let len = u64::from_le_bytes(take(data, 8)?.try_into()?);
    let bytes = take(data, usize::try_from(len)?)?.to_vec();
    Ok(Record {
        path: header.path,
        entry: (!header.deleted).then_some(Entry {
            bytes,
            exec: header.exec,
        }),
        clock: header.clock,
    })
}

pub fn decode_all(mut data: &[u8]) -> Result<BTreeMap<String, Record>> {
    let mut records = BTreeMap::new();
    while !data.is_empty() {
        let record = decode(&mut data)?;
        records.insert(record.path.clone(), record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let records = [
            Record {
                path: "library/skills/s/run.sh".into(),
                entry: Some(Entry {
                    bytes: vec![0, 159, 146, 150],
                    exec: true,
                }),
                clock: Clock::from([("a".into(), 2)]),
            },
            Record {
                path: "termf.toml".into(),
                entry: None,
                clock: Clock::new(),
            },
        ];
        let mut data = Vec::new();
        for r in &records {
            encode(r, &mut data);
        }
        let back = decode_all(&data).unwrap();
        assert_eq!(back.values().cloned().collect::<Vec<_>>(), records);
        assert!(decode_all(&data[..data.len() - 1]).is_err());
    }

    #[test]
    fn clocks_order_histories() {
        let a = Clock::from([("a".into(), 2), ("b".into(), 1)]);
        let b = Clock::from([("a".into(), 1), ("b".into(), 3)]);
        assert!(!descends(&a, &b) && !descends(&b, &a));
        let merged = next(&a, &b, "c");
        assert!(descends(&merged, &a) && descends(&merged, &b));
        assert_eq!(merged["c"], 1);
        assert!(descends(&a, &Clock::new()));
    }
}
