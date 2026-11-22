---
title: Configuration
order: 2
---

TermF keeps its settings in plain TOML files that the app edits for you and that you can also edit by hand: `termf.toml` for the machine, one file per project.

## Where the files are

In your config folder: `~/Library/Application Support/termf` on macOS, `~/.config/termf` on Linux, `%APPDATA%\termf` on Windows.

| File | What it holds |
|---|---|
| `termf.toml` | Where to look for repositories, your agents, shortcuts, and the rest of the settings |
| `projects/<id>.toml` | A project: its repositories, ticket sources, team and squads, and `[roles.<role>]` overrides |
| `local.toml` | Settings that stay on this machine and are never [synced](sync.md) |

A repository can also carry `.termf/project.toml`, for a few role settings of its own (see [Change a role](team.md#role-file)).

## termf.toml

```toml
# where TermF looks for repositories, and how deep
roots = ["~"]
scan_depth = 5

# keep the machine awake: "auto" (while an agent works), "always" or "off"
keep_awake = "auto"
keep_display_on = false

# context gauge and automatic reset, in % of the context
context_reset = true
context_warning_at = 50
context_reset_at = 70

# the command-line agents you can open in a terminal: name = command
[agents]
claude = "claude"

[ci]
notify = "mine"

[updates]
channel = "stable"
check = true

# only the shortcuts you changed, per system: macos, linux, windows
[keys.macos]
new_session = "Cmd+T"
```

| Key | Default | What it does |
|---|---|---|
| `roots` | `["~"]` | Folders where TermF looks for your repositories |
| `scan_depth` | `5` | How deep it looks |
| `editor` | none | The external editor files open in ([Files](files.md)) |
| `keep_awake` | `"auto"` | `"auto"`, `"always"` or `"off"` |
| `keep_display_on` | `false` | Keep the display on too |
| `quick_terminal_shortcut` | none | A system-wide shortcut for the quick terminal |
| `context_reset` | `true` | Hand off, clear and resume a session whose context fills up ([Terminals](terminals.md#status)) |
| `context_warning_at` | `50` | Warning threshold, in % |
| `context_reset_at` | `70` | Reset threshold, in % |
| `[agents]` | `claude = "claude"` | Command-line agents: name and command. Also `env`, `env_allow` and `secrets` ([security](security.md#agent-env)) |
| `[tickets] agent` | `claude` | The agent a ticket's session runs |
| `[tickets] cleanup` | `"auto"` | After a merge: `"auto"`, `"ask"` or `"off"` ([after the merge](runs.md#after)) |
| `[tickets] worktrees` | `"repo"` | Where run worktrees go: `"repo"` or `"data"` ([worktrees](git.md#worktrees)) |
| `[ci] notify` | `"mine"` | CI notifications: `"mine"`, `"all"` or `"off"` |
| `[artifacts] refresh_hours` | `6` | How often your claude.ai artifacts are read again |
| `[keys.<os>]` | none | Changed shortcuts ([Keyboard shortcuts](shortcuts.md)) |
| `[vault] auto_lock_minutes` | `10` | Vault lock delay ([Vault](vault.md)) |
| `[memory] enabled`, `exclude` | on | The project expert's memory ([the expert](team.md#expert)) |
| `[api] confirm_cli` | `true` | Reserved actions from the command line wait for you in the app ([security](security.md#local-api)) |
| `[tray] icon`, `badge` | on | The menu bar icon and the dock badge |
| `[updates] channel`, `check` | `"stable"`, on | [Updates](updates.md) |

Team settings such as `[team]` and `[roles.<role>]` can go in `termf.toml` for every project, or in a project's file for one.

## A project's file

```toml
# projects/acme.toml
name = "Acme"

[[repos]]
path = "~/code/acme/api"

[[repos]]
path = "~/code/acme/web"

[roles.front]
model = "opus"
skills = ["+accessibility"]
```

The tables a project's file can hold, each described on its page:

| Table | Page |
|---|---|
| `name`, `[[repos]]` (`path`, optional `name`) | [Projects](projects.md) |
| `remote_control`, `[remote_control_roles]` | [Remote Control](remote-control.md) |
| `[trackers.*]` | [Tickets](tickets.md) |
| `[team]`: `triage`, `approve_route`, `plan_gate`, `review`, `review_rounds`, `review_nits`, `max_turns`, `staff`, `subplan_gate` | [Runs](runs.md#triage), [review](runs.md#code-review), [squads](team.md#squads) |
| `[[squads]]`, `[squads.roles.<role>]` | [Squads](team.md#squads) |
| `[roles.<role>]` | [Change a role](team.md#role-file), [Codex and Gemini](other-agents.md) |
| `[rules]`, `[workflows] default` | [Checks and rules](runs.md#checks) |
| `[visual]` | [Visual review](visual-review.md) |
| `[[ops.connections]]` | [Servers, databases and logs](ops.md) |
| `[lsp] enabled` | [Language servers](library.md#language-servers) |
| `[browser]`: `origins`, `tiles` | [The agents' browser](team.md#agent-browser) |
| `[agents.secrets]` | [Keys on request](security.md#agent-env) |

Library items (roles, rules, skills, preferences, workflows, commands) live in the [library](library.md), in layers, not in these files.
