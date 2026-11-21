# TermF

A lightweight, local-first cockpit for AI coding agents (Claude Code first) across all your repositories. Built with Rust and Tauri, no Electron.

[Version française](README.fr.md)

> **Early development.** Phases 0 and 1 of the [roadmap](docs/ARCHITECTURE.md#17-feuille-de-route) are done. TermF finds every git repository on your machine and opens agents or shells in as many of them as you like, side by side in a terminal grid. It shows which agent is waiting for you and notifies you. It also has a file explorer and editor, projects, broadcast to several sessions, saved layouts, a quick terminal, a `termf` CLI and a mode that keeps the computer awake while agents work.

## Where it is heading

- Open dozens of agent sessions across repositories in two clicks, and see at a glance which one is waiting for you.
- Link projects to tickets (Jira, GitHub Projects) and let a team of role-based agents (product, front-end, back-end, DevOps, security) take a ticket to a pull request, each with the right model and effort.
- Workflows and rules you switch on per project, ticket or task: strict scope, no unrequested text changes, visual before/after review.
- Secure production access (logs, SQL, `.env`) where the AI never holds credentials.
- Encrypted settings sync, SSH key management, a small built-in editor.

The full design, in French, is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Install a build

Installers for macOS (Apple Silicon and Intel), Windows and Linux are attached to each [release](https://github.com/vassilidev/TermF/releases). Builds are not signed yet, so the first launch needs one extra step:

- **macOS**: after moving TermF to Applications, run `xattr -dr com.apple.quarantine /Applications/TermF.app` once (or right-click the app, Open, then Open again).
- **Windows**: on the SmartScreen warning, click "More info", then "Run anyway".
- **Linux**: `chmod +x` the AppImage, or install the `.deb` / `.rpm`.

## Run from source

Prerequisites: Rust (stable), Node 22+, pnpm, and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
cd app
pnpm install
pnpm tauri dev
```

On first launch TermF creates `termf.toml` in your config folder (`~/Library/Application Support/termf` on macOS, `~/.config/termf` on Linux, `%APPDATA%\termf` on Windows). By default it scans your home folder for repositories and offers one agent, `claude`; add your own:

```toml
roots = ["~", "D:/dev"]
scan_depth = 5

[agents]
claude = "claude"
codex = "codex"
```

To group several repositories into one project, add `projects/<id>.toml` next to it:

```toml
name = "Acme"

[[repos]]
path = "~/Projects/acme-api"

[[repos]]
path = "~/Projects/acme-front"
```

## Fork it

TermF is meant to be forked and bent to your own way of working. Most customization lives in plain files (settings, roles, rules, workflows) rather than in code, so a fork can stay small and keep pulling upstream. Contributions are welcome too.

## License

[MIT](LICENSE)
