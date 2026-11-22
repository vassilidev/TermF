//! Remote Control (ARCHITECTURE §6.6): which sessions get it by default, the name they show on the phone, and the
//! link to one on claude.ai.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, value};

/// Set by Claude Code in its hooks' environment while Remote Control is connected: the remote session's id.
pub const ENV_BRIDGE: &str = "CLAUDE_CODE_BRIDGE_SESSION_ID";
/// Where `termf hook` puts that id in what it forwards to TermF.
pub const PAYLOAD_BRIDGE: &str = "termf_bridge";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Off,
    /// The session starts without it and its tile asks.
    Ask,
    On,
}

/// A tile's Remote Control, as the UI shows it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// `<project>/<ticket>/<role>`, the name on the phone.
    pub name: String,
    /// The user wants it on.
    pub on: bool,
    /// The session's page on claude.ai, once Claude Code says it is connected.
    pub url: Option<String>,
    /// The change waits for the end of the agent's turn.
    pub pending: bool,
    /// The tile asks whether to turn it on.
    pub ask: bool,
    /// The session read production data: off, and it stays so (§12.4).
    pub locked: bool,
}

/// A project's `remote_control` and `[remote_control_roles]`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub default: Mode,
    pub roles: BTreeMap<String, Mode>,
}

impl Policy {
    pub fn mode(&self, role: Option<&str>) -> Mode {
        role.and_then(|r| self.roles.get(r))
            .copied()
            .unwrap_or(self.default)
    }

    /// From a project file: a value TermF does not know counts as off, rather than making the project unreadable.
    pub(crate) fn read(default: Option<&toml::Value>, roles: Option<&toml::Value>) -> Self {
        let mode = |v: &toml::Value| v.clone().try_into::<Mode>().unwrap_or_default();
        Self {
            default: default.map(mode).unwrap_or_default(),
            roles: roles
                .and_then(toml::Value::as_table)
                .map(|t| t.iter().map(|(role, m)| (role.clone(), mode(m))).collect())
                .unwrap_or_default(),
        }
    }
}

fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "off",
        Mode::Ask => "ask",
        Mode::On => "on",
    }
}

/// Saves a project's policy, keeping the rest of its file and its comments.
pub fn save(config_dir: &Path, project: &str, policy: &Policy) -> Result<()> {
    let file = crate::project_file(config_dir, project)?;
    let mut doc: DocumentMut = std::fs::read_to_string(&file)?
        .parse()
        .with_context(|| format!("invalid {}", file.display()))?;
    doc["remote_control"] = value(mode_str(policy.default));
    doc.remove("remote_control_roles");
    if !policy.roles.is_empty() {
        let roles = crate::table_in(doc.as_table_mut(), "remote_control_roles");
        for (role, mode) in &policy.roles {
            roles.insert(role, value(mode_str(*mode)));
        }
    }
    crate::write_atomic(&file, &doc.to_string())
}

/// What a session starts with, `(on, ask)`: the user's choice at launch, else the project's and the role's mode; a
/// session that read production data never gets it (the "trifecta" rule, §12.4).
pub fn at_start(chosen: Option<bool>, mode: Mode, locked: bool) -> (bool, bool) {
    match (locked, chosen) {
        (true, _) => (false, false),
        (false, Some(on)) => (on, false),
        (false, None) => (mode == Mode::On, mode == Mode::Ask),
    }
}

/// `<project>/<ticket>/<role>` with what is known, the folder's name standing for the ticket of a session that has
/// none: easy to find in the list on the phone. A ticket key with an owner (`acme/api#482`) keeps its last part.
pub fn name(project: Option<&str>, ticket: Option<&str>, role: Option<&str>) -> String {
    let ticket = ticket.map(|t| t.rsplit('/').next().unwrap_or(t));
    let parts: Vec<String> = [project, ticket, role]
        .into_iter()
        .flatten()
        .map(|p| p.replace('/', "-").trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect();
    let name = parts.join("/");
    match name.char_indices().nth(80) {
        Some((cut, _)) => name[..cut].to_owned(),
        None => name,
    }
}

/// The longest link TermF shows: the most its QR code holds (`app/src/lib/qr-logic.ts`, version 10, level M).
pub const LINK_MAX: usize = 213;
const LINK_START: &str = "https://claude.ai/code/";

/// The page of a Remote Control session on claude.ai, from the id its hooks report. That id comes through the
/// session's own process, so anything but a plain id, short enough for the QR code, is refused.
pub fn link(bridge: &str) -> Option<String> {
    let plain = !bridge.is_empty()
        && bridge.len() <= LINK_MAX - LINK_START.len()
        && bridge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    plain.then(|| format!("{LINK_START}{bridge}"))
}

/// The conversation a session's options start in: the one `--resume` or `--session-id` names.
pub fn conversation(args: &[String]) -> Option<&str> {
    let at = args
        .iter()
        .position(|a| matches!(a.as_str(), "--resume" | "-r" | "--session-id"))?;
    args.get(at + 1).map(String::as_str)
}

/// A session's options without those that pick its conversation, so that it can start again in another one:
/// `--resume <id>`, `--session-id <id>`, `--fork-session`, `--continue`, and a first message after `--`.
pub fn strip_conversation(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--" => break,
            "--resume" | "-r" | "--session-id" => {
                it.next();
            }
            "--fork-session" | "--continue" | "-c" => {}
            _ => kept.push(arg.clone()),
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_role_overrides_the_project() {
        let policy = Policy {
            default: Mode::Off,
            roles: BTreeMap::from([("cto".into(), Mode::On), ("back".into(), Mode::Ask)]),
        };
        assert_eq!(policy.mode(None), Mode::Off);
        assert_eq!(policy.mode(Some("front")), Mode::Off);
        assert_eq!(policy.mode(Some("cto")), Mode::On);
        assert_eq!(policy.mode(Some("back")), Mode::Ask);
        let ask = Policy {
            default: Mode::Ask,
            roles: BTreeMap::from([("qa".into(), Mode::Off)]),
        };
        assert_eq!(ask.mode(Some("front")), Mode::Ask);
        assert_eq!(ask.mode(Some("qa")), Mode::Off);
    }

    #[test]
    fn starts_with_the_choice_then_the_mode_never_when_locked() {
        assert_eq!(at_start(None, Mode::Off, false), (false, false));
        assert_eq!(at_start(None, Mode::Ask, false), (false, true));
        assert_eq!(at_start(None, Mode::On, false), (true, false));
        assert_eq!(at_start(Some(true), Mode::Off, false), (true, false));
        assert_eq!(at_start(Some(false), Mode::On, false), (false, false));
        for mode in [Mode::Off, Mode::Ask, Mode::On] {
            assert_eq!(at_start(Some(true), mode, true), (false, false));
            assert_eq!(at_start(None, mode, true), (false, false));
        }
    }

    #[test]
    fn readable_names() {
        assert_eq!(
            name(Some("acme"), Some("ACME-123"), Some("back")),
            "acme/ACME-123/back"
        );
        assert_eq!(
            name(Some("acme"), Some("acme/api#482"), None),
            "acme/api#482"
        );
        assert_eq!(name(Some("acme"), Some("api"), None), "acme/api");
        assert_eq!(name(None, Some("api"), Some("")), "api");
        assert_eq!(name(None, None, None), "");
        assert_eq!(name(Some("a/b"), None, Some(" qa ")), "a-b/qa");
        assert_eq!(name(Some(&"é".repeat(100)), None, None).chars().count(), 80);
    }

    #[test]
    fn links_only_plain_ids() {
        assert_eq!(
            link("session_01AbC-x_9").as_deref(),
            Some("https://claude.ai/code/session_01AbC-x_9")
        );
        for bad in [
            "",
            "a/b",
            "x?y=1",
            "../evil",
            "a b",
            "javascript:x",
            &"a".repeat(191),
        ] {
            assert_eq!(link(bad), None, "{bad}");
        }
        let longest = link(&"a".repeat(190)).unwrap();
        assert_eq!(longest.len(), LINK_MAX, "the QR code holds every link");
    }

    #[test]
    fn the_conversation_a_session_starts_in() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            conversation(&args(&["--session-id", "c0ffee", "first message"])),
            Some("c0ffee")
        );
        assert_eq!(
            conversation(&args(&["--resume", "abc", "--model", "opus"])),
            Some("abc")
        );
        assert_eq!(conversation(&args(&["--model", "opus"])), None);
        assert_eq!(conversation(&args(&["--resume"])), None);
    }

    #[test]
    fn strips_what_picks_the_conversation() {
        let args = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            strip_conversation(&args(&[
                "--resume",
                "abc",
                "--model",
                "opus",
                "--fork-session",
                "--effort",
                "high"
            ])),
            args(&["--model", "opus", "--effort", "high"])
        );
        assert_eq!(
            strip_conversation(&args(&["--session-id", "x", "--", "first message"])),
            Vec::<String>::new()
        );
        assert_eq!(
            strip_conversation(&args(&["-c", "--permission-mode", "plan", "-r", "y"])),
            args(&["--permission-mode", "plan"])
        );
    }

    #[test]
    fn reads_and_saves_a_project_policy() {
        let dir = std::env::temp_dir().join(format!("termf-remote-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projects")).unwrap();
        std::fs::write(
            dir.join("projects/acme.toml"),
            "# mine\nname = \"Acme\"\nremote_control = \"sometimes\"\n",
        )
        .unwrap();
        let read = |dir: &Path| crate::load_projects(dir).unwrap().remove(0).remote;
        assert_eq!(read(&dir), Policy::default(), "unknown counts as off");
        let policy = Policy {
            default: Mode::Ask,
            roles: BTreeMap::from([("cto".into(), Mode::On)]),
        };
        save(&dir, "acme", &policy).unwrap();
        assert_eq!(read(&dir), policy);
        let text = std::fs::read_to_string(dir.join("projects/acme.toml")).unwrap();
        assert!(text.starts_with("# mine\n"), "{text}");
        save(&dir, "acme", &Policy::default()).unwrap();
        assert_eq!(read(&dir), Policy::default());
        let text = std::fs::read_to_string(dir.join("projects/acme.toml")).unwrap();
        assert!(!text.contains("remote_control_roles"), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
