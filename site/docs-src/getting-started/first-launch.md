---
title: First launch and what you need
order: 2
---

On first launch, TermF checks the tools it needs, tells you how to install the missing ones, sets up the machine with you, step by step, and shows you around.

## What you need

| Tool | Why |
|---|---|
| [Claude Code](https://code.claude.com) 2.1.288 or later, signed in | The CTO and the team run on it, with your subscription or your API key |
| `git` | Worktrees, branches and deliveries |
| [GitHub CLI](https://cli.github.com), signed in | GitHub tickets, pull requests and CI. Optional otherwise |
| Chrome, Chromium or Edge | The [visual review](visual-review.md) and [the agents' browser](team.md#agent-browser). Optional otherwise |

TermF runs on macOS 11 or later (Apple Silicon and Intel), Windows (x64 and ARM64) and Linux (x86_64 and ARM64) with WebKitGTK 4.1. It finds Claude Code's native install by itself and tells you how to update an older one.

Codex CLI and Gemini CLI are optional: when they are installed, TermF finds them (see [Codex and Gemini](other-agents.md)).

## The first launch

1. Open TermF. **Welcome** asks whether this is your first machine with TermF, or whether you already use it on another one.
2. **Claude Code**: TermF checks it, and the Home lists any tool that is missing, with the official command to copy and its official page.
3. **This machine**, on a first machine:
   - **What you use on this machine**: **Detect what is installed**, then **Keep the selection** for [My environment](environment.md);
   - **Your vault**: a master password, then its recovery key to **Copy** or **Download**;
   - **Your settings on all your machines**: the [encrypted sync](sync.md), with a preview before joining one.
4. On another machine, the sync comes first and brings your settings, projects and vault: **Join your sync**. The vault step is skipped once the vault has arrived.
5. Then your first project, the CTO, and where to find help.

A step already done is not listed. **Later** skips one: the list on the left marks it **Skipped, to resume**, and the Home offers **Resume** until it is done. The tour comes back from the help (<kbd>?</kbd>) whenever you want, and **Settings › What you need** puts `termf` on your `PATH` in one click (see [the termf CLI](cli.md)).

## Find your way around

- The navigation on the left lists your projects; each one unfolds its views: Home, Terminals, Tickets, Files, Git, CI/CD, Design system, Vault, Ops and Configuration.
- **This machine**, at the foot of the navigation, holds the tools that are not tied to a project, and the Settings gear stays next to it.
- <kbd>⌘K</kbd> opens the palette, grouped by view; <kbd>?</kbd> lists every shortcut.
- <kbd>⌘J</kbd> jumps to the next thing waiting for you, in any project.

## Your first session, without the team

You can use TermF as a terminal for Claude Code before creating any project:

1. <kbd>⌘T</kbd> opens Claude Code, or a shell, in any repository, in a tab.
2. <kbd>⌘D</kbd> splits the view.
3. A badge and a notification tell you when a session waits for you.

See [Terminals and sessions](terminals.md).

> [!NOTE]
> TermF is in early development. The macOS app is signed and notarized by Apple; the Windows build is not signed yet (see [Installation](installation.md#open-it-the-first-time)). Every feature is driven in the real app by end-to-end tests on macOS with a fake Claude Code, but little has been tried by hand on real projects yet, and Windows and Linux are built and tested in CI, not yet by hand.

Next: [create a project](create-a-project.md).
