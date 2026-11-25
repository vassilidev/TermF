---
title: Jira, several sites and spaces
order: 3
---

Connect Jira once, step by step, and a project reads the tickets of several Jira sites, each with the spaces you pick.

![Connect Jira: TermF offers the Atlassian connection Claude Code already has, or an API token](shot:jira "Connect Jira: through Claude Code, or with an API token. Sample data.")

## Connect Jira {#connect}

1. Open the project's **Configuration**, then **Tickets**, and **Connect Jira step by step**. The CTO can open it too, from <kbd>⌘I</kbd>.
2. **How TermF reads your Jira**:
   - **Use your Atlassian connection in Claude Code**, when Claude Code has one: nothing to create, the sites and spaces are read through it;
   - **With an API token**: TermF reads Jira directly, faster.
3. **Sites**: check the sites this project reads. TermF lists the ones already connected, the ones found in the commits and one guessed from your e-mail, or you type another.
4. **Account**, with a token: your Atlassian e-mail and a token. **Create a token on Atlassian** opens the page where you make one.
5. **Spaces**: check the spaces of each site. Those whose keys appear in the commits, such as `APP` or `API`, are checked already.
6. **Connect Jira**.

The token is checked by reading your spaces, then kept in the system keychain, one per site and e-mail. It never goes to a file.

## Several sites and spaces {#sites}

Each site is one `[[trackers.jira]]` table in the project's file, with the keys of its spaces. A project can list as many sites as it needs.

```toml
[[trackers.jira]]
site = "acme.atlassian.net"
email = "alex@acme.dev"
spaces = ["APP", "API"]

[[trackers.jira]]
site = "acme-labs.atlassian.net"
email = "alex@acme.dev"
spaces = ["LAB"]
```

- No spaces: every space of the site.
- A pasted key such as `APP-142` goes to the site that reads `APP`.
- The older single `[trackers.jira]` table still reads the same, and is written in the new form the next time you save.
- The manual form shows each site with its spaces, its token, **Test**, and under **Advanced: the To do JQL** a JQL of your own.

## What you get {#what}

- **Tickets › To do**: the open tickets assigned to you, in the spaces of each site.
- **Tickets › Browse**: every ticket of the spaces read, grouped by space, with filters by space, state, assignee and the current sprint, and a search. See [The ticket board](kanban.md).
- Each ticket opens in full inside TermF: see [A ticket in full](ticket-details.md).

> [!NOTE]
> Through Claude Code's Atlassian connection, a search takes a few seconds and has no next page, since each read goes through Claude Code.
>
> **Tried so far.** Never against a real Jira site: built from Atlassian's documentation and checked with recorded answers, automated tests and a simulated backend.
