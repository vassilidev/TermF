---
title: Claude view and usage
order: 13
---

The Claude view shows all of Claude Code's setup on this machine in one place, your claude.ai artifacts, and how much of your usage limits is left.

## The Claude Code view {#claude-view}

Claude Code's setup is spread over several files and scopes. The Claude view shows it by scope, says which scope decides, and lets you change the simple things through a preview.

![The Claude view: version, account, model, permission mode and everything installed](shot:claude "Everything Claude Code knows on this machine. Sample data.")

- Settings, permissions, hooks and environment, by scope, with the one that decides.
- MCP servers, plugins and marketplaces; skills, agents, commands, output styles, rules, memory files.
- Past sessions: resume, duplicate, purge; usage and limits; account and version.

Simple settings are edited through a diff preview, and secrets are masked. A marketplace command is shown with its hash and runs only after you accept.

```sh
# settings by scope
termf claude settings get | set | unset

# MCP servers
termf claude mcp add | remove

# past sessions
termf claude sessions
```

> [!NOTE]
> Renaming a Claude session from TermF is not built.
>
> **Tried so far.** `claude doctor` ran with a real Claude Code 2.1.330; the view was checked with a simulated backend.

## Usage and limits {#usage}

A dollar amount means two different things depending on how Claude Code is paid. With an API key, it is billed. With a Claude subscription, nothing extra is billed: a run draws on your 5-hour and weekly usage limits. TermF reads how your Claude Code is paid and keeps the limits in view.

1. TermF reads the billing mode from Claude Code: a subscription, an API key, or a cloud provider.
2. For a subscriber, every amount reads “≈ x API equivalent”, and a budget is a usage cap, still in dollars.
3. A gauge at the foot of the navigation shows the fullest window. A click shows both, when they reset, and **Claude › Usage**.
4. The CTO reads the same limits before it starts a team: when one is nearly used up, it answers directly or picks a lighter role, and says so.

A warning shows from 80 % of a window, in the gauge and in the conversation, and a session that hits its limit says when it resumes. The limits come from the sessions TermF starts, so there is no reading until the first answer of one. **Claude › Usage** reads the billing mode again.

> [!NOTE]
> The plan's name comes from a field Claude Code does not document.
>
> **Tried so far.** The billing mode and the limit events follow Claude Code's and the Agent SDK's documentation, covered by automated tests and a simulated backend.

## claude.ai artifacts {#artifacts}

TermF lists your claude.ai artifacts in a table you can sort and filter, to open, copy, share, edit or delete them.

1. New artifacts are indexed as Claude Code publishes them.
2. The account's list is read by a hidden, restricted session, every six hours (`[artifacts] refresh_hours`).
3. Deleting goes through a session that may only delete those artifacts, and always asks you first.

> [!NOTE]
> The [project expert](team.md#expert) indexes artifacts by title and link only.
>
> **Tried so far.** Checked with a simulated backend.
