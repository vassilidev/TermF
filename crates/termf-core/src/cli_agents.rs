//! Codex CLI and Gemini CLI as TermF starts them, in a terminal or headless (docs/AGENTS-CLI.md): what reports
//! their state, the library's instructions, and the secrets they are kept from. Nothing is written in the user's
//! own files: Codex takes `-c` overrides, Gemini a system settings file of TermF's and a `--policy` file.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{Value, json};

use crate::claude::Tool;

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

/// The most characters of instructions a Codex terminal's command line carries: cmd.exe takes 8191 in all.
const INSTRUCTIONS_MAX: usize = if cfg!(windows) { 5000 } else { 100_000 };

/// `-c developer_instructions=…` for a Codex terminal (Codex has no option that reads them from a file, only one that
/// replaces its own instructions): Codex parses the value as TOML, whose basic strings take JSON's escapes. Too
/// long for a command line, the text is cut at a line and says so; the second value is how many characters went.
pub fn codex_instructions(text: &str) -> (String, usize) {
    let (text, cut) = match text.char_indices().nth(INSTRUCTIONS_MAX) {
        Some((end, _)) => {
            let end = text[..end].rfind('\n').unwrap_or(end);
            let kept = format!(
                "{}\n\n[TermF cut the rest of these instructions to fit the command line.]",
                &text[..end]
            );
            (kept, text[end..].chars().count())
        }
        None => (text.to_owned(), 0),
    };
    let value = serde_json::to_string(&text).unwrap_or_default();
    let arg = format!(
        "developer_instructions={}",
        value.replace('\u{7f}', "\\u007f")
    );
    (arg, cut)
}

/// `-c notify=[…]`: Codex runs `<termf> hook agent-turn-complete`, then your own `notify` with the same JSON, read
/// from `$CODEX_HOME/config.toml` (the `profile` given, else the file's own, first; Codex ignores a project's
/// `notify`). The second value says why yours cannot follow, when it cannot.
pub fn codex_notify(
    termf: &Path,
    codex_home: &Path,
    profile: Option<&str>,
) -> (String, Option<String>) {
    let mut notify = vec![
        termf.to_string_lossy().into_owned(),
        "hook".into(),
        CODEX_TURN_DONE.into(),
    ];
    let file = codex_home.join("config.toml");
    let mut warning = None;
    match fs::read_to_string(&file).map(|text| toml::from_str::<toml::Table>(&text)) {
        Err(_) => {}
        Ok(Err(_)) => {
            warning = Some(format!(
                "{} cannot be read: your own Codex notify does not run in this terminal",
                file.display()
            ));
        }
        Ok(Ok(config)) => {
            let profile = profile.or(config.get("profile").and_then(toml::Value::as_str));
            let in_profile = profile
                .and_then(|p| config.get("profiles")?.get(p)?.get("notify"))
                .cloned();
            let yours = in_profile.or_else(|| config.get("notify").cloned());
            let words = yours.map(|v| {
                let words = v.as_array()?.iter().map(|w| w.as_str().map(str::to_owned));
                words.collect::<Option<Vec<String>>>()
            });
            match words {
                None => {}
                Some(Some(words)) => notify.extend(words),
                Some(None) => {
                    warning = Some(format!(
                        "the notify of {} is not a list of words: it does not run in this terminal",
                        file.display()
                    ));
                }
            }
        }
    }
    (format!("notify={}", json!(notify)), warning)
}

/// `$CODEX_HOME` as the login shell sets it, as a terminal's Codex sees it, else `~/.codex`.
pub fn codex_home() -> PathBuf {
    crate::shell::login_var("CODEX_HOME")
        .or_else(|| std::env::var_os("CODEX_HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::expand_tilde("~/.codex"))
}

/// The profile an `[agents]` command gives Codex (`-p`, `--profile`).
fn codex_profile(command: &str) -> Option<&str> {
    let mut words = command.split_whitespace();
    while let Some(word) = words.next() {
        if let Some(name) = word.strip_prefix("--profile=") {
            return Some(name);
        }
        if word == "-p" || word == "--profile" {
            return words.next();
        }
    }
    None
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

/// The commands [`crate::claude::SECRET_DENY`] refuses, as Gemini CLI's `commandRegex`: a bare dump of the
/// environment (`env`, `printenv`, `export`, `set`, `declare -p|-x`), `cat` of a file whose name starts with `.env`,
/// and any command that names TermF's tokens. `export NODE_ENV=…` or `set -e` stay allowed.
pub const GEMINI_DENIED_COMMANDS: &str = r"^(env|printenv|export|set)$|^(env|printenv)\s|(^|\s)\S*/printenv(\s|$)|^export -p|^declare -[px]|^cat\s(.*\s)?(\S*/)?\.env\S*|dev\.termf\.cli|DEV\.TERMF\.CLI|TERMF_MCP_TOKEN";
/// The files it refuses to read, as `argsPattern` over a tool's JSON arguments: a name that starts with `.env` or
/// `id_`, ends with `.pem`, a process's `environ`, TermF's token folder.
pub const GEMINI_DENIED_FILES: &str =
    r#"[/"]\.env[^/"]*"|\.pem"|[/"]id_[^/"]*"|/proc/[^"]*/environ|dev\.termf\.cli|DEV\.TERMF\.CLI"#;

/// [`crate::claude::SECRET_DENY`] for Gemini CLI, as rules of its policy engine (`--policy`), in `dir`: secret files
/// are not read, the environment is not dumped, TermF's tokens are not named. Defense in depth, as for Claude Code.
/// The same for every session, so one file, replaced whole.
pub fn write_gemini_policy(dir: &Path) -> Result<PathBuf> {
    let mut rules = String::from("# TermF: what no Gemini CLI session it starts may do.\n");
    for body in [
        format!("toolName = \"run_shell_command\"\ncommandRegex = '{GEMINI_DENIED_COMMANDS}'"),
        format!(
            "toolName = [\"read_file\", \"read_many_files\"]\nargsPattern = '{GEMINI_DENIED_FILES}'"
        ),
    ] {
        rules.push_str(&format!(
            "\n[[rule]]\n{body}\ndecision = \"deny\"\npriority = 999\ndenyMessage = \"TermF keeps secrets out of reach\"\n"
        ));
    }
    fs::create_dir_all(dir)?;
    let file = dir.join("gemini-policy.toml");
    let tmp = dir.join(format!(".gemini-policy.{}.tmp", std::process::id()));
    fs::write(&tmp, rules)?;
    fs::rename(&tmp, &file)?;
    Ok(file)
}

/// What a Codex CLI or Gemini CLI terminal starts with, besides its program.
#[derive(Debug, Default, PartialEq)]
pub struct Tile {
    /// Options right after the program, unquoted.
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Said in the terminal before the agent starts.
    pub warnings: Vec<String>,
    /// Older than TermF needs: run as it is, without TermF's status, instructions or rules.
    pub old: bool,
}

/// [`Tile`] of `command` for `tool`. `dir`: the session's folder, none without the local API; `termf`: the CLI its
/// status hooks run; `prompt`: the library's instructions; `data_dir`: where the secret rules go without a folder.
pub fn tile(
    tool: Tool,
    command: &str,
    dir: Option<&Path>,
    termf: Option<&Path>,
    prompt: Option<&Path>,
    data_dir: &Path,
) -> Result<Tile> {
    let mut tile = Tile::default();
    let program = command.split_whitespace().next().unwrap_or(tool.program());
    if let Err(e) = crate::claude::binary(program) {
        tile.old = true;
        tile.warnings.push(format!(
            "TermF: {e:#}. This terminal runs it without TermF's status, instructions or secret rules."
        ));
        return Ok(tile);
    }
    let text = prompt
        .and_then(|p| fs::read_to_string(p).ok())
        .filter(|t| !t.trim().is_empty());
    match tool {
        Tool::Codex => {
            if let Some(termf) = termf {
                let (notify, warning) = codex_notify(termf, &codex_home(), codex_profile(command));
                tile.args.extend(["-c".into(), notify]);
                tile.warnings
                    .extend(warning.map(|w| format!("TermF: {w}.")));
            }
            if let Some(text) = text {
                let (instructions, cut) = codex_instructions(&text);
                tile.args.extend(["-c".into(), instructions]);
                if cut > 0 {
                    tile.warnings.push(format!(
                        "TermF: the library's instructions lost their last {cut} characters to fit the command line."
                    ));
                }
            }
        }
        Tool::Gemini => {
            if let Some(dir) = dir {
                let settings = gemini_settings(dir, termf, prompt, None)?;
                tile.env.push((
                    GEMINI_SETTINGS_ENV.into(),
                    settings.to_string_lossy().into_owned(),
                ));
            }
            let policy = write_gemini_policy(dir.unwrap_or(data_dir))?;
            tile.args
                .extend(["--policy".into(), policy.to_string_lossy().into_owned()]);
        }
        Tool::Claude => {}
    }
    Ok(tile)
}

/// `command` once `lines` are printed, by the shell `termf_term::shell_command` runs it with.
pub fn after_lines(command: &str, lines: &[String], quote: impl Fn(&str) -> String) -> String {
    if lines.is_empty() {
        return command.to_owned();
    }
    if cfg!(windows) {
        // cmd.exe: `^` escapes what `echo` would read as its own.
        let escape = |l: &str| {
            l.chars().fold(String::new(), |mut out, c| {
                if "&|<>^%\"".contains(c) {
                    out.push('^');
                }
                out.push(c);
                out
            })
        };
        let echoes: Vec<String> = lines
            .iter()
            .map(|l| format!("echo {}", escape(l)))
            .collect();
        format!("{}& {command}", echoes.join("& "))
    } else {
        let quoted: Vec<String> = lines.iter().map(|l| quote(l)).collect();
        format!("printf '%s\\n\\n' {}; {command}", quoted.join(" "))
    }
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

    fn notify_words(arg: &str) -> Vec<String> {
        let parsed: toml::Table = toml::from_str(arg).unwrap();
        let words = parsed["notify"].as_array().unwrap().iter();
        words.map(|v| v.as_str().unwrap().to_owned()).collect()
    }

    #[test]
    fn codex_overrides_are_toml() {
        let text = "Be \"brief\".\nUse\ttabs \\ and é\u{7f}";
        let (arg, cut) = codex_instructions(text);
        let parsed: toml::Table = toml::from_str(&arg).unwrap();
        assert_eq!(parsed["developer_instructions"].as_str(), Some(text));
        assert_eq!(cut, 0);

        let long = format!("{}\nlast line", "x".repeat(INSTRUCTIONS_MAX));
        let (arg, cut) = codex_instructions(&format!("first line\n{long}"));
        let parsed: toml::Table = toml::from_str(&arg).unwrap();
        let kept = parsed["developer_instructions"].as_str().unwrap();
        assert!(
            kept.starts_with("first line\n\n[TermF cut"),
            "cut at a line"
        );
        assert_eq!(cut, long.chars().count() + 1);

        let home = temp("codex");
        let termf = Path::new("/opt/Term F/termf");
        let (mine, warning) = codex_notify(termf, &home, None);
        assert_eq!(
            notify_words(&mine),
            ["/opt/Term F/termf", "hook", CODEX_TURN_DONE]
        );
        assert_eq!(warning, None);
        fs::write(
            home.join("config.toml"),
            "model = \"o9\"\nnotify = [\"python3\", \"/home/alex/notify.py\"]\n\
             [profiles.work]\nnotify = [\"say\", \"done\"]\n",
        )
        .unwrap();
        let (chained, _) = codex_notify(termf, &home, None);
        assert_eq!(
            notify_words(&chained)[3..],
            ["python3", "/home/alex/notify.py"],
            "yours still runs"
        );
        let (work, _) = codex_notify(termf, &home, codex_profile("codex -p work --yolo"));
        assert_eq!(notify_words(&work)[3..], ["say", "done"], "the profile's");
        fs::write(
            home.join("config.toml"),
            "profile = \"work\"\n[profiles.work]\nnotify = [\"say\"]\n",
        )
        .unwrap();
        assert_eq!(
            notify_words(&codex_notify(termf, &home, None).0)[3..],
            ["say"]
        );
        assert_eq!(codex_profile("codex --profile=ci"), Some("ci"));
        fs::write(home.join("config.toml"), "notify = \"say done\"\n").unwrap();
        let (alone, warning) = codex_notify(termf, &home, None);
        assert_eq!(notify_words(&alone).len(), 3);
        assert!(warning.unwrap().contains("not a list of words"));
        fs::write(home.join("config.toml"), "notify = [").unwrap();
        assert!(
            codex_notify(termf, &home, None)
                .1
                .unwrap()
                .contains("cannot be read")
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
        let termf = quote(Path::new("/opt/termf"));
        assert_eq!(
            before[1]["hooks"][0]["command"].as_str().unwrap(),
            format!("{termf} hook BeforeTool")
        );
        let start = &settings["hooks"]["SessionStart"][0]["hooks"][0]["command"];
        assert_eq!(
            start.as_str().unwrap(),
            format!("{termf} hook SessionStart {}", quote(&prompt))
        );
        #[cfg(unix)]
        assert_eq!(termf, "'/opt/termf'");
        assert_eq!(settings["mcpServers"]["termf"]["args"], json!(["mcp"]));
        let context = gemini_context(&prompt).unwrap();
        assert_eq!(
            context["hookSpecificOutput"]["additionalContext"],
            "## Role: front"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    /// Gemini CLI reads the rules as JavaScript regular expressions; these use nothing the two read differently.
    #[test]
    fn gemini_policy_refuses_what_secret_deny_does_and_no_more() {
        let dir = temp("policy");
        let policy = fs::read_to_string(write_gemini_policy(&dir).unwrap()).unwrap();
        let parsed: toml::Table = toml::from_str(&policy).unwrap();
        let rules = parsed["rule"].as_array().unwrap();
        assert!(rules.iter().all(|r| r["decision"].as_str() == Some("deny")));
        let commands = regex::Regex::new(rules[0]["commandRegex"].as_str().unwrap()).unwrap();
        for denied in [
            "env",
            "env FOO=1 make",
            "printenv",
            "printenv HOME",
            "/usr/bin/printenv",
            "export",
            "export -p",
            "set",
            "declare -p",
            "declare -x",
            "cat .env",
            "cat -n src/.env.local",
            "echo $TERMF_MCP_TOKEN",
            "cat ~/Library/Application\\ Support/dev.termf.cli/token",
        ] {
            assert!(commands.is_match(denied), "{denied}");
        }
        for allowed in [
            "export NODE_ENV=production",
            "set -e",
            "cat config.environment.ts",
            "grep -r printenv src/",
            "environment-check",
            "cargo test",
        ] {
            assert!(!commands.is_match(allowed), "{allowed}");
        }
        let files = regex::Regex::new(rules[1]["argsPattern"].as_str().unwrap()).unwrap();
        let args = |path: &str| json!({ "file_path": path }).to_string();
        for denied in [
            "/repo/.env",
            ".env.local",
            "/home/alex/.ssh/id_ed25519",
            "certs/server.pem",
            "/proc/1/environ",
        ] {
            assert!(files.is_match(&args(denied)), "{denied}");
        }
        for allowed in [
            "src/config.environment.ts",
            "src/identity.rs",
            "docs/env.md",
            "a.env.md",
        ] {
            assert!(!files.is_match(&args(allowed)), "{allowed}");
        }
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "no temporary file left"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_terminal_without_termf_or_too_old_still_starts() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("tile");
        let fake = |name: &str, version: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let data = dir.join("data");
        let gemini = fake("gemini", "0.70.0");
        let plain = tile(
            Tool::Gemini,
            &gemini.to_string_lossy(),
            None,
            None,
            None,
            &data,
        )
        .unwrap();
        let policy = data.join("gemini-policy.toml");
        assert_eq!(
            plain.args,
            ["--policy".to_owned(), policy.to_string_lossy().into_owned()]
        );
        assert!(
            policy.is_file(),
            "the secret rules even without the local API"
        );
        assert!(!plain.old && plain.warnings.is_empty() && plain.env.is_empty());

        let codex = fake("codex", "codex-cli 0.90.0");
        let prompt = dir.join("prompt.md");
        fs::write(&prompt, "## Role: docs").unwrap();
        let old = tile(
            Tool::Codex,
            &codex.to_string_lossy(),
            None,
            None,
            Some(&prompt),
            &data,
        )
        .unwrap();
        assert!(old.old && old.args.is_empty(), "run as it is");
        assert!(
            old.warnings[0].contains("Codex CLI 0.90.0 at")
                && old.warnings[0].contains("without TermF's status"),
            "{:?}",
            old.warnings
        );
        let codex = fake("codex", "codex-cli 0.171.0");
        let recent = tile(
            Tool::Codex,
            &codex.to_string_lossy(),
            None,
            None,
            Some(&prompt),
            &data,
        )
        .unwrap();
        assert_eq!(
            recent.args,
            [
                "-c".to_owned(),
                "developer_instructions=\"## Role: docs\"".into()
            ]
        );

        let quote = |s: &str| format!("'{s}'");
        assert_eq!(after_lines("codex", &[], quote), "codex");
        assert_eq!(
            after_lines("codex -c x", &["TermF: too old.".into()], quote),
            "printf '%s\\n\\n' 'TermF: too old.'; codex -c x"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
