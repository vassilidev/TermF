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
    "SessionEnd",
];

/// Whether an agent command runs Claude Code, e.g. `claude` or `/usr/local/bin/claude --model opus`.
pub fn is_claude(command: &str) -> bool {
    command
        .split_whitespace()
        .next()
        .and_then(|program| Path::new(program).file_stem())
        .is_some_and(|stem| stem == "claude")
}

/// Writes a plugin (`.claude-plugin/plugin.json` + `hooks/hooks.json`) whose hooks run `<termf> hook <Event>`.
pub fn write_plugin(dir: &Path, termf: &Path) -> Result<()> {
    let termf = termf.to_string_lossy();
    let hooks: Map<String, Value> = HOOK_EVENTS
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
    fn writes_a_hooks_plugin() {
        let dir = std::env::temp_dir().join(format!("termf-plugin-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let termf = Path::new("/opt/Term F/termf");
        write_plugin(&dir, termf).unwrap();

        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(dir.join(".claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "termf");

        let hooks: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("hooks/hooks.json")).unwrap())
                .unwrap();
        for event in HOOK_EVENTS {
            let hook = &hooks["hooks"][event][0]["hooks"][0];
            assert_eq!(hook["type"], "command");
            assert_eq!(hook["command"], "/opt/Term F/termf");
            assert_eq!(hook["args"], json!(["hook", event]));
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
