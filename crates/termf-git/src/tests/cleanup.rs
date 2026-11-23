use std::fs;

use super::Sandbox;
use crate::*;

const DAY: i64 = 86_400;

/// A clone with every case: merged, squash-merged, gone, unpushed, old branches; dirty, orphaned, run, detached and
/// locked worktrees.
fn messy(sb: &Sandbox) -> std::path::PathBuf {
    let acme = sb.clone("acme");
    let wt = |name: &str| sb.path(name).to_string_lossy().into_owned();

    sb.git(&acme, &["switch", "-q", "-c", "merged"]);
    sb.commit(&acme, "merged.txt", "1\n", "Merged work");
    sb.git(&acme, &["switch", "-q", "main"]);
    sb.git(&acme, &["merge", "-q", "--ff-only", "merged"]);
    sb.git(&acme, &["push", "-q", "origin", "main"]);

    sb.git(&acme, &["switch", "-q", "-c", "squashed"]);
    sb.commit(&acme, "squashed.txt", "1\n", "Squashed on GitHub");

    sb.git(&acme, &["switch", "-q", "-c", "gone", "main"]);
    sb.commit(&acme, "gone.txt", "1\n", "Pushed then deleted there");
    sb.git(&acme, &["push", "-q", "-u", "origin", "gone"]);
    sb.git(&acme, &["push", "-q", "origin", "--delete", "gone"]);

    sb.git(&acme, &["switch", "-q", "-c", "unpushed", "main"]);
    sb.commit(&acme, "a.txt", "1\n", "First");
    sb.commit(&acme, "b.txt", "1\n", "Second");

    sb.git(&acme, &["switch", "-q", "-c", "old", "main"]);
    fs::write(acme.join("old.txt"), "1\n").unwrap();
    sb.git(&acme, &["add", "old.txt"]);
    sb.git(
        &acme,
        &["commit", "-q", "-m", "Old", "--date=2020-01-01T00:00:00Z"],
    );
    sb.git(&acme, &["switch", "-q", "main"]);

    sb.git(&acme, &["worktree", "add", "-q", "-b", "wip", &wt("dirty")]);
    fs::write(sb.path("dirty").join("notes.md"), "draft\n").unwrap();
    sb.git(
        &acme,
        &["worktree", "add", "-q", "-b", "orphan", &wt("orphaned")],
    );
    fs::remove_dir_all(sb.path("orphaned")).unwrap();
    sb.git(
        &acme,
        &["worktree", "add", "-q", "-b", "termf/run-7", &wt("run-7")],
    );
    sb.commit(&sb.path("run-7"), "run.txt", "1\n", "Run 7");
    sb.git(
        &acme,
        &["worktree", "add", "-q", "-b", "termf/run-8", &wt("run-8")],
    );
    sb.git(
        &acme,
        &["worktree", "add", "-q", "--detach", &wt("detached")],
    );
    sb.git(
        &acme,
        &["worktree", "add", "-q", "-b", "kept", &wt("locked")],
    );
    sb.git(&acme, &["worktree", "lock", &wt("locked")]);
    acme
}

fn runs(sb: &Sandbox) -> Vec<RunRef> {
    let run = |id, name: &str, done| RunRef {
        id,
        worktree: Some(sb.path(name).to_string_lossy().into_owned()),
        branch: Some(format!("termf/{name}")),
        open: true,
        done,
    };
    vec![run(7, "run-7", true), run(8, "run-8", false)]
}

#[test]
fn sorts_branches_and_worktrees_and_says_what_each_loses() {
    let sb = Sandbox::new("cleanup");
    let acme = messy(&sb);
    let now = now();
    // GitHub knows a pull request by the commit it merged.
    let tip = |rev: &str| sb.git(&acme, &["rev-parse", rev]);
    let merged = [tip("squashed"), tip("termf/run-7")];
    let c = sb.git.cleanup(&acme, &runs(&sb), &merged, now).unwrap();
    assert_eq!(c.default_branch.as_deref(), Some("origin/main"));

    let branch = |name: &str| {
        c.branches
            .iter()
            .find(|b| b.name == name)
            .unwrap_or_else(|| panic!("no {name} in {:?}", c.branches))
    };
    // The current and default branch, and those checked out in a worktree, are not listed as branches.
    let names: Vec<&str> = c.branches.iter().map(|b| b.name.as_str()).collect();
    assert!(
        !names.contains(&"main") && !names.contains(&"wip") && !names.contains(&"termf/run-7"),
        "{names:?}"
    );

    let b = branch("merged");
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[Reason::Merged][..], 0, true)
    );
    let b = branch("squashed");
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[Reason::PrMerged][..], 1, false)
    );
    let b = branch("gone");
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[Reason::Gone][..], 1, false)
    );
    let b = branch("unpushed");
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[][..], 2, false)
    );
    let b = branch("old");
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[Reason::Stale][..], 1, false)
    );

    let worktree = |name: &str| {
        c.worktrees
            .iter()
            .find(|w| w.path.ends_with(&format!("/{name}")))
            .unwrap_or_else(|| panic!("no {name} in {:?}", c.worktrees))
    };
    assert_eq!(
        c.worktrees.len(),
        6,
        "the repository's folder is not one: {:?}",
        c.worktrees
    );

    let w = worktree("dirty");
    assert_eq!(
        (w.branch.as_deref(), w.changes, w.lost, w.suggested),
        (Some("wip"), 1, 0, false)
    );
    assert_eq!(w.reasons, [Reason::Merged]);

    let w = worktree("orphaned");
    assert_eq!(w.reasons, [Reason::Merged, Reason::Prunable]);
    assert_eq!(
        (w.branch.as_deref(), w.changes, w.suggested),
        (Some("orphan"), 0, true)
    );

    let w = worktree("run-7");
    assert_eq!(
        (w.run, w.close_run, w.lost, w.suggested),
        (Some(7), true, 1, false)
    );
    assert_eq!(w.reasons, [Reason::PrMerged, Reason::Run]);
    assert_eq!(w.branch.as_deref(), Some("termf/run-7"));
    assert_eq!(w.checked_out.as_deref(), Some("termf/run-7"));

    let w = worktree("run-8");
    assert_eq!(
        (w.blocked, w.suggested, w.close_run),
        (Some(Blocked::RunOpen), false, false)
    );

    let w = worktree("detached");
    assert_eq!(
        (w.branch.as_deref(), w.reasons.as_slice()),
        (None, &[Reason::Detached][..])
    );
    assert!(w.head.is_some() && !w.suggested);
    assert_eq!((w.lost, w.checked_out.as_deref()), (0, None));

    assert_eq!(worktree("locked").blocked, Some(Blocked::Locked));
}

#[test]
fn keeps_a_worktree_branch_whose_commits_would_be_lost() {
    let sb = Sandbox::new("cleanup-keep");
    let acme = sb.clone("acme");
    let path = sb.path("feature").to_string_lossy().into_owned();
    sb.git(&acme, &["worktree", "add", "-q", "-b", "feature", &path]);
    sb.commit(&sb.path("feature"), "f.txt", "1\n", "Not pushed");
    let c = sb.git.cleanup(&acme, &[], &[], now()).unwrap();
    let w = &c.worktrees[0];
    // Removing the folder loses nothing; the branch stays with its commit, and an undo puts the folder back on it.
    assert_eq!((w.branch.as_deref(), w.lost, w.changes), (None, 0, 0));
    assert_eq!(w.checked_out.as_deref(), Some("feature"));
}

#[test]
fn a_merged_pull_request_vouches_only_for_the_commit_it_merged() {
    let sb = Sandbox::new("cleanup-pr");
    let acme = sb.clone("acme");
    sb.git(&acme, &["switch", "-q", "-c", "feature"]);
    let merged = sb.commit(&acme, "f.txt", "1\n", "Squashed on GitHub");
    sb.git(&acme, &["switch", "-q", "main"]);
    let at = |acme: &std::path::Path| {
        sb.git
            .cleanup(acme, &[], std::slice::from_ref(&merged), now())
            .unwrap()
            .branches[0]
            .clone()
    };
    let b = at(&acme);
    // Merged by its pull request, but its commit is still only here: listed, never picked for the user.
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[Reason::PrMerged][..], 1, false)
    );

    // A commit made after the merge is not in the pull request: the branch no longer counts as merged.
    sb.git(&acme, &["switch", "-q", "feature"]);
    sb.commit(&acme, "g.txt", "1\n", "After the merge");
    sb.git(&acme, &["switch", "-q", "main"]);
    let b = at(&acme);
    assert_eq!(
        (b.reasons.as_slice(), b.lost, b.suggested),
        (&[][..], 2, false)
    );

    // Its worktree keeps the branch: the commit made since would be lost with it.
    let path = sb.path("wt").to_string_lossy().into_owned();
    sb.git(&acme, &["worktree", "add", "-q", &path, "feature"]);
    let c = sb
        .git
        .cleanup(&acme, &[], std::slice::from_ref(&merged), now())
        .unwrap();
    let w = &c.worktrees[0];
    assert_eq!((w.branch.as_deref(), w.lost, w.suggested), (None, 0, false));
}

#[test]
fn a_detached_worktree_loses_the_commits_only_its_head_holds() {
    let sb = Sandbox::new("cleanup-detached");
    let acme = sb.clone("acme");
    let path = sb.path("spike").to_string_lossy().into_owned();
    sb.git(&acme, &["worktree", "add", "-q", "--detach", &path]);
    sb.commit(&sb.path("spike"), "s.txt", "1\n", "On no branch");
    let c = sb.git.cleanup(&acme, &[], &[], now()).unwrap();
    let w = &c.worktrees[0];
    assert_eq!(
        (w.reasons.as_slice(), w.lost, w.suggested),
        (&[Reason::Detached][..], 1, false)
    );
}

#[test]
fn an_old_branch_is_stale() {
    let sb = Sandbox::new("cleanup-stale");
    let acme = sb.clone("acme");
    sb.git(&acme, &["branch", "side"]);
    let tip: i64 = sb
        .git(&acme, &["log", "-1", "--format=%at"])
        .parse()
        .unwrap();
    let at = |days: i64| {
        sb.git
            .cleanup(&acme, &[], &[], tip + days * DAY)
            .unwrap()
            .branches[0]
            .reasons
            .clone()
    };
    assert_eq!(at(STALE_DAYS - 1), [Reason::Merged]);
    assert_eq!(at(STALE_DAYS + 1), [Reason::Merged, Reason::Stale]);
}

#[test]
fn removes_a_worktree_only_with_the_changes_seen_then_puts_it_back() {
    let sb = Sandbox::new("cleanup-remove");
    let acme = messy(&sb);
    let dirty = sb.path("dirty").to_string_lossy().into_owned();
    let err = sb
        .git
        .remove_worktree(&acme, &dirty, 0)
        .unwrap_err()
        .to_string();
    assert!(err.contains("changed since you looked"), "{err}");
    assert!(sb.path("dirty").exists());
    sb.git.remove_worktree(&acme, &dirty, 1).unwrap();
    assert!(!sb.path("dirty").exists());
    // The branch stays: deleting it is a separate, checked step.
    sb.git(&acme, &["rev-parse", "--verify", "refs/heads/wip"]);

    let orphaned = sb.path("orphaned").to_string_lossy().into_owned();
    sb.git.remove_worktree(&acme, &orphaned, 0).unwrap();
    let main = acme.to_string_lossy().into_owned();
    assert!(sb.git.remove_worktree(&acme, &main, 0).is_err());
    let locked = sb.path("locked").to_string_lossy().into_owned();
    assert!(sb.git.remove_worktree(&acme, &locked, 0).is_err());

    let list = sb.git(&acme, &["worktree", "list", "--porcelain"]);
    assert!(
        !list.contains("/dirty\n") && !list.contains("/orphaned\n"),
        "{list}"
    );

    sb.git
        .restore_worktree(&acme, &dirty, Some("wip"), "")
        .unwrap();
    assert_eq!(
        sb.git(&sb.path("dirty"), &["branch", "--show-current"]),
        "wip"
    );
    assert!(
        sb.git
            .restore_worktree(&acme, &dirty, Some("wip"), "")
            .is_err()
    );
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
