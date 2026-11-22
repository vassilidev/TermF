//! Property tests: many cases drawn from a fixed seed, so a failure replays.

use termf_core::rng::{AWKWARD, Rng};

use super::*;

/// A value of the vault: never made of the mask's own dots, which would be found in any masked text.
fn value(rng: &mut Rng) -> String {
    loop {
        let mut v = rng.text(AWKWARD, 24);
        if rng.chance(3) {
            v.push_str(&rng.text(&['A', 'b', '7', '+', '/', '='], 30));
        }
        if v.len() >= 4 && !v.contains('•') {
            return v;
        }
    }
}

#[test]
fn the_masker_never_lets_a_value_through() {
    let mut rng = Rng::new(0x7e57);
    for case in 0..2000 {
        let values: Vec<String> = (0..1 + rng.below(4)).map(|_| value(&mut rng)).collect();
        let masker = Masker::new(
            &values
                .iter()
                .map(|v| Secret::new(v.clone()))
                .collect::<Vec<_>>(),
        );
        let mut text = String::new();
        for _ in 0..rng.below(6) {
            text.push_str(&rng.text(AWKWARD, 12));
            if rng.chance(2) {
                text.push_str(rng.pick(&values));
            }
        }
        let masked = masker.mask(&text);
        for v in &values {
            assert!(
                !masked.text.contains(v.as_str()),
                "case {case}: {v:?} left in {:?} (from {text:?})",
                masked.text
            );
        }
        if values.iter().any(|v| text.contains(v.as_str())) {
            assert!(masked.count > 0, "case {case}");
        }
    }
}

/// A byte flipped, a piece cut out or doubled, characters that break parsers put in.
fn mutate(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for _ in 0..1 + rng.below(4) {
        if out.is_empty() {
            break;
        }
        let at = rng.below(out.len());
        match rng.below(5) {
            0 => out[at] ^= 1 << rng.below(8),
            1 => out.truncate(at),
            2 => {
                let end = (at + rng.below(64)).min(out.len());
                out.drain(at..end);
            }
            3 => {
                let end = (at + rng.below(64)).min(out.len());
                let piece = out[at..end].to_vec();
                out.splice(at..at, piece);
            }
            _ => {
                let junk = rng.text(AWKWARD, 4);
                out.splice(at..at, junk.into_bytes());
            }
        }
    }
    out
}

#[test]
fn other_managers_files_never_crash_the_import_whatever_their_damage() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/import");
    let mut seeds: Vec<Vec<u8>> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| fs::read(e.unwrap().path()).unwrap())
        .collect();
    let data = fs::read(dir.join("1password-export.data")).unwrap();
    seeds.push(crate::zip::tests::zip(&[("export.data", &data)]));
    let mut rng = Rng::new(0x1309);
    for seed in &seeds {
        for _ in 0..400 {
            let _ = crate::foreign::parse(&mutate(&mut rng, seed));
        }
    }
    for _ in 0..2000 {
        let len = rng.below(200);
        let _ = crate::foreign::parse(&rng.bytes(len));
        let _ = crate::foreign::parse(rng.text(AWKWARD, 200).as_bytes());
    }
}

#[test]
fn a_csv_export_reads_back_as_written() {
    let env = Env::new("csv-props");
    let vault = env.vault();
    vault.init(&human(), PASSWORD, false).unwrap();
    // What the export changes on purpose: an apostrophe before what a spreadsheet would run, addresses split
    // at commas, user names and titles trimmed.
    let plain = |rng: &mut Rng, comma: bool| loop {
        let v = rng.text(AWKWARD, 30);
        let t = v.trim();
        if !t.is_empty()
            && !t.starts_with(['=', '+', '-', '@'])
            && !t.contains('\0')
            && (comma || !t.contains(','))
        {
            return t.to_owned();
        }
    };
    let mut rng = Rng::new(0xc5f);
    let mut drafts = Vec::new();
    for i in 0..60 {
        let note = rng.chance(4);
        let draft = ItemDraft {
            id: None,
            kind: if note { Kind::Note } else { Kind::Login },
            project: rng.chance(2).then(|| "acme".to_owned()),
            title: format!("{i} {}", plain(&mut rng, true)),
            urls: if note {
                vec![]
            } else {
                vec![plain(&mut rng, false)]
            },
            username: if note {
                String::new()
            } else {
                plain(&mut rng, true)
            },
            password: (!note).then(|| Secret::new(plain(&mut rng, true))),
            totp: None,
            notes: Some(Secret::new(format!(
                "{}\r\n{}",
                plain(&mut rng, true),
                plain(&mut rng, true)
            ))),
            agents: false,
        };
        vault.save_item(&human(), draft.clone()).unwrap();
        drafts.push(draft);
    }
    let mut out = Vec::new();
    vault
        .export_csv(&human(), None, &mut |bytes| {
            out = bytes.to_vec();
            Ok(())
        })
        .unwrap();
    let parsed = crate::foreign::parse(&out).unwrap();
    assert_eq!(parsed.source, "bitwarden-csv");
    assert_eq!(parsed.candidates.len(), drafts.len());
    for draft in &drafts {
        let c = parsed
            .candidates
            .iter()
            .find(|c| c.title == draft.title)
            .unwrap_or_else(|| panic!("{:?} missing", draft.title));
        let shown = |s: &Option<Secret>| s.as_ref().map(|s| s.expose().to_owned());
        assert_eq!(c.kind, draft.kind, "{:?}", draft.title);
        assert_eq!(c.folder, draft.project.clone().unwrap_or_default());
        assert_eq!(
            c.urls.iter().map(|u| u.trim()).collect::<Vec<_>>(),
            draft
                .urls
                .iter()
                .map(|u| u.trim())
                .filter(|u| !u.is_empty())
                .collect::<Vec<_>>()
        );
        assert_eq!(c.username, draft.username);
        assert_eq!(shown(&c.password), shown(&draft.password));
        assert_eq!(shown(&c.notes), shown(&draft.notes));
    }
}

#[test]
fn a_damaged_zip_is_refused_or_gives_the_file_as_it_was() {
    let mut rng = Rng::new(0x21f);
    let data: Vec<u8> = (0..3000u32).map(|i| (i * 7 % 251) as u8).collect();
    let archive = crate::zip::tests::zip(&[("a", b"first"), ("export.data", &data)]);
    for _ in 0..5000 {
        if let Ok(read) = crate::zip::read(&mutate(&mut rng, &archive), "export.data", 1 << 20) {
            assert_eq!(*read, data);
        }
    }
}
