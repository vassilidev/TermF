//! Edge cases against the real git: odd paths, line endings, empty, detached and nested repositories.

use std::fs;

use super::Sandbox;
use crate::*;

fn pick(hunk: usize, lines: &[usize]) -> Pick {
    Pick {
        hunk,
        lines: Some(lines.to_vec()),
    }
}

#[test]
fn odd_paths_are_listed_diffed_staged_and_blamed() {
    let sb = Sandbox::new("edge-odd paths");
    let repo = sb.clone("dépôt acmé");
    let names = [
        "dossier é/fichier à.txt",
        "-rf",
        "*.txt",
        "tab\there.txt",
        "quote\"d.txt",
        "日本語.md",
    ];
    for name in names {
        if let Some(dir) = std::path::Path::new(name).parent() {
            fs::create_dir_all(repo.join(dir)).unwrap();
        }
        // Distinct contents: `--follow` takes an identical file for a copy source.
        fs::write(repo.join(name), format!("{name}\ntwo\n")).unwrap();
        sb.git(&repo, &["add", "--", &format!(":(literal){name}")]);
        sb.git(&repo, &["commit", "-q", "-m", "Add"]);
        fs::write(repo.join(name), format!("{name}\nTWO\nthree\n")).unwrap();
    }
    let state = sb.git.state(&repo).unwrap();
    let mut listed: Vec<_> = state.worktrees[0]
        .changes
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    listed.sort();
    let mut expected = names.to_vec();
    expected.sort();
    assert_eq!(listed, expected);

    for name in names {
        let diff = sb.git.diff(&repo, name, None, false).unwrap();
        assert_eq!(diff.path, name);
        assert_eq!(diff.hunks.len(), 1, "{name}: {diff:?}");
        let added = diff.hunks[0]
            .lines
            .iter()
            .position(|l| l.text == "three")
            .unwrap();
        sb.git
            .stage(&repo, name, Some(&[pick(0, &[added])]), Some(&diff.id))
            .unwrap();
        let staged = sb.git.diff(&repo, name, None, true).unwrap();
        assert_eq!((staged.added, staged.removed), (1, 0), "{name}");
        let blame = sb.git.blame(&repo, name).unwrap();
        assert_eq!(blame.lines.len(), 3, "{name}");
        let history = sb.git.history(&repo, name, 10).unwrap();
        assert_eq!(history.commits.len(), 1, "{name}: {history:?}");
    }
    // `*.txt` staged alone: a pathspec would have taken every .txt file.
    sb.git.stage(&repo, "*.txt", None, None).unwrap();
    let staged = sb
        .git
        .diff(&repo, "dossier é/fichier à.txt", None, true)
        .unwrap();
    assert_eq!(staged.added, 1);
}

#[test]
fn crlf_lines_are_staged_one_by_one_and_kept_crlf() {
    let sb = Sandbox::new("edge-crlf");
    let repo = sb.clone("acme");
    sb.commit(&repo, "w.txt", "a\r\nb\r\nc\r\n", "CRLF");
    fs::write(repo.join("w.txt"), "a\r\nB\r\nc\r\nd\r\n").unwrap();
    let diff = sb.git.diff(&repo, "w.txt", None, false).unwrap();
    let d = diff.hunks[0]
        .lines
        .iter()
        .position(|l| l.kind == '+' && l.text.trim_end() == "d")
        .unwrap();
    sb.git
        .stage(&repo, "w.txt", Some(&[pick(0, &[d])]), Some(&diff.id))
        .unwrap();
    assert_eq!(
        sb.git.run(&repo, &["show", ":w.txt"]).unwrap(),
        "a\r\nb\r\nc\r\nd\r\n"
    );
    let diff = sb.git.diff(&repo, "w.txt", None, false).unwrap();
    let pos = |kind: char, text: &str| {
        diff.hunks[0]
            .lines
            .iter()
            .position(|l| l.kind == kind && l.text.trim_end() == text)
            .unwrap()
    };
    sb.git
        .discard(
            &repo,
            "w.txt",
            Some(&[pick(0, &[pos('-', "b"), pos('+', "B")])]),
            &diff.id,
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("w.txt")).unwrap(),
        "a\r\nb\r\nc\r\nd\r\n"
    );
}

#[test]
fn an_empty_repository_has_a_state_a_graph_and_a_first_commit() {
    let sb = Sandbox::new("edge-empty");
    let repo = sb.path("empty");
    fs::create_dir_all(&repo).unwrap();
    sb.git(&repo, &["init", "-q"]);
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.head, None);
    assert_eq!(state.current.as_deref(), Some("main"));
    assert!(sb.git.graph(&repo, 100).unwrap().commits.is_empty());
    assert!(sb.git.tags(&repo).unwrap().is_empty());
    assert!(sb.git.stashes(&repo).unwrap().is_empty());
    fs::write(repo.join("a.txt"), "a").unwrap();
    sb.git.stage(&repo, "a.txt", None, None).unwrap();
    let oid = sb.git.commit(&repo, "First", false, None).unwrap();
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.head.as_deref(), Some(oid.as_str()));
    assert_eq!(sb.git.graph(&repo, 100).unwrap().commits.len(), 1);
    assert_eq!(sb.git.show(&repo, &oid).unwrap().files.len(), 1);
}

#[test]
fn a_detached_head_and_a_submodule_are_read() {
    let sb = Sandbox::new("edge-detached");
    let repo = sb.clone("acme");
    let first = sb.git(&repo, &["rev-parse", "HEAD"]);
    sb.commit(&repo, "b.txt", "b\n", "Second");
    sb.git(&repo, &["checkout", "-q", "--detach", &first]);
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.current, None);
    assert_eq!(state.head.as_deref(), Some(first.as_str()));
    assert_eq!(sb.git.graph(&repo, 100).unwrap().commits.len(), 2);

    sb.git(&repo, &["checkout", "-q", "main"]);
    let remote = sb.path("remote.git");
    sb.git(
        &repo,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            remote.to_str().unwrap(),
            "sub",
        ],
    );
    sb.git(&repo, &["commit", "-q", "-m", "Submodule"]);
    fs::write(repo.join("sub/README.md"), "changed\n").unwrap();
    let state = sb.git.state(&repo).unwrap();
    let changes = &state.worktrees[0].changes;
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].path, "sub");
    let diff = sb.git.diff(&repo, "sub", None, false).unwrap();
    assert!(!diff.partial, "{diff:?}");
    let err = sb
        .git
        .stage(&repo, "sub", Some(&[pick(0, &[1])]), Some(&diff.id))
        .unwrap_err();
    assert!(
        format!("{err:#}").contains("cannot be staged in parts"),
        "{err:#}"
    );
    sb.git(&repo.join("sub"), &["commit", "-q", "-am", "Moved"]);
    let diff = sb.git.diff(&repo, "sub", None, false).unwrap();
    assert!(!diff.partial);
    sb.git.stage(&repo, "sub", None, None).unwrap();
    assert!(!sb.git.diff(&repo, "sub", None, true).unwrap().partial);
}
