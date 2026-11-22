---
title: Projects
order: 1
---

A project groups the repositories you work on together; it has its own Home, its own CTO conversation, its own tickets, team and settings.

## What a project holds

- **Repositories**: one or several, anywhere on your machine. Add more later from the project's **Configuration**.
- **Views**: Home, Terminals, Tickets, Files, Git, CI/CD, Design system, Vault, Ops and Configuration unfold under the project in the navigation.
- **A team**: the roles, their settings and the [squads](team.md#squads).
- **Ticket sources**, a worktree recipe, workflows and rules.

A project is a plain file, `projects/<id>.toml`, which you can edit by hand (see [Configuration](configuration.md)). To create one, see [Create a project](create-a-project.md).

## The project's Home {#home}

Each project opens on its Home: the CTO's conversation with the decisions in place, and beside it what waits for you, who is on duty and the runs under way.

![TermF's project Home: the conversation with the CTO, with a plan waiting for approval, and on the right what waits for you and the agents at work](shot:home "The project's Home. Sample data.")

1. Choose the project in the navigation (<kbd>⌘O</kbd>, or <kbd>⌃1</kbd>…<kbd>⌃9</kbd>); its views unfold under it.
2. The Home shows the conversation, then **Waiting for you**, **On duty**, the runs and the first tickets of the inbox.
3. Plans, questions, permissions and deliveries are cards you answer in place.
4. <kbd>⌘J</kbd> goes to the next thing waiting, in any project: terminals first, then agents, then team runs.

An empty conversation with the CTO or the expert offers three examples to start from. In a narrow window, the navigation folds into icons, with their names as tooltips.

**This machine**, at the foot of the navigation, holds the tools that are not tied to a project, and the Settings gear stays next to it.

```sh
# the tickets waiting, from a terminal
termf inbox
```

> [!NOTE]
> Driven by automated tests in the real app, not yet used by hand on a real project.
>
> **Tried so far.** The first launch, the Home and the navigation are driven in the real app on macOS and Linux by end-to-end tests, with a fake Claude Code and GitHub CLI.
