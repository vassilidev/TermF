---
title: Terminals and sessions
order: 2
---

Terminals keep every Claude Code session and shell in one window, in tabs and free splits, each showing whether it is working, waiting for you or done.

![Terminals in TermF: tabs, and two Claude Code sessions side by side above a shell, one of them waiting for an answer](shot:terminals "Tabs and splits, with each session's state. Sample data.")

## Tabs and splits {#terminals}

Each tab is a tree of free splits, with Claude Code or any shell in any repository.

| Shortcut | Action |
|---|---|
| <kbd>⌘T</kbd> | A new session: Claude Code or a shell, in any repository |
| <kbd>⌘D</kbd> | Split right |
| <kbd>⌘⇧D</kbd> | Split down |
| <kbd>⌥⌘</kbd> + arrows | Move between splits |
| <kbd>⌘↵</kbd> | Zoom one split |

Arrange a tab as one per tab, side by side, 2 × 2, auto grid or focus. Save layouts with their tabs and splits. Broadcast a message to the sessions you tick.

Tabs reorder by dragging and rename by double-click. Closing a running session asks first. A terminal out of sight lets go of its display: its output keeps filling a buffer, replayed with its scroll position when you come back.

Codex CLI, Gemini CLI or any command-line agent opens in a terminal too: see [Codex and Gemini](other-agents.md), and `[agents]` in the [configuration](configuration.md).

```sh
# the sessions
termf ls

# open a session (an agent waits for your confirmation)
termf open <path> [--agent claude]

# type into a session (waits for your confirmation)
termf say <session> <text>
```

> [!NOTE]
> On Linux and Windows, Ctrl+letter goes to the shell inside a terminal: TermF's shortcuts there need Shift. A terminal that writes more than about 1 MB while out of sight loses the start of its scroll; a full-screen program (vim, top) comes back as it redraws itself. Dragging tabs and split handles has only been checked with a simulated backend.
>
> **Tried so far.** Driven in the real app on macOS and Linux by end-to-end tests: a shell in a tile, a split beside it, a command broadcast to both.

## Session state and context reset {#status}

Each Claude Code session shows its state, read from Claude Code's own hooks, and how full its context is. A notification tells you when one waits or finishes.

1. TermF hooks into each Claude Code session it starts: starting, working, waiting for you, idle, done.
2. The context gauge comes from Claude Code's status line, chained after your own.
3. Past the threshold, at the end of a turn, the agent writes a handoff, TermF sends `/clear`, and the agent resumes from the handoff.
4. The reset stops if you typed, if the agent asked for approval or went back to work, or if a task runs in the background.

By default, TermF warns at 50 % of the context and resets at 70 %, and notifies you for **Waiting for you** and **Finished**.

| Setting | What it does |
|---|---|
| `context_reset` | Turns the automatic reset on or off |
| `context_reset_at` | The reset threshold |
| `context_warning_at` | The warning threshold |
| The canary preference | Off by default: every reply starts with your first name, so a degraded context shows at once |

```sh
# print each session's state changes, live
termf watch

# the status line TermF chains after yours
termf statusline
```

> [!NOTE]
> After Escape, the state goes back to idle only after about a minute. The reset is not wired to the team's headless sessions yet.
>
> **Tried so far.** With a real Claude Code 2.1.330 in a tile launched as TermF launches it: the states from starting to working, waiting for a permission and idle, and the context reset with its handoff, once.

## Two panels, help and the tour {#panels}

A second panel opens any view beside the current one: a run while an agent works, or the CTO beside the terminals.

1. <kbd>⌘\</kbd> opens the second panel, <kbd>⌥⌘\</kbd> moves between them. ⌘-click or right-click opens a view beside.
2. <kbd>?</kbd> opens the help with every shortcut.
3. <kbd>⌘K</kbd> opens the palette, grouped by view.
4. A four-step tour opens at first launch, and again from the help.

Every shortcut can be remapped, with conflicts flagged: see [Keyboard shortcuts](shortcuts.md).

> [!NOTE]
> <kbd>⌘\</kbd> on an AZERTY keyboard, and WebKit's own context menu, have not been tried in the real window.
>
> **Tried so far.** The help, the tour, the palette and a remapped shortcut are driven in the real app on macOS by end-to-end tests.
