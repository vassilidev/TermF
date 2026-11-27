---
title: Keyboard shortcuts
order: 3
---

Every action of TermF has a shortcut you can change; this page lists the defaults on macOS and on Linux and Windows.

Press <kbd>?</kbd> in the app for the same list, as it is on your machine. To change one, open **Settings › Shortcuts**: conflicts are flagged, and the combinations a system keeps for itself are refused.

## General

| Action | macOS | Linux and Windows |
|---|---|---|
| Palette | <kbd>⌘K</kbd> | <kbd>Ctrl</kbd> <kbd>K</kbd> |
| Help and every shortcut | <kbd>?</kbd> | <kbd>?</kbd> |
| Switch project | <kbd>⌘O</kbd> | <kbd>Ctrl</kbd> <kbd>O</kbd> |
| Project 1 to 9 | <kbd>⌃1</kbd> … <kbd>⌃9</kbd> | <kbd>Alt</kbd> <kbd>1</kbd> … <kbd>9</kbd> |
| Talk to the CTO | <kbd>⌘⇧H</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>H</kbd> |
| Expand the Ask panel's conversation | <kbd>⌘⇧I</kbd> | <kbd>Alt</kbd> <kbd>Shift</kbd> <kbd>I</kbd> |
| Find a conversation | <kbd>⌘⇧K</kbd> | <kbd>Alt</kbd> <kbd>Shift</kbd> <kbd>K</kbd> |
| Next thing waiting for you | <kbd>⌘J</kbd> | <kbd>Ctrl</kbd> <kbd>J</kbd> |
| Second panel | <kbd>⌘\</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>\</kbd> |
| Other panel | <kbd>⌥⌘\</kbd> | <kbd>Alt</kbd> <kbd>Shift</kbd> <kbd>\</kbd> |
| Quick terminal | <kbd>⌃</kbd> + backquote | <kbd>Ctrl</kbd> + backquote |
| Quick terminal, from anywhere on the system | none until you set one | none until you set one |
| Launch a ticket | <kbd>⌘⇧L</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>L</kbd> |

## Terminals

| Action | macOS | Linux and Windows |
|---|---|---|
| New session | <kbd>⌘T</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>T</kbd> |
| Tile 1 to 9 | <kbd>⌘1</kbd> … <kbd>⌘9</kbd> | <kbd>Ctrl</kbd> <kbd>1</kbd> … <kbd>9</kbd> |
| Previous tab | <kbd>⌘⇧[</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Tab</kbd> |
| Next tab | <kbd>⌘⇧]</kbd> | <kbd>Ctrl</kbd> <kbd>Tab</kbd> |
| Split right | <kbd>⌘D</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>D</kbd> |
| Split down | <kbd>⌘⇧D</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>E</kbd> |
| Close split | <kbd>⌘W</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>W</kbd> |
| Focus left, right, up, down | <kbd>⌥⌘</kbd> + arrow | <kbd>Alt</kbd> + arrow |
| Resize left, right, up, down | <kbd>⌃⌘</kbd> + arrow | <kbd>Alt</kbd> <kbd>Shift</kbd> + arrow |
| Equalize splits | <kbd>⌃⌘=</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>=</kbd> |
| Zoom the active split | <kbd>⌘↵</kbd> | <kbd>Ctrl</kbd> <kbd>Enter</kbd> |
| Broadcast to the ticked sessions | <kbd>⌘⇧↵</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>Enter</kbd> |

## Files

| Action | macOS | Linux and Windows |
|---|---|---|
| Find a file | <kbd>⌘P</kbd> | <kbd>Ctrl</kbd> <kbd>P</kbd> |
| Search the project | <kbd>⌘⇧F</kbd> | <kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>F</kbd> |

In the SQL console of [Ops](ops.md#sql-console), <kbd>⌘↵</kbd> runs the query.

## In a terminal, on Linux and Windows

Ctrl or Alt + a letter, and Alt + a digit without Shift, stay with the shell inside a terminal; with Shift, they trigger TermF's action. That is why most defaults on Linux and Windows include Shift.

## Change them in termf.toml

The shortcuts editor writes only what differs from the defaults, per system, in `[keys.<os>]` of `termf.toml`. A combination is written like `Ctrl+Alt+Cmd+Shift+K` (`Cmd` is ⌘, or Super); an action numbered 1 to 9 keeps only its modifiers, such as `"Cmd"`; `""` means no shortcut. See [Configuration](configuration.md).
