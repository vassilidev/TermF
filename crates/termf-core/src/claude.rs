//! Claude Code adapter: an ephemeral plugin per session whose hooks report the session state to TermF.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

mod binary;
pub use binary::*;

/// The events `ipc::hook_status` understands.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// Whether an agent command runs Claude Code, e.g. `claude` or `/usr/local/bin/claude --model opus`.
/// Only a command that starts with the `claude` program gets status hooks and Remote Control.
pub fn is_claude(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .and_then(|program| Path::new(program).file_stem())
        .is_some_and(|stem| stem == "claude")
}

/// What a Claude Code session gives the programs it runs (seen in 2.1.330's environment). A Claude Code started
/// under them is a child of that session: it reports to it and saves no transcript ("Transcript saving is off —
/// inherited CLAUDE_CODE_CHILD_SESSION marker"), so `--resume` and a hand-back lose its conversation. TermF may itself
/// run under one (started from it); what TermF starts never gets them.
pub const SESSION_MARKERS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_CHILD_NOTES_DIR",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
];

/// Inserts `args` right after the program word, so the rest of the command (pipes, redirections) is untouched.
pub fn after_program(command: &str, args: &str) -> String {
    let command = command.trim_start();
    let end = command.find(char::is_whitespace).unwrap_or(command.len());
    format!("{} {args}{}", &command[..end], &command[end..])
}

/// Reads a `claude -p --output-format json|stream-json --json-schema …` run of `claude`: the structured answer and
/// what it cost.
pub fn structured_output(
    claude: &Install,
    status: ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(Value, Option<f64>)> {
    let text = String::from_utf8_lossy(stdout);
    // Its answer is the `result` line, whatever a wrapper or the stream printed before: a run that died after its
    // `init` has none, and says why on stderr.
    let Some(result) = text.lines().rev().find_map(|l| {
        serde_json::from_str::<Value>(l.trim())
            .ok()
            .filter(|v| v["type"] == "result")
    }) else {
        bail!("{}", failure(claude, status, stderr));
    };
    let cost = result["total_cost_usd"].as_f64();
    match result.get("structured_output") {
        Some(answer) if result["is_error"] != true && answer.is_object() => {
            Ok((answer.clone(), cost))
        }
        // A run stopped by `--max-budget-usd` only has its `subtype` to say so.
        _ => bail!(
            "Claude Code: {}",
            result["result"]
                .as_str()
                .or(result["subtype"].as_str())
                .unwrap_or("no structured answer")
        ),
    }
}

/// Writes a plugin (`.claude-plugin/plugin.json` + `hooks/hooks.json`) whose hooks run `<termf> hook <Event>`.
/// `guard`, for a ticket run's session: Bash commands also go through `<termf> guard`, which refuses pushes
/// and tracker writes (defense in depth: a script the agent writes can still push).
pub fn write_plugin(dir: &Path, termf: &Path, guard: bool) -> Result<()> {
    let path = termf;
    let termf = termf.to_string_lossy();
    let mut hooks: Map<String, Value> = HOOK_EVENTS
        .iter()
        .map(|event| {
            // Exec form (`args`): no shell, so the path needs no quoting.
            let hook = json!({
                "type": "command",
                "command": termf,
                "args": ["hook", event],
                "timeout": 5,
            });
            (event.to_string(), json!([{ "hooks": [hook] }]))
        })
        .collect();
    if guard && let Some(Value::Array(pre)) = hooks.get_mut("PreToolUse") {
        pre.push(guard_hook(path));
    }
    fs::create_dir_all(dir.join(".claude-plugin"))?;
    fs::create_dir_all(dir.join("hooks"))?;
    fs::write(
        dir.join(".claude-plugin/plugin.json"),
        serde_json::to_string_pretty(&json!({
            "name": "termf",
            "description": "Reports this session's state to TermF",
        }))?,
    )?;
    fs::write(
        dir.join("hooks/hooks.json"),
        serde_json::to_string_pretty(&json!({ "hooks": hooks }))?,
    )?;
    Ok(())
}

/// The `PreToolUse` hook that sends each Bash command through `<termf> guard`, and the tools that reach out of the
/// machine (the web, other MCP servers), which it lets through unless the session read prod (`taint`).
fn guard_hook(termf: &Path) -> Value {
    json!({
        "matcher": GUARDED,
        "hooks": [{ "type": "command", "command": termf.to_string_lossy(), "args": ["guard"], "timeout": 5 }],
    })
}

const GUARDED: &str = "Bash|WebFetch|WebSearch|mcp__.*";

/// Settings with only the guard hook of a ticket run's tile, for a headless session's `--settings`.
pub fn guard_settings(termf: &Path) -> Value {
    json!({ "hooks": { "PreToolUse": [guard_hook(termf)] } })
}

/// Denied to every Claude Code session TermF starts (ARCHITECTURE §12.2): secret files under its folder and
/// dumps of its environment. Defense in depth, not a guarantee: a rule does not see a program called by its
/// path elsewhere, inside `sh -c`, or a script the agent writes. Deny rules of every settings scope add up, so
/// the user's own still hold.
pub const SECRET_DENY: &[&str] = &[
    "Read(**/.env*)",
    "Read(**/*.pem)",
    "Read(**/id_*)",
    "Read(//proc/**/environ)",
    "Bash(printenv)",
    "Bash(printenv *)",
    "Bash(*/printenv*)",
    "Bash(env)",
    "Bash(env *)",
    "Bash(export)",
    "Bash(export -p*)",
    "Bash(set)",
    "Bash(declare -p*)",
    "Bash(declare -x*)",
    "Bash(cat *.env*)",
    // The user's token (`ipc::USER_TOKEN_DIR`), on any drive, and the session's own `termf mcp` token
    // (`ipc::ENV_MCP_TOKEN`), which the commands it runs inherit. File systems that ignore case: a Read rule
    // checks the resolved path (any case on macOS, and `cat`, `head`… with it), Bash rules have no pattern for
    // case, so only these two spellings; PowerShell rules ignore case.
    "Read(//**/dev.termf.cli/**)",
    "Read(//**/DEV.TERMF.CLI/**)",
    "Bash(*dev.termf.cli*)",
    "Bash(*DEV.TERMF.CLI*)",
    "PowerShell(*dev.termf.cli*)",
    "Bash(*TERMF_MCP_TOKEN*)",
    "PowerShell(*TERMF_MCP_TOKEN*)",
];

/// Settings holding only [`SECRET_DENY`], for a session without TermF's hooks. The same for every session, so
/// one file, replaced whole.
pub fn write_deny_settings(dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let file = dir.join("claude-deny.json");
    let tmp = dir.join(format!(".claude-deny.{}.tmp", std::process::id()));
    fs::write(
        &tmp,
        serde_json::to_string_pretty(&json!({ "permissions": { "deny": SECRET_DENY } }))?,
    )?;
    fs::rename(&tmp, &file)?;
    Ok(file)
}

/// Adds `more` to session settings: objects merged key by key, lists joined, as Claude Code merges its scopes.
pub fn merge_settings(base: &mut Value, more: &Value) {
    match (base, more) {
        (Value::Object(base), Value::Object(more)) => {
            for (key, value) in more {
                match base.get_mut(key) {
                    Some(mine) => merge_settings(mine, value),
                    None => {
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (Value::Array(base), Value::Array(more)) => base.extend(more.iter().cloned()),
        (base, more) => *base = more.clone(),
    }
}

/// Writes the file given to `--settings`: a status line running `<termf> statusline`, which reports the
/// context to TermF, then runs the status line you set yourself, if any, saved next to it; and [`SECRET_DENY`].
pub fn write_settings(dir: &Path, termf: &Path, cwd: &Path, claude_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut status_line = user_status_line(cwd, claude_dir)
        .filter(|s| s["type"] == "command" && s["command"].is_string())
        .unwrap_or_else(|| json!({ "type": "command" }));
    let mut command = format!("{} statusline", quote(termf));
    if let Some(yours) = status_line["command"].as_str() {
        let chained = dir.join("statusline");
        fs::write(&chained, yours)?;
        command = format!("{command} {}", quote(&chained));
    }
    status_line["command"] = command.into();
    let file = dir.join("settings.json");
    fs::write(
        &file,
        serde_json::to_string_pretty(&json!({
            "statusLine": status_line,
            "permissions": { "deny": SECRET_DENY },
        }))?,
    )?;
    Ok(file)
}

/// The status line set in the repository's local settings, else its shared ones, else yours.
fn user_status_line(cwd: &Path, claude_dir: &Path) -> Option<Value> {
    [
        cwd.join(".claude/settings.local.json"),
        cwd.join(".claude/settings.json"),
        claude_dir.join("settings.json"),
    ]
    .iter()
    .find_map(|file| {
        let settings: Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
        settings.get("statusLine").cloned()
    })
}

/// One word for the shell Claude Code runs a status line in: sh, or Git Bash on Windows.
fn quote(path: &Path) -> String {
    let path = path.to_string_lossy();
    if cfg!(windows) {
        format!("\"{}\"", path.replace('\\', "/"))
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn sets_a_status_line_that_chains_yours() {
        let root = std::env::temp_dir().join(format!("termf-settings-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (dir, repo, claude_dir) = (root.join("s"), root.join("repo"), root.join("home"));
        fs::create_dir_all(repo.join(".claude")).unwrap();
        fs::create_dir_all(&claude_dir).unwrap();
        let termf = Path::new("/opt/Term F/termf");
        let status_line = |file: &Path| -> Value {
            let settings: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
            settings["statusLine"].clone()
        };

        let file = write_settings(&dir, termf, &repo, &claude_dir).unwrap();
        assert_eq!(
            status_line(&file),
            json!({"type": "command", "command": "'/opt/Term F/termf' statusline"})
        );
        assert!(!dir.join("statusline").exists());

        fs::write(
            claude_dir.join("settings.json"),
            r#"{"statusLine": {"type": "command", "command": "~/mine.sh", "padding": 2}}"#,
        )
        .unwrap();
        fs::write(
            repo.join(".claude/settings.json"),
            r#"{"statusLine": {"type": "command", "command": "jq -r .model.id"}}"#,
        )
        .unwrap();
        let file = write_settings(&dir, termf, &repo, &claude_dir).unwrap();
        let chained = dir.join("statusline");
        assert_eq!(
            status_line(&file)["command"],
            format!("'/opt/Term F/termf' statusline '{}'", chained.display())
        );
        assert_eq!(
            fs::read_to_string(&chained).unwrap(),
            "jq -r .model.id",
            "the project's first"
        );

        fs::remove_file(repo.join(".claude/settings.json")).unwrap();
        let file = write_settings(&dir, termf, &repo, &claude_dir).unwrap();
        assert_eq!(status_line(&file)["padding"], 2, "your other fields kept");
        assert_eq!(fs::read_to_string(&chained).unwrap(), "~/mine.sh");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn denies_secret_files_and_keeps_a_tasks_own_rules() {
        let root = std::env::temp_dir().join(format!("termf-deny-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file = write_settings(
            &root.join("s"),
            Path::new("termf"),
            &root.join("repo"),
            &root.join("home"),
        )
        .unwrap();
        let read = |file: PathBuf| -> Value {
            serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap()
        };
        let mut settings = read(file);
        let denied = settings["permissions"]["deny"].clone();
        assert_eq!(denied, json!(SECRET_DENY));
        for rule in [
            "Read(**/.env*)",
            "Read(**/*.pem)",
            "Read(**/id_*)",
            "Bash(env)",
        ] {
            assert!(denied.as_array().unwrap().contains(&json!(rule)), "{rule}");
        }
        assert!(
            !SECRET_DENY.iter().any(|r| r.contains("*printenv*")),
            "`grep -r printenv src/` stays allowed"
        );
        let (token, mcp) = (crate::ipc::USER_TOKEN_DIR, crate::ipc::ENV_MCP_TOKEN);
        for rule in [
            format!("Read(//**/{token}/**)"),
            format!("Bash(*{token}*)"),
            format!("Bash(*{mcp}*)"),
        ] {
            assert!(denied.as_array().unwrap().contains(&json!(rule)), "{rule}");
        }
        assert!(settings["statusLine"]["command"].is_string());

        merge_settings(
            &mut settings,
            &json!({"permissions": {"deny": ["Bash", "Edit"], "defaultMode": "plan"}, "model": "haiku"}),
        );
        let deny = settings["permissions"]["deny"].as_array().unwrap();
        assert_eq!(deny.len(), SECRET_DENY.len() + 2);
        assert_eq!(deny[SECRET_DENY.len()], "Bash");
        assert_eq!(settings["permissions"]["defaultMode"], "plan");
        assert_eq!(settings["model"], "haiku");
        assert!(settings["statusLine"]["command"].is_string());

        let alone = read(write_deny_settings(&root.join("data")).unwrap());
        assert_eq!(alone, json!({"permissions": {"deny": SECRET_DENY}}));
        assert_eq!(
            fs::read_dir(root.join("data")).unwrap().count(),
            1,
            "no temporary file left"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn recognizes_claude_commands() {
        assert!(is_claude("claude"));
        assert!(is_claude("claude --model opus"));
        assert!(is_claude("/usr/local/bin/claude"));
        assert!(!is_claude("codex"));
        assert!(!is_claude("claude-dev"));
        assert!(!is_claude(""));
    }

    #[test]
    fn inserts_args_after_the_program() {
        assert_eq!(after_program("claude", "--x 'a b'"), "claude --x 'a b'");
        assert_eq!(
            after_program("  claude --model opus | tee log", "--x"),
            "claude --x --model opus | tee log"
        );
        assert_eq!(
            after_program("/opt/claude\t-c", "--x"),
            "/opt/claude --x\t-c"
        );
    }

    #[test]
    fn writes_a_hooks_plugin() {
        let dir = std::env::temp_dir().join(format!("termf-plugin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let termf = Path::new("/opt/Term F/termf");
        write_plugin(&dir, termf, false).unwrap();

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(dir.join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "termf");

        let read = || -> Value {
            serde_json::from_str(&fs::read_to_string(dir.join("hooks/hooks.json")).unwrap())
                .unwrap()
        };
        let hooks = read();
        for event in HOOK_EVENTS {
            let hook = &hooks["hooks"][event][0]["hooks"][0];
            assert_eq!(hook["type"], "command");
            assert_eq!(hook["command"], "/opt/Term F/termf");
            assert_eq!(hook["args"], json!(["hook", event]));
        }
        assert_eq!(hooks["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);

        write_plugin(&dir, termf, true).unwrap();
        let guard = &read()["hooks"]["PreToolUse"][1];
        assert_eq!(guard["matcher"], "Bash|WebFetch|WebSearch|mcp__.*");
        assert_eq!(guard["hooks"][0]["command"], "/opt/Term F/termf");
        assert_eq!(guard["hooks"][0]["args"], json!(["guard"]));
        assert_eq!(
            &guard_settings(termf)["hooks"]["PreToolUse"][0],
            guard,
            "a headless session gets the tile's hook"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
