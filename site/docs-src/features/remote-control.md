---
title: Remote Control
order: 15
---

Remote Control lets you pick up a session from your phone, and the menu bar and dock show what waits for you without opening the window.

## Turn on Remote Control

Claude Code's Remote Control lets you continue a session from claude.ai or the Claude app on your phone. TermF turns it on per session, per project or per role.

1. In a project's **Configuration**, set Remote Control to **Off**, **Ask** or **On**, for the whole project or for one role (the CTO only, for instance).
2. Or open a session from the launcher with its **Remote Control** switch. The tile's header then shows a phone, green when it is connected.
3. Click it for the session's name, its QR code and its link. Scan it with your phone, copy or open it, or turn it off.
4. The navigation lists every session open to remote control, with **Turn all off**.

It is off for every project until you turn it on. Turning it on or off waits for the end of the agent's turn, and keeps the same conversation. A session that read production data through [Ops](ops.md) is cut and locked: Remote Control stays off for it.

In the project's file: `remote_control`, and `[remote_control_roles]` for one role.

## The menu bar and the dock

A menu bar icon and a badge on the dock say how many things wait for you, even with the window closed. The menu bar icon lists the sessions open to remote control, what waits for you, the quick terminal and **Stay awake**. On Windows, the badge is an amber dot on the taskbar icon.

Both are on by default; **Settings › Menu bar and dock** turns them off (`[tray] icon` and `badge`).

> [!NOTE]
> Never tried with a real Remote Control: it needs a signed-in claude.ai account, which the tests never use. It was built from Claude Code's documentation, with a fake Claude Code in a real terminal.
>
> **Tried so far.** Driven in the real app on macOS by an end-to-end test (a fake Claude Code opened with Remote Control on); the QR code is checked module by module against macOS's own generator.
