//! Claude Code's own files, sessions, usage and account commands, in throwaway folders with a fake `claude`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::settings::Scope;
use crate::{Env, account, native, sessions, usage};

const TOKEN: &str = "ghp_0123456789abcdefghij0123456789abcd";

#[test]
fn turns_a_skill_off_through_skill_overrides() {
    let (_, env) = tree("skill-off");
    write(
        &env.claude_dir.join("settings.json"),
        &format!("{{\n  \"env\": {{\n    \"GITHUB_TOKEN\": \"{TOKEN}\"\n  }}\n}}\n"),
    );
    let edit = |value: &str| crate::settings::Edit::Set {
        key: "skillOverrides.review-pr".into(),
        value: Some(json!(value)),
    };
    let preview = crate::settings::preview(&env, Scope::User, &edit("off")).unwrap();
    assert!(
        preview.diff.contains(r#""review-pr": "off""#),
        "{}",
        preview.diff
    );
    assert!(
        !preview.diff.contains(TOKEN),
        "the lines around a change are masked"
    );
    assert!(crate::settings::preview(&env, Scope::User, &edit("hidden")).is_err());
}

fn tree(name: &str) -> (PathBuf, Env) {
    let root =
        std::env::temp_dir().join(format!("termf-claude-native-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("repo")).unwrap();
    let env = Env {
        claude_dir: root.join("home/.claude"),
        managed_dir: root.join("managed"),
        repo: Some(root.join("repo")),
        path: Some(root.join("bin").into_os_string()),
        claude: "claude".into(),
    };
    (root, env)
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn only_claude_codes_markdown_files_are_writable() {
    let (root, env) = tree("writable");
    let skill = env.claude_dir.join("skills/review/SKILL.md");
    assert!(native::writable(&env, &skill).is_ok());
    assert!(native::writable(&env, &root.join("repo/.claude/agents/qa.md")).is_ok());
    assert!(native::writable(&env, &root.join("repo/CLAUDE.local.md")).is_ok());
    assert!(native::writable(&env, &env.claude_dir.join("CLAUDE.md")).is_ok());
    fs::create_dir_all(env.claude_dir.join("projects/-repo/memory")).unwrap();
    assert!(native::writable(&env, &env.claude_dir.join("projects/-repo/memory/user.md")).is_ok());

    for path in [
        env.claude_dir.join("settings.json"),
        env.claude_dir.join(".credentials.json"),
        env.claude_dir.join("skills/../settings.md"),
        env.claude_dir.join("projects/-repo/abc.jsonl"),
        root.join("repo/README.md"),
        PathBuf::from("skills/x/SKILL.md"),
        env.managed_dir.join("CLAUDE.md"),
    ] {
        assert!(native::writable(&env, &path).is_err(), "{}", path.display());
    }
}

#[test]
fn reads_masked_and_reveals_only_when_asked() {
    let (_, env) = tree("read");
    let file = env.claude_dir.join("agents/deploy.md");
    write(
        &file,
        &format!("---\nname: deploy\n---\nUse GITHUB_TOKEN={TOKEN} to push.\n"),
    );
    let shown = native::read(&env, &file, false).unwrap();
    assert!(shown.masked && !shown.text.contains(TOKEN) && !shown.read_only);
    let revealed = native::read(&env, &file, true).unwrap();
    assert!(!revealed.masked && revealed.text.contains(TOKEN));
    assert!(native::preview(&env, &file, &shown.text, revealed.hash.as_deref()).is_err());

    write(
        &env.managed_dir.join("CLAUDE.md"),
        &format!("token {TOKEN}\n"),
    );
    let managed = native::read(&env, &env.managed_dir.join("CLAUDE.md"), true).unwrap();
    assert!(managed.read_only && !managed.text.contains(TOKEN));
    assert!(
        native::save(
            &env,
            &env.managed_dir.join("CLAUDE.md"),
            "x",
            managed.hash.as_deref()
        )
        .is_err()
    );
}

#[test]
fn saves_with_a_diff_and_never_over_a_change_made_meanwhile() {
    let (_, env) = tree("save");
    let file = env.claude_dir.join("CLAUDE.md");
    let created = native::save(&env, &file, "# Me\n", None).unwrap();
    assert_eq!(created.diff, "+# Me\n");
    assert!(
        native::save(&env, &file, "# Other\n", None).is_err(),
        "exists already"
    );

    let opened = native::read(&env, &file, true).unwrap().hash;
    let preview = native::preview(&env, &file, "# Me\nShort answers.", opened.as_deref()).unwrap();
    assert_eq!(preview.diff, " # Me\n+Short answers.\n");
    assert_eq!(preview.text, "# Me\nShort answers.\n");
    fs::write(&file, "# Me, edited elsewhere\n").unwrap();
    let stale = native::save(&env, &file, "# Me\nShort answers.\n", opened.as_deref());
    assert!(stale.unwrap_err().to_string().contains("changed since"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# Me, edited elsewhere\n"
    );
}

#[test]
fn creates_each_kind_with_its_frontmatter() {
    let (root, env) = tree("create");
    let skill = native::create(&env, "skills", Scope::User, "review", "Reviews: diffs").unwrap();
    assert_eq!(skill, env.claude_dir.join("skills/review/SKILL.md"));
    assert_eq!(
        fs::read_to_string(&skill).unwrap(),
        "---\nname: review\ndescription: \"Reviews: diffs\"\n---\n\n"
    );
    let command = native::create(&env, "commands", Scope::Project, "ship", "Ships").unwrap();
    assert_eq!(command, root.join("repo/.claude/commands/ship.md"));
    assert!(
        fs::read_to_string(&command)
            .unwrap()
            .starts_with("---\ndescription: \"Ships\"\n")
    );
    assert!(native::create(&env, "skills", Scope::User, "review", "again").is_err());
    for bad in ["../x", "Up", "-flag", "a/b", ""] {
        assert!(
            native::create(&env, "agents", Scope::User, bad, "").is_err(),
            "{bad}"
        );
    }
    assert!(native::create(&env, "settings", Scope::User, "x", "").is_err());
    assert!(native::create(&env, "agents", Scope::Local, "x", "").is_err());
}

#[test]
fn deletes_into_the_trash_with_a_skills_folder_and_a_memorys_index_line() {
    let (root, env) = tree("delete");
    let trash = root.join("trash");
    let skill = env.claude_dir.join("skills/review/SKILL.md");
    write(&skill, "---\nname: review\n---\n");
    write(
        &env.claude_dir.join("skills/review/scripts/check.sh"),
        "echo ok\n",
    );
    let gone = native::delete(&env, &skill, &trash).unwrap();
    assert!(!env.claude_dir.join("skills/review").exists());
    assert!(Path::new(&gone.trashed).join("scripts/check.sh").is_file());

    let memory = env.claude_dir.join("projects/-repo/memory");
    write(&memory.join("user.md"), "Likes short answers.\n");
    write(
        &memory.join("MEMORY.md"),
        "- [User](user.md) — preferences\n- [Build](build.md) — how to build\n",
    );
    let gone = native::delete(&env, &memory.join("user.md"), &trash).unwrap();
    assert_eq!(
        gone.index.as_deref(),
        Some(memory.join("MEMORY.md").to_str().unwrap())
    );
    assert_eq!(
        fs::read_to_string(memory.join("MEMORY.md")).unwrap(),
        "- [Build](build.md) — how to build\n"
    );
    assert!(native::delete(&env, &env.claude_dir.join("settings.md"), &trash).is_err());
}

#[test]
fn names_project_folders_like_claude_code() {
    assert_eq!(
        sessions::folder_name(Path::new("/Users/alex/acme/api")),
        "-Users-alex-acme-api"
    );
    assert_eq!(
        sessions::folder_name(Path::new("/tmp/my.repo_x")),
        "-tmp-my-repo-x"
    );
}

#[test]
fn reads_a_transcripts_title_and_first_prompt_only() {
    let head = [
        json!({"type": "user", "isMeta": true, "message": {"content": "<local-command-caveat>…"}, "cwd": "/w/api"}),
        json!({"type": "user", "message": {"content": [{"type": "text", "text": format!("Fix the login, token {TOKEN}")}]}}),
        json!({"type": "ai-title", "aiTitle": "Fix login"}),
    ]
    .map(|v| v.to_string())
    .join("\n");
    let tail = format!(
        "{{\"type\":\"assistant\",\"mess\n{}\n{}\n",
        json!({"type": "custom-title", "customTitle": "login-fix"}),
        json!({"type": "last-prompt", "lastPrompt": "and the tests"}),
    );
    let meta = sessions::meta(&head, &tail);
    assert_eq!(meta.title.as_deref(), Some("login-fix"));
    assert_eq!(meta.cwd.as_deref(), Some("/w/api"));
    let prompt = meta.prompt.unwrap();
    assert!(
        prompt.starts_with("Fix the login, token") && !prompt.contains(TOKEN),
        "{prompt}"
    );
    assert_eq!(sessions::meta("", "").title, None);
    assert_eq!(
        sessions::meta(
            &json!({"type":"last-prompt","lastPrompt":"x".repeat(300)}).to_string(),
            ""
        )
        .prompt
        .unwrap()
        .chars()
        .count(),
        121
    );
}

#[test]
fn lists_a_repositorys_sessions_and_its_worktrees_only() {
    let (root, env) = tree("sessions");
    let repo = root.join("repo");
    let name = sessions::folder_name(&fs::canonicalize(&repo).unwrap());
    let projects = env.claude_dir.join("projects");
    let id = "11111111-2222-3333-4444-555555555555";
    write(
        &projects.join(&name).join(format!("{id}.jsonl")),
        &json!({"type": "user", "message": {"content": "hello"}, "cwd": repo}).to_string(),
    );
    write(
        &projects
            .join(format!("{name}--claude-worktrees-termf-1"))
            .join("22222222-2222-3333-4444-555555555555.jsonl"),
        "{}\n",
    );
    write(
        &projects.join(format!("{name}-other")).join("x.jsonl"),
        "{}\n",
    );
    write(&projects.join(&name).join("memory/MEMORY.md"), "- x\n");

    let listed = sessions::list(&env, Some(&repo));
    assert_eq!(listed.len(), 2, "{listed:?}");
    let own = listed.iter().find(|p| p.folder == name).unwrap();
    assert_eq!(own.sessions.len(), 1);
    assert_eq!(own.sessions[0].id, id);
    assert_eq!(own.sessions[0].prompt.as_deref(), Some("hello"));
    assert!(own.sessions[0].size > 0);
    assert_eq!(sessions::list(&env, None).len(), 3);

    assert!(sessions::check_id(id).is_ok());
    for bad in ["--resume", "x", "../11111111-2222-3333-4444-5555555555"] {
        assert!(sessions::check_id(bad).is_err(), "{bad}");
    }
    assert!(sessions::check_background("a1b2c3").is_ok());
    assert!(sessions::check_background("-rf").is_err());
}

#[test]
fn reads_usage_from_the_status_line() {
    let reading = usage::read(&json!({
        "cost": {"total_cost_usd": 0.42},
        "model": {"id": "claude-opus-5-5", "display_name": "Opus"},
        "rate_limits": {"five_hour": {"used_percentage": 23.5, "resets_at": 1738425600}},
    }));
    assert_eq!(reading.cost, Some(0.42));
    assert_eq!(reading.model.as_deref(), Some("Opus"));
    let five = reading.limits.five_hour.unwrap();
    assert_eq!(
        (five.used_percentage, five.resets_at),
        (23.5, Some(1738425600))
    );
    assert!(reading.limits.seven_day.is_none());
    assert!(usage::read(&json!({})).limits.is_empty());
}

#[cfg(unix)]
fn fake(bin: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = bin.join("claude");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn runs_claude_codes_own_commands_and_masks_what_they_print() {
    let (root, env) = tree("commands");
    let log = root.join("claude.log");
    fake(
        &root.join("bin"),
        &format!(
            r#"echo "$*" >> '{log}'
case "$*" in
  "agents --json --all") echo '[{{"id":"a1b2c3","kind":"background","state":"working","cwd":"/w","env":"TOKEN={TOKEN}"}}]' ;;
  "doctor") echo "Claude Code doctor"; echo "Proxy: https://me:hunter2secret@proxy.test" ;;
  "update") echo "Updated to 2.1.331" ;;
  "auth logout") echo "Successfully logged out" ;;
  "stop a1b2c3") echo "Stopped" ;;
  purge*--dry-run) echo "Dry run: 1 item(s) would be deleted." ;;
  purge*--yes) echo "Deleted 1 item(s)." ;;
  *) echo "unexpected: $*" >&2; exit 1 ;;
esac"#,
            log = log.display()
        ),
    );
    let background = sessions::background(&env);
    assert_eq!(background.error, None);
    assert_eq!(background.sessions[0]["id"], "a1b2c3");
    assert!(!background.sessions[0].to_string().contains(TOKEN));

    let doctor = account::doctor(&env).unwrap();
    assert!(doctor.starts_with("Claude Code doctor") && !doctor.contains("hunter2secret"));
    assert_eq!(account::update(&env).unwrap(), "Updated to 2.1.331");
    assert_eq!(account::logout(&env).unwrap(), "Successfully logged out");
    assert_eq!(sessions::stop(&env, "a1b2c3").unwrap(), "Stopped");
    assert!(sessions::stop(&env, "--all").is_err());

    let project = root.join("repo");
    assert!(
        sessions::purge_preview(&env, &project)
            .unwrap()
            .starts_with("Dry run")
    );
    assert!(
        sessions::purge(&env, &project)
            .unwrap()
            .starts_with("Deleted")
    );
    assert!(sessions::purge(&env, Path::new("relative")).is_err());

    let calls = fs::read_to_string(&log).unwrap();
    let target = project.display();
    assert!(calls.contains(&format!("purge {target} --dry-run\n")));
    assert!(calls.contains(&format!("purge {target} --yes\n")));
    assert!(
        !calls.contains("stop --all"),
        "a refused id never reaches claude"
    );

    fake(&root.join("bin"), "echo 'not logged in' >&2; exit 1");
    assert_eq!(
        account::logout(&env).unwrap_err().to_string(),
        "not logged in"
    );
    assert!(sessions::background(&env).error.is_some());
}

#[test]
fn diffs_documents_line_by_line() {
    let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\n";
    let after = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl\n";
    assert_eq!(
        native::line_diff(before, after),
        " a\n-b\n+B\n c\n d\n e\n…\n i\n j\n k\n+l\n"
    );
    assert_eq!(native::line_diff("x\n", "x\n"), "");
    assert_eq!(native::line_diff("", "new\n"), "+new\n");
}
