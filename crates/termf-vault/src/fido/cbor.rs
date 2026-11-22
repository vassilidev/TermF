//! The CBOR subset CTAP2 messages use: integers, byte and text strings, arrays, maps and booleans, definite
//! lengths only, maps written in CTAP2's canonical key order.

use anyhow::{Result, bail, ensure};

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
}

impl Value {
    pub fn text(s: &str) -> Self {
        Self::Text(s.into())
    }

    pub fn bytes(b: &[u8]) -> Self {
        Self::Bytes(b.into())
    }

    /// The value at an integer key of a map.
    pub fn at(&self, key: i64) -> Option<&Value> {
        self.find(&Value::Int(key))
    }

    /// The value at a text key of a map.
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.find(&Value::text(key))
    }

    fn find(&self, key: &Value) -> Option<&Value> {
        match self {
            Self::Map(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }
}

fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let major = major << 5;
    match n {
        0..=23 => out.push(major | n as u8),
        24..=0xff => out.extend([major | 24, n as u8]),
        0x100..=0xffff => {
            out.push(major | 25);
            out.extend((n as u16).to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(major | 26);
            out.extend((n as u32).to_be_bytes());
        }
        _ => {
            out.push(major | 27);
            out.extend(n.to_be_bytes());
        }
    }
}

pub fn encode(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write(value, &mut out);
    out
}

fn write(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Int(n) if *n >= 0 => head(out, 0, *n as u64),
        Value::Int(n) => head(out, 1, !(*n) as u64),
        Value::Bytes(b) => {
            head(out, 2, b.len() as u64);
            out.extend(b);
        }
        Value::Text(t) => {
            head(out, 3, t.len() as u64);
            out.extend(t.as_bytes());
        }
        Value::Array(items) => {
            head(out, 4, items.len() as u64);
            for item in items {
                write(item, out);
            }
        }
        Value::Map(entries) => {
            // CTAP2 canonical order: by major type, then by encoded length, then byte by byte.
            let mut encoded: Vec<(Vec<u8>, &Value)> =
                entries.iter().map(|(k, v)| (encode(k), v)).collect();
            encoded.sort_by(|(a, _), (b, _)| (a[0] >> 5, a.len(), a).cmp(&(b[0] >> 5, b.len(), b)));
            head(out, 5, encoded.len() as u64);
            for (key, v) in encoded {
                out.extend(key);
                write(v, out);
            }
        }
        Value::Bool(b) => out.push(if *b { 0xf5 } else { 0xf4 }),
        Value::Null => out.push(0xf6),
    }
}

/// One value at the start of `data`, and how many bytes it took.
pub fn decode(data: &[u8]) -> Result<(Value, usize)> {
    let mut at = 0;
    let value = read(data, &mut at, 0)?;
    Ok((value, at))
}

fn take<'a>(data: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8]> {
    let end = at.checked_add(n).filter(|end| *end <= data.len());
    let Some(end) = end else {
        bail!("truncated CBOR")
    };
    let bytes = &data[*at..end];
    *at = end;
    Ok(bytes)
}

fn read(data: &[u8], at: &mut usize, depth: usize) -> Result<Value> {
    ensure!(depth < 16, "CBOR nested too deep");
    let first = take(data, at, 1)?[0];
    let (major, info) = (first >> 5, first & 31);
    if major == 7 {
        return match info {
            20 => Ok(Value::Bool(false)),
            21 => Ok(Value::Bool(true)),
            22 | 23 => Ok(Value::Null),
            _ => bail!("unsupported CBOR value"),
        };
    }
    let n = match info {
        0..=23 => u64::from(info),
        24 => u64::from(take(data, at, 1)?[0]),
        25 => u64::from(u16::from_be_bytes(take(data, at, 2)?.try_into()?)),
        26 => u64::from(u32::from_be_bytes(take(data, at, 4)?.try_into()?)),
        27 => u64::from_be_bytes(take(data, at, 8)?.try_into()?),
        _ => bail!("indefinite CBOR lengths are not supported"),
    };
    let len = || usize::try_from(n).map_err(|_| anyhow::anyhow!("CBOR length too large"));
    Ok(match major {
        0 => Value::Int(i64::try_from(n)?),
        1 => Value::Int(-1 - i64::try_from(n)?),
        2 => Value::Bytes(take(data, at, len()?)?.to_vec()),
        3 => Value::Text(String::from_utf8(take(data, at, len()?)?.to_vec())?),
        4 => {
            // Each item takes a byte at least: a length beyond the input is a lie.
            ensure!(len()? <= data.len() - *at, "truncated CBOR");
            let items = (0..len()?)
                .map(|_| read(data, at, depth + 1))
                .collect::<Result<_>>()?;
            Value::Array(items)
        }
        5 => {
            ensure!(len()? <= data.len() - *at, "truncated CBOR");
            let entries = (0..len()?)
                .map(|_| Ok((read(data, at, depth + 1)?, read(data, at, depth + 1)?)))
                .collect::<Result<_>>()?;
            Value::Map(entries)
        }
        _ => bail!("unsupported CBOR value"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_writes_the_canonical_order() {
        let cose = Value::Map(vec![
            (Value::Int(-3), Value::bytes(&[2; 32])),
            (Value::Int(1), Value::Int(2)),
            (Value::Int(-1), Value::Int(1)),
            (Value::Int(3), Value::Int(-25)),
            (Value::Int(-2), Value::bytes(&[1; 32])),
        ]);
        let bytes = encode(&cose);
        // 1, 3, -1, -2, -3: RFC 8152's example order, which CTAP2 keys follow.
        assert_eq!(&bytes[..5], &[0xa5, 0x01, 0x02, 0x03, 0x38]);
        let (back, used) = decode(&bytes).unwrap();
        assert_eq!(used, bytes.len());
        assert_eq!(back.at(3).and_then(Value::as_int), Some(-25));
        assert_eq!(back.at(-2).and_then(Value::as_bytes), Some(&[1u8; 32][..]));

        let text = Value::Map(vec![
            (Value::text("type"), Value::text("public-key")),
            (Value::text("id"), Value::bytes(b"x")),
        ]);
        let bytes = encode(&text);
        assert_eq!(&bytes[..4], &[0xa2, 0x62, b'i', b'd'], "shorter key first");
        assert_eq!(
            decode(&bytes).unwrap().0.field("id"),
            Some(&Value::bytes(b"x"))
        );

        for n in [
            0, 23, 24, 255, 256, 65_535, 65_536, -1, -24, -25, -256, -257,
        ] {
            assert_eq!(decode(&encode(&Value::Int(n))).unwrap().0, Value::Int(n));
        }
        let mixed = Value::Array(vec![Value::Bool(true), Value::Bool(false), Value::Null]);
        assert_eq!(decode(&encode(&mixed)).unwrap().0, mixed);
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(decode(&[]).is_err());
        assert!(decode(&[0x58, 0x10, 1, 2]).is_err(), "bytes past the end");
        assert!(
            decode(&[0x9a, 0xff, 0xff, 0xff, 0xff]).is_err(),
            "huge array"
        );
        assert!(decode(&[0x5f]).is_err(), "indefinite length");
        assert!(decode(&[0xfb, 0, 0, 0, 0, 0, 0, 0, 0]).is_err(), "float");
        assert!(decode(&[0xc0, 0x00]).is_err(), "tag");
        assert!(decode(&[0x81; 40]).is_err(), "too deep");
    }
}
