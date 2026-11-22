---
title: Your first task for the CTO
order: 4
---

You give work to the team by writing to the CTO on the project's Home; this page walks through a first task, from the request to the pull request.

![TermF's project Home: the conversation with the CTO, with a plan waiting for approval, and on the right what waits for you and the agents at work](shot:home "The project's Home: the CTO's conversation and what waits for you. Sample data.")

## 1. Ask

On the project's Home, write to the CTO, or press <kbd>⌘⇧H</kbd> from anywhere:

```text
Start the team on #57
```

A ticket from a [source](tickets.md) the project follows, a pasted URL or key, or a change described in your own words all work. You can also start from **Tickets**: **With the team** sits next to each ticket.

The CTO answers with a card asking your approval to start a team run. Nothing starts before you click **Allow**.

## 2. Let the CTO size the team

The CTO reads the request and the code, and [sizes the team](runs.md#triage) to the ticket:

- a small fix goes to one developer and a short review, without asking you anything;
- a large or sensitive one shows you its path and its plan first, and waits for your approval at each.

When something real is unclear, you get [questions with proposed answers](runs.md#questions): change what does not suit you, then send them all at once.

## 3. Follow the run

The run has its own page, opened from the Home. It shows the steps, the agent at work on each and what it is doing, what waits for you, and the budget spent. You can:

- write to a role at work: `@front …`;
- **Take over** a step in a terminal, then **Hand back**;
- **Emergency stop** every agent at once.

See [Runs and their steps](runs.md).

## 4. Deliver

When the team is done, the run is ready to deliver.

1. **Deliver…** shows the files to commit and the commits not pushed yet.
2. **Deliver** commits that, pushes the branch and opens the pull request.

No agent can push or open a pull request for you: the [run guard](security.md#guard) refuses it.

> [!WARNING]
> A team run costs more than one Claude Code session, even sized down: about 3.4 times on a small ticket, 4.8 times on a complex one, where it does better. Each run has a budget, 15 dollars by default, that stops it when it is spent. With a Claude subscription, nothing extra is billed: the run draws on your usage limits. For a small, clear change, one session in a terminal tab (<kbd>⌘T</kbd>) is often enough: see [the team or one session?](team-vs-solo.md)

## Next

- [The CTO and the team](team.md): every role, and how to change one.
- [Runs and their steps](runs.md): every step, gate and setting.
