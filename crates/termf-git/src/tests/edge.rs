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
    let mut names = vec!["dossier é/fichier à.txt", "-rf", "日本語.md"];
    // Not valid file names on Windows.
    if cfg!(unix) {
        names.extend(["*.txt", "tab\there.txt", "quote\"d.txt"]);
    }
    for &name in &names {
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
    let mut expected = names.clone();
    expected.sort();
    assert_eq!(listed, expected);

    for &name in &names {
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
    if cfg!(unix) {
        sb.git.stage(&repo, "*.txt", None, None).unwrap();
        let staged = sb
            .git
            .diff(&repo, "dossier é/fichier à.txt", None, true)
            .unwrap();
        assert_eq!(staged.added, 1);
    }
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
    // TermF's diff stays the short one whatever the user's `diff.submodule` says.
    sb.git(&repo, &["config", "diff.submodule", "diff"]);
    let state = sb.git.state(&repo).unwrap();
    let changes = &state.worktrees[0].changes;
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(changes[0].path, "sub");
    let diff = sb.git.diff(&repo, "sub", None, false).unwrap();
    assert!(!diff.partial, "{diff:?}");
    assert_eq!((diff.added, diff.removed), (1, 1), "{diff:?}");
    assert!(diff.hunks[0].lines[1].text.ends_with("-dirty"), "{diff:?}");
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

/// xorshift64 with a fixed seed: the same cases on every run.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % n.max(1)
    }
}

fn random_text(rng: &mut Rng, base: &[String], eol: &str) -> String {
    let mut lines = base.to_vec();
    for _ in 0..rng.below(6) {
        let at = rng.below(lines.len() as u64 + 1) as usize;
        match rng.below(3) {
            0 if at < lines.len() => {
                lines.remove(at);
            }
            1 if at < lines.len() => lines[at] = format!("é{}", rng.below(1000)),
            _ => lines.insert(at, format!("new {}", rng.below(1000))),
        }
    }
    let mut text = lines.join(eol);
    if !lines.is_empty() && rng.below(3) > 0 {
        text.push_str(eol);
    }
    text
}

fn every_line(diff: &FileDiff) -> Vec<Pick> {
    (0..diff.hunks.len())
        .map(|h| pick(h, &(0..diff.hunks[h].lines.len()).collect::<Vec<_>>()))
        .collect()
}

/// Any subset of lines staged, then the rest, gives the work tree; unstaged line by line gives HEAD back; and
/// discarded line by line gives the index back.
#[test]
fn staging_random_lines_adds_up_to_the_file() {
    let sb = Sandbox::new("edge-random");
    let repo = sb.clone("acme");
    let mut rng = Rng(0x5eed_0f7e_3f00);
    let show = |sb: &Sandbox| sb.git.run(&repo, &["show", ":f.txt"]).unwrap();
    for case in 0..25 {
        let eol = if rng.below(4) == 0 { "\r\n" } else { "\n" };
        let base: Vec<String> = (0..rng.below(12)).map(|i| format!("line {i}")).collect();
        let head = random_text(&mut rng, &base, eol);
        fs::write(repo.join("f.txt"), &head).unwrap();
        sb.git(&repo, &["add", "f.txt"]);
        sb.git(&repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
        let lines: Vec<String> = head.lines().map(str::to_owned).collect();
        let work = random_text(&mut rng, &lines, eol);
        fs::write(repo.join("f.txt"), &work).unwrap();
        let what = format!("case {case}: {head:?} -> {work:?}");

        let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
        if !diff.partial {
            assert!(diff.hunks.is_empty(), "{what}: {diff:?}");
            continue;
        }
        let subset: Vec<Pick> = (0..diff.hunks.len())
            .map(|h| {
                let lines: Vec<usize> = (0..diff.hunks[h].lines.len())
                    .filter(|_| rng.below(2) == 0)
                    .collect();
                pick(h, &lines)
            })
            .collect();
        if let Err(e) = sb.git.stage(&repo, "f.txt", Some(&subset), Some(&diff.id)) {
            assert!(
                format!("{e:#}").contains("no change picked"),
                "{what}: {e:#}"
            );
        }
        let rest = sb.git.diff(&repo, "f.txt", None, false).unwrap();
        if !rest.hunks.is_empty() {
            sb.git
                .stage(&repo, "f.txt", Some(&every_line(&rest)), Some(&rest.id))
                .unwrap_or_else(|e| panic!("{what}: {e:#}"));
        }
        assert_eq!(show(&sb), work, "{what}");

        let staged = sb.git.diff(&repo, "f.txt", None, true).unwrap();
        sb.git
            .unstage(
                &repo,
                "f.txt",
                None,
                Some(&every_line(&staged)),
                Some(&staged.id),
            )
            .unwrap_or_else(|e| panic!("{what}: {e:#}"));
        assert_eq!(show(&sb), head, "{what}");

        let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
        sb.git
            .discard(&repo, "f.txt", Some(&every_line(&diff)), &diff.id)
            .unwrap_or_else(|e| panic!("{what}: {e:#}"));
        assert_eq!(
            fs::read_to_string(repo.join("f.txt")).unwrap(),
            head,
            "{what}"
        );
    }
}
