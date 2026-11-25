---
title: Tickets
order: 2
---

Ticket sources bring the tickets of your trackers into the project, so any of them can go to the team or open in a Claude Code session.

## Sources

A project can follow several sources at once, and their inbox shows in the app:

| Source | How TermF reads it |
|---|---|
| GitHub Issues | The GitHub CLI, with no setting at all |
| GitHub Projects | The GitHub CLI: `[trackers.github] project = "owner/number"` |
| GitLab issues (gitlab.com or your own) | `glab` when it is signed in, otherwise a token |
| Jira Cloud, several sites and spaces | A token, or Claude Code's Atlassian connection |
| Linear | A token |
| Azure DevOps Boards | A token |
| Any MCP server | A read-only Claude Code session (Haiku) that queries the server |

Tokens go to the system keychain, never in a file.

## Follow a source

1. Open the project's **Configuration** and add its sources, or accept the ones the setup proposes: a GitLab remote proposes GitLab, `ABC-12` keys in the commits propose Linear or Jira. For Jira, **Connect Jira step by step**: see [Jira, several sites and spaces](jira.md). Or ask the CTO with <kbd>⌘I</kbd>: it finds your tracker and connects it, through its card.
2. The inbox lists your tickets. A failing source does not block the others.
3. Pick a ticket, or paste a URL or a key: `owner/repo#42`, `#42`, `ACME-123`, `ENG-123`, `group/project#42`, `AB#123`.
4. **With the team** starts a team run. **Launch** (<kbd>⌘⇧L</kbd>) opens a Claude Code session in the ticket's own worktree.

```sh
# the tickets of every source
termf inbox

# open a ticket's run (waits for your confirmation in the app)
termf run <ticket>
```

## Browse, board and details {#browse}

**Tickets › Browse** shows every ticket of a source, beyond the ones waiting for you: as a list grouped by space, or as a board whose columns are the source's statuses. Drag a card to move the ticket in its tracker, or open it in full in TermF, with its description, files and comments.

![The ticket board of a Jira site, with To do, In Progress, In Review and Done columns](shot:board "The board of a Jira site. Sample data.")

See [The ticket board](kanban.md) and [A ticket in full](ticket-details.md).

## Settings

Each source has its own table in the project's file, `[trackers.*]`, and each Jira site its own `[[trackers.jira]]`: the board, team or query, mine only, the states to show, a comment with the pull request's link, a transition. A self-hosted GitLab takes `[trackers.gitlab] host`.

By default, nothing is written back to the tracker: no comment, no transition, until you turn it on.

> [!NOTE]
> Linear, GitLab and Azure DevOps have not been tried against the real services yet: they were built from their documentation and tested against a local stand-in. A GitLab merge request's pipelines are not shown, and Azure Repos gets no pull request: the branch is pushed, you open it there. GitHub Projects has no conditional requests, so it is read again in full.
>
> **Tried so far.** A ticket opened in its worktree has run with a real Claude Code; GitHub Issues were tried against the real GitHub; the other sources, with fake servers and CLIs.

Next: what happens once a ticket goes to the team, in [Runs and their steps](runs.md).
