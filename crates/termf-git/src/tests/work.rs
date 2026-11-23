use std::fs;
use std::path::Path;
use std::process::Command;

use super::Sandbox;
use crate::work::{build_patch, marker_line, parse_blame, parse_diff, parse_history, unquote};
use crate::*;

const TEN: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";

fn twenty() -> String {
    (1..=20).map(|i| format!("line {i}\n")).collect()
}

/// git as the user would run it, failure allowed.
fn try_git(sb: &Sandbox, dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .envs(sb.git.env.iter().map(|(k, v)| (k, v)))
        .env("GIT_EDITOR", ":")
        .output()
        .unwrap()
        .status
        .success()
}

fn index_of(diff: &FileDiff, kind: char, text: &str) -> (usize, usize) {
    for (h, hunk) in diff.hunks.iter().enumerate() {
        if let Some(l) = hunk
            .lines
            .iter()
            .position(|l| l.kind == kind && l.text == text)
        {
            return (h, l);
        }
    }
    panic!("no {kind}{text} in {diff:?}");
}

fn pick(hunk: usize, lines: &[usize]) -> Pick {
    Pick {
        hunk,
        lines: Some(lines.to_vec()),
    }
}

#[test]
fn parses_a_multi_file_diff() {
    let out = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n\
               @@ -1,3 +1,3 @@ fn main()\n one\n-two\n+TWO\n three\n\
               diff --git a/old.txt b/new.txt\nsimilarity index 90%\nrename from old.txt\nrename to new.txt\n\
               index 3..4 100644\n--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+a\n\
               diff --git a/logo.png b/logo.png\nnew file mode 100644\nindex 0..5\nBinary files /dev/null and b/logo.png differ\n\
               diff --git \"a/sp\\303\\251cial \\\"x\\\".txt\" \"b/sp\\303\\251cial \\\"x\\\".txt\"\ndeleted file mode 100644\n\
               index 6..0\n--- \"a/sp\\303\\251cial \\\"x\\\".txt\"\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\n\
               diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n";
    let files = parse_diff(out, 100);
    assert_eq!(files.len(), 5);
    let a = &files[0];
    assert_eq!(
        (a.path.as_str(), a.status, a.partial),
        ("src/a.rs", 'M', true)
    );
    assert_eq!((a.added, a.removed), (1, 1));
    assert_eq!(a.hunks[0].section, "fn main()");
    let two = &a.hunks[0].lines[1];
    assert_eq!((two.kind, two.old, two.new), ('-', Some(2), None));
    assert_eq!(a.hunks[0].lines[2].new, Some(2));
    let renamed = &files[1];
    assert_eq!(renamed.path, "new.txt");
    assert_eq!(renamed.from.as_deref(), Some("old.txt"));
    assert_eq!(renamed.status, 'R');
    assert!(!renamed.partial, "a rename is staged whole");
    assert!(renamed.hunks[0].lines[0].no_eol && !renamed.hunks[0].lines[1].no_eol);
    assert!(files[2].binary && files[2].status == 'A' && !files[2].partial);
    assert_eq!(files[2].path, "logo.png");
    assert_eq!(files[3].path, "spécial \"x\".txt");
    assert_eq!(files[3].status, 'D');
    assert_eq!(files[4].path, "run.sh");
    assert!(!files[4].partial && files[4].hunks.is_empty());
    assert_ne!(files[0].id, files[1].id);

    // Past the cap a file is counted, never shown in part.
    let cut = parse_diff(out, 4);
    assert!(!cut[0].cut && cut[0].hunks[0].lines.len() == 4);
    assert!(cut[1].cut && cut[1].hunks.is_empty() && cut[1].added == 1);
    assert!(!cut[1].partial);
}

#[test]
fn unquotes_paths() {
    assert_eq!(unquote("plain name"), "plain name");
    assert_eq!(unquote(r#""tab\there""#), "tab\there");
    assert_eq!(unquote(r#""\303\251t\303\251""#), "été");

    // A binary file names itself only on the `diff --git` line, quoted.
    let out = "diff --git \"a/caf\\303\\251 \\\"x\\\".png\" \"b/caf\\303\\251 \\\"x\\\".png\"\nindex 1..2 100644\n\
               Binary files \"a/caf\\303\\251 \\\"x\\\".png\" and \"b/caf\\303\\251 \\\"x\\\".png\" differ\n";
    let files = parse_diff(out, 100);
    assert_eq!(files[0].path, "café \"x\".png");
    assert!(files[0].binary);
}

#[test]
fn reads_empty_context_lines_without_their_space() {
    // `diff.suppressBlankEmpty` drops the space of an empty context line.
    let out = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,4 +1,4 @@\n a\n\n-b\n+B\n c\n";
    let lines = &parse_diff(out, 100)[0].hunks[0].lines;
    assert_eq!(lines.len(), 5);
    assert_eq!(
        (
            lines[1].kind,
            lines[1].text.as_str(),
            lines[1].old,
            lines[1].new
        ),
        (' ', "", Some(2), Some(2))
    );
    assert_eq!(
        (lines[2].old, lines[3].new, lines[4].new),
        (Some(3), Some(3), Some(4))
    );
}

#[test]
fn builds_patches_of_picked_lines() {
    let out = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n\
               @@ -1,4 +1,5 @@\n a\n-b\n+B\n+c\n d\n e\n\
               @@ -10,3 +11,2 @@\n x\n-y\n z\n";
    let diff = &parse_diff(out, 100)[0];
    // Only the added `c`: `-b` stays as context, `+B` is left out, and the next hunk moves down by one.
    let staged = build_patch(diff, &[pick(0, &[3]), pick(1, &[1])], false).unwrap();
    assert!(
        staged.ends_with("@@ -1,4 +1,5 @@\n a\n b\n+c\n d\n e\n@@ -10,3 +11,2 @@\n x\n-y\n z\n"),
        "{staged}"
    );
    // Reversed, a line left alone is on the side applied to: `+B` stays as context, `-b` is left out.
    let unstaged = build_patch(diff, &[pick(0, &[3])], true).unwrap();
    assert!(
        unstaged.ends_with("@@ -1,4 +1,5 @@\n a\n B\n+c\n d\n e\n"),
        "{unstaged}"
    );
    let shifted = build_patch(diff, &[pick(0, &[3]), pick(1, &[1])], true).unwrap();
    assert!(
        shifted.ends_with("@@ -10,3 +11,2 @@\n x\n-y\n z\n"),
        "{shifted}"
    );
    assert!(
        build_patch(diff, &[pick(0, &[0])], false).is_err(),
        "context only"
    );
    let whole = build_patch(
        diff,
        &[Pick {
            hunk: 1,
            lines: None,
        }],
        false,
    )
    .unwrap();
    assert!(
        whole.ends_with("@@ -10,3 +10,2 @@\n x\n-y\n z\n"),
        "{whole}"
    );
}

#[test]
fn parses_blame_and_history() {
    let a = "a".repeat(40);
    let z = "0".repeat(40);
    let out = format!(
        "{a} 1 1 2\nauthor Alex\nauthor-time 1700000000\nsummary First\nfilename f\n\tone\n\
         {a} 2 2\n\ttwo\n{z} 3 3 1\nauthor Not Committed Yet\nauthor-time 1800000000\nsummary Version of f\n\tthree\n"
    );
    let blame = parse_blame(&out);
    let oids: Vec<&str> = blame.lines.iter().map(|l| l.oid.as_str()).collect();
    assert_eq!(oids, [a.as_str(), a.as_str(), z.as_str()]);
    assert_eq!(blame.lines[1].text, "two");
    assert_eq!(blame.commits[&a].subject, "First");
    assert_eq!(blame.commits[&a].date, 1_700_000_000);
    assert_eq!(blame.commits.len(), 2);

    let out = "\x01c3\0c2\0Alex\x00170\0Edit\0\nM\0new.rs\0\
               \x01c2\0c1\0Alex\x00160\0Move\0\nR100\0old.rs\0new.rs\0\
               \x01c1\0\0Alex\x00150\0Start\0\nA\0old.rs\0";
    let history = parse_history(out, "new.rs");
    let paths: Vec<&str> = history.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["new.rs", "new.rs", "old.rs"]);
    assert_eq!(history[1].commit.subject, "Move");
    assert_eq!(history[2].commit.parents.len(), 0);
}

#[test]
fn finds_conflict_markers() {
    assert_eq!(
        marker_line("a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> x\n"),
        Some(2)
    );
    assert_eq!(marker_line("Title\n=======\n<<<<<<<<< not one\n"), None);
    assert_eq!(marker_line("ok\n>>>>>>>\n"), Some(2));
}

#[test]
fn stages_and_unstages_lines_and_hunks() {
    let sb = Sandbox::new("lines");
    let repo = sb.clone("acme");
    sb.commit(&repo, "f.txt", &twenty(), "Twenty lines");
    let edited = twenty()
        .replace("line 2\n", "LINE 2\n")
        .replace("line 3\n", "line 3\nline 3b\n")
        .replace("line 18\n", "");
    fs::write(repo.join("f.txt"), &edited).unwrap();
    let index = |sb: &Sandbox| sb.git(&repo, &["show", ":f.txt"]) + "\n";

    let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    assert!(diff.partial);
    assert_eq!(diff.hunks.len(), 2);
    let (h, l) = index_of(&diff, '+', "line 3b");
    sb.git
        .stage(&repo, "f.txt", Some(&[pick(h, &[l])]), Some(&diff.id))
        .unwrap();
    assert_eq!(
        index(&sb),
        twenty().replace("line 3\n", "line 3\nline 3b\n")
    );

    // A removed line alone, from the second hunk.
    let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    let (h, l) = index_of(&diff, '-', "line 18");
    sb.git
        .stage(&repo, "f.txt", Some(&[pick(h, &[l])]), Some(&diff.id))
        .unwrap();
    assert_eq!(
        index(&sb),
        twenty()
            .replace("line 3\n", "line 3\nline 3b\n")
            .replace("line 18\n", "")
    );

    // Unstaging a line of the staged diff gives it back to the work tree's diff.
    let staged = sb.git.diff(&repo, "f.txt", None, true).unwrap();
    let (h, l) = index_of(&staged, '+', "line 3b");
    sb.git
        .unstage(
            &repo,
            "f.txt",
            None,
            Some(&[pick(h, &[l])]),
            Some(&staged.id),
        )
        .unwrap();
    assert_eq!(index(&sb), twenty().replace("line 18\n", ""));
    assert_eq!(fs::read_to_string(repo.join("f.txt")).unwrap(), edited);

    // A diff that changed since it was read is refused, never applied to other lines.
    let old = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    fs::write(
        repo.join("f.txt"),
        edited.replace("line 14\n", "line fourteen\n"),
    )
    .unwrap();
    let err = format!(
        "{:#}",
        sb.git
            .stage(&repo, "f.txt", Some(&[pick(0, &[1])]), Some(&old.id))
            .unwrap_err()
    );
    assert!(err.contains("changed since you looked"), "{err}");

    // A whole hunk, then the whole file.
    let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    let whole = Pick {
        hunk: 0,
        lines: None,
    };
    sb.git
        .stage(&repo, "f.txt", Some(&[whole]), Some(&diff.id))
        .unwrap();
    assert!(index(&sb).contains("LINE 2\nline 3\nline 3b\n"));
    assert!(!index(&sb).contains("line fourteen"));
    sb.git.stage(&repo, "f.txt", None, None).unwrap();
    assert!(
        sb.git
            .diff(&repo, "f.txt", None, false)
            .unwrap()
            .hunks
            .is_empty()
    );
    sb.git.unstage(&repo, "f.txt", None, None, None).unwrap();
    assert_eq!(index(&sb), twenty());
}

#[test]
fn stages_up_to_a_missing_final_newline() {
    let sb = Sandbox::new("eol");
    let repo = sb.clone("acme");
    sb.commit(&repo, "f.txt", "a\nb", "No newline");
    fs::write(repo.join("f.txt"), "a\nb\nc").unwrap();
    let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    let all = Pick {
        hunk: 0,
        lines: None,
    };
    sb.git
        .stage(&repo, "f.txt", Some(&[all]), Some(&diff.id))
        .unwrap();
    let index = |sb: &Sandbox| sb.git.run(&repo, &["show", ":f.txt"]).unwrap();
    assert_eq!(index(&sb), "a\nb\nc");

    // Some of the lines after it: `b` gains its newline, nothing sticks to it.
    let one_line = |sb: &Sandbox, kind: char, text: &str, staged: bool| {
        let diff = sb.git.diff(&repo, "f.txt", None, staged).unwrap();
        let (h, l) = index_of(&diff, kind, text);
        let picks = [pick(h, &[l])];
        if staged {
            sb.git
                .unstage(&repo, "f.txt", None, Some(&picks), Some(&diff.id))
        } else {
            sb.git.stage(&repo, "f.txt", Some(&picks), Some(&diff.id))
        }
        .unwrap();
    };
    sb.git.unstage(&repo, "f.txt", None, None, None).unwrap();
    one_line(&sb, '+', "c", false);
    assert_eq!(index(&sb), "a\nb\nc");
    sb.git.unstage(&repo, "f.txt", None, None, None).unwrap();
    one_line(&sb, '+', "b", false);
    assert_eq!(index(&sb), "a\nb\nb\n");

    // Unstaged alone, the removed `b` comes back before the lines kept, with a newline.
    sb.git.stage(&repo, "f.txt", None, None).unwrap();
    one_line(&sb, '-', "b", true);
    assert_eq!(index(&sb), "a\nb\nb\nc");
}

#[test]
fn discards_lines_hunks_files_and_untracked_ones() {
    let sb = Sandbox::new("discard");
    let repo = sb.clone("acme");
    sb.commit(&repo, "f.txt", TEN, "Ten lines");
    fs::write(
        repo.join("f.txt"),
        TEN.replace("two\n", "TWO\n").replace("nine\n", "NINE\n"),
    )
    .unwrap();
    let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    let (h, minus) = index_of(&diff, '-', "two");
    let (_, plus) = index_of(&diff, '+', "TWO");
    sb.git
        .discard(&repo, "f.txt", Some(&[pick(h, &[minus, plus])]), &diff.id)
        .unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("f.txt")).unwrap(),
        TEN.replace("nine\n", "NINE\n")
    );

    // What is staged stays: only the work tree goes back to it.
    sb.git.stage(&repo, "f.txt", None, None).unwrap();
    fs::write(repo.join("f.txt"), "rewritten\n").unwrap();
    let diff = sb.git.diff(&repo, "f.txt", None, false).unwrap();
    assert!(sb.git.discard(&repo, "f.txt", None, "stale").is_err());
    sb.git.discard(&repo, "f.txt", None, &diff.id).unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("f.txt")).unwrap(),
        TEN.replace("nine\n", "NINE\n")
    );

    // An untracked file reads as all new, and an untracked folder lists what it holds.
    fs::write(repo.join("notes.md"), "todo\nlater").unwrap();
    fs::create_dir_all(repo.join("drafts/old")).unwrap();
    fs::write(repo.join("drafts/a.md"), "a").unwrap();
    fs::write(repo.join("drafts/old/b.md"), "b").unwrap();
    let notes = sb.git.diff(&repo, "notes.md", None, false).unwrap();
    assert_eq!(notes.untracked, ["notes.md"]);
    assert_eq!((notes.status, notes.added, notes.partial), ('A', 2, false));
    assert!(notes.hunks[0].lines[1].no_eol);
    let drafts = sb.git.diff(&repo, "drafts/", None, false).unwrap();
    assert_eq!(drafts.untracked, ["drafts/a.md", "drafts/old/b.md"]);
    assert!(drafts.hunks.is_empty());
    assert!(
        sb.git
            .discard(&repo, "drafts/", Some(&[pick(0, &[0])]), &drafts.id)
            .is_err()
    );
    sb.git.discard(&repo, "drafts/", None, &drafts.id).unwrap();
    assert!(!repo.join("drafts").exists());
    sb.git.stage(&repo, "notes.md", None, None).unwrap();
    assert_eq!(sb.git(&repo, &["show", ":notes.md"]), "todo\nlater");
    assert!(sb.git.diff(&repo, "../outside", None, false).is_err());

    // A name git would read as a pattern is only that file.
    fs::write(repo.join("[nm]otes.md"), "glob\n").unwrap();
    let glob = sb.git.diff(&repo, "[nm]otes.md", None, false).unwrap();
    assert_eq!(glob.untracked, ["[nm]otes.md"]);
    sb.git
        .discard(&repo, "[nm]otes.md", None, &glob.id)
        .unwrap();
    assert!(!repo.join("[nm]otes.md").exists() && repo.join("notes.md").exists());
}

#[test]
fn unstages_before_the_first_commit() {
    let sb = Sandbox::new("unborn");
    let repo = sb.path("fresh");
    fs::create_dir_all(&repo).unwrap();
    sb.git(&repo, &["init", "-q"]);
    fs::write(repo.join("a.txt"), "a\n").unwrap();
    sb.git.stage(&repo, "a.txt", None, None).unwrap();
    let staged = sb.git.diff(&repo, "a.txt", None, true).unwrap();
    assert_eq!(staged.status, 'A');
    sb.git.unstage(&repo, "a.txt", None, None, None).unwrap();
    assert_eq!(sb.git(&repo, &["ls-files"]), "");
    sb.git.stage(&repo, "a.txt", None, None).unwrap();
    let oid = sb
        .git
        .commit(&repo, "First\n\nWith a body.", false, None)
        .unwrap();
    assert_eq!(
        sb.git(&repo, &["log", "-1", "--format=%B"]),
        "First\n\nWith a body."
    );
    assert_eq!(sb.git(&repo, &["rev-parse", "HEAD"]), oid);
}

#[test]
fn commits_and_amends_only_what_is_not_pushed() {
    let sb = Sandbox::new("commit");
    let repo = sb.clone("acme");
    let pushed = sb.git(&repo, &["rev-parse", "HEAD"]);
    assert!(sb.git.commit(&repo, "  ", false, None).is_err());
    let err = format!(
        "{:#}",
        sb.git
            .commit(&repo, "Nothing staged", false, None)
            .unwrap_err()
    );
    assert!(err.contains("nothing"), "{err}");

    let err = format!(
        "{:#}",
        sb.git
            .commit(&repo, "Reword", true, Some(&pushed))
            .unwrap_err()
    );
    assert!(err.contains("already on origin/main"), "{err}");

    fs::write(repo.join("api.rs"), "v1\n").unwrap();
    sb.git.stage(&repo, "api.rs", None, None).unwrap();
    let first = sb.git.commit(&repo, "Add the API", false, None).unwrap();
    let detail = sb.git.show(&repo, "HEAD").unwrap();
    assert_eq!(detail.remote, None);
    assert_eq!(detail.message, "Add the API");
    assert!(
        sb.git
            .commit(&repo, "Add the API v1", true, Some(&pushed))
            .is_err(),
        "HEAD moved"
    );
    let amended = sb
        .git
        .commit(
            &repo,
            "Add the API v1\n\nFirst version.",
            true,
            Some(&first),
        )
        .unwrap();
    assert_ne!(amended, first);
    assert_eq!(sb.git(&repo, &["rev-list", "--count", "HEAD"]), "2");
    assert_eq!(
        sb.git(&repo, &["log", "-1", "--format=%s"]),
        "Add the API v1"
    );
}

#[cfg(unix)]
#[test]
fn a_failing_hook_stops_the_commit_with_its_output() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::new("hook");
    let repo = sb.clone("acme");
    let hooks = sb.path("hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'lint: 2 problems in api.rs'\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    sb.git(
        &repo,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    fs::write(repo.join("api.rs"), "v1\n").unwrap();
    sb.git.stage(&repo, "api.rs", None, None).unwrap();
    let err = format!(
        "{:#}",
        sb.git
            .commit(&repo, "Add the API", false, None)
            .unwrap_err()
    );
    assert!(err.contains("lint: 2 problems in api.rs"), "{err}");
    assert_eq!(sb.git(&repo, &["rev-list", "--count", "HEAD"]), "1");
}

#[test]
fn stashes_list_show_apply_pop_and_drop() {
    let sb = Sandbox::new("stash");
    let repo = sb.clone("acme");
    let err = format!("{:#}", sb.git.stash_push(&repo, "", false).unwrap_err());
    assert!(err.contains("No local changes"), "{err}");

    fs::write(repo.join("README.md"), "acme draft\n").unwrap();
    fs::write(repo.join("todo.md"), "later\n").unwrap();
    sb.git.stash_push(&repo, "Draft readme", true).unwrap();
    assert!(!repo.join("todo.md").exists());
    fs::write(repo.join("README.md"), "acme other\n").unwrap();
    sb.git.stash_push(&repo, "", false).unwrap();

    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.stashes.len(), 2);
    let [newest, draft] = [&state.stashes[0], &state.stashes[1]];
    assert_eq!((newest.index, draft.index), (0, 1));
    assert!(draft.message.ends_with("Draft readme"), "{}", draft.message);
    assert!(
        newest.message.starts_with("WIP on main"),
        "{}",
        newest.message
    );

    let files = sb.git.stash_show(&repo, 1, &draft.oid).unwrap();
    let mut paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["README.md", "todo.md"]);
    assert!(
        sb.git.stash_show(&repo, 0, &draft.oid).is_err(),
        "wrong index"
    );

    let applied = sb.git.stash_apply(&repo, 1, &draft.oid, false).unwrap();
    assert!(applied.ok && !applied.pending);
    assert_eq!(fs::read_to_string(repo.join("todo.md")).unwrap(), "later\n");
    assert_eq!(sb.git.stashes(&repo).unwrap().len(), 2, "apply keeps it");

    // Popping over a conflicting change: git's conflict, and the stash kept.
    sb.git.stage(&repo, "README.md", None, None).unwrap();
    sb.git.stage(&repo, "todo.md", None, None).unwrap();
    sb.git.commit(&repo, "Draft", false, None).unwrap();
    let popped = sb.git.stash_apply(&repo, 0, &newest.oid, true).unwrap();
    assert!(!popped.ok && popped.pending, "{popped:?}");
    assert!(popped.output.contains("CONFLICT"), "{}", popped.output);
    assert_eq!(sb.git.stashes(&repo).unwrap().len(), 2);
    // Its conflicts left, nothing else starts: git's refusal would read as stopping on them.
    for err in [
        sb.git.merge(&repo, "main").unwrap_err(),
        sb.git.stash_apply(&repo, 1, &draft.oid, false).unwrap_err(),
    ] {
        assert!(format!("{err:#}").contains("conflicts are left"), "{err:#}");
    }
    sb.git
        .resolve(&repo, "README.md", Resolution::Theirs)
        .unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        "acme other\n"
    );

    let left = sb.git.stashes(&repo).unwrap();
    assert!(sb.git.stash_drop(&repo, 0, &left[1].oid).is_err());
    sb.git.stash_drop(&repo, 0, &left[0].oid).unwrap();
    assert_eq!(sb.git.stashes(&repo).unwrap().len(), 1);
}

/// `main` and `feature` both change the README; the clone sits on main.
fn diverged(sb: &Sandbox) -> std::path::PathBuf {
    let repo = sb.clone("acme");
    sb.git(&repo, &["switch", "-q", "-c", "feature"]);
    sb.commit(&repo, "README.md", "feature\n", "Feature readme");
    sb.commit(&repo, "feature.rs", "f\n", "Feature code");
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.commit(&repo, "README.md", "main\n", "Main readme");
    repo
}

#[test]
fn merge_conflicts_are_resolved_then_continued_or_aborted() {
    let sb = Sandbox::new("merge");
    let repo = diverged(&sb);
    let feature = sb.git(&repo, &["rev-parse", "feature"]);

    let out = sb.git.merge(&repo, "feature").unwrap();
    assert!(!out.ok && out.pending);
    assert!(out.output.contains("CONFLICT"), "{}", out.output);
    let op = sb.git.state(&repo).unwrap().worktrees[0]
        .operation
        .clone()
        .unwrap();
    assert_eq!(op.kind, OperationKind::Merge);
    assert_eq!(op.theirs.as_deref(), Some(feature.as_str()));
    let err = format!("{:#}", sb.git.merge(&repo, "feature").unwrap_err());
    assert!(err.contains("merge is in progress"), "{err}");
    assert!(sb.git.proceed(&repo, Step::Skip).is_err());

    // Continuing with a conflict left: git says so, and the merge waits.
    let early = sb.git.proceed(&repo, Step::Continue).unwrap();
    assert!(!early.ok && early.pending, "{early:?}");

    let err = format!(
        "{:#}",
        sb.git
            .resolve(&repo, "README.md", Resolution::Edited)
            .unwrap_err()
    );
    assert!(err.contains("conflict markers (line 1)"), "{err}");
    fs::write(repo.join("README.md"), "main and feature\n").unwrap();
    sb.git
        .resolve(&repo, "README.md", Resolution::Edited)
        .unwrap();
    assert!(
        sb.git
            .resolve(&repo, "README.md", Resolution::Ours)
            .is_err()
    );
    let done = sb.git.proceed(&repo, Step::Continue).unwrap();
    assert!(done.ok && !done.pending, "{done:?}");
    assert_eq!(
        sb.git(&repo, &["log", "-1", "--format=%P"])
            .split(' ')
            .count(),
        2
    );
    assert_eq!(sb.git.operation(&repo).unwrap(), None);

    // Aborted: the branch is back as it was.
    sb.git(&repo, &["reset", "-q", "--hard", "HEAD^"]);
    let before = sb.git(&repo, &["rev-parse", "HEAD"]);
    assert!(sb.git.merge(&repo, "feature").unwrap().pending);
    sb.git
        .resolve(&repo, "README.md", Resolution::Ours)
        .unwrap();
    let aborted = sb.git.proceed(&repo, Step::Abort).unwrap();
    assert!(aborted.ok && !aborted.pending);
    assert_eq!(sb.git(&repo, &["rev-parse", "HEAD"]), before);
    assert!(sb.git.proceed(&repo, Step::Abort).is_err(), "nothing left");

    // A merge refused before it starts is git's error.
    fs::write(repo.join("README.md"), "unsaved\n").unwrap();
    let err = format!("{:#}", sb.git.merge(&repo, "feature").unwrap_err());
    assert!(err.contains("git merge failed"), "{err}");
}

#[test]
fn rebase_conflicts_show_their_step_and_can_be_skipped() {
    let sb = Sandbox::new("rebase");
    let repo = diverged(&sb);
    sb.git(&repo, &["switch", "-q", "feature"]);
    let head = sb.git(&repo, &["rev-parse", "HEAD"]);
    let main = sb.git(&repo, &["rev-parse", "main"]);

    let plan = sb.git.rebase_plan(&repo, "main").unwrap();
    assert_eq!((plan.total, plan.pushed), (2, 0));
    assert_eq!(plan.commits[0].subject, "Feature code");
    assert!(sb.git.rebase(&repo, "main", &main).is_err(), "HEAD moved");

    let out = sb.git.rebase(&repo, "main", &head).unwrap();
    assert!(!out.ok && out.pending, "{out:?}");
    let op = sb.git.operation(&repo).unwrap().unwrap();
    assert_eq!(op.kind, OperationKind::Rebase);
    assert_eq!(op.branch.as_deref(), Some("feature"));
    assert_eq!(op.onto.as_deref(), Some(main.as_str()));
    assert_eq!((op.step, op.total), (1, 2));
    assert!(op.theirs.is_some());
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.current, None, "HEAD is detached during a rebase");
    assert!(state.worktrees[0].changes.iter().any(|c| c.conflict));

    // In a rebase, ours is the branch rebased onto.
    sb.git
        .resolve(&repo, "README.md", Resolution::Ours)
        .unwrap();
    let skipped = sb.git.proceed(&repo, Step::Skip).unwrap();
    assert!(skipped.ok && !skipped.pending, "{skipped:?}");
    assert_eq!(
        sb.git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature"
    );
    assert_eq!(
        sb.git(&repo, &["log", "-2", "--format=%s"]),
        "Feature code\nMain readme"
    );

    // Pushed commits are counted: rewriting them needs a force push.
    sb.git(&repo, &["push", "-q", "-u", "origin", "feature"]);
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.commit(&repo, "main.rs", "m\n", "More on main");
    sb.git(&repo, &["switch", "-q", "feature"]);
    sb.commit(&repo, "local.rs", "l\n", "Local only");
    let plan = sb.git.rebase_plan(&repo, "main").unwrap();
    assert_eq!((plan.total, plan.pushed), (2, 1));
}

#[test]
fn rebase_continues_after_an_edit_and_aborts() {
    let sb = Sandbox::new("rebase-continue");
    let repo = diverged(&sb);
    sb.git(&repo, &["switch", "-q", "feature"]);
    let head = sb.git(&repo, &["rev-parse", "HEAD"]);
    assert!(sb.git.rebase(&repo, "main", &head).unwrap().pending);
    fs::write(repo.join("README.md"), "main + feature\n").unwrap();
    sb.git
        .resolve(&repo, "README.md", Resolution::Edited)
        .unwrap();
    let out = sb.git.proceed(&repo, Step::Continue).unwrap();
    assert!(out.ok && !out.pending, "{out:?}");
    assert_eq!(
        sb.git(&repo, &["log", "-3", "--format=%s"]),
        "Feature code\nFeature readme\nMain readme"
    );

    sb.git(&repo, &["reset", "-q", "--hard", &head]);
    assert!(sb.git.rebase(&repo, "main", &head).unwrap().pending);
    let out = sb.git.proceed(&repo, Step::Abort).unwrap();
    assert!(out.ok && !out.pending);
    assert_eq!(sb.git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        sb.git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature"
    );
}

#[test]
fn cherry_picks_and_reverts() {
    let sb = Sandbox::new("pick");
    let repo = diverged(&sb);
    let code = sb.git(&repo, &["rev-parse", "feature"]);
    let readme = sb.git(&repo, &["rev-parse", "feature^"]);

    let out = sb.git.cherry_pick(&repo, &code).unwrap();
    assert!(out.ok && !out.pending, "{out:?}");
    assert_eq!(sb.git(&repo, &["log", "-1", "--format=%s"]), "Feature code");

    let out = sb.git.cherry_pick(&repo, &readme).unwrap();
    assert!(!out.ok && out.pending);
    let op = sb.git.operation(&repo).unwrap().unwrap();
    assert_eq!(op.kind, OperationKind::CherryPick);
    assert_eq!(op.theirs.as_deref(), Some(readme.as_str()));
    sb.git
        .resolve(&repo, "README.md", Resolution::Theirs)
        .unwrap();
    let out = sb.git.proceed(&repo, Step::Continue).unwrap();
    assert!(out.ok && !out.pending, "{out:?}");
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        "feature\n"
    );

    let picked = sb.git(&repo, &["rev-parse", "HEAD"]);
    let out = sb.git.revert(&repo, &picked).unwrap();
    assert!(out.ok && !out.pending, "{out:?}");
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).unwrap(),
        "main\n"
    );
    assert!(
        sb.git(&repo, &["log", "-1", "--format=%s"])
            .starts_with("Revert")
    );
    assert!(sb.git.revert(&repo, "--hard").is_err());
}

#[test]
fn keeps_a_side_that_deleted_the_file() {
    let sb = Sandbox::new("deleted");
    let repo = sb.clone("acme");
    sb.git(&repo, &["switch", "-q", "-c", "gone"]);
    sb.git(&repo, &["rm", "-q", "README.md"]);
    sb.git(&repo, &["commit", "-q", "-m", "Remove the readme"]);
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.commit(&repo, "README.md", "kept\n", "Edit the readme");
    assert!(sb.git.merge(&repo, "gone").unwrap().pending);
    sb.git
        .resolve(&repo, "README.md", Resolution::Theirs)
        .unwrap();
    assert!(!repo.join("README.md").exists());
    assert!(sb.git.proceed(&repo, Step::Continue).unwrap().ok);
}

#[test]
fn tags_are_listed_created_pushed_and_deleted() {
    let sb = Sandbox::new("tags");
    let repo = sb.clone("acme");
    let base = sb.git(&repo, &["rev-parse", "HEAD"]);
    sb.git
        .tag_create(&repo, "v1.0.0", &base, Some("Release 1.0.0\n\nNotes"))
        .unwrap();
    let side = sb.commit(&repo, "x.rs", "x\n", "Experiment");
    sb.git(&repo, &["reset", "-q", "--hard", &base]);
    sb.git.tag_create(&repo, "try", &side, None).unwrap();
    assert!(sb.git.tag_create(&repo, "bad..name", &base, None).is_err());
    assert!(sb.git.tag_create(&repo, "v1.0.0", &base, None).is_err());

    let tags = sb.git.tags(&repo).unwrap();
    let v1 = tags.iter().find(|t| t.name == "v1.0.0").unwrap();
    assert!(v1.annotated);
    assert_eq!(
        (v1.target.as_str(), v1.message.as_str()),
        (base.as_str(), "Release 1.0.0")
    );
    assert_eq!(v1.tagger, "Alex");
    assert_ne!(v1.oid, v1.target);
    let try_tag = tags.iter().find(|t| t.name == "try").unwrap();
    assert!(!try_tag.annotated && try_tag.message.is_empty());
    assert_eq!(try_tag.oid, side);

    // Only the tag holds the experiment: deleting it loses that commit.
    assert_eq!(sb.git.tag_unique(&repo, "try").unwrap().total, 1);
    assert_eq!(sb.git.tag_unique(&repo, "v1.0.0").unwrap().total, 0);
    assert!(sb.git.tag_delete(&repo, "try", &base).is_err(), "stale");
    sb.git.tag_delete(&repo, "try", &side).unwrap();

    assert_eq!(sb.git.tag_push(&repo, "v1.0.0").unwrap(), "origin");
    assert!(
        sb.git(&repo, &["ls-remote", "origin", "refs/tags/v1.0.0"])
            .starts_with(&v1.oid)
    );
    assert!(sb.git.tag_push(&repo, "nope").is_err());
}

#[test]
fn file_history_follows_renames_and_blame_names_each_line() {
    let sb = Sandbox::new("history");
    let repo = sb.clone("acme");
    sb.commit(&repo, "old.rs", "fn a() {}\n", "Start");
    sb.git(&repo, &["mv", "old.rs", "new.rs"]);
    sb.git(&repo, &["commit", "-q", "-m", "Move"]);
    let edit = sb.commit(&repo, "new.rs", "fn a() {}\nfn b() {}\n", "Add b");
    fs::write(repo.join("new.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();

    let history = sb.git.history(&repo, "new.rs", 10).unwrap();
    let seen: Vec<(&str, &str)> = history
        .commits
        .iter()
        .map(|c| (c.commit.subject.as_str(), c.path.as_str()))
        .collect();
    assert_eq!(
        seen,
        [("Add b", "new.rs"), ("Move", "new.rs"), ("Start", "old.rs")]
    );
    assert!(!history.more);
    let short = sb.git.history(&repo, "new.rs", 2).unwrap();
    assert!(short.more && short.commits.len() == 2);

    let blame = sb.git.blame(&repo, "new.rs").unwrap();
    assert_eq!(blame.lines.len(), 3);
    assert_eq!(blame.lines[1].oid, edit);
    assert_eq!(blame.commits[&edit].subject, "Add b");
    assert!(
        blame.lines[2].oid.bytes().all(|b| b == b'0'),
        "not committed"
    );
    assert!(sb.git.blame(&repo, "missing.rs").is_err());
}

#[test]
fn shows_a_commit_merges_and_the_first_one() {
    let sb = Sandbox::new("show");
    let repo = diverged(&sb);
    sb.git(
        &repo,
        &["merge", "-q", "-X", "ours", "--no-edit", "feature"],
    );
    let merge = sb.git.show(&repo, "HEAD").unwrap();
    assert_eq!(merge.commit.parents.len(), 2);
    let paths: Vec<&str> = merge.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["feature.rs"], "against the first parent");
    let first = sb.git(&repo, &["rev-list", "--max-parents=0", "HEAD"]);
    let root = sb.git.show(&repo, &first).unwrap();
    assert_eq!(root.commit.subject, "Initial commit");
    assert_eq!(root.files[0].status, 'A');
    assert_eq!(root.remote.as_deref(), Some("origin/main"));
    assert_eq!(root.email, "alex@example.com");
    assert!(sb.git.show(&repo, "-p").is_err());
}

#[test]
fn reset_plans_say_what_is_lost() {
    let sb = Sandbox::new("reset");
    let repo = sb.clone("acme");
    let base = sb.git(&repo, &["rev-parse", "HEAD"]);
    sb.commit(&repo, "a.rs", "a\n", "A");
    let b = sb.commit(&repo, "b.rs", "b\n", "B");
    sb.git(&repo, &["branch", "keep", &b]);
    let c = sb.commit(&repo, "c.rs", "c\n", "C");
    fs::write(repo.join("a.rs"), "changed\n").unwrap();
    fs::write(repo.join("scratch.txt"), "").unwrap();

    let plan = sb.git.reset_plan(&repo, &base).unwrap();
    assert_eq!(plan.branch, "main");
    assert_eq!(plan.leaving, 3);
    assert_eq!(plan.lost.total, 1, "A and B stay on keep");
    assert_eq!(plan.lost.commits[0].oid, c);
    let paths: Vec<&str> = plan.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["a.rs"], "untracked files are not lost");

    assert!(
        sb.git.reset(&repo, &base, ResetMode::Soft, &base).is_err(),
        "HEAD moved"
    );
    sb.git.reset(&repo, &b, ResetMode::Soft, &c).unwrap();
    assert_eq!(sb.git(&repo, &["diff", "--cached", "--name-only"]), "c.rs");
    sb.git.reset(&repo, &b, ResetMode::Mixed, &b).unwrap();
    assert_eq!(sb.git(&repo, &["diff", "--cached", "--name-only"]), "");
    assert!(repo.join("c.rs").exists());
    sb.git.reset(&repo, &base, ResetMode::Hard, &b).unwrap();
    assert!(!repo.join("a.rs").exists() && repo.join("scratch.txt").exists());
    sb.git(&repo, &["switch", "-q", "--detach"]);
    assert!(sb.git.reset_plan(&repo, &b).is_err());
}

#[test]
fn force_pushes_with_a_lease_never_on_the_default_branch() {
    let sb = Sandbox::new("force");
    let repo = sb.clone("acme");
    let other = sb.clone("other");
    sb.git(&repo, &["switch", "-q", "-c", "feature"]);
    sb.commit(&repo, "f.rs", "1\n", "First");
    sb.git(&repo, &["push", "-q", "-u", "origin", "feature"]);
    let err = format!("{:#}", sb.git.force_plan(&repo, "main").unwrap_err());
    assert!(err.contains("default branch"), "{err}");

    // Rewritten here: the plan names what the push drops, and the push goes through.
    sb.git(&repo, &["commit", "-q", "--amend", "-m", "First, reworded"]);
    let plan = sb.git.force_plan(&repo, "feature").unwrap();
    assert_eq!(plan.remote, "origin");
    assert_eq!(plan.lost.total, 1);
    assert_eq!(plan.lost.commits[0].subject, "First");
    assert!(
        sb.git.force_push(&repo, "feature", "0000").is_err(),
        "stale"
    );
    sb.git.force_push(&repo, "feature", &plan.theirs).unwrap();
    let tip = sb.git(&repo, &["rev-parse", "feature"]);
    assert!(
        sb.git(&repo, &["ls-remote", "origin", "refs/heads/feature"])
            .starts_with(&tip)
    );

    // Someone pushed since our last fetch: the lease refuses, nothing is lost.
    sb.git(&other, &["fetch", "-q", "origin"]);
    sb.git(&other, &["switch", "-q", "feature"]);
    let theirs = sb.commit(&other, "g.rs", "2\n", "Theirs");
    sb.git(&other, &["push", "-q", "origin", "feature"]);
    sb.git(&repo, &["commit", "-q", "--amend", "-m", "First, again"]);
    let plan = sb.git.force_plan(&repo, "feature").unwrap();
    let err = format!(
        "{:#}",
        sb.git
            .force_push(&repo, "feature", &plan.theirs)
            .unwrap_err()
    );
    assert!(
        err.contains("stale info") || err.contains("rejected"),
        "{err}"
    );
    let remote = sb.git(&repo, &["ls-remote", "origin", "refs/heads/feature"]);
    assert!(remote.starts_with(&theirs));

    // Fetched but never integrated: refused too (what --force-if-includes does).
    assert!(try_git(&sb, &repo, &["fetch", "-q", "origin"]));
    let plan = sb.git.force_plan(&repo, "feature").unwrap();
    assert_eq!(plan.theirs, theirs);
    let err = format!(
        "{:#}",
        sb.git
            .force_push(&repo, "feature", &plan.theirs)
            .unwrap_err()
    );
    assert!(err.contains("never integrated"), "{err}");
    assert!(
        sb.git(&repo, &["ls-remote", "origin", "refs/heads/feature"])
            .starts_with(&theirs)
    );

    // Integrated then rewritten away (a rebase): still in the reflog, so the push goes through.
    sb.git(&repo, &["reset", "-q", "--hard", &theirs]);
    sb.git(
        &repo,
        &["commit", "-q", "--amend", "-m", "Theirs, reworded"],
    );
    sb.git.force_push(&repo, "feature", &theirs).unwrap();
}

#[test]
fn diff_since_a_base_counts_commits_and_the_work_tree_not_the_base_moving_on() {
    let sb = Sandbox::new("since");
    let repo = sb.clone("acme");
    sb.git(&repo, &["switch", "-q", "-c", "termf/1-export"]);
    sb.commit(&repo, "export.py", "def export():\n    pass\n", "Export");
    fs::write(repo.join("README.md"), "acme\nexport\n").unwrap();
    // The base moves on after the run left it: nothing of it shows.
    sb.git(&repo, &["stash", "-q"]);
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.commit(&repo, "later.md", "later\n", "Later on main");
    sb.git(&repo, &["switch", "-q", "termf/1-export"]);
    sb.git(&repo, &["stash", "pop", "-q"]);

    let files = sb.git.diff_since(&repo, "main").unwrap();
    let got: Vec<(&str, u32, u32)> = files
        .iter()
        .map(|f| (f.path.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(got, [("README.md", 1, 0), ("export.py", 2, 0)]);
    assert!(sb.git.diff_since(&repo, "-x").is_err(), "an option is no base");
}
