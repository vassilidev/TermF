---
title: Agents that act for you
order: 3
---

The agent you ask with <kbd>⌘I</kbd> can do what its screen does: add a database, connect Jira, move a ticket, redeploy a site. It never acts on its own: each action waits on a card, and you say yes or no.

![The Ask panel on Ops: the DevOps engineer wants to add a production database, and its card asks for the password, which goes to the vault](shot:act "An agent adds a database: the card says what it does and asks for the password. Sample data.")

## How it works {#how}

1. Ask in your own words from any screen, for example "add the production database" on **Ops**, or "connect our Jira" on **Tickets**.
2. The agent reads what the screen can do and what it already holds, by name only: connections, saved queries, ticket sources, deploy targets, tasks, the names of the vault's secrets.
3. It proposes one action. A card shows who asks, such as **devops wants to act in TermF**, the action, and in plain words exactly what it will do.
4. **Allow** runs it, **Deny** refuses it, with a reason if you like. The screen shows the result, as if you had done it yourself.

The CTO, the [project expert](team.md#expert) and every conversation of the panel can act this way. Each one gets the actions of the screen it was asked from, plus a few that work anywhere: create, unlock or lock the vault, change topic, delete archived conversations.

## The card {#card}

The card is the only way an action runs. It says:

- what the action does, with its exact target: the connection, the ticket, the site;
- what you lose, when the action deletes or replaces something: a connection, a secret, what is live on a site;
- the fields TermF asks you for itself, such as a password or a token.

Not sure? Type a question in the message box while the card waits, or click **Ask for help**: the question answers the card, the agent explains, then asks again if it still needs the action.

## Secrets stay with you {#secrets}

An agent never sees a password, a token or any secret value.

- A secret field shows up empty on the card. You type the value there, and TermF sends it straight to the [vault](vault.md), or to the system keychain for a ticket token.
- The agent may say where to find it, such as "DB_PASSWORD in the .env of the server". It never gets the value back.
- An agent that tries to pass a secret itself is refused.
- A production secret asks for your master password again, on the same card.

## Prerequisites {#prerequisites}

Some actions need something first: a vault, an unlocked vault, a database, a ticket source, a deploy tool. When it is missing, the action does not reach you. The agent learns what is missing and proposes the action that sets it up first:

- no vault yet: it proposes to create one, and you pick the master password on the card;
- a locked vault: it proposes to unlock it;
- no database for a query: it proposes to add the connection;
- no deploy tool on CI/CD: it proposes to connect one;
- no ticket source: it proposes to connect Jira or add a source.

Two cards, one after the other: the prerequisite, then the action.

## What agents can do, screen by screen {#screens}

| Screen | What the agent can do |
|---|---|
| Ops | Add, change, test or delete a connection; save, run, change or delete a query; turn a query into a recipe; change Redis, reload Nginx, retry or forget a Laravel job; connect Forge, import its sites, redeploy one |
| Tickets | Connect Jira, add, change, test or remove a source; search tickets, read a board, move, assign or comment on a ticket |
| CI/CD | Connect a deploy tool, change or delete one; rerun or cancel a run, start a workflow, redeploy; add, delete or redeliver a webhook |
| Vault | Create, unlock or lock the vault; set or delete a secret |
| Library and Claude | Turn an item on or off, copy one, install, set or uninstall a plugin |
| Git | Create, switch to or rename a branch; clean up branches and worktrees |
| Scheduler | Create, change, run or delete a task |
| Settings, project and setup | Change a setting, change the project, apply the setup, scan the design system |

A query an agent runs shows its result on the screen. The agent only learns how many rows came back and their columns.

## What agents never do {#never}

Some things stay yours only, by design:

- typing in a terminal, or sending a message for you;
- answering what an agent asks or proposes, so it never approves itself;
- reading, copying or revealing a secret, and the vault's own security;
- SSH keys, and what agents may reach: Claude Code's settings, hooks and MCP servers, Remote Control;
- delivering a run and merging its pull request;
- installing updates, deleting the project.

> [!NOTE]
> Each new screen command gets an action, or a written reason why not: an automated test checks the whole list.
>
> **Tried so far.** The cards were checked with a simulated backend and automated tests; actions have not run yet with a real Claude Code.
