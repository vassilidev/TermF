use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::artifacts::{self, Failure, Listed, ListingError, Owner, Policy, Refresh, Step};
use crate::cli::{self, Health};
use crate::settings::{self, Scope};
use crate::{Env, State, mask};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("termf-claude-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A throwaway `~/.claude`, a repository and a managed folder; commands find nothing on `PATH` unless a test
/// puts fakes there.
fn tree(name: &str) -> (PathBuf, Env) {
    let root = temp(name);
    let env = Env {
        claude_dir: root.join("home/.claude"),
        managed_dir: root.join("managed"),
        repo: Some(root.join("repo")),
        path: Some(root.join("bin").into_os_string()),
        claude: "claude".into(),
    };
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("repo")).unwrap();
    (root, env)
}

const TOKEN: &str = "ghp_0123456789abcdefghij0123456789abcd";

#[test]
fn reads_each_scope_and_who_decides() {
    let (root, env) = tree("scopes");
    write(
        &env.claude_dir.join("settings.json"),
        &json!({
            "model": "sonnet",
            "cleanupPeriodDays": 30,
            "permissions": {"allow": ["Bash(git status)"], "defaultMode": "acceptEdits"},
            "env": {"GITHUB_TOKEN": TOKEN, "NODE_ENV": "development"},
            "hooks": {"PostToolUse": [{"matcher": "Edit", "hooks": [{"type": "command", "command": "prettier --write"}]}]},
        })
        .to_string(),
    );
    write(
        &root.join("repo/.claude/settings.json"),
        &json!({"model": "opus", "permissions": {"allow": ["Bash(npm test)"]}}).to_string(),
    );
    write(&root.join("repo/.claude/settings.local.json"), "{ not json");
    write(
        &env.managed_dir.join("managed-settings.json"),
        &json!({"permissions": {"defaultMode": "plan"}}).to_string(),
    );

    let s = settings::masked(settings::read(&env));
    let state = |scope| s.files.iter().find(|f| f.scope == scope).unwrap();
    assert_eq!(state(Scope::User).state, State::Ok);
    assert_eq!(state(Scope::Project).state, State::Ok);
    assert_eq!(state(Scope::Local).state, State::Invalid);
    assert!(state(Scope::Local).error.is_some());
    assert_eq!(state(Scope::Managed).state, State::Ok);

    let key = |k: &str| s.effective.iter().find(|e| e.key == k).unwrap();
    assert_eq!(key("model").value, "opus");
    assert_eq!(key("model").scope, Scope::Project);
    assert_eq!(key("model").others, [Scope::User]);
    assert_eq!(key("permissions.defaultMode").value, "plan");
    assert_eq!(key("permissions.defaultMode").scope, Scope::Managed);
    let allow = key("permissions.allow");
    assert!(allow.merged);
    assert_eq!(allow.value, json!(["Bash(git status)", "Bash(npm test)"]));
    assert_eq!(key("cleanupPeriodDays").scope, Scope::User);
    assert_eq!(key("env.GITHUB_TOKEN").value, mask::MASK);
    assert_eq!(key("env.NODE_ENV").value, "development");
    assert!(s.effective.iter().all(|e| e.key != "hooks"));
    assert_eq!(
        state(Scope::User).value.as_ref().unwrap()["env"]["GITHUB_TOKEN"],
        mask::MASK
    );

    let no_repo = Env { repo: None, ..env };
    let s = settings::read(&no_repo);
    assert_eq!(s.files.len(), 2, "user and managed only");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn edits_a_setting_atomically_in_strict_json() {
    let (root, env) = tree("edit");
    let user = env.claude_dir.join("settings.json");
    write(
        &user,
        "{\n    \"theme\": \"dark\",\n    \"model\": \"sonnet\",\n    \"env\": {\n        \"A\": \"1\"\n    }\n}\n",
    );

    let preview = settings::preview(&env, Scope::User, "model", Some(&json!("opus"))).unwrap();
    assert_eq!(preview.diff.matches('-').count(), 1, "{}", preview.diff);
    assert!(preview.diff.contains("-    \"model\": \"sonnet\","));
    assert!(preview.diff.contains("+    \"model\": \"opus\","));
    assert!(
        fs::read_to_string(&user).unwrap().contains("sonnet"),
        "a preview writes nothing"
    );

    settings::save(&env, Scope::User, "model", Some(&json!("opus"))).unwrap();
    let text = fs::read_to_string(&user).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["model"], "opus");
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["theme", "model", "env"], "the order is kept");
    assert!(
        text.starts_with("{\n    \"theme\""),
        "the indentation is kept"
    );
    assert!(
        fs::read_dir(&env.claude_dir)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().contains("tmp")),
        "no temporary file left"
    );

    settings::save(
        &env,
        Scope::User,
        "permissions.defaultMode",
        Some(&json!("plan")),
    )
    .unwrap();
    settings::save(&env, Scope::User, "model", None).unwrap();
    let value: Value = serde_json::from_str(&fs::read_to_string(&user).unwrap()).unwrap();
    assert_eq!(value["permissions"]["defaultMode"], "plan");
    assert!(value.get("model").is_none());

    // The project file and its folder are created.
    settings::save(&env, Scope::Local, "autoMemoryEnabled", Some(&json!(false))).unwrap();
    let local: Value = serde_json::from_str(
        &fs::read_to_string(root.join("repo/.claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(local, json!({"autoMemoryEnabled": false}));

    for (scope, key, value) in [
        (Scope::User, "effortLevel", json!("max")),
        (Scope::User, "cleanupPeriodDays", json!(0)),
        (Scope::User, "autoUpdatesChannel", json!("nightly")),
        (Scope::User, "model", json!("")),
        (Scope::User, "apiKeyHelper", json!("x")),
        (
            Scope::Project,
            "permissions.defaultMode",
            json!("bypassPermissions"),
        ),
        (Scope::Managed, "model", json!("opus")),
    ] {
        assert!(
            settings::preview(&env, scope, key, Some(&value)).is_err(),
            "{key} = {value}"
        );
    }

    write(&root.join("repo/.claude/settings.json"), "{ broken");
    let err = settings::save(&env, Scope::Project, "model", Some(&json!("opus"))).unwrap_err();
    assert!(err.to_string().contains("not a valid JSON object"), "{err}");
    assert_eq!(
        fs::read_to_string(root.join("repo/.claude/settings.json")).unwrap(),
        "{ broken"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[cfg(unix)]
#[test]
fn writes_through_a_symlinked_settings_file() {
    let (root, env) = tree("symlink");
    let real = root.join("dotfiles/settings.json");
    write(&real, "{}");
    fs::create_dir_all(&env.claude_dir).unwrap();
    std::os::unix::fs::symlink(&real, env.claude_dir.join("settings.json")).unwrap();
    settings::save(&env, Scope::User, "effortLevel", Some(&json!("high"))).unwrap();
    assert!(
        fs::symlink_metadata(env.claude_dir.join("settings.json"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::read_to_string(&real)
            .unwrap()
            .contains("\"effortLevel\": \"high\"")
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn parses_claude_mcp_list_and_get() {
    let list = "Checking MCP server health…\n\n\
echo-srv: /bin/cat -u - ✘ Failed to connect — -32601: Method not found\n\
web: https://api.test/mcp?token=abc (HTTP) - ✔ Connected\n\
proj-srv: /bin/echo hi - ⏸ Pending approval (run `claude` to approve)\n\
claude.ai Gmail: https://gmail.mcp.claude.com/mcp - ! Needs authentication\n\
plugin:figma:figma: npx figma-mcp --api-key=s3cr3t - ✓ Connected\n";
    let servers = cli::parse_mcp_list(list);
    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "echo-srv",
            "web",
            "proj-srv",
            "claude.ai Gmail",
            "plugin:figma:figma"
        ]
    );
    let health: Vec<&Health> = servers.iter().map(|s| &s.health).collect();
    assert_eq!(
        health,
        [
            &Health::Failed,
            &Health::Connected,
            &Health::Pending,
            &Health::NeedsAuth,
            &Health::Connected
        ]
    );
    assert_eq!(
        servers[1].target,
        format!("https://api.test/mcp?token={} (HTTP)", mask::MASK)
    );
    assert_eq!(
        servers[4].target,
        format!("npx figma-mcp --api-key={}", mask::MASK)
    );
    assert!(
        cli::parse_mcp_list("No MCP servers configured. Use `claude mcp add` to add a server.\n")
            .is_empty()
    );

    let get = "echo-srv:\n  Scope: User config (available in all your projects)\n  Status: ✘ Failed to connect\n  Type: stdio\n  Command: /bin/cat\n  Args: -u\n  Environment:\n    API_TOKEN=sk-secret-123\n    LOG=debug\n  Headers:\n    Authorization: Bearer abc\n\nTo remove this server, run: claude mcp remove echo-srv -s user\n";
    let (details, scope) = cli::parse_mcp_get(get);
    assert_eq!(scope.as_deref(), Some("user"));
    assert_eq!(details["Type"], "stdio");
    assert_eq!(details["Command"], "/bin/cat");
    let mut details = Value::Object(details);
    mask::mask(&mut details);
    assert_eq!(details["Environment"]["API_TOKEN"], mask::MASK);
    assert_eq!(details["Environment"]["LOG"], "debug");
    assert_eq!(details["Headers"]["Authorization"], mask::MASK);
}

#[test]
fn parses_auth_status_and_versions() {
    let logged_out = r#"{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty"}"#;
    assert_eq!(cli::parse_auth(logged_out).unwrap()["loggedIn"], false);
    let logged_in = r#"{"loggedIn": true, "authMethod": "claude.ai", "email": "me@example.test", "oauthToken": "abc", "subscriptionType": "max"}"#;
    let auth = cli::parse_auth(logged_in).unwrap();
    assert_eq!(auth["authMethod"], "claude.ai");
    assert_eq!(auth["oauthToken"], mask::MASK);
    assert!(cli::parse_auth("Not logged in.").is_none());
    assert_eq!(
        cli::first_version("2.1.330 (Claude Code)\n").as_deref(),
        Some("2.1.330")
    );
    assert_eq!(
        cli::first_version("gh version 2.83.0 (2026-01-01)\nhttps://x").as_deref(),
        Some("2.83.0")
    );
    assert_eq!(cli::first_version("v22.20.0\n").as_deref(), Some("22.20.0"));
    assert_eq!(
        cli::first_version("git version 2.50.1 (Apple Git-155)").as_deref(),
        Some("2.50.1")
    );
    assert_eq!(cli::first_version(""), None);
    assert_eq!(
        cli::install_method(Path::new(
            "/opt/x/lib/node_modules/@anthropic-ai/claude-code/cli.js"
        )),
        "npm"
    );
    assert_eq!(
        cli::install_method(Path::new("/home/me/.local/share/claude/versions/2.1.330")),
        "native"
    );
    assert_eq!(
        cli::install_method(Path::new("/opt/homebrew/Caskroom/claude-code/2.1/claude")),
        "homebrew"
    );
}

#[cfg(unix)]
fn fake(bin: &Path, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = bin.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// The whole overview with fake `claude`, `gh` and `git` on PATH, and nothing secret in what leaves the crate.
#[cfg(unix)]
#[test]
fn builds_the_overview_with_secrets_masked() {
    let (root, env) = tree("overview");
    let bin = root.join("bin");
    let log = root.join("claude.log");
    fake(
        &bin,
        "claude",
        &format!(
            r#"echo "$*" >> '{log}'
case "$*" in
  "mcp list") printf 'Checking MCP server health…\n\nsrv: /bin/srv --token {TOKEN} - ✔ Connected\n' ;;
  "mcp get srv") printf 'srv:\n  Scope: Local config (private to you in this project)\n  Type: stdio\n  Environment:\n    SRV_API_KEY=plainvalue\n\nTo remove this server, run: claude mcp remove srv -s local\n' ;;
  "plugin list --json") echo '[{{"id":"hello@mkt","version":"1.2.0","scope":"user","enabled":true}}]' ;;
  "plugin marketplace list --json") echo '[{{"name":"mkt","source":"github","repo":"acme/mkt"}}]' ;;
  "auth status") echo '{{"loggedIn": true, "authMethod": "claude.ai", "email": "me@example.test"}}' ;;
  "--version") echo '2.1.330 (Claude Code)' ;;
  *) exit 2 ;;
esac"#,
            log = log.display()
        ),
    );
    fake(&bin, "gh", "echo 'gh version 2.83.0 (2026-01-01)'");
    fake(&bin, "git", "echo 'git version 2.50.1'");

    let claude = &env.claude_dir;
    write(
        &claude.join("settings.json"),
        &json!({
            "env": {"ANTHROPIC_API_KEY": "sk-ant-api03-abcdefghijklmnop"},
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": format!("notify --token {TOKEN}")}]}]},
            "statusLine": {"type": "command", "command": "~/statusline.sh"},
        })
        .to_string(),
    );
    write(&claude.join("CLAUDE.md"), "# Mine\n");
    write(
        &claude.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Reviews a diff\n---\nBody\n",
    );
    write(&claude.join("skills/notes.txt"), "not a skill");
    write(
        &claude.join("agents/critic.md"),
        "---\nname: critic\ndescription: >\n  Finds flaws\n---\n",
    );
    write(&claude.join("commands/git/pr.md"), "Open a PR\n");
    write(
        &claude.join("rules/style.md"),
        "---\ndescription: [broken\n---\n",
    );
    write(&claude.join("themes/night.json"), "{}");
    write(&claude.join("keybindings.json"), "[]");
    write(
        &claude.join("projects/-repo/memory/MEMORY.md"),
        "- [a](a.md)\n",
    );
    write(&claude.join("projects/-repo/memory/a.md"), "remember");
    write(
        &claude.join(".credentials.json"),
        &format!("{{\"token\": \"{TOKEN}\"}}"),
    );
    write(
        &root.join("repo/.claude/commands/ship.md"),
        "---\ndescription: Ships\n---\n",
    );
    write(&root.join("repo/CLAUDE.local.md"), "local");

    let overview = crate::overview(&env);

    assert_eq!(overview.mcp.servers.len(), 1);
    let srv = &overview.mcp.servers[0];
    assert_eq!(srv.scope.as_deref(), Some("local"));
    assert_eq!(
        srv.details.as_ref().unwrap()["Environment"]["SRV_API_KEY"],
        mask::MASK
    );
    assert_eq!(overview.plugins.plugins[0]["id"], "hello@mkt");
    assert_eq!(overview.plugins.marketplaces[0]["name"], "mkt");
    assert_eq!(
        overview.account.auth.as_ref().unwrap()["authMethod"],
        "claude.ai"
    );
    assert_eq!(overview.account.version.as_deref(), Some("2.1.330"));
    let tool = |name: &str| overview.tools.iter().find(|t| t.name == name).unwrap();
    assert_eq!(tool("gh").version.as_deref(), Some("2.83.0"));
    assert_eq!(tool("git").version.as_deref(), Some("2.50.1"));
    assert_eq!(tool("node").version, None);

    let folder = |kind: &str, scope: Scope| {
        overview
            .extensions
            .iter()
            .find(|f| f.kind == kind && f.scope == scope)
            .unwrap()
    };
    let skills = &folder("skills", Scope::User).items;
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].description.as_deref(), Some("Reviews a diff"));
    assert_eq!(
        folder("agents", Scope::User).items[0]
            .description
            .as_deref(),
        Some("Finds flaws")
    );
    assert_eq!(folder("commands", Scope::User).items[0].name, "git:pr");
    assert_eq!(
        folder("rules", Scope::User).items[0].description,
        None,
        "invalid frontmatter"
    );
    assert_eq!(folder("commands", Scope::Project).items[0].name, "ship");
    assert!(
        overview
            .extensions
            .iter()
            .all(|f| f.kind != "output-styles"),
        "absent folders are left out"
    );

    assert_eq!(overview.hooks.len(), 1);
    assert_eq!(overview.hooks[0].event, "Stop");
    assert_eq!(
        overview.hooks[0].run,
        format!("notify --token {}", mask::MASK)
    );

    let memory = |scope| {
        overview
            .memory
            .files
            .iter()
            .find(|f| f.scope == Some(scope))
            .unwrap()
    };
    assert_eq!(memory(Scope::User).state, State::Ok);
    assert_eq!(memory(Scope::User).size, Some(7));
    assert_eq!(memory(Scope::Project).state, State::Absent);
    assert_eq!(memory(Scope::Local).state, State::Ok);
    assert_eq!(overview.memory.auto.len(), 1);
    assert_eq!(
        overview.memory.auto[0].files,
        [("MEMORY.md".into(), 12), ("a.md".into(), 8)]
    );
    assert_eq!(overview.interface.themes, ["night"]);
    assert_eq!(overview.interface.keybindings.state, State::Ok);

    // What the page shows, copies and exports as JSON.
    let json = serde_json::to_string(&overview).unwrap();
    for secret in [TOKEN, "sk-ant-api03-abcdefghijklmnop", "plainvalue"] {
        assert!(!json.contains(secret), "{secret} leaked");
    }
    assert!(!json.contains(".credentials"), "never read");
    // A Markdown export the page builds is masked once more on its way to the file.
    let markdown = format!(
        "| env.ANTHROPIC_API_KEY | sk-ant-api03-abcdefghijklmnop |\n\"GITHUB_TOKEN\": \"{TOKEN}\""
    );
    let exported = crate::export_text(&markdown);
    assert!(
        !exported.contains("sk-ant-api03") && !exported.contains(TOKEN),
        "{exported}"
    );

    let calls = fs::read_to_string(&log).unwrap();
    assert!(
        calls
            .lines()
            .all(|l| !l.contains("add") && !l.contains("remove")),
        "read-only: {calls}"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn reads_a_missing_claude_folder() {
    let (root, env) = tree("empty");
    let overview = crate::overview(&Env { repo: None, ..env });
    assert!(
        overview
            .settings
            .files
            .iter()
            .all(|f| f.state == State::Absent)
    );
    assert!(overview.mcp.error.is_some(), "no claude on PATH");
    assert!(
        overview.extensions.is_empty()
            && overview.hooks.is_empty()
            && overview.memory.auto.is_empty()
    );
    fs::remove_dir_all(&root).unwrap();
}

const URL: &str = "https://claude.ai/code/artifact/3f2a9c1e-7b4d-4e8a-9f10-2c3d4e5f6a7b";
const ID: &str = "3f2a9c1e-7b4d-4e8a-9f10-2c3d4e5f6a7b";

#[test]
fn recognizes_artifact_links() {
    assert_eq!(artifacts::artifact_url(URL), Some((ID.into(), URL.into())));
    assert_eq!(
        artifacts::artifact_url("https://claude.ai/artifact/abc_12?ref=x"),
        Some(("abc_12".into(), "https://claude.ai/artifact/abc_12".into()))
    );
    for bad in [
        "https://claude.ai/code/artifacts",
        "https://evil.test/code/artifact/abc",
        "http://claude.ai/code/artifact/abc",
        "https://claude.ai/code/artifact/",
        "https://claude.ai/code/artifact/abc\"onload=x",
    ] {
        assert_eq!(artifacts::artifact_url(bad), None, "{bad}");
    }
}

/// Where TermF's hidden sessions run.
fn own() -> &'static Path {
    Path::new("/data/claude-agent")
}

/// The `PostToolUse` hook of TermF's sessions feeds the index.
#[test]
fn indexes_what_sessions_publish_open_and_delete() {
    let root = temp("index");
    let index = root.join("artifacts.json");
    write(
        &root.join("repo/report.html"),
        "<html><head><title>Q3 &amp; Q4 report</title></head></html>",
    );
    let hook = |input: Value, response: Value| json!({"tool_name": "Artifact", "cwd": root.join("repo").to_string_lossy(), "tool_input": input, "tool_response": response});

    let published = hook(
        json!({"action": "publish", "file_path": "report.html"}),
        json!(format!("Published {URL} (private). Watching it.")),
    );
    assert!(artifacts::record(&index, &published, 100, own()).unwrap());
    let list = artifacts::load(&index).artifacts;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, ID);
    assert_eq!(list[0].title.as_deref(), Some("Q3 & Q4 report"));
    assert_eq!(list[0].owner, Some(Owner::Me));
    assert_eq!(
        list[0].cwd.as_deref(),
        Some(root.join("repo").to_string_lossy().as_ref())
    );

    let read = hook(
        json!({"action": "read", "url": "https://claude.ai/artifact/other1"}),
        json!({"content": "…"}),
    );
    assert!(artifacts::record(&index, &read, 200, own()).unwrap());
    assert_eq!(
        artifacts::load(&index).artifacts[0].id,
        "other1",
        "most recent first"
    );

    assert!(
        !artifacts::record(
            &index,
            &json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
            300,
            own()
        )
        .unwrap()
    );
    assert!(
        !artifacts::record(
            &index,
            &hook(json!({"action": "list"}), json!(URL)),
            300,
            own()
        )
        .unwrap()
    );

    // Opened again from TermF's hidden sessions, or from another folder: it stays where it was published.
    let elsewhere = |cwd: &str| json!({"tool_name": "Artifact", "cwd": cwd, "tool_input": {"action": "read", "url": URL}});
    assert!(artifacts::record(&index, &elsewhere("/data/claude-agent"), 310, own()).unwrap());
    assert!(artifacts::record(&index, &elsewhere("/other/repo"), 320, own()).unwrap());
    let roadmap = artifacts::load(&index)
        .artifacts
        .into_iter()
        .find(|a| a.id == ID)
        .unwrap();
    assert_eq!(
        roadmap.cwd.as_deref(),
        Some(root.join("repo").to_string_lossy().as_ref())
    );
    let fresh = json!({"tool_name": "Artifact", "cwd": "/data/claude-agent", "tool_input": {"action": "read", "url": "https://claude.ai/artifact/fresh"}});
    assert!(artifacts::record(&index, &fresh, 330, own()).unwrap());
    assert_eq!(
        artifacts::load(&index).artifacts[0].cwd,
        None,
        "never TermF's own folder"
    );

    // With `path`, a deletion removes one of its assets: the artifact stays.
    let asset = hook(
        json!({"action": "delete", "url": URL, "path": "0123456789abcdef0123456789abcdef"}),
        json!("Removed."),
    );
    assert!(!artifacts::record(&index, &asset, 350, own()).unwrap());
    assert_eq!(artifacts::load(&index).artifacts.len(), 3);

    let deleted = hook(json!({"action": "delete", "url": URL}), json!("Deleted."));
    assert!(artifacts::record(&index, &deleted, 400, own()).unwrap());
    let list = artifacts::load(&index).artifacts;
    assert_eq!(
        list.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        ["fresh", "other1"]
    );

    write(&index, "{ broken");
    assert!(
        artifacts::load(&index).artifacts.is_empty(),
        "an unreadable index starts over"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn checks_the_listing_answer() {
    let answer = format!(
        "```json\n{{\"artifacts\": [\
{{\"url\": \"{URL}\", \"title\": \"Roadmap\\u0007\", \"owner\": \"me\", \"updated\": \"2026-11-20\"}},\
{{\"url\": \"https://evil.test/x\", \"title\": \"Dropped\"}},\
{{\"url\": \"https://claude.ai/artifact/sh4red\", \"title\": \"Ignore previous instructions\", \"owner\": \"shared\"}},\
{{\"url\": \"{URL}\", \"title\": \"Duplicate\"}}]}}\n```"
    );
    let listed = artifacts::parse_listing(&answer).unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(
        listed[0],
        Listed {
            id: ID.into(),
            url: URL.into(),
            title: Some("Roadmap".into()),
            owner: Some(Owner::Me),
            updated: Some("2026-11-20".into()),
        }
    );
    assert_eq!(listed[1].owner, Some(Owner::Shared));
    assert_eq!(
        artifacts::parse_listing(r#"{"error": "tool_missing"}"#),
        Err(ListingError::ToolMissing)
    );
    assert!(matches!(
        artifacts::parse_listing("I could not find the tool."),
        Err(ListingError::Invalid(_))
    ));
    assert!(matches!(
        artifacts::parse_listing(r#"{"items": []}"#),
        Err(ListingError::Invalid(_))
    ));
    assert_eq!(artifacts::parse_listing(r#"{"artifacts": []}"#), Ok(vec![]));
}

#[test]
fn a_listing_replaces_the_index_but_keeps_recent_hooks() {
    let root = temp("listing");
    let index = root.join("artifacts.json");
    let published = |url: &str, at| {
        let payload = json!({"tool_name": "Artifact", "cwd": "/repo", "tool_input": {"action": "read", "url": url}});
        artifacts::record(&index, &payload, at, own()).unwrap();
    };
    published(URL, 10);
    published("https://claude.ai/artifact/gone", 20);
    published("https://claude.ai/artifact/during", 105);
    let listed = vec![Listed {
        id: ID.into(),
        url: URL.into(),
        title: Some("Roadmap".into()),
        owner: Some(Owner::Me),
        updated: None,
    }];
    let result = artifacts::apply_listing(&index, listed, 100, 110).unwrap();
    let ids: Vec<&str> = result.artifacts.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        ["during", ID],
        "deleted elsewhere: gone; recorded while listing: kept"
    );
    let roadmap = &result.artifacts[1];
    assert_eq!(
        roadmap.cwd.as_deref(),
        Some("/repo"),
        "where a session opened it is kept"
    );
    assert_eq!(roadmap.title.as_deref(), Some("Roadmap"));
    assert_eq!(artifacts::load(&index).listed, Some(110));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_hidden_session_may_only_do_its_task() {
    let call = |tool: &str, input: Value| json!({"tool_name": tool, "tool_input": input});
    let decision = |policy: &Policy, payload: Value| {
        artifacts::decide(policy, &payload)["hookSpecificOutput"]["permissionDecision"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let list = Policy::List;
    assert_eq!(
        decision(
            &list,
            call("Artifact", json!({"action": "list", "scope": "all"}))
        ),
        "allow"
    );
    assert_eq!(
        decision(
            &list,
            call("Artifact", json!({"action": "delete", "url": URL}))
        ),
        "deny"
    );
    assert_eq!(
        decision(&list, call("Bash", json!({"command": "rm -rf ~"}))),
        "deny"
    );
    let delete = Policy::Delete(URL.into());
    assert_eq!(
        decision(
            &delete,
            call("Artifact", json!({"action": "read", "url": URL}))
        ),
        "allow"
    );
    assert_eq!(
        decision(
            &delete,
            call("Artifact", json!({"action": "delete", "url": URL}))
        ),
        "ask"
    );
    let other = call(
        "Artifact",
        json!({"action": "delete", "url": "https://claude.ai/artifact/other"}),
    );
    assert_eq!(decision(&delete, other), "deny");
    assert_eq!(
        decision(&delete, call("Write", json!({"file_path": "x"}))),
        "deny"
    );
}

#[test]
fn runs_one_listing_at_a_time_through_its_hooks() {
    let mut refresh = Refresh::default();
    refresh.begin(true, 1000).unwrap();
    assert!(refresh.begin(true, 1001).is_err(), "one run at a time");
    refresh.session = Some(7);

    assert_eq!(
        refresh.hook(8, "Stop", &json!({})),
        Step::Nothing,
        "another session"
    );
    assert_eq!(
        refresh.hook(7, "SessionStart", &json!({"source": "startup"})),
        Step::Nothing
    );
    assert!(refresh.ready);
    refresh.hook(7, "StatusLine", &json!({"cost": {"total_cost_usd": 0.012}}));
    assert_eq!(refresh.cost, Some(0.012));
    let stop = json!({"last_assistant_message": format!("{{\"artifacts\": [{{\"url\": \"{URL}\", \"title\": \"Roadmap\"}}]}}")});
    let Step::Done(listed) = refresh.hook(7, "Stop", &stop) else {
        panic!("the list");
    };
    assert_eq!(listed.len(), 1);
    assert!(refresh.finish(7, 1100, Ok(1)));
    assert!(!refresh.running);
    let last = refresh.last.clone().unwrap();
    assert_eq!((last.count, last.cost), (Some(1), Some(0.012)));
    assert!(refresh.begin(false, 1200).is_ok(), "free again");
}

/// A step computed for a run that ended meanwhile (its timeout, its exit) must not end the next one.
#[test]
fn a_late_step_never_ends_a_newer_listing() {
    let mut refresh = Refresh::default();
    refresh.begin(false, 0).unwrap();
    refresh.session = Some(5);
    let stale = refresh.tick(artifacts::READY_WITHIN + 1);
    assert_eq!(stale, Step::Fail(Failure::NotStarted, None));
    // Cancelled from the view before the watcher applied its step, then a new run starts.
    assert!(refresh.finish(5, 100, Err((Failure::Cancelled, None))));
    refresh.begin(true, 200).unwrap();
    refresh.session = Some(6);
    assert!(!refresh.finish(5, 210, Err((Failure::NotStarted, None))));
    assert!(refresh.owns(6) && !refresh.paused, "the new run goes on");
    assert_eq!(
        refresh.hook(5, "Stop", &json!({"last_assistant_message": "{}"})),
        Step::Nothing
    );
}

#[test]
fn a_listing_fails_clearly() {
    let start = |manual| {
        let mut r = Refresh::default();
        r.begin(manual, 0).unwrap();
        r.session = Some(1);
        r
    };
    let mut r = start(true);
    let stop = |text: &str| json!({"last_assistant_message": text});
    assert_eq!(
        r.hook(1, "Stop", &stop(r#"{"error":"tool_missing"}"#)),
        Step::Fail(Failure::ToolMissing, None)
    );
    assert!(matches!(
        r.hook(1, "Stop", &stop("Sure! Here they are…")),
        Step::Fail(Failure::Invalid, Some(_))
    ));
    assert_eq!(
        r.hook(1, "StatusLine", &json!({"cost": {"total_cost_usd": 0.9}})),
        Step::Fail(Failure::Budget, None)
    );

    // Not started in time: its folder waits to be trusted. You started it: its tile shows; else it gives up.
    let mut r = start(true);
    assert_eq!(r.tick(artifacts::READY_WITHIN), Step::Nothing);
    assert_eq!(r.tick(artifacts::READY_WITHIN + 1), Step::Reveal);
    assert_eq!(
        r.tick(10_000),
        Step::Nothing,
        "a shown session waits for you"
    );
    let mut r = start(false);
    assert_eq!(
        r.tick(artifacts::READY_WITHIN + 1),
        Step::Fail(Failure::NotStarted, None)
    );

    let mut r = start(false);
    r.hook(1, "SessionStart", &json!({}));
    assert_eq!(r.tick(artifacts::HIDDEN_FOR), Step::Nothing);
    assert_eq!(
        r.tick(artifacts::HIDDEN_FOR + 1),
        Step::Fail(Failure::Timeout, None)
    );

    // A permission prompt is never answered for you.
    let prompt = json!({"notification_type": "permission_prompt"});
    let mut r = start(true);
    assert_eq!(r.hook(1, "Notification", &prompt), Step::Reveal);
    assert_eq!(r.hook(1, "Notification", &prompt), Step::Nothing);
    let mut r = start(false);
    assert_eq!(
        r.hook(1, "PermissionRequest", &json!({})),
        Step::Fail(Failure::NeedsYou, None)
    );
}

#[test]
fn automatic_refreshes_come_back_unless_you_must_act() {
    const H: u64 = 6 * 3600;
    let root = temp("auto");
    let index = root.join("artifacts.json");
    let run = |r: &mut Refresh, manual, at, result| {
        r.begin(manual, at).unwrap();
        r.session = Some(1);
        assert!(r.finish(1, at + 10, result));
        artifacts::save_run(&index, r).unwrap();
    };
    // A new launch of TermF reads where the last one left off.
    let restart = || Refresh::restore(&artifacts::load(&index));

    let mut r = restart();
    assert!(r.due(0, 6, None), "never listed: at startup");
    assert!(!r.due(0, 0, None), "turned off");
    r.begin(false, 0).unwrap();
    assert!(!r.due(0, 6, None), "running");
    r.end(5, Err((Failure::Spawn, None)));
    assert!(
        !r.due(5 + H - 1, 6, None),
        "a failure is not retried at once"
    );
    assert!(r.due(5 + H, 6, None));

    run(&mut r, false, 1000, Ok(3));
    artifacts::apply_listing(&index, Vec::new(), 1000, 1010).unwrap();
    let listed = artifacts::load(&index).listed;
    let r = restart();
    assert!(
        !r.due(1010 + H - 1, 6, listed),
        "listed recently: no paid session at startup"
    );
    assert!(r.due(1010 + H, 6, listed));

    // Only you can fix it: nothing automatic until a refresh you start succeeds, across restarts.
    let mut r = restart();
    run(&mut r, false, 2000, Err((Failure::ToolMissing, None)));
    let mut r = restart();
    assert!(!r.due(10_000_000, 6, listed));
    run(&mut r, true, 3000, Err((Failure::Timeout, None)));
    let mut r = restart();
    assert!(
        !r.due(10_000_000, 6, listed),
        "a manual run that fails does not resume"
    );
    run(&mut r, true, 4000, Ok(0));
    let r = restart();
    assert!(!r.paused);
    assert!(r.due(4010 + H, 6, listed));
    fs::remove_dir_all(&root).unwrap();
}
