---
title: Files
order: 5
---

TermF has a file explorer, a small editor and a quick terminal next to the agents, for when you need to look at a file without opening your editor.

## Find and edit a file

1. <kbd>⌘P</kbd> finds a file by name; <kbd>⌘⇧F</kbd> searches the text of the project.
2. The file opens in a CodeMirror editor, with its git diff. Save, or open it in your external editor.

## The quick terminal

<kbd>Ctrl</kbd> + the backquote key opens a quick terminal over the window, on every system. It has no system-wide shortcut until you set one in **Settings › Shortcuts**.

## Keep awake

While an agent works, and two minutes after, the machine does not sleep. This is automatic; the display may still sleep.

## Settings

| Setting | What it does |
|---|---|
| `keep_awake` | Keep the machine awake while agents work |
| `keep_display_on` | Keep the display on too |
| `editor` | The external editor files open in |
| Quick terminal shortcut | A global shortcut for the quick terminal |

> [!NOTE]
> <kbd>⌘Q</kbd> does not protect unsaved tabs. A tool defined only in `.zshrc` is not found for the external editor.
>
> **Tried so far.** Driven in the real app on macOS by end-to-end tests: a file opened, edited and saved, a file found by name, a text found across the repository, the quick terminal.
