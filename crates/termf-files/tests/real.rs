//! The real git in a folder with accents and spaces. Alone in its test binary: it gives the whole process a
//! throwaway HOME and git config, so the user's own git config never changes what TermF's git calls answer.

use std::fs;
use std::path::Path;
use std::process::Command;

use termf_files::{Content, Files, Saved};

fn s(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Names, status, search, CRLF and a missing final newline kept on save, and a read-only file refused without
/// being touched.
#[test]
fn real_files_with_odd_names_and_endings() {
    let dir = std::env::temp_dir().join(format!("termf-files-dépôt acmé-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let home = dir.join("home");
    let repo = dir.join("mon dépôt");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(repo.join("dossier é")).unwrap();
    let config = home.join(".gitconfig");
    fs::write(
        &config,
        "[user]\n\tname = Alex\n\temail = alex@example.com\n",
    )
    .unwrap();
    // SAFETY: the only test of this binary, set before any thread or child process starts.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("GIT_CONFIG_GLOBAL", &config);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    let run = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
    };
    run(&["init", "-q"]);
    fs::write(repo.join("dossier é/à b.txt"), "un\r\ndeux\r\n").unwrap();
    fs::write(repo.join("日本.md"), "sans fin").unwrap();
    run(&["add", "-A"]);
    run(&["commit", "-q", "-m", "x"]);
    fs::write(repo.join("dossier é/à b.txt"), "un\r\nDEUX\r\n").unwrap();
    fs::write(repo.join("nouveau fichier.txt"), "").unwrap();
    let files = Files::default();
    files.set_roots([&repo]);
    let root = s(&repo);

    let status = files.status(root).unwrap();
    assert_eq!(status.get("dossier é/à b.txt"), Some(&'M'), "{status:?}");
    assert_eq!(status.get("nouveau fichier.txt"), Some(&'?'), "{status:?}");
    assert_eq!(
        files.find(root, "àb", true).unwrap()[0],
        "dossier é/à b.txt"
    );
    let hits = files.search(root, "deux", false).unwrap().hits;
    assert_eq!(hits.len(), 1);
    assert_eq!(
        (hits[0].path.as_str(), hits[0].line),
        ("dossier é/à b.txt", 2)
    );

    let opened = files.read(root, "dossier é/à b.txt").unwrap();
    let Content::Text { text, eol } = opened.content else {
        panic!("not text")
    };
    assert_eq!(eol, Some("\r\n"));
    let Some(Content::Text {
        text: committed, ..
    }) = files.head(root, "dossier é/à b.txt").unwrap()
    else {
        panic!("no HEAD version")
    };
    assert_eq!(committed, "un\r\ndeux\r\n");
    let saved = files
        .write(
            root,
            "dossier é/à b.txt",
            &text.replace("un", "UN"),
            Some(opened.stamp),
        )
        .unwrap();
    assert!(matches!(saved, Saved::Saved { .. }));
    assert_eq!(
        fs::read(repo.join("dossier é/à b.txt")).unwrap(),
        b"UN\r\nDEUX\r\n"
    );
    let Content::Text { text, eol } = files.read(root, "日本.md").unwrap().content else {
        panic!("not text")
    };
    assert_eq!((text.as_str(), eol), ("sans fin", Some("\n")));
    assert!(matches!(
        files.read(root, "nouveau fichier.txt").unwrap().content,
        Content::Text { .. }
    ));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let locked = repo.join("日本.md");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o444)).unwrap();
        // root writes through the mode bits: nothing to check then.
        if fs::OpenOptions::new().write(true).open(&locked).is_err() {
            assert!(files.write(root, "日本.md", "écrasé", None).is_err());
            assert_eq!(fs::read_to_string(&locked).unwrap(), "sans fin");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}
