---
title: A ticket in full
order: 3
---

A click on a ticket, in the list or on the board, opens the whole ticket in a panel: its description, files, comments and links, so you rarely need the browser.

![A Jira ticket open beside the board: its status, assignee, priority and sprint, the description, the attachments and the subtasks](shot:ticket "A Jira ticket read in TermF, beside the board. Sample data.")

## What the panel shows {#panel}

- **The description**, rendered: Jira's rich text becomes headings, lists, code, tables, mentions and links; the other sources' Markdown too. Never raw JSON or tags.
- **The attachments** of a Jira ticket, read with the site's token: images as thumbnails that enlarge on a click, other files to save.
- **The details**: type, status, assignee, reporter, priority, sprint, space, dates and labels.
- **Subtasks** and **Linked tickets**, which open in the same panel.
- **The comments**, and a box to write one.

## Act on it {#act}

- **Status**: pick another one. It changes at once, and comes back with the reason if Jira refuses the transition.
- **Comment**: writes on the ticket in its tracker. A source read through MCP is read-only.
- **Launch a run**: hands the ticket to the team. See [Runs and their steps](runs.md).
- **Ask the CTO**: opens <kbd>⌘I</kbd> with the ticket attached.
- **Open in the browser**, when you need the tracker itself.

> [!NOTE]
> Attachments and the full details (type, priority, sprint, subtasks, links) come from Jira only. For the other sources, the panel shows the description and the comments.
>
> **Tried so far.** Never against a real tracker: checked with recorded answers, automated tests and a simulated backend.
