use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

mod work;

/// A throwaway HOME and git config, a bare remote and a clone of it: never the developer's config or repositories.
struct Sandbox {
    root: PathBuf,
    git: Git,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("termf-git-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&bin).unwrap();
        let config = home.join("gitconfig");
        fs::write(
            &config,
            "[user]\n\tname = Alex\n\temail = alex@example.com\n[commit]\n\tgpgsign = false\n\
             [tag]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n[core]\n\thooksPath = /dev/null\n\tautocrlf = false\n",
        )
        .unwrap();
        let path = std::env::join_paths(std::iter::once(bin.clone()).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .unwrap();
        let env = vec![
            ("HOME".into(), home.to_string_lossy().into_owned()),
            (
                "GIT_CONFIG_GLOBAL".into(),
                config.to_string_lossy().into_owned(),
            ),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("PATH".into(), path.to_string_lossy().into_owned()),
        ];
        Self {
            root,
            git: Git {
                gh: bin.join("gh"),
                env,
            },
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .envs(self.git.env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn commit(&self, dir: &Path, file: &str, text: &str, message: &str) -> String {
        fs::write(dir.join(file), text).unwrap();
        self.git(dir, &["add", file]);
        self.git(dir, &["commit", "-q", "-m", message]);
        self.git(dir, &["rev-parse", "HEAD"])
    }

    /// `remote.git`, seeded with one commit on main, and `name` cloned from it (so `origin/HEAD` is set).
    fn clone(&self, name: &str) -> PathBuf {
        let remote = self.path("remote.git");
        if !remote.exists() {
            let seed = self.path("seed");
            fs::create_dir_all(&seed).unwrap();
            self.git(&seed, &["init", "-q"]);
            self.commit(&seed, "README.md", "acme\n", "Initial commit");
            self.git(&self.root, &["init", "-q", "--bare", "remote.git"]);
            self.git(&seed, &["push", "-q", remote.to_str().unwrap(), "main"]);
        }
        self.git(&self.root, &["clone", "-q", "remote.git", name]);
        self.path(name)
    }

    #[cfg(unix)]
    fn fake_gh(&self, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        let gh = &self.git.gh;
        fs::write(gh, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(gh, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn branch<'a>(state: &'a State, name: &str) -> &'a Branch {
    state
        .branches
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no branch {name} in {:?}", state.branches))
}

#[test]
fn parses_refs_with_any_subject() {
    let out = "refs/heads/main\0aaa\0\0*\0\0refs/remotes/origin/main\0origin\0refs/heads/main\0/w/acme\0Alex\x001700000000\0Fix: a, b\0\n\
               refs/tags/v1\0ttt\0bbb\0 \0\0\0\0\0\0\0\0Release 1\0\n";
    let refs = parse_refs(out);
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].name, "refs/heads/main");
    assert!(refs[0].head);
    assert_eq!(refs[0].upstream_remote, "origin");
    assert_eq!(refs[0].date, 1_700_000_000);
    assert_eq!(refs[0].subject, "Fix: a, b");
    assert_eq!(refs[1].name, "refs/tags/v1");
    assert_eq!(refs[1].peeled, "bbb");
    assert!(!refs[1].head);
}

#[test]
fn parses_log_records() {
    let out = "c3\0c2 f1\0Alex\x00170\0Merge branch 'feature'\0c2\0c1\0Sam\x00160\0Subject with\ttab\0c1\0\0Alex\x00150\0Initial\0";
    let commits = parse_log(out);
    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].parents, ["c2", "f1"]);
    assert_eq!(commits[1].subject, "Subject with\ttab");
    assert!(commits[2].parents.is_empty());
    assert_eq!(commits[2].date, 150);
}

#[test]
fn parses_worktree_list() {
    let out = "worktree /w/acme\nHEAD aaa\nbranch refs/heads/main\n\n\
               worktree /w/acme/.claude/worktrees/termf-12-ACME-7-export\nHEAD bbb\nbranch refs/heads/termf/ACME-7-export\nlocked\n\n\
               worktree /w/gone\nHEAD ccc\ndetached\nprunable gitdir file points to non-existent location\n\n";
    let list = parse_worktrees(out);
    assert_eq!(list.len(), 3);
    assert!(list[0].main && !list[1].main);
    assert_eq!(list[0].branch.as_deref(), Some("main"));
    assert_eq!(list[1].branch.as_deref(), Some("termf/ACME-7-export"));
    assert!(list[1].locked);
    assert_eq!(list[2].branch, None);
    assert_eq!(list[2].head.as_deref(), Some("ccc"));
    assert!(list[2].prunable);
}

#[test]
fn parses_status_v2() {
    let out = "1 .M N... 100644 100644 100644 a a src/main.rs\0\
               1 A. N... 000000 100644 100644 0 b new file.rs\0\
               2 R. N... 100644 100644 100644 a a R100 moved.rs\0old.rs\0\
               u UU N... 100644 100644 100644 100644 a b c conflict.rs\0\
               ? notes.md\0";
    let (changes, more) = parse_status(out);
    assert_eq!(more, 0);
    assert_eq!(changes.len(), 5);
    assert_eq!((changes[0].index, changes[0].tree), ('.', 'M'));
    assert_eq!(changes[1].path, "new file.rs");
    assert_eq!(changes[2].path, "moved.rs");
    assert_eq!(changes[2].from.as_deref(), Some("old.rs"));
    assert!(changes[3].conflict && !changes[2].conflict);
    assert_eq!(
        (changes[4].path.as_str(), changes[4].index),
        ("notes.md", '?')
    );
}

#[test]
fn counts_changes_beyond_the_listed_ones() {
    let out: String = (0..MAX_CHANGES + 3).map(|i| format!("? f{i}\0")).collect();
    let (changes, more) = parse_status(&out);
    assert_eq!((changes.len(), more), (MAX_CHANGES, 3));
}

#[test]
fn finds_the_remote_of_a_remote_branch() {
    let remotes = vec![
        "origin".to_owned(),
        "team/fork".to_owned(),
        "team".to_owned(),
    ];
    assert_eq!(
        remote_of("refs/remotes/team/fork/feature", &remotes).map(String::as_str),
        Some("team/fork")
    );
    assert_eq!(
        remote_of("refs/remotes/team/feature", &remotes).map(String::as_str),
        Some("team")
    );
    assert_eq!(remote_of("refs/remotes/originx/a", &remotes), None);
}

#[test]
fn empty_repository_has_no_branch_and_no_commit() {
    let sb = Sandbox::new("empty");
    let repo = sb.path("empty");
    fs::create_dir_all(&repo).unwrap();
    sb.git(&repo, &["init", "-q"]);
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.current.as_deref(), Some("main"));
    assert_eq!(state.head, None);
    assert!(state.branches.is_empty());
    assert_eq!(state.default_branch, None);
    assert_eq!(state.worktrees.len(), 1);
    let graph = sb.git.graph(&repo, 50).unwrap();
    assert!(graph.commits.is_empty() && !graph.more);
}

#[test]
fn state_and_graph_of_branches_merges_tags_and_worktrees() {
    let sb = Sandbox::new("state");
    let repo = sb.clone("acme");
    let c1 = sb.git(&repo, &["rev-parse", "HEAD"]);
    sb.git(&repo, &["switch", "-q", "-c", "feature/export"]);
    let f1 = sb.commit(&repo, "export.rs", "csv\n", "Export invoices as CSV");
    sb.git(&repo, &["switch", "-q", "main"]);
    let c2 = sb.commit(&repo, "README.md", "acme v2\n", "Describe the API");
    sb.git(&repo, &["tag", "-a", "v1.0.0", "-m", "Release 1.0.0"]);
    sb.git(
        &repo,
        &[
            "merge",
            "-q",
            "--no-ff",
            "feature/export",
            "-m",
            "Merge feature/export",
        ],
    );
    let merge = sb.git(&repo, &["rev-parse", "HEAD"]);
    sb.git(&repo, &["branch", "topic", &c1]);
    let wt = sb.path("wt-topic");
    sb.git(
        &repo,
        &["worktree", "add", "-q", wt.to_str().unwrap(), "topic"],
    );
    let t1 = sb.commit(&wt, "topic.rs", "t\n", "Start the topic");
    fs::write(wt.join("topic.rs"), "changed\n").unwrap();
    fs::write(repo.join("README.md"), "dirty\n").unwrap();
    fs::write(repo.join("scratch.txt"), "").unwrap();

    let graph = sb.git.graph(&repo, 100).unwrap();
    let oids: Vec<&str> = graph.commits.iter().map(|c| c.oid.as_str()).collect();
    assert_eq!(oids.len(), 5, "{oids:?}");
    let pos = |oid: &str| oids.iter().position(|o| *o == oid).unwrap();
    for c in &graph.commits {
        for p in &c.parents {
            assert!(pos(&c.oid) < pos(p), "a child comes before its parents");
        }
    }
    let m = &graph.commits[pos(&merge)];
    assert_eq!(m.parents, [c2.clone(), f1.clone()]);
    assert_eq!(
        m.refs,
        [Ref {
            name: "main".into(),
            kind: RefKind::Local
        }]
    );
    let refs_of = |oid: &str| graph.commits[pos(oid)].refs.clone();
    assert!(refs_of(&c2).contains(&Ref {
        name: "v1.0.0".into(),
        kind: RefKind::Tag
    }));
    assert!(refs_of(&c1).contains(&Ref {
        name: "origin/main".into(),
        kind: RefKind::Remote
    }));
    assert!(!refs_of(&c1).iter().any(|r| r.name == "origin/HEAD"));
    assert_eq!(refs_of(&t1)[0].name, "topic");

    let limited = sb.git.graph(&repo, 2).unwrap();
    assert_eq!(limited.commits.len(), 2);
    assert!(limited.more);

    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.current.as_deref(), Some("main"));
    assert_eq!(state.head.as_deref(), Some(merge.as_str()));
    assert_eq!(state.default_branch.as_deref(), Some("origin/main"));
    assert_eq!(state.remotes, ["origin"]);
    let main = branch(&state, "main");
    assert!(main.current);
    assert_eq!(main.upstream.as_deref(), Some("origin/main"));
    assert_eq!((main.ahead, main.behind), (3, 0));
    assert_eq!(main.tip.subject, "Merge feature/export");
    assert_eq!(main.tip.author, "Alex");
    assert_eq!(main.merged, Some(false), "not on origin/main yet");
    let origin = branch(&state, "origin/main");
    assert_eq!(origin.remote.as_deref(), Some("origin"));
    assert_eq!(origin.merged, Some(true));
    assert!(!state.branches.iter().any(|b| b.name == "origin/HEAD"));
    let topic = branch(&state, "topic");
    assert_eq!(topic.merged, Some(false));
    assert_eq!(topic.upstream, None);
    let topic_wt = topic.worktree.as_deref().unwrap();
    assert!(topic_wt.ends_with("wt-topic"), "{topic_wt}");
    assert_eq!(branch(&state, "feature/export").worktree, None);

    assert_eq!(state.worktrees.len(), 2);
    let here = &state.worktrees[0];
    assert!(here.main);
    assert_eq!(here.branch.as_deref(), Some("main"));
    let mut seen: Vec<(&str, char, char)> = here
        .changes
        .iter()
        .map(|c| (c.path.as_str(), c.index, c.tree))
        .collect();
    seen.sort();
    assert_eq!(seen, [("README.md", '.', 'M'), ("scratch.txt", '?', '?')]);
    let other = &state.worktrees[1];
    assert_eq!(other.branch.as_deref(), Some("topic"));
    assert_eq!(other.changes.len(), 1);
    assert_eq!(other.error, None);

    // Pushed, main is in the default branch and level with its upstream.
    fs::write(repo.join("README.md"), "acme v2\n").unwrap();
    sb.git(&repo, &["push", "-q", "origin", "main"]);
    let state = sb.git.state(&repo).unwrap();
    let main = branch(&state, "main");
    assert_eq!((main.ahead, main.behind, main.merged), (0, 0, Some(true)));
    assert_eq!(branch(&state, "feature/export").merged, Some(true));
}

#[test]
fn reports_conflicts_in_the_work_tree() {
    let sb = Sandbox::new("conflict");
    let repo = sb.clone("acme");
    sb.git(&repo, &["switch", "-q", "-c", "other"]);
    sb.commit(&repo, "README.md", "theirs\n", "Theirs");
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.commit(&repo, "README.md", "ours\n", "Ours");
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["merge", "-q", "other"])
        .envs(sb.git.env.iter().map(|(k, v)| (k, v)))
        .output()
        .unwrap();
    assert!(!out.status.success(), "the merge conflicts");
    let state = sb.git.state(&repo).unwrap();
    let changes = &state.worktrees[0].changes;
    assert_eq!(changes.len(), 1);
    assert!(changes[0].conflict);
    assert_eq!((changes[0].index, changes[0].tree), ('U', 'U'));
}

#[test]
fn a_file_untracked_but_still_in_head_comes_twice() {
    let sb = Sandbox::new("rm-cached");
    let repo = sb.clone("acme");
    sb.git(&repo, &["rm", "-q", "--cached", "README.md"]);
    let state = sb.git.state(&repo).unwrap();
    let mut seen: Vec<(&str, char, char)> = state.worktrees[0]
        .changes
        .iter()
        .map(|c| (c.path.as_str(), c.index, c.tree))
        .collect();
    seen.sort();
    // The UI keys each change by its letters and path, never by its path alone.
    assert_eq!(seen, [("README.md", '?', '?'), ("README.md", 'D', '.')]);
}

#[test]
fn batch_mode_is_added_to_openssh_only() {
    assert_eq!(
        batch_ssh("ssh -i ~/.ssh/acme").as_deref(),
        Some("ssh -i ~/.ssh/acme -o BatchMode=yes")
    );
    assert_eq!(
        batch_ssh("/usr/bin/ssh").as_deref(),
        Some("/usr/bin/ssh -o BatchMode=yes")
    );
    assert_eq!(
        batch_ssh(r"C:\Windows\System32\OpenSSH\ssh.exe").as_deref(),
        Some(r"C:\Windows\System32\OpenSSH\ssh.exe -o BatchMode=yes")
    );
    assert_eq!(batch_ssh("plink -batch"), None);
}

#[cfg(unix)]
#[test]
fn ssh_never_asks_and_keeps_the_repository_command() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::new("ssh");
    let repo = sb.clone("acme");
    // A fake ssh on the PATH that writes down how git called it, then fails like an unreachable host.
    let log = sb.path("ssh-args");
    let ssh = sb.path("bin").join("ssh");
    fs::write(
        &ssh,
        format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 255\n", log.display()),
    )
    .unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o755)).unwrap();
    sb.git(
        &repo,
        &["remote", "add", "far", "ssh://git.example.invalid/acme.git"],
    );

    assert!(sb.git.fetch(&repo).is_err());
    sb.git(
        &repo,
        &[
            "config",
            "core.sshCommand",
            &format!("{} -i acme-key", ssh.display()),
        ],
    );
    assert!(sb.git.fetch(&repo).is_err());
    let calls = fs::read_to_string(&log).unwrap();
    let calls: Vec<&str> = calls.lines().collect();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].starts_with("-o BatchMode=yes"), "{calls:?}");
    assert!(
        calls[1].starts_with("-i acme-key -o BatchMode=yes"),
        "{calls:?}"
    );
}

#[test]
fn switch_creates_a_tracking_branch_and_relays_git_refusals() {
    let sb = Sandbox::new("switch");
    let repo = sb.clone("acme");
    sb.git(&repo, &["switch", "-q", "-c", "feature"]);
    sb.commit(&repo, "README.md", "feature\n", "Feature");
    sb.git(&repo, &["push", "-q", "-u", "origin", "feature"]);
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.git(&repo, &["branch", "-D", "feature"]);

    sb.git.switch(&repo, "origin/feature").unwrap();
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(state.current.as_deref(), Some("feature"));
    assert_eq!(
        branch(&state, "feature").upstream.as_deref(),
        Some("origin/feature")
    );

    // A change the switch would overwrite: git's own refusal reaches the user, and nothing moves.
    fs::write(repo.join("README.md"), "unsaved\n").unwrap();
    let err = format!("{:#}", sb.git.switch(&repo, "main").unwrap_err());
    assert!(err.contains("would be overwritten"), "{err}");
    assert_eq!(
        sb.git.state(&repo).unwrap().current.as_deref(),
        Some("feature")
    );
    assert!(sb.git.switch(&repo, "--orphan").is_err());
    assert!(sb.git.switch(&repo, "nope").is_err());
}

#[test]
fn pull_fast_forwards_only() {
    let sb = Sandbox::new("pull");
    let repo = sb.clone("acme");
    let other = sb.clone("other");

    let up = sb.git.pull(&repo).unwrap();
    assert_eq!(up.outcome, PullOutcome::UpToDate);

    let theirs = sb.commit(&other, "api.rs", "v1\n", "Add the API");
    sb.git(&other, &["push", "-q", "origin", "main"]);
    let pulled = sb.git.pull(&repo).unwrap();
    assert_eq!(
        (pulled.outcome, pulled.ahead, pulled.behind),
        (PullOutcome::Updated, 0, 1)
    );
    assert_eq!(sb.git(&repo, &["rev-parse", "HEAD"]), theirs);

    // Commits on both sides: nothing changes, and the counts say why.
    sb.commit(&other, "api.rs", "v2\n", "Their change");
    sb.git(&other, &["push", "-q", "origin", "main"]);
    let ours = sb.commit(&repo, "web.rs", "w\n", "Our change");
    let diverged = sb.git.pull(&repo).unwrap();
    assert_eq!(
        (diverged.outcome, diverged.ahead, diverged.behind),
        (PullOutcome::Diverged, 1, 1)
    );
    assert_eq!(sb.git(&repo, &["rev-parse", "HEAD"]), ours);

    sb.git(&repo, &["switch", "-q", "-c", "local-only"]);
    assert_eq!(sb.git.pull(&repo).unwrap().outcome, PullOutcome::NoUpstream);

    // An upstream deleted on the remote: pull finds it out by itself, no fetch before.
    sb.git(&other, &["push", "-q", "origin", "main:gone-soon"]);
    sb.git(&repo, &["fetch", "-q", "origin"]);
    sb.git(
        &repo,
        &["switch", "-q", "-c", "follows", "origin/gone-soon"],
    );
    sb.git(&other, &["push", "-q", "origin", "--delete", "gone-soon"]);
    assert_eq!(sb.git.pull(&repo).unwrap().outcome, PullOutcome::Gone);
    let state = sb.git.state(&repo).unwrap();
    assert!(branch(&state, "follows").gone);

    sb.git(&repo, &["switch", "-q", "--detach"]);
    assert!(sb.git.pull(&repo).is_err());
}

#[test]
fn push_sets_the_upstream_and_never_forces() {
    let sb = Sandbox::new("push");
    let repo = sb.clone("acme");
    let other = sb.clone("other");

    sb.git(&repo, &["switch", "-q", "-c", "feature/csv"]);
    sb.commit(&repo, "csv.rs", "csv\n", "CSV");
    let first = sb.git.push(&repo, "feature/csv").unwrap();
    assert_eq!(
        first,
        Push {
            remote: "origin".into(),
            upstream_set: true
        }
    );
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(
        branch(&state, "feature/csv").upstream.as_deref(),
        Some("origin/feature/csv")
    );
    sb.commit(&repo, "csv.rs", "csv 2\n", "CSV 2");
    assert!(!sb.git.push(&repo, "feature/csv").unwrap().upstream_set);

    // A branch that tracks origin/main under another name pushes to its own name, never onto main.
    let main_before = sb.git(&other, &["ls-remote", "origin", "refs/heads/main"]);
    sb.git(&repo, &["branch", "--track", "fix", "origin/main"]);
    sb.git(&repo, &["switch", "-q", "fix"]);
    sb.commit(&repo, "fix.rs", "fix\n", "Fix");
    assert!(sb.git.push(&repo, "fix").unwrap().upstream_set);
    assert_eq!(
        sb.git(&other, &["ls-remote", "origin", "refs/heads/main"]),
        main_before
    );
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(
        branch(&state, "fix").upstream.as_deref(),
        Some("origin/fix")
    );

    // Someone else pushed first: git refuses, nothing is forced.
    sb.commit(&other, "api.rs", "theirs\n", "Theirs");
    sb.git(&other, &["push", "-q", "origin", "main"]);
    sb.git(&repo, &["switch", "-q", "main"]);
    sb.commit(&repo, "web.rs", "ours\n", "Ours");
    let err = format!("{:#}", sb.git.push(&repo, "main").unwrap_err());
    assert!(err.contains("rejected"), "{err}");
    assert!(sb.git.push(&repo, "nope").is_err());

    let lonely = sb.path("lonely");
    fs::create_dir_all(&lonely).unwrap();
    sb.git(&lonely, &["init", "-q"]);
    sb.commit(&lonely, "a", "a", "A");
    let err = format!("{:#}", sb.git.push(&lonely, "main").unwrap_err());
    assert!(err.contains("no remote"), "{err}");
}

#[test]
fn creates_renames_and_deletes_branches() {
    let sb = Sandbox::new("branches");
    let repo = sb.clone("acme");
    let base = sb.git(&repo, &["rev-parse", "HEAD"]);

    sb.git
        .create(&repo, "from-remote", "origin/main", false)
        .unwrap();
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(
        branch(&state, "from-remote").upstream,
        None,
        "tracks nothing"
    );
    assert_eq!(state.current.as_deref(), Some("main"));

    sb.git.create(&repo, "work", &base, true).unwrap();
    assert_eq!(
        sb.git.state(&repo).unwrap().current.as_deref(),
        Some("work")
    );
    let err = format!(
        "{:#}",
        sb.git.create(&repo, "bad name", "main", false).unwrap_err()
    );
    assert!(err.contains("not a valid branch name"), "{err}");
    assert!(sb.git.create(&repo, "work", "main", false).is_err());

    sb.git.rename(&repo, "from-remote", "renamed").unwrap();
    let state = sb.git.state(&repo).unwrap();
    assert!(state.branches.iter().any(|b| b.name == "renamed"));
    assert!(!state.branches.iter().any(|b| b.name == "from-remote"));

    // What a branch alone holds is what deleting it loses.
    let one = sb.commit(&repo, "a.rs", "a\n", "First on work");
    let two = sb.commit(&repo, "b.rs", "b\n", "Second on work");
    sb.git(&repo, &["switch", "-q", "main"]);
    let unique = sb.git.unique(&repo, "work").unwrap();
    assert_eq!(unique.total, 2);
    let oids: Vec<&str> = unique.commits.iter().map(|c| c.oid.as_str()).collect();
    assert_eq!(oids, [two.as_str(), one.as_str()]);
    assert_eq!(sb.git.unique(&repo, "renamed").unwrap().total, 0);

    let err = format!("{:#}", sb.git.delete(&repo, "work", &one).unwrap_err());
    assert!(err.contains("new commits"), "{err}");
    sb.git.delete(&repo, "work", &two).unwrap();
    assert!(
        !sb.git
            .state(&repo)
            .unwrap()
            .branches
            .iter()
            .any(|b| b.name == "work")
    );
    let err = format!("{:#}", sb.git.delete(&repo, "main", &base).unwrap_err());
    assert!(err.contains("main"), "the current branch stays: {err}");
}

#[test]
fn deletes_a_remote_branch() {
    let sb = Sandbox::new("remote-delete");
    let repo = sb.clone("acme");
    sb.git(&repo, &["push", "-q", "origin", "main:old/feature"]);
    sb.git(&repo, &["fetch", "-q", "origin"]);
    assert!(
        branch(&sb.git.state(&repo).unwrap(), "origin/old/feature")
            .remote
            .is_some()
    );
    sb.git
        .delete_remote(&repo, "origin", "old/feature")
        .unwrap();
    let state = sb.git.state(&repo).unwrap();
    assert!(
        !state
            .branches
            .iter()
            .any(|b| b.name == "origin/old/feature")
    );
    assert!(
        sb.git(&repo, &["ls-remote", "origin", "refs/heads/old/feature"])
            .is_empty()
    );
    assert!(sb.git.delete_remote(&repo, "origin", "-x").is_err());

    // Never the remote's default branch: the target of origin/HEAD, else main or master.
    sb.git(&repo, &["push", "-q", "origin", "main:trunk"]);
    sb.git(&repo, &["remote", "set-head", "origin", "trunk"]);
    let err = format!(
        "{:#}",
        sb.git.delete_remote(&repo, "origin", "trunk").unwrap_err()
    );
    assert!(err.contains("default branch"), "{err}");
    sb.git(&repo, &["remote", "set-head", "origin", "-d"]);
    assert!(sb.git.delete_remote(&repo, "origin", "main").is_err());
    assert_eq!(
        sb.git(
            &repo,
            &["ls-remote", "origin", "refs/heads/main", "refs/heads/trunk"]
        )
        .lines()
        .count(),
        2
    );
}

#[test]
fn push_goes_where_git_would_in_a_fork() {
    let sb = Sandbox::new("fork");
    let repo = sb.clone("acme");
    // `upstream` is the original, `origin` the user's fork, where git pushes by `remote.pushDefault`.
    sb.git(&repo, &["remote", "rename", "origin", "upstream"]);
    sb.git(
        &sb.root,
        &["clone", "-q", "--bare", "remote.git", "fork.git"],
    );
    let fork = sb.path("fork.git");
    sb.git(&repo, &["remote", "add", "origin", fork.to_str().unwrap()]);
    sb.git(&repo, &["config", "remote.pushDefault", "origin"]);
    sb.git(&repo, &["push", "-q", "upstream", "main:feature"]);
    sb.git(&repo, &["fetch", "-q", "upstream"]);
    sb.git(
        &repo,
        &[
            "switch",
            "-q",
            "-c",
            "feature",
            "--track",
            "upstream/feature",
        ],
    );
    let tip = sb.commit(&repo, "fork.rs", "fork\n", "Work in the fork");
    let original = sb.git(&repo, &["ls-remote", "upstream", "refs/heads/feature"]);

    let pushed = sb.git.push(&repo, "feature").unwrap();
    assert_eq!(
        pushed,
        Push {
            remote: "origin".into(),
            upstream_set: false
        }
    );
    assert!(
        sb.git(&repo, &["ls-remote", "origin", "refs/heads/feature"])
            .starts_with(&tip)
    );
    assert_eq!(
        sb.git(&repo, &["ls-remote", "upstream", "refs/heads/feature"]),
        original
    );
    let state = sb.git.state(&repo).unwrap();
    assert_eq!(
        branch(&state, "feature").upstream.as_deref(),
        Some("upstream/feature"),
        "the triangle stays"
    );

    // The branch's own push remote comes first.
    sb.git(&repo, &["config", "branch.feature.pushRemote", "upstream"]);
    assert_eq!(sb.git.push(&repo, "feature").unwrap().remote, "upstream");
    assert!(
        sb.git(&repo, &["ls-remote", "upstream", "refs/heads/feature"])
            .starts_with(&tip)
    );
}

#[test]
fn fetch_prunes_deleted_branches() {
    let sb = Sandbox::new("fetch");
    let repo = sb.clone("acme");
    let other = sb.clone("other");
    sb.git(&other, &["push", "-q", "origin", "main:short-lived"]);
    sb.git.fetch(&repo).unwrap();
    assert!(
        sb.git
            .state(&repo)
            .unwrap()
            .branches
            .iter()
            .any(|b| b.name == "origin/short-lived")
    );
    sb.git(&other, &["push", "-q", "origin", "--delete", "short-lived"]);
    sb.git.fetch(&repo).unwrap();
    assert!(
        !sb.git
            .state(&repo)
            .unwrap()
            .branches
            .iter()
            .any(|b| b.name == "origin/short-lived")
    );
}

#[cfg(unix)]
#[test]
fn reads_pull_requests_from_gh_and_stays_quiet_without_it() {
    let sb = Sandbox::new("prs");
    let repo = sb.clone("acme");
    assert!(sb.git.prs(&repo).is_empty(), "no gh");
    sb.fake_gh(
        r#"[ "$1 $2" = "pr list" ] || exit 3
echo '[{"number":12,"title":"Export CSV","state":"OPEN","isDraft":false,"url":"https://github.com/acme/api/pull/12","headRefName":"feature/csv","isCrossRepository":false},
{"number":9,"title":"From a fork","state":"OPEN","isDraft":true,"url":"https://github.com/acme/api/pull/9","headRefName":"main","isCrossRepository":true}]'"#,
    );
    let prs = sb.git.prs(&repo);
    assert_eq!(
        prs,
        [Pr {
            number: 12,
            title: "Export CSV".into(),
            state: "OPEN".into(),
            draft: false,
            url: "https://github.com/acme/api/pull/12".into(),
            branch: "feature/csv".into(),
        }]
    );
    sb.fake_gh("echo 'not logged in' >&2; exit 1");
    assert!(sb.git.prs(&repo).is_empty());
    sb.fake_gh("echo 'not json'");
    assert!(sb.git.prs(&repo).is_empty());
}
