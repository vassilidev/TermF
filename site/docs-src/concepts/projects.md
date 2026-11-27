---
title: Projects
order: 1
---

A project groups the repositories you work on together; it has its own Home, its own CTO conversation, its own tickets, team and settings.

## What a project holds

- **Repositories**: one or several, anywhere on your machine. Add more later from the project's **Configuration**.
- **Views**: Home, Terminals, Tickets, Files, Git, CI/CD, Design system, Vault, Ops and Configuration unfold under the project in the navigation.
- **A team**: the roles, their settings and the [squads](team.md#squads).
- **Ticket sources**, a worktree recipe, [MCP servers and connectors](mcp-connectors.md), workflows and rules.

A project is a plain file, `projects/<id>.toml`, which you can edit by hand (see [Configuration](configuration.md)). To create one, see [Create a project](create-a-project.md).

## Several repositories {#repos}

Git, CI/CD, Files and Design system share one choice: **All repositories**, or one. It is kept from view to view and from one launch to the next, and a project of several repositories opens on **All repositories**. Each repository is read on its own, side by side, and an error stays on its row.

| View | With **All repositories** |
|---|---|
| Git | A row per repository: its branch, ahead and behind, changes, an operation under way, its open pull requests. Clean up, fetch, pull and push need one repository |
| CI/CD | The last state of each repository and its last runs. Deployments and tools stay the project's; webhooks need one repository |
| Files | The search, and <kbd>⌘P</kbd>, cover every repository and name each result's; the tree shows each repository as a root |
| Design system | A summary per repository: tokens, components, inconsistencies |

A click on a row opens that repository. A small label also names the repository on terminal tiles, under the title of conversations and on launched tickets.

## The project's Home {#home}

Each project opens on its Home: one sentence on what waits for you and how many agents are working, then what waits, the agents at work, the runs and the tickets to handle. The CTO is one bar away, at the bottom of every screen.

![TermF's project Home: two things wait for an answer, the agents live with what each is doing, the runs and the tickets to handle, and the bar to ask the CTO at the bottom](shot:home "The project's Home. Sample data.")

1. Choose the project in the navigation (<kbd>⌘O</kbd>, or <kbd>⌃1</kbd>…<kbd>⌃9</kbd>); its views unfold under it.
2. What waits for you comes first, as cards: plans, questions, permissions and deliveries. **Answer** opens the right place.
3. **Live** lists the agents at work, each with what it is doing in plain words; then come the runs and the **Tickets to handle**.
4. Ask the CTO, or another role with `@role`, in the bar at the bottom, or with <kbd>⌘I</kbd> from any screen (see [Ask from any screen](ask-panel.md)).
5. <kbd>⌘J</kbd> goes to the next thing waiting, in any project: terminals first, then agents, then team runs.

Until the first steps are done (Claude Code, the tickets, a first run and its delivery), the Home also shows a short **Getting started** list, which you can hide.

In a narrow window, the navigation folds into icons, with their names as tooltips.

**This machine**, at the foot of the navigation, holds the tools that are not tied to a project, and the Settings gear stays next to it.

```sh
# the tickets waiting, from a terminal
termf inbox
```

> [!NOTE]
> Driven by automated tests in the real app, not yet used by hand on a real project.
>
> **Tried so far.** The first launch, the Home and the navigation are driven in the real app on macOS and Linux by end-to-end tests, with a fake Claude Code and GitHub CLI.
