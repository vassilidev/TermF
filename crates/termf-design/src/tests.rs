use std::path::PathBuf;

use super::*;

/// A throwaway repository with these files.
fn repo(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termf-design-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for (file, text) in files {
        let path = dir.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    dir
}

fn token<'a>(d: &'a Design, name: &str) -> &'a Token {
    d.tokens.iter().find(|t| t.name == name).unwrap_or_else(|| {
        panic!(
            "no {name} in {:?}",
            d.tokens.iter().map(|t| &t.name).collect::<Vec<_>>()
        )
    })
}

fn kinds(d: &Design, kind: Kind) -> Vec<&Finding> {
    d.findings.iter().filter(|f| f.kind == kind).collect()
}

#[test]
fn smoke() {
    let dir = repo("smoke", &[("a.css", ":root { --bg: #fff; }")]);
    let d = scan(&dir);
    assert_eq!(token(&d, "--bg").color.as_deref(), Some("#ffffff"));
    assert!(kinds(&d, Kind::Unused).len() == 1);
}
