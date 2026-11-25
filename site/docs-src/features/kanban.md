---
title: The ticket board
order: 3
---

**Tickets › Browse** shows every ticket of a source, as a list or as a board whose columns are the source's own statuses. Drag a card to move the ticket, open it in full, or hand it to the team.

![The ticket board of a Jira site: To do, In Progress, In Review and Done columns, each card with its key, title, assignee and labels](shot:board "A Jira site's board, its spaces together. Sample data.")

## Browse the tickets {#browse}

1. Open **Tickets**, then **Browse**, and pick a source: a Jira site, GitHub Issues, a GitHub Project, Linear, GitLab or Azure DevOps.
2. Filter by project or space, state (**Open**, **Done**, **All**), assignee (**To me**, **To nobody**, **Assigned to anyone**) and **Current sprint**, or search by words.
3. **List** groups the tickets by space. **Board** sorts them into columns. Both show 50 tickets at a time, and **Load more** brings the next ones.
4. A click on a ticket opens it in full, in a panel: see [A ticket in full](ticket-details.md).

## The board {#board}

The columns come from the source itself:

| Source | Columns |
|---|---|
| Jira | The statuses of the workflows of the spaces read: to do first, then in progress, then done |
| GitHub Projects | The options of the project's Status field |
| Linear, Azure DevOps | The states of their tickets |
| An MCP source | Its states, to look at only |

Each card shows the key, the title, the assignee and the labels, and the state of its TermF run if there is one, else **Launch a run**.

### Move a ticket {#move}

Drag a card to another column: the ticket changes status in its tracker, through a Jira transition, a Project's Status field, or a Linear or Azure state.

- The card moves at once. If the tracker refuses, a workflow rule for example, it goes back with the reason.
- It is your own move: no approval card.
- From the keyboard, change the status in the ticket's panel.

## When the AI helps {#ai}

- **Sort with AI**, on a board: the CTO proposes tickets to move, a priority and the tickets to launch, then applies them through its actions, one card at a time, with your approval.
- **Organize with AI**, when a source has no columns, such as GitHub Issues without a Project, or GitLab: the CTO proposes columns, from your labels or by creating a GitHub Project, then sets them up with your approval.

Agents can read a board, search tickets, move, assign and comment on a ticket, always through a card. See [Agents that act for you](agent-actions.md).

> [!NOTE]
> GitHub Projects, Linear, GitLab and Azure DevOps list what their inbox reads, filtered by words and by **To nobody**: their own search is not used yet. Linear and Azure DevOps columns come from the tickets loaded, so an empty column does not show. A source read through MCP has one page of results and cannot move tickets.
>
> **Tried so far.** Never against a real Jira, GitHub, Linear or Azure DevOps: checked with recorded answers, automated tests and a simulated backend.
