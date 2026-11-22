use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;
use serde_json::json;

use super::*;

const VAULT_VALUE: &str = "correct-horse-battery-staple";
const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

struct Fixture {
    paths: Paths,
    repo: PathBuf,
}

fn put(file: &Path, text: &str) {
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=alex", "-c", "user.email=alex@acme.test"])
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn vault(text: &str) -> String {
    text.replace(VAULT_VALUE, termf_claude::mask::MASK)
}

fn fixture(name: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("termf-memory-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (config, data, claude) = (root.join("config"), root.join("data"), root.join("claude"));
    let repo = root.join("acme-api");
    put(&config.join("termf.toml"), "");
    put(
        &config.join("projects/acme.toml"),
        &format!(
            "name = \"Acme\"\n[[repos]]\npath = {:?}\n",
            repo.to_string_lossy()
        ),
    );
    put(
        &config.join("projects/acme/library/preferences/style.md"),
        "---\ndescription: house style\n---\nInvoices are rounded half to even.\n",
    );

    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    put(
        &repo.join("README.md"),
        "# Acme API\nThe billing service computes invoices nightly.\n",
    );
    put(
        &repo.join("docs/deploy.md"),
        &format!(
            "Deploy with the token {TOKEN} and never by hand.\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA/BG+abc\n-----END OPENSSH PRIVATE KEY-----\n"
        ),
    );
    put(
        &repo.join("CLAUDE.md"),
        "Run the tests with cargo nextest.\n",
    );
    put(&repo.join("src/main.rs"), "fn main() {}\n");
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &[
            "commit",
            "-q",
            "-m",
            "Add the reconciliation job\n\nIt matches payments to invoices.",
        ],
    );

    let folder = claude.join("projects").join(sources::folder_name(&repo));
    let line = |kind: &str, text: &str| {
        let content = if kind == "user" {
            json!(text)
        } else {
            json!([{"type": "text", "text": text}, {"type": "tool_use", "name": "Bash", "input": {"command": "rm -rf /"}}])
        };
        json!({"type": kind, "timestamp": "2026-11-20T09:15:00.000Z", "cwd": repo, "message": {"role": kind, "content": content}}).to_string()
    };
    put(
        &folder.join("11111111-1111-1111-1111-111111111111.jsonl"),
        &[
            json!({"type": "custom-title", "customTitle": "Webhook retries"}).to_string(),
            line("user", "Why do Stripe webhooks retry three times?"),
            line(
                "assistant",
                &format!("Because of the idempotency window; the password is {VAULT_VALUE}."),
            ),
            "{not json".into(),
        ]
        .join("\n"),
    );
    put(
        &claude
            .join("projects")
            .join(format!(
                "{}--claude-worktrees-termf-7-fix",
                sources::folder_name(&repo)
            ))
            .join("22222222-2222-2222-2222-222222222222.jsonl"),
        &line("user", "Rename the ledger table in the worktree"),
    );
    put(
        &folder.join("memory/MEMORY.md"),
        "- The ledger uses cents, never floats.\n",
    );
    put(
        &claude.join("projects/-elsewhere/33333333-3333-3333-3333-333333333333.jsonl"),
        &line("user", "Unrelated project about gardening"),
    );

    fs::create_dir_all(&data).unwrap();
    let db = Connection::open(data.join("termf.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE runs (id INTEGER PRIMARY KEY, project TEXT, ticket_key TEXT, title TEXT);
         INSERT INTO runs VALUES (4, 'acme', 'ACME-12', 'Fix rounding'), (5, 'other', 'X-1', 'Not ours');
         CREATE TABLE team_runs (run INTEGER PRIMARY KEY, state TEXT, data TEXT, updated INTEGER);
         CREATE TABLE agent_sessions (id INTEGER PRIMARY KEY, spec TEXT, updated INTEGER);
         CREATE TABLE agent_events (id INTEGER PRIMARY KEY, session INTEGER, kind TEXT, data TEXT);",
    )
    .unwrap();
    let team = json!({"target": {"project": "acme", "ticket": "ACME-12", "title": "Fix rounding"},
        "plan": {"summary": "Round invoices with banker's rounding", "acceptance": [{"id": "AC1", "text": "totals match the ledger"}],
                 "tasks": [{"id": "t1", "role": "back", "goal": "change the rounding helper"}]},
        "steps": [{"key": "po", "report": {"summary": "criteria framed from the ticket"}}]});
    db.execute(
        "INSERT INTO team_runs VALUES (4, 'done', ?1, 100)",
        [team.to_string()],
    )
    .unwrap();
    let spec = json!({"cwd": repo, "title": "expert", "project": "acme", "role": "expert"});
    db.execute(
        "INSERT INTO agent_sessions VALUES (9, ?1, 200)",
        [spec.to_string()],
    )
    .unwrap();
    for (kind, data) in [
        (
            "user",
            json!({"kind": "user", "text": "Where are refunds computed?"}),
        ),
        (
            "text",
            json!({"kind": "text", "text": "In the refunds module, see src/refunds.rs."}),
        ),
        (
            "text",
            json!({"kind": "text", "text": "subagent chatter", "parent": "call1"}),
        ),
    ] {
        db.execute(
            "INSERT INTO agent_events (session, kind, data) VALUES (9, ?1, ?2)",
            [kind, &data.to_string()],
        )
        .unwrap();
    }
    put(
        &data.join("runs/4/journal.md"),
        "# Fix rounding\n\n## Journal\n\n- 2026-11-21 10:00 UTC: Claude Code session 3 opened\n",
    );
    put(&data.join("runs/5/journal.md"), "# Not ours\n");
    put(
        &data.join("artifacts.json"),
        &json!({"artifacts": [
            {"id": "a1", "url": "https://claude.ai/artifact/a1", "title": "Billing architecture", "cwd": repo, "seen": 300},
            {"id": "a2", "url": "https://claude.ai/artifact/a2", "title": "Holiday plans", "cwd": "/somewhere/else", "seen": 300}
        ]})
        .to_string(),
    );

    Fixture {
        paths: Paths {
            config_dir: config,
            data_dir: data,
            claude_dir: claude,
        },
        repo,
    }
}

fn sources_of(hits: &[Hit]) -> Vec<Source> {
    hits.iter().map(|h| h.source).collect()
}

#[test]
fn indexes_every_source_of_the_project() {
    let f = fixture("sources");
    let memory = Memory::open(&f.paths.data_dir).unwrap();
    let done = memory.index(&f.paths, "acme", &vault).unwrap();
    let status = memory.status(&f.paths.config_dir, "acme").unwrap();
    assert!(
        status.sources.iter().all(|s| s.error.is_none()),
        "{status:?}"
    );
    let count = |source| {
        status
            .sources
            .iter()
            .find(|s| s.source == source)
            .unwrap()
            .docs
    };
    assert_eq!(
        count(Source::Claude),
        2,
        "the repository's and its worktree's conversations"
    );
    assert_eq!(
        count(Source::Memory),
        2,
        "CLAUDE.md and the automatic memory"
    );
    assert_eq!(
        count(Source::Docs),
        2,
        "README and deploy, not CLAUDE.md nor code"
    );
    assert_eq!(count(Source::Git), 1);
    assert_eq!(
        count(Source::Runs),
        2,
        "the journal and the team of run 4 only"
    );
    assert_eq!(count(Source::Agents), 1);
    assert_eq!(count(Source::Library), 1);
    assert_eq!(
        count(Source::Artifacts),
        1,
        "only the artifact opened in the project"
    );
    assert_eq!(done.changed, 12);

    let find = |q: &str| memory.search("acme", q, &[], 10).unwrap();
    let hits = find("why do webhooks retry");
    assert_eq!(hits[0].source, Source::Claude);
    assert_eq!(hits[0].title, "Webhook retries");
    assert!(
        hits[0]
            .locator
            .contains("11111111-1111-1111-1111-111111111111")
    );
    assert!(
        hits[0].snippet.contains("[webhooks]"),
        "{}",
        hits[0].snippet
    );
    assert_eq!(sources_of(&find("ledger cents"))[0], Source::Memory);
    assert!(sources_of(&find("reconciliation")).contains(&Source::Git));
    assert!(sources_of(&find("banker")).contains(&Source::Runs));
    assert!(sources_of(&find("refunds")).contains(&Source::Agents));
    assert!(sources_of(&find("rounded")).contains(&Source::Library));
    assert_eq!(
        sources_of(&find("billing architecture"))[0],
        Source::Artifacts
    );
    assert!(
        find("worktree ledger table")
            .iter()
            .any(|h| h.locator.contains("2222"))
    );
    assert!(
        find("gardening").is_empty(),
        "another project's conversation"
    );
    assert!(find("holiday").is_empty());
    assert!(find("subagent chatter").is_empty());
    assert!(find("rm -rf").is_empty(), "tool calls are not indexed");
    let git_only = memory
        .search("acme", "invoices", &[Source::Git], 10)
        .unwrap();
    assert_eq!(sources_of(&git_only), [Source::Git]);
    assert!(
        memory
            .search("other", "rounding", &[], 10)
            .unwrap()
            .is_empty()
    );
    // FTS5's own syntax is never taken from the question.
    for odd in ["\"", "NEAR(a b)", "a* OR -b", "(", "x:y", "", "é"] {
        memory.search("acme", odd, &[], 10).unwrap();
    }
}

#[test]
fn indexes_only_what_changed() {
    let f = fixture("incremental");
    let memory = Memory::open(&f.paths.data_dir).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    assert_eq!(
        memory.index(&f.paths, "acme", &vault).unwrap(),
        Indexed::default(),
        "nothing changed"
    );

    put(&f.repo.join("README.md"), "# Acme API\nNow with refunds.\n");
    fs::remove_file(f.repo.join("CLAUDE.md")).unwrap();
    put(&f.repo.join("NOTES.md"), "Quarterly close checklist.\n");
    git(&f.repo, &["add", "-A"]);
    git(&f.repo, &["commit", "-q", "-m", "Notes"]);
    let done = memory.index(&f.paths, "acme", &vault).unwrap();
    // README and NOTES (docs), the new commit; CLAUDE.md gone.
    assert_eq!(
        done,
        Indexed {
            changed: 3,
            removed: 1
        }
    );
    assert!(
        !memory
            .search("acme", "quarterly", &[], 5)
            .unwrap()
            .is_empty()
    );
    assert!(memory.search("acme", "nightly", &[], 5).unwrap().is_empty());
}

#[test]
fn never_stores_a_secret() {
    let f = fixture("masking");
    let memory = Memory::open(&f.paths.data_dir).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    let db = Connection::open(f.paths.data_dir.join("termf.db")).unwrap();
    let bodies: Vec<String> = db
        .prepare("SELECT title || ' ' || body FROM memory_fts")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let all = bodies.join("\n");
    for secret in [VAULT_VALUE, TOKEN, "b3BlbnNzaC1rZXktdjEAAAAA"] {
        assert!(!all.contains(secret), "{secret} was indexed");
    }
    assert!(all.contains("Deploy with the token"));
    assert!(memory.search("acme", "staple", &[], 5).unwrap().is_empty());
}

#[test]
fn reads_a_document_page_by_page() {
    let f = fixture("read");
    put(
        &f.repo.join("docs/long.md"),
        &"é ledger line\n".repeat(4000),
    );
    git(&f.repo, &["add", "-A"]);
    let memory = Memory::open(&f.paths.data_dir).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    let hit = memory
        .search("acme", "ledger line", &[Source::Docs], 1)
        .unwrap()
        .remove(0);
    let first = memory.read("acme", hit.id, 0).unwrap();
    assert!(first.text.len() <= PAGE && first.size > PAGE);
    let next = first.next.unwrap();
    let second = memory.read("acme", hit.id, next).unwrap();
    assert_eq!(second.offset, next);
    let e = next + second.text.find('é').unwrap();
    let inside = memory.read("acme", hit.id, e + 1).unwrap();
    assert_eq!(inside.offset, e, "back to a character's start");
    assert!(
        memory.read("other", hit.id, 0).is_err(),
        "another project's document"
    );
    assert!(memory.read("acme", 999_999, 0).is_err());
}

#[test]
fn a_source_switched_off_or_the_memory_off_is_emptied() {
    let f = fixture("settings");
    let memory = Memory::open(&f.paths.data_dir).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    let mut settings = termf_core::MemorySettings {
        enabled: true,
        exclude: vec!["claude".into()],
    };
    termf_core::save_memory(&f.paths.config_dir, &settings).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    assert!(
        memory
            .search("acme", "webhooks", &[], 5)
            .unwrap()
            .is_empty()
    );
    let status = memory.status(&f.paths.config_dir, "acme").unwrap();
    let claude = status
        .sources
        .iter()
        .find(|s| s.source == Source::Claude)
        .unwrap();
    assert!(!claude.included && claude.docs == 0);

    settings.enabled = false;
    termf_core::save_memory(&f.paths.config_dir, &settings).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    let status = memory.status(&f.paths.config_dir, "acme").unwrap();
    assert!(!status.enabled && status.sources.iter().all(|s| s.docs == 0));
}

#[test]
fn a_source_that_fails_keeps_what_it_had() {
    let f = fixture("failure");
    let memory = Memory::open(&f.paths.data_dir).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    fs::remove_dir_all(f.repo.join(".git")).unwrap();
    memory.index(&f.paths, "acme", &vault).unwrap();
    let status = memory.status(&f.paths.config_dir, "acme").unwrap();
    let docs = status
        .sources
        .iter()
        .find(|s| s.source == Source::Docs)
        .unwrap();
    assert!(docs.error.is_some() && docs.docs == 2, "{docs:?}");
    assert!(memory.index(&f.paths, "nope", &vault).is_err());
}
