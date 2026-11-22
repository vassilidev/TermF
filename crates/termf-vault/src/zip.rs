//! Just enough of ZIP to read one file out of an archive (1Password's `.1pux`): stored or deflated, never
//! encrypted, no ZIP64. Bounded, so an archive that would inflate without end is refused.

use std::io::Read;

use anyhow::{Context, Result, bail, ensure};
use flate2::Crc;
use flate2::read::DeflateDecoder;
use zeroize::Zeroizing;

const END: u32 = 0x0605_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const LOCAL: u32 = 0x0403_4b50;

pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(&LOCAL.to_le_bytes())
}

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?).into())
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?)).ok()
}

/// The file `name` of the archive, `max` bytes at most once inflated.
pub fn read(archive: &[u8], name: &str, max: usize) -> Result<Zeroizing<Vec<u8>>> {
    let damaged = "this ZIP archive is damaged";
    // The end record is in the last 22 bytes plus a comment of 64 KiB at most.
    let from = archive.len().saturating_sub(22 + 0xffff);
    let end = (from..archive.len().saturating_sub(21))
        .rev()
        .find(|&i| u32_at(archive, i) == Some(END as usize))
        .context(damaged)?;
    let entries = u16_at(archive, end + 10).context(damaged)?;
    let mut at = u32_at(archive, end + 16).context(damaged)?;
    for _ in 0..entries {
        ensure!(u32_at(archive, at) == Some(CENTRAL as usize), damaged);
        let field = |offset| u16_at(archive, at + offset).context(damaged);
        let (flags, method) = (field(8)?, field(10)?);
        let crc = u32_at(archive, at + 16).context(damaged)?;
        let size = u32_at(archive, at + 20).context(damaged)?;
        let (name_len, extra, comment) = (field(28)?, field(30)?, field(32)?);
        let local = u32_at(archive, at + 42).context(damaged)?;
        let entry = archive.get(at + 46..at + 46 + name_len).context(damaged)?;
        at += 46 + name_len + extra + comment;
        if entry != name.as_bytes() {
            continue;
        }
        ensure!(flags & 1 == 0, "this ZIP archive is encrypted");
        ensure!(u32_at(archive, local) == Some(LOCAL as usize), damaged);
        let start = local
            + 30
            + u16_at(archive, local + 26).context(damaged)?
            + u16_at(archive, local + 28).context(damaged)?;
        let data = archive.get(start..start + size).context(damaged)?;
        let mut out = Zeroizing::new(Vec::new());
        match method {
            0 => out.extend_from_slice(data),
            8 => {
                DeflateDecoder::new(data)
                    .take(max as u64 + 1)
                    .read_to_end(&mut out)
                    .context(damaged)?;
            }
            _ => bail!("this ZIP archive uses a compression TermF does not read"),
        }
        ensure!(out.len() <= max, "{name} is too large");
        let mut check = Crc::new();
        check.update(&out);
        ensure!(check.sum() as usize == crc, damaged);
        return Ok(out);
    }
    bail!("{name} is not in this archive")
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use flate2::{Compression, write::DeflateEncoder};

    use super::*;

    /// A ZIP of deflated files, as 1Password writes one.
    pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for (name, data) in files {
            let mut deflate = DeflateEncoder::new(Vec::new(), Compression::default());
            deflate.write_all(data).unwrap();
            let packed = deflate.finish().unwrap();
            let mut crc = Crc::new();
            crc.update(data);
            let offset = out.len() as u32;
            let header = |sig: u32, central: bool| {
                let mut h = sig.to_le_bytes().to_vec();
                if central {
                    h.extend(20u16.to_le_bytes());
                }
                h.extend(20u16.to_le_bytes());
                h.extend(0u16.to_le_bytes());
                h.extend(8u16.to_le_bytes());
                h.extend([0u8; 4]);
                h.extend(crc.sum().to_le_bytes());
                h.extend((packed.len() as u32).to_le_bytes());
                h.extend((data.len() as u32).to_le_bytes());
                h.extend((name.len() as u16).to_le_bytes());
                h.extend(0u16.to_le_bytes());
                if central {
                    h.extend([0u8; 10]);
                    h.extend(offset.to_le_bytes());
                }
                h.extend(name.as_bytes());
                h
            };
            out.extend(header(LOCAL, false));
            out.extend(&packed);
            central.extend(header(CENTRAL, true));
        }
        let start = out.len() as u32;
        out.extend(&central);
        out.extend(END.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(start.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    #[test]
    fn reads_one_file_and_refuses_what_it_cannot() {
        let archive = zip(&[
            ("export.attributes", b"{}"),
            ("export.data", b"{\"accounts\": []}"),
        ]);
        assert!(is_zip(&archive));
        assert_eq!(
            &*read(&archive, "export.data", 100).unwrap(),
            b"{\"accounts\": []}"
        );
        assert!(read(&archive, "export.data", 5).is_err(), "too large");
        assert!(read(&archive, "missing", 100).is_err());
        assert!(read(&archive[..archive.len() - 30], "export.data", 100).is_err());
        assert!(read(b"PK\x03\x04 not really", "export.data", 100).is_err());
    }
}
