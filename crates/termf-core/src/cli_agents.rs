//! Codex CLI and Gemini CLI as TermF starts them, in a terminal or headless (docs/AGENTS-CLI.md): what reports
//! their state, the library's instructions, and the secrets they are kept from. Nothing is written in the user's
//! own files: Codex takes `-c` overrides, Gemini a system settings file of TermF's and a `--policy` file.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

/// The event `termf hook` gets from Codex's `notify`, the only one it sends: a turn ended.
pub const CODEX_TURN_DONE: &str = "agent-turn-complete";

/// The Gemini CLI hooks TermF reads a status from (`ipc::hook_status`).
pub const GEMINI_HOOKS: &[&str] = &[
    "SessionStart",
    "BeforeAgent",
    "BeforeTool",
    "AfterTool",
    "Notification",
    "AfterAgent",
    "SessionEnd",
];

/// Where Gemini CLI reads the system settings file TermF stands in for, with the ones already there merged in.
pub const GEMINI_SETTINGS_ENV: &str = "GEMINI_CLI_SYSTEM_SETTINGS_PATH";

/// `-c developer_instructions=…`: Codex parses the value as TOML, whose basic strings take JSON's escapes.
pub fn codex_instructions(text: &str) -> String {
    let value = serde_json::to_string(text).unwrap_or_default();
    format!(
        "developer_instructions={}",
        value.replace('\u{7f}', "\\u007f")
    )
}

/// `-c notify=[…]`: Codex runs `<termf> hook agent-turn-complete`, then your own `notify`, read from
/// `$CODEX_HOME/config.toml`, with the same JSON.
pub fn codex_notify(termf: &Path, codex_home: &Path) -> String {
    let mut notify = vec![
        termf.to_string_lossy().into_owned(),
        "hook".into(),
        CODEX_TURN_DONE.into(),
    ];
    let yours = fs::read_to_string(codex_home.join("config.toml"))
        .ok()
        .and_then(|text| toml::from_str::<toml::Table>(&text).ok())
        .and_then(|t| t.get("notify")?.as_array().cloned())
        .unwrap_or_default();
    notify.extend(yours.iter().filter_map(|v| v.as_str().map(str::to_owned)));
    format!("notify={}", json!(notify))
}

/// `$CODEX_HOME`, else `~/.codex`.
pub fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::expand_tilde("~/.codex"))
}

/// `-c` overrides that give Codex a `termf mcp` server (`command`, `args`, `env` as `Agents::serve_mcp` makes it).
/// The token never goes on the command line: Codex passes the variables `env_vars` names from its own
/// environment.
pub fn codex_mcp(server: &Value, token_var: &str) -> Vec<String> {
    let mut args = Vec::new();
    for key in ["command", "args"] {
        args.push(format!("mcp_servers.termf.{key}={}", server[key]));
    }
    for (name, value) in server["env"].as_object().into_iter().flatten() {
        if name != token_var {
            args.push(format!("mcp_servers.termf.env.{name}={value}"));
        }
    }
    args.push(format!("mcp_servers.termf.env_vars={}", json!([token_var])));
    args
}

/// Writes the Gemini CLI system settings file of a session in `dir`: the one already set for the machine, if any,
/// with TermF's status hooks (`termf`) and its `termf mcp` server (`mcp`) added. `prompt`: the library's
/// instructions, which its `SessionStart` hook hands to Gemini.
pub fn gemini_settings(
    dir: &Path,
    termf: Option<&Path>,
    prompt: Option<&Path>,
    mcp: Option<&Value>,
) -> Result<PathBuf> {
    let current = std::env::var_os(GEMINI_SETTINGS_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(gemini_system_settings);
    let mut settings = fs::read_to_string(&current)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let mut more = json!({});
    if let Some(termf) = termf {
        let hooks: serde_json::Map<String, Value> = GEMINI_HOOKS
            .iter()
            .map(|event| {
                let mut command = format!("{} hook {event}", quote(termf));
                if let Some(prompt) = prompt.filter(|_| *event == "SessionStart") {
                    command = format!("{command} {}", quote(prompt));
                }
                let hook = json!({ "type": "command", "command": command, "timeout": 5000 });
                (event.to_string(), json!([{ "hooks": [hook] }]))
            })
            .collect();
        more["hooks"] = hooks.into();
    }
    if let Some(server) = mcp {
        more["mcpServers"] = json!({ "termf": server });
    }
    crate::claude::merge_settings(&mut settings, &more);
    fs::create_dir_all(dir)?;
    let file = dir.join("gemini-settings.json");
    fs::write(&file, serde_json::to_string_pretty(&settings)?)?;
    Ok(file)
}

/// Gemini CLI's own place for the machine's settings, from its configuration reference.
fn gemini_system_settings() -> PathBuf {
    PathBuf::from(if cfg!(target_os = "macos") {
        "/Library/Application Support/GeminiCli/settings.json"
    } else if cfg!(windows) {
        r"C:\ProgramData\gemini-cli\settings.json"
    } else {
        "/etc/gemini-cli/settings.json"
    })
}

/// What `termf hook SessionStart <file>` prints for Gemini: the instructions as context of the session.
pub fn gemini_context(prompt: &Path) -> Option<Value> {
    let text = fs::read_to_string(prompt).ok()?;
    Some(json!({ "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": text,
    } }))
}

/// [`crate::claude::SECRET_DENY`] for Gemini CLI, as rules of its policy engine (`--policy`): secret files are
/// not read, the environment is not dumped, TermF's tokens are not named. Defense in depth, as for Claude Code.
pub fn write_gemini_policy(dir: &Path) -> Result<PathBuf> {
    let mut rules = String::from("# TermF: what no Gemini CLI session it starts may do.\n");
    let rule = |rules: &mut String, body: &str| {
        rules.push_str(&format!(
            "\n[[rule]]\n{body}\ndecision = \"deny\"\npriority = 999\ndenyMessage = \"TermF keeps secrets out of reach\"\n"
        ));
    };
    for prefix in [
        "printenv",
        "env",
        "export",
        "set",
        "declare -p",
        "declare -x",
    ] {
        rule(
            &mut rules,
            &format!("toolName = \"run_shell_command\"\ncommandPrefix = \"{prefix}\""),
        );
    }
    let named = r"dev\.termf\.cli|DEV\.TERMF\.CLI|TERMF_MCP_TOKEN";
    rule(
        &mut rules,
        &format!("toolName = \"run_shell_command\"\ncommandRegex = '{named}|cat .*\\.env'"),
    );
    rule(
        &mut rules,
        &format!(
            "toolName = [\"read_file\", \"read_many_files\"]\nargsPattern = '\\.env|\\.pem|[/\"]id_|/proc/[^\"]*/environ|{named}'"
        ),
    );
    fs::create_dir_all(dir)?;
    let file = dir.join("gemini-policy.toml");
    fs::write(&file, rules)?;
    Ok(file)
}

/// One word for the shell Gemini CLI runs a hook command in.
fn quote(path: &Path) -> String {
    let path = path.to_string_lossy();
    if cfg!(windows) {
        format!("\"{path}\"")
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("termf-cli-agents-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn codex_overrides_are_toml() {
        let text = "Be \"brief\".\nUse\ttabs \\ and é\u{7f}";
        let arg = codex_instructions(text);
        let parsed: toml::Table = toml::from_str(&arg).unwrap();
        assert_eq!(parsed["developer_instructions"].as_str(), Some(text));

        let home = temp("codex");
        let termf = Path::new("/opt/Term F/termf");
        let mine: toml::Table = toml::from_str(&codex_notify(termf, &home)).unwrap();
        assert_eq!(
            mine["notify"],
            toml::Value::Array(vec![
                "/opt/Term F/termf".into(),
                "hook".into(),
                CODEX_TURN_DONE.into()
            ])
        );
        fs::write(
            home.join("config.toml"),
            "model = \"o9\"\nnotify = [\"python3\", \"/home/alex/notify.py\"]\n",
        )
        .unwrap();
        let chained: toml::Table = toml::from_str(&codex_notify(termf, &home)).unwrap();
        let chained: Vec<&str> = chained["notify"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(
            &chained[3..],
            ["python3", "/home/alex/notify.py"],
            "yours still runs"
        );

        let server = json!({"command": "/opt/termf", "args": ["mcp"], "env": {"TERMF_SOCKET": "/s", "TERMF_MCP_TOKEN": "${TERMF_MCP_TOKEN}"}});
        let args = codex_mcp(&server, "TERMF_MCP_TOKEN");
        let mut config = toml::Table::new();
        for arg in &args {
            assert!(!arg.contains("${"), "no token on the command line: {arg}");
            let (key, value) = arg.split_once('=').unwrap();
            let value: toml::Table = toml::from_str(&format!("v = {value}")).unwrap();
            config.insert(key.into(), value["v"].clone());
        }
        assert_eq!(
            config["mcp_servers.termf.command"].as_str(),
            Some("/opt/termf")
        );
        assert_eq!(
            config["mcp_servers.termf.env_vars"],
            toml::Value::Array(vec!["TERMF_MCP_TOKEN".into()])
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn gemini_settings_keep_the_machines_and_add_the_hooks() {
        let dir = temp("gemini");
        let system = dir.join("system.json");
        fs::write(&system, r#"{"general": {"vimMode": true}, "hooks": {"BeforeTool": [{"hooks": [{"type": "command", "command": "audit"}]}]}}"#).unwrap();
        // SAFETY: only this test reads the variable.
        unsafe { std::env::set_var(GEMINI_SETTINGS_ENV, &system) };
        let prompt = dir.join("system-prompt.md");
        fs::write(&prompt, "## Role: front").unwrap();
        let file = gemini_settings(
            &dir.join("s"),
            Some(Path::new("/opt/termf")),
            Some(&prompt),
            Some(&json!({"command": "/opt/termf", "args": ["mcp"]})),
        )
        .unwrap();
        unsafe { std::env::remove_var(GEMINI_SETTINGS_ENV) };
        let settings: Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(settings["general"]["vimMode"], true);
        let before = settings["hooks"]["BeforeTool"].as_array().unwrap();
        assert_eq!(before.len(), 2, "the machine's hook stays");
        assert_eq!(
            before[1]["hooks"][0]["command"],
            "'/opt/termf' hook BeforeTool"
        );
        let start = &settings["hooks"]["SessionStart"][0]["hooks"][0]["command"];
        assert_eq!(
            start.as_str().unwrap(),
            format!("'/opt/termf' hook SessionStart '{}'", prompt.display())
        );
        assert_eq!(settings["mcpServers"]["termf"]["args"], json!(["mcp"]));
        let context = gemini_context(&prompt).unwrap();
        assert_eq!(
            context["hookSpecificOutput"]["additionalContext"],
            "## Role: front"
        );

        let policy = fs::read_to_string(write_gemini_policy(&dir).unwrap()).unwrap();
        let parsed: toml::Table = toml::from_str(&policy).unwrap();
        let rules = parsed["rule"].as_array().unwrap();
        assert!(rules.iter().all(|r| r["decision"].as_str() == Some("deny")));
        assert!(
            rules
                .iter()
                .any(|r| r.get("commandPrefix").and_then(|p| p.as_str()) == Some("printenv"))
        );
        assert!(policy.contains("TERMF_MCP_TOKEN") && policy.contains("\\.env"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
