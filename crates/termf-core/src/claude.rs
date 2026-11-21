//! Claude Code adapter: an ephemeral plugin per session whose hooks report the session state to TermF.

use std::fs;
use std::path::Path;

use anyhow::Result;
use serde_json::{Map, Value, json};

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

/// Inserts `args` right after the program word, so the rest of the command (pipes, redirections) is untouched.
pub fn after_program(command: &str, args: &str) -> String {
    let command = command.trim_start();
    let end = command.find(char::is_whitespace).unwrap_or(command.len());
    format!("{} {args}{}", &command[..end], &command[end..])
}

/// Writes a plugin (`.claude-plugin/plugin.json` + `hooks/hooks.json`) whose hooks run `<termf> hook <Event>`.
/// `guard`, for a ticket run's session: Bash commands also go through `<termf> guard`, which refuses pushes
/// and tracker writes (defense in depth: a script the agent writes can still push).
pub fn write_plugin(dir: &Path, termf: &Path, guard: bool) -> Result<()> {
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
        pre.push(json!({
            "matcher": "Bash",
            "hooks": [{ "type": "command", "command": termf, "args": ["guard"], "timeout": 5 }],
        }));
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(guard["matcher"], "Bash");
        assert_eq!(guard["hooks"][0]["command"], "/opt/Term F/termf");
        assert_eq!(guard["hooks"][0]["args"], json!(["guard"]));
        fs::remove_dir_all(&dir).unwrap();
    }
}
