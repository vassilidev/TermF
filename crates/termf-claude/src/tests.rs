use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::artifacts::{
    self, Deletion, Failure, Listed, ListingError, Outcome, Owner, Policy, Refresh, Step,
};
use crate::cli::{self, Health};
#[cfg(unix)]
use crate::manage::{self, McpAction, PluginAction};
use crate::settings::{self, Edit, Scope};
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
        home: Some(root.join("home")),
        claude: "claude".into(),
    };
    fs::create_dir_all(root.join("bin")).unwrap();
    fs::create_dir_all(root.join("repo")).unwrap();
    (root, env)
}

const TOKEN: &str = "ghp_0123456789abcdefghij0123456789abcd";

fn set(key: &str, value: Value) -> Edit {
    Edit::Set {
        key: key.into(),
        value: Some(value),
    }
}

fn unset(key: &str) -> Edit {
    Edit::Set {
        key: key.into(),
        value: None,
    }
}

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

    let preview = settings::preview(&env, Scope::User, &set("model", json!("opus"))).unwrap();
    assert_eq!(preview.diff.matches('-').count(), 1, "{}", preview.diff);
    assert!(preview.diff.contains("-    \"model\": \"sonnet\","));
    assert!(preview.diff.contains("+    \"model\": \"opus\","));
    assert!(
        fs::read_to_string(&user).unwrap().contains("sonnet"),
        "a preview writes nothing"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&user, fs::Permissions::from_mode(0o600)).unwrap();
    }
    settings::save(&env, Scope::User, &set("model", json!("opus")), None).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&user).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the file keeps its permissions");
    }
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
        &set("permissions.defaultMode", json!("plan")),
        None,
    )
    .unwrap();
    settings::save(&env, Scope::User, &unset("model"), None).unwrap();
    let value: Value = serde_json::from_str(&fs::read_to_string(&user).unwrap()).unwrap();
    assert_eq!(value["permissions"]["defaultMode"], "plan");
    assert!(value.get("model").is_none());

    // The project file and its folder are created.
    settings::save(
        &env,
        Scope::Local,
        &set("autoMemoryEnabled", json!(false)),
        None,
    )
    .unwrap();
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
        (Scope::User, "fallbackModel", json!(["a", "b", "c", "d"])),
        (Scope::User, "permissions.allow", json!(["Bassh(ls)"])),
        (Scope::User, "env.NODE ENV", json!("x")),
        (Scope::User, "env.A", json!(mask::MASK)),
        (
            Scope::Project,
            "permissions.defaultMode",
            json!("bypassPermissions"),
        ),
        (
            Scope::Project,
            "permissions",
            json!({"defaultMode": "auto"}),
        ),
        (Scope::Project, "env.GITHUB_TOKEN", json!("abc")),
        (Scope::Managed, "model", json!("opus")),
    ] {
        assert!(
            settings::preview(&env, scope, &set(key, value.clone())).is_err(),
            "{key} = {value}"
        );
    }
    // Any key, known to the schema or not, as long as it is JSON.
    for (key, value) in [
        ("apiKeyHelper", json!("~/bin/key.sh")),
        ("someFutureKey", json!({"a": [1, 2]})),
        ("permissions.defaultMode", json!("manual")),
    ] {
        settings::preview(&env, Scope::User, &set(key, value)).unwrap();
    }

    write(&root.join("repo/.claude/settings.json"), "{ broken");
    let err = settings::save(&env, Scope::Project, &set("model", json!("opus")), None).unwrap_err();
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
    settings::save(&env, Scope::User, &set("effortLevel", json!("high")), None).unwrap();
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

/// A wrapper set in `[agents]` is asked its version, and found on PATH.
#[cfg(unix)]
#[test]
fn a_wrapper_tells_its_version_and_path() {
    let (root, mut env) = tree("wrapper");
    let bin = root.join("bin");
    fake(
        &bin,
        "cc-wrap",
        r#"case "$*" in "--version") echo '2.1.330 (Claude Code)' ;; "auth status") echo '{"loggedIn": false}' ;; esac"#,
    );
    env.claude = "cc-wrap".into();
    let account = crate::cli::account(&env);
    assert_eq!(account.version.as_deref(), Some("2.1.330"));
    assert_eq!(
        account.path,
        Some(bin.join("cc-wrap").to_string_lossy().into_owned())
    );
    assert_eq!(account.binary.install.unwrap().version, None);
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
  "plugin list --json --available") echo '{{"installed":[{{"id":"hello@mkt","version":"1.2.0","scope":"user","enabled":true}}],"available":[{{"pluginId":"lint@mkt","name":"lint","marketplaceName":"mkt","source":"./lint"}}]}}' ;;
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
    assert_eq!(overview.plugins.available[0]["pluginId"], "lint@mkt");
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

    // Deleted while a listing ran: the listing, begun before, still holds it, and must not bring it back.
    let deleted = json!({"tool_name": "Artifact", "tool_input": {"action": "delete", "url": URL}});
    assert!(artifacts::record(&index, &deleted, 205, own()).unwrap());
    let listed = || {
        vec![Listed {
            id: ID.into(),
            url: URL.into(),
            title: None,
            owner: None,
            updated: None,
        }]
    };
    let result = artifacts::apply_listing(&index, listed(), 200, 210).unwrap();
    assert!(result.artifacts.iter().all(|a| a.id != ID));
    assert!(artifacts::load(&index).deleted.is_empty());
    let result = artifacts::apply_listing(&index, listed(), 300, 310).unwrap();
    assert!(
        result.artifacts.iter().any(|a| a.id == ID),
        "a later listing is the truth again"
    );
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
    // You confirmed these two in TermF: their deletion goes ahead, nothing else does.
    let delete = Policy::Delete(vec![ID.into(), "abc".into()]);
    for url in [URL, "https://claude.ai/artifact/abc?x=1"] {
        assert_eq!(
            decision(
                &delete,
                call("Artifact", json!({"action": "delete", "url": url}))
            ),
            "allow",
            "{url}"
        );
    }
    for refused in [
        call(
            "Artifact",
            json!({"action": "delete", "url": "https://claude.ai/artifact/other"}),
        ),
        call(
            "Artifact",
            json!({"action": "delete", "url": format!("{URL}x")}),
        ),
        call("Artifact", json!({"action": "delete"})),
        call(
            "Artifact",
            json!({"action": "delete", "url": URL, "path": "0123456789abcdef0123456789abcdef"}),
        ),
        call("Artifact", json!({"action": "read", "url": URL})),
        call("Artifact", json!({"action": "publish", "url": URL})),
        call("Artifact", json!({"action": "list"})),
        call("Bash", json!({"command": "rm -rf ~"})),
        call("Write", json!({"file_path": "x"})),
        call("mcp__x__Artifact", json!({"action": "delete", "url": URL})),
    ] {
        assert_eq!(decision(&delete, refused.clone()), "deny", "{refused}");
    }
}

fn deleting(urls: &[&str]) -> Deletion {
    let mut d = Deletion::default();
    let urls: Vec<String> = urls.iter().map(|u| u.to_string()).collect();
    d.begin(&urls, 1000).unwrap();
    d.session = Some(4);
    d
}

const OTHER: &str = "https://claude.ai/code/artifact/other-1";

#[test]
fn deletes_several_artifacts_in_one_hidden_session() {
    let mut d = deleting(&[URL, OTHER, URL, "https://evil.test/artifact/x"]);
    assert_eq!(
        d.items.len(),
        2,
        "the same one twice, and a foreign link, dropped"
    );
    assert!(Deletion::default().begin(&[], 0).is_err());
    assert!(
        d.begin(&[URL.into()], 1001).is_err(),
        "one deletion at a time"
    );

    let done = |url: &str| json!({"tool_name": "Artifact", "tool_input": {"action": "delete", "url": url}, "tool_response": "deleted"});
    assert_eq!(d.hook(9, "PostToolUse", &done(URL)), Step::Nothing);
    assert_eq!(d.items[0].outcome, Outcome::Pending, "another session");
    assert_eq!(d.hook(4, "PostToolUse", &done(URL)), Step::Nothing);
    assert_eq!(d.items[0].outcome, Outcome::Deleted);

    let answer = json!({"last_assistant_message": format!(
        "{{\"deleted\": [\"{URL}\", \"{OTHER}\"], \"failed\": []}}"
    )});
    assert_eq!(d.hook(4, "Stop", &answer), Step::Answered);
    assert_eq!(
        d.items[1].outcome,
        Outcome::Failed,
        "said deleted, but no call went through"
    );
    assert!(d.finish(4, None));
    assert!(!d.running && d.failure.is_none());
    assert!(!d.finish(4, Some((Failure::Closed, None))), "already over");
}

#[test]
fn a_deletion_says_why_it_failed() {
    let mut d = deleting(&[URL, OTHER]);
    let answer = json!({"last_assistant_message": format!(
        "{{\"deleted\": [], \"failed\": [{{\"url\": \"{OTHER}\", \"error\": \"not the owner\"}}]}}"
    )});
    d.hook(4, "Stop", &answer);
    assert_eq!(d.items[1].error.as_deref(), Some("not the owner"));
    assert_eq!(d.items[0].outcome, Outcome::Failed);
    assert!(d.failure.is_none());

    let mut d = deleting(&[URL]);
    let stop = |text: &str| json!({"last_assistant_message": text});
    assert_eq!(
        d.hook(4, "Stop", &stop(r#"{"error": "tool_missing"}"#)),
        Step::Fail(Failure::ToolMissing, None)
    );
    // Claude asks instead of deleting: its tile shows and waits for your answer, then for its JSON.
    let mut d = deleting(&[URL]);
    assert_eq!(
        d.hook(4, "Stop", &stop("Are you sure? {maybe}")),
        Step::Reveal
    );
    assert_eq!(d.hook(4, "Stop", &stop("Which one?")), Step::Nothing);
    assert!(d.running && d.items[0].outcome == Outcome::Pending);
    assert_eq!(d.tick(1_000_000), Step::Nothing);
    assert_eq!(
        d.hook(4, "Stop", &stop(r#"{"deleted": [], "failed": []}"#)),
        Step::Answered
    );
    let mut d = deleting(&[URL]);
    d.hook(
        4,
        "PostToolUse",
        &json!({"tool_name": "Artifact", "tool_input": {"action": "delete", "url": URL}}),
    );
    assert_eq!(
        d.hook(4, "Stop", &stop("Done.")),
        Step::Answered,
        "nothing left to do"
    );

    // Waiting for you (its folder to trust, a permission): its tile shows, once, and waits as long as it takes.
    let mut d = deleting(&[URL]);
    assert_eq!(d.tick(1000 + artifacts::READY_WITHIN), Step::Nothing);
    assert_eq!(d.tick(1001 + artifacts::READY_WITHIN), Step::Reveal);
    assert_eq!(d.tick(1_000_000), Step::Nothing);
    let mut d = deleting(&[URL]);
    let prompt = json!({"notification_type": "permission_prompt"});
    assert_eq!(d.hook(4, "Notification", &prompt), Step::Reveal);
    assert_eq!(d.hook(4, "PermissionRequest", &json!({})), Step::Nothing);

    let mut d = deleting(&[URL]);
    d.hook(4, "SessionStart", &json!({}));
    assert_eq!(
        d.tick(1001 + d.limits().0),
        Step::Fail(Failure::Timeout, None)
    );
    assert!(d.finish(4, Some((Failure::Timeout, None))));
    assert_eq!(d.items[0].outcome, Outcome::Failed);
    assert_eq!(d.failure, Some(Failure::Timeout));
}

#[test]
fn a_bigger_deletion_gets_more_time_and_money_up_to_a_cap() {
    let urls: Vec<String> = (0..150)
        .map(|i| format!("https://claude.ai/code/artifact/a{i}"))
        .collect();
    let mut d = Deletion::default();
    d.begin(&urls, 0).unwrap();
    d.session = Some(4);
    d.hook(4, "SessionStart", &json!({}));
    let (secs, usd) = d.limits();
    assert_eq!(secs, artifacts::HIDDEN_FOR + 150 * artifacts::DELETE_SECS);
    assert_eq!(d.tick(artifacts::HIDDEN_FOR + 1), Step::Nothing);
    assert_eq!(d.tick(secs + 1), Step::Fail(Failure::Timeout, None));
    let status = |c: f64| json!({"cost": {"total_cost_usd": c}});
    assert_eq!(
        d.hook(4, "StatusLine", &status(artifacts::MAX_COST_USD + 1.0)),
        Step::Nothing
    );
    assert_eq!(
        d.hook(4, "StatusLine", &status(usd + 0.01)),
        Step::Fail(Failure::Budget, None)
    );

    let many: Vec<String> = (0..1000)
        .map(|i| format!("https://claude.ai/code/artifact/a{i}"))
        .collect();
    let mut huge = Deletion::default();
    huge.begin(&many, 0).unwrap();
    assert!(huge.limits().0 < 3600 && huge.limits().1 < 10.0, "capped");
}

/// A fake `claude` plays the hidden session: it deletes every link of its prompt, then tries one more. Each call
/// goes through TermF's `PreToolUse` answer as in Claude Code; only allowed ones reach `PostToolUse`.
#[cfg(unix)]
#[test]
fn a_fake_claude_deletes_only_what_you_confirmed() {
    let root = temp("fake-delete");
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fake(
        &bin,
        "claude",
        r#"for url in $(printf '%s\n' "$1" | grep -o 'https://claude.ai/[A-Za-z0-9/_-]*'); do
  printf '{"action":"delete","url":"%s"}\n' "$url"
done
printf '{"action":"delete","url":"https://claude.ai/code/artifact/not-confirmed"}\n'
printf '{"answer":"{\\"deleted\\": [], \\"failed\\": []}"}\n'"#,
    );
    let mut d = deleting(&[URL, OTHER]);
    let policy = Policy::Delete(d.items.iter().map(|i| i.id.clone()).collect());
    let urls: Vec<String> = d.items.iter().map(|i| i.url.clone()).collect();
    let out = std::process::Command::new(bin.join("claude"))
        .arg(artifacts::delete_prompt(&urls))
        .output()
        .unwrap();
    let (mut refused, mut answer) = (Vec::new(), None);
    for line in String::from_utf8(out.stdout).unwrap().lines() {
        let call: Value = serde_json::from_str(line).unwrap();
        if let Some(text) = call["answer"].as_str() {
            answer = Some(text.to_owned());
            continue;
        }
        let payload = json!({"tool_name": "Artifact", "tool_input": call});
        let decision = artifacts::decide(&policy, &payload);
        if decision["hookSpecificOutput"]["permissionDecision"] == "allow" {
            d.hook(4, "PostToolUse", &payload);
        } else {
            refused.push(call["url"].as_str().unwrap().to_owned());
        }
    }
    assert_eq!(refused, ["https://claude.ai/code/artifact/not-confirmed"]);
    let stop = json!({"last_assistant_message": answer.unwrap()});
    assert_eq!(d.hook(4, "Stop", &stop), Step::Answered);
    assert!(d.items.iter().all(|i| i.outcome == Outcome::Deleted));
    assert!(d.finish(4, None) && d.failure.is_none());
    fs::remove_dir_all(&root).unwrap();
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

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn edits_lists_and_hooks_item_by_item() {
    let (root, env) = tree("lists");
    let user = env.claude_dir.join("settings.json");
    write(
        &user,
        "{\n\t\"model\": \"sonnet\",\n\t\"permissions\": {\n\t\t\"allow\": [\"Bash(npm test)\", \"Read\"]\n\t}\n}\n",
    );
    let save = |edit: Edit| settings::save(&env, Scope::User, &edit, None).unwrap();

    let p = save(Edit::Add {
        key: "permissions.deny".into(),
        value: json!("Bash(git push *)"),
    });
    assert!(p.diff.contains("+\t\t\"deny\": ["), "{}", p.diff);
    save(Edit::Move {
        key: "permissions.allow".into(),
        index: 1,
        to: "permissions.ask".into(),
        value: Some(json!("Read")),
    });
    let v = read_json(&user);
    assert_eq!(v["permissions"]["allow"], json!(["Bash(npm test)"]));
    assert_eq!(v["permissions"]["ask"], json!(["Read"]));
    assert_eq!(v["permissions"]["deny"], json!(["Bash(git push *)"]));
    assert!(
        fs::read_to_string(&user).unwrap().contains("\n\t\"model\""),
        "tabs kept"
    );

    let twice = settings::preview(
        &env,
        Scope::User,
        &Edit::Add {
            key: "permissions.deny".into(),
            value: json!("Bash(git push *)"),
        },
    );
    assert!(twice.is_err(), "already there");
    let wrong = settings::preview(
        &env,
        Scope::User,
        &Edit::Drop {
            key: "permissions.allow".into(),
            index: 5,
            value: None,
        },
    );
    assert!(wrong.unwrap_err().to_string().contains("refresh"));

    // A list emptied by a removal goes, and so does its object.
    save(Edit::Drop {
        key: "permissions.allow".into(),
        index: 0,
        value: None,
    });
    save(Edit::Drop {
        key: "permissions.ask".into(),
        index: 0,
        value: None,
    });
    save(Edit::Drop {
        key: "permissions.deny".into(),
        index: 0,
        value: None,
    });
    assert_eq!(read_json(&user), json!({"model": "sonnet"}));

    save(set("env.NODE_ENV", json!("production")));
    save(set("env.API_TOKEN", json!(TOKEN)));
    assert_eq!(
        settings::reveal(&env, Scope::User, "env.API_TOKEN").unwrap(),
        TOKEN
    );
    assert_eq!(
        settings::reveal(&env, Scope::User, "env.MISSING").unwrap(),
        Value::Null
    );

    let hook = |matcher: Option<&str>, command: &str| Edit::AddHook {
        event: "PostToolUse".into(),
        matcher: matcher.map(Into::into),
        command: command.into(),
    };
    save(hook(Some("Edit|Write"), "prettier --write"));
    save(hook(Some("Edit|Write"), "eslint --fix"));
    save(hook(None, "echo done"));
    let v = read_json(&user);
    let groups = v["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(groups.len(), 2, "the same matcher shares its group");
    assert_eq!(groups[0]["hooks"][1]["command"], "eslint --fix");
    assert!(groups[1].get("matcher").is_none());

    let s = settings::read(&env);
    let hooks = crate::hooks(&s);
    let eslint = hooks.iter().find(|h| h.run == "eslint --fix").unwrap();
    assert_eq!((eslint.group, eslint.index), (0, 1));

    for (group, index) in [(0, 1), (1, 0), (0, 0)] {
        save(Edit::DropHook {
            event: "PostToolUse".into(),
            group,
            index,
            run: None,
        });
    }
    assert!(read_json(&user).get("hooks").is_none(), "nothing left");

    for bad in [
        hook(None, " "),
        Edit::AddHook {
            event: "Post Tool".into(),
            matcher: None,
            command: "x".into(),
        },
        Edit::DropHook {
            event: "Stop".into(),
            group: 0,
            index: 0,
            run: None,
        },
    ] {
        assert!(
            settings::preview(&env, Scope::User, &bad).is_err(),
            "{bad:?}"
        );
    }
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn refuses_secrets_in_the_shared_project_file() {
    let (root, env) = tree("shared");
    let hook = Edit::AddHook {
        event: "Stop".into(),
        matcher: None,
        command: "curl -H 'Authorization: Bearer abc' https://x.test".into(),
    };
    let rule = Edit::Add {
        key: "permissions.allow".into(),
        value: json!(format!(
            "Bash(git clone https://{TOKEN}@github.com/acme/api)"
        )),
    };
    for edit in [&hook, &rule, &set("env.ANTHROPIC_API_KEY", json!("x"))] {
        let err = settings::preview(&env, Scope::Project, edit).unwrap_err();
        assert!(err.to_string().contains("local scope"), "{err}");
        settings::preview(&env, Scope::Local, edit).unwrap();
    }
    settings::preview(&env, Scope::Project, &set("env.NODE_ENV", json!("test"))).unwrap();
    fs::remove_dir_all(&root).unwrap();
}

/// A hook already in the shared file with a secret is the user's to move out; it does not block other hooks,
/// and neither do references (`${VAR}`), while a literal default or `$Password` is a secret.
#[test]
fn only_what_an_edit_brings_in_is_checked_for_secrets() {
    let (root, env) = tree("added-hook");
    write(
        &root.join("repo/.claude/settings.json"),
        &json!({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": format!("curl -H 'Authorization: Bearer {TOKEN}' https://x.test")}
        ]}]}})
        .to_string(),
    );
    let hook = |command: &str| Edit::AddHook {
        event: "PreToolUse".into(),
        matcher: Some("Bash".into()),
        command: command.into(),
    };
    settings::save(&env, Scope::Project, &hook("./scripts/guard.sh"), None).unwrap();
    let v = read_json(&root.join("repo/.claude/settings.json"));
    assert_eq!(
        v["hooks"]["PreToolUse"][0]["hooks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for value in ["${API_KEY}", "${API_KEY:-}", "$API_KEY"] {
        settings::preview(&env, Scope::Project, &set("env.API_KEY", json!(value))).unwrap();
    }
    for value in ["${API_KEY:-sk-ant-api03-abcdefghijklmnop}", "$Up3rS3cret"] {
        assert!(
            settings::preview(&env, Scope::Project, &set("env.API_KEY", json!(value))).is_err()
        );
    }
    fs::remove_dir_all(&root).unwrap();
}

/// Saving writes what the preview showed: a file changed since, or an item that is no longer where the page saw
/// it, is refused.
#[test]
fn saves_only_what_the_preview_showed() {
    let (root, env) = tree("stale");
    let user = env.claude_dir.join("settings.json");
    write(
        &user,
        &json!({"permissions": {"allow": ["Bash(ls)", "Bash(rm -rf *)"]}, "env": {"A": "1"},
            "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify --token abc"}]}]}})
        .to_string(),
    );
    let drop = |index, value: &str| Edit::Drop {
        key: "permissions.allow".into(),
        index,
        value: Some(json!(value)),
    };
    let p = settings::preview(&env, Scope::User, &drop(0, "Bash(ls)")).unwrap();
    // Someone else edits the file between the preview and the save.
    write(
        &user,
        &json!({"permissions": {"allow": ["Bash(rm -rf *)", "Bash(ls)"]}, "env": {"A": "1"}})
            .to_string(),
    );
    let err = settings::save(&env, Scope::User, &drop(0, "Bash(ls)"), Some(&p.hash)).unwrap_err();
    assert!(
        err.to_string().contains("changed since its preview"),
        "{err}"
    );
    assert!(
        settings::preview(&env, Scope::User, &drop(0, "Bash(ls)")).is_err(),
        "not at 0 any more"
    );
    let p = settings::preview(&env, Scope::User, &drop(1, "Bash(ls)")).unwrap();
    settings::save(&env, Scope::User, &drop(1, "Bash(ls)"), Some(&p.hash)).unwrap();
    assert_eq!(
        read_json(&user)["permissions"]["allow"],
        json!(["Bash(rm -rf *)"])
    );

    write(
        &user,
        &json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "notify --token abc"}]}]}})
            .to_string(),
    );
    let drop_hook = |run: &str| Edit::DropHook {
        event: "Stop".into(),
        group: 0,
        index: 0,
        run: Some(run.into()),
    };
    assert!(settings::preview(&env, Scope::User, &drop_hook("other.sh")).is_err());
    let shown = format!("notify --token {}", mask::MASK);
    settings::save(&env, Scope::User, &drop_hook(&shown), None).unwrap();
    assert_eq!(read_json(&user), json!({}));
    fs::remove_dir_all(&root).unwrap();
}

/// Removing the last key of an object removes the object too, as removing from a list does.
#[test]
fn removing_a_key_leaves_no_empty_object() {
    let (root, env) = tree("prune-set");
    let user = env.claude_dir.join("settings.json");
    write(
        &user,
        &json!({"model": "opus", "env": {"A": "1"}, "permissions": {"defaultMode": "plan"}})
            .to_string(),
    );
    settings::save(&env, Scope::User, &unset("env.A"), None).unwrap();
    settings::save(&env, Scope::User, &unset("permissions.defaultMode"), None).unwrap();
    assert_eq!(read_json(&user), json!({"model": "opus"}));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn checks_permission_rules_as_documented() {
    for (list, rule) in [
        ("allow", "Bash"),
        ("allow", "Bash(npm run *)"),
        ("allow", "Bash(ls:*)"),
        ("allow", "Read(./.env)"),
        ("allow", "Edit(//Users/alex/**)"),
        ("allow", "WebFetch(domain:example.com)"),
        ("allow", "Agent(Explore)"),
        ("allow", "mcp__github"),
        ("allow", "mcp__github__search_repositories"),
        ("allow", "mcp__github__*"),
        ("allow", "mcp__plugin_my-plugin_db__get_*"),
        ("deny", "*"),
        ("deny", "mcp__*"),
        ("ask", "Agent(model:opus)"),
        ("deny", "Bash(echo (x))"),
    ] {
        settings::rule(list, rule).unwrap_or_else(|e| panic!("{list} {rule}: {e}"));
    }
    for (list, rule) in [
        ("allow", ""),
        ("allow", " Bash"),
        ("allow", "Bassh"),
        ("allow", "Bash()"),
        ("allow", "Bash(ls"),
        ("allow", "Bash(ls)x"),
        ("allow", "Bash ls"),
        ("allow", "mcp__github__search(q:x)"),
        ("allow", "mcp__"),
        ("allow", "mcp__*"),
        ("allow", "*"),
        ("allow", "B*"),
    ] {
        assert!(settings::rule(list, rule).is_err(), "{list} {rule:?}");
    }
}

#[test]
fn checks_values_against_the_embedded_schema() {
    use crate::schema;
    assert!(schema::tools().iter().any(|t| t == "Bash"));
    let errors = |key: &str, value: Value| {
        let mut e = Vec::new();
        schema::check(schema::at(key).unwrap(), &value, key, &mut e);
        e
    };
    assert!(errors("effortLevel", json!("high")).is_empty());
    assert!(!errors("effortLevel", json!("max")).is_empty());
    assert!(!errors("cleanupPeriodDays", json!(0)).is_empty());
    assert!(!errors("cleanupPeriodDays", json!("30")).is_empty());
    assert!(errors("enabledPlugins", json!({"a@b": true})).is_empty());
    assert!(!errors("enabledPlugins", json!({"a@b": "yes"})).is_empty());
    let hook = schema::item("hooks.Stop").unwrap();
    let mut e = Vec::new();
    schema::check(
        hook,
        &json!({"hooks": [{"type": "command"}]}),
        "hooks.Stop",
        &mut e,
    );
    assert!(e.iter().any(|e| e.contains("command")), "{e:?}");
    assert!(schema::at("noSuchKey").is_none());
}

/// A fake `claude` that logs its arguments and stdin, and answers like Claude Code 2.1.330.
#[cfg(unix)]
fn fake_claude(root: &Path) -> PathBuf {
    let log = root.join("claude.log");
    fake(
        &root.join("bin"),
        "claude",
        &format!(
            r#"echo "ARGS $*" >> '{log}'
case "$*" in
  "mcp add-json --scope"*) printf 'STDIN ' >> '{log}'; /bin/cat >> '{log}'; echo >> '{log}'; echo "Added stdio MCP server $5 to $4 config" ;;
  "mcp remove ghost"*) echo 'No MCP server found with name: "ghost"' >&2; exit 1 ;;
  "mcp remove"*|"mcp logout"*|"mcp reset-project-choices") echo done ;;
  "plugin install --json --scope user risky@mkt") echo 'npx risky-installer'; echo '{{"command":"install","outcome":"failed","message":"The marketplace declares a command; it was only displayed","shownCommand":{{"command":"npx risky-installer","plugin":"risky@mkt","sha256":"{sha}"}}}}'; exit 1 ;;
  "plugin install --json --scope user risky@mkt --accept-command {sha}") echo '{{"command":"install","outcome":"ok","message":"Successfully installed plugin: risky@mkt (scope: user)"}}' ;;
  "plugin enable --json hello@mkt") echo '{{"command":"enable","outcome":"failed","message":"Plugin \"hello\" is already enabled","failureCode":"already_in_goal_state","alreadyInGoalState":true}}'; exit 1 ;;
  "plugin uninstall --json --scope user nope@mkt") echo '{{"command":"uninstall","outcome":"failed","message":"Failed to uninstall plugin \"nope@mkt\": not installed"}}'; exit 1 ;;
  "plugin marketplace remove --json mkt") echo '{{"command":"marketplace-remove","outcome":"ok","marketplace":"mkt","message":"Successfully removed marketplace: mkt"}}' ;;
  *) echo "unexpected: $*" >&2; exit 2 ;;
esac"#,
            log = log.display(),
            sha = "a".repeat(64),
        ),
    );
    log
}

#[cfg(unix)]
#[test]
fn changes_mcp_servers_only_through_claude_mcp() {
    let (root, env) = tree("mcp");
    let log = fake_claude(&root);
    let server = json!({"type": "stdio", "command": "npx", "args": ["-y", "srv"], "env": {"SRV_API_KEY": "s3cret"}});
    let message = manage::mcp(
        &env,
        &McpAction::Add {
            name: "srv".into(),
            scope: "local".into(),
            server: server.clone(),
        },
    )
    .unwrap();
    assert_eq!(message, "Added stdio MCP server srv to local config");
    let calls = fs::read_to_string(&log).unwrap();
    assert!(
        calls.contains("ARGS mcp add-json --scope local srv -\n"),
        "{calls}"
    );
    assert!(
        calls.contains(&format!("STDIN {server}")),
        "the server, with its secret, goes on stdin only: {calls}"
    );
    assert_eq!(calls.matches("s3cret").count(), 1);

    let refused = [
        McpAction::Add {
            name: "srv".into(),
            scope: "project".into(),
            server: server.clone(),
        },
        McpAction::Add {
            name: "-x".into(),
            scope: "user".into(),
            server: server.clone(),
        },
        McpAction::Add {
            name: "web".into(),
            scope: "user".into(),
            server: json!({"type": "http", "url": "ftp://x"}),
        },
        McpAction::Add {
            name: "web".into(),
            scope: "user".into(),
            server: json!({"type": "http", "url": "https://x.test", "headers": {"Authorization": mask::MASK}}),
        },
        McpAction::Add {
            name: "srv".into(),
            scope: "global".into(),
            server: server.clone(),
        },
        McpAction::Remove {
            name: "--all".into(),
            scope: None,
        },
    ];
    let before = fs::read_to_string(&log).unwrap();
    for action in &refused {
        assert!(manage::mcp(&env, action).is_err(), "{action:?}");
    }
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        before,
        "claude never ran"
    );

    for action in [
        McpAction::Remove {
            name: "srv".into(),
            scope: Some("local".into()),
        },
        McpAction::Logout { name: "srv".into() },
        McpAction::Reset,
    ] {
        manage::mcp(&env, &action).unwrap();
    }
    let calls = fs::read_to_string(&log).unwrap();
    for line in [
        "ARGS mcp remove srv --scope local",
        "ARGS mcp logout srv",
        "ARGS mcp reset-project-choices",
    ] {
        assert!(calls.contains(line), "{line}: {calls}");
    }
    let err = manage::mcp(
        &env,
        &McpAction::Remove {
            name: "ghost".into(),
            scope: None,
        },
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("No MCP server found"),
        "as Claude Code said it: {err}"
    );

    let no_repo = Env { repo: None, ..env };
    assert!(manage::mcp(&no_repo, &McpAction::Reset).is_err());
    fs::remove_dir_all(&root).unwrap();
}

#[cfg(unix)]
#[test]
fn installs_a_plugin_only_once_its_command_is_accepted() {
    let (root, env) = tree("plugins");
    let log = fake_claude(&root);
    let install = |accept: Option<String>| PluginAction::Install {
        plugin: "risky@mkt".into(),
        scope: "user".into(),
        accept,
    };
    let shown = manage::plugin(&env, &install(None)).unwrap();
    let sha = "a".repeat(64);
    assert_eq!(shown.shown.as_ref().unwrap()["sha256"], sha.as_str());
    assert_eq!(
        shown.shown.as_ref().unwrap()["command"],
        "npx risky-installer"
    );
    let done = manage::plugin(&env, &install(Some(sha))).unwrap();
    assert!(done.shown.is_none());
    assert!(done.message.contains("installed"));
    assert!(manage::plugin(&env, &install(Some("not-a-sha".into()))).is_err());

    let already = manage::plugin(
        &env,
        &PluginAction::Enable {
            plugin: "hello@mkt".into(),
            scope: None,
        },
    )
    .unwrap();
    assert!(already.message.contains("already enabled"));
    let err = manage::plugin(
        &env,
        &PluginAction::Uninstall {
            plugin: "nope@mkt".into(),
            scope: "user".into(),
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("not installed"), "{err}");
    manage::plugin(
        &env,
        &PluginAction::MarketplaceRemove {
            name: "mkt".into(),
            scope: None,
        },
    )
    .unwrap();
    let calls = fs::read_to_string(&log).unwrap();
    assert!(
        !calls.contains(" -y") && !calls.contains("--yes"),
        "never answers for you: {calls}"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[cfg(unix)]
#[test]
fn edits_the_repository_mcp_json_after_a_diff() {
    let (root, env) = tree("mcpjson");
    let file = root.join("repo/.mcp.json");
    let (_, text) = manage::mcp_json(&env).unwrap();
    assert_eq!(text, "", "absent");
    write(
        &file,
        "{\n  \"mcpServers\": {\n    \"old\": {\"command\": \"srv\", \"env\": {\"OLD_TOKEN\": \"kept\"}}\n  }\n}\n",
    );
    let next = "{\n  \"mcpServers\": {\n    \"old\": {\"command\": \"srv\", \"env\": {\"OLD_TOKEN\": \"kept\"}},\n    \"web\": {\"type\": \"http\", \"url\": \"https://x.test/mcp\", \"headers\": {\"Authorization\": \"Bearer ${API_KEY}\"}}\n  }\n}";
    let p = manage::mcp_json_preview(&env, next).unwrap();
    assert!(p.diff.contains("+    \"web\""), "{}", p.diff);
    assert!(fs::read_to_string(&file).unwrap().contains("kept"));
    manage::mcp_json_save(&env, next, Some(&p.hash)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), format!("{next}\n"));
    let stale = manage::mcp_json_save(&env, "{}", Some(&p.hash)).unwrap_err();
    assert!(
        stale.to_string().contains("changed since its preview"),
        "{stale}"
    );

    for bad in [
        "{ nope",
        "[]",
        "{\"mcpServers\": {\"x\": {\"type\": \"http\"}}}",
        "{\"mcpServers\": {\"x\": {\"command\": \"srv\", \"env\": {\"API_TOKEN\": \"abc\"}}}}",
    ] {
        assert!(manage::mcp_json_preview(&env, bad).is_err(), "{bad}");
    }
    fs::remove_dir_all(&root).unwrap();
}
