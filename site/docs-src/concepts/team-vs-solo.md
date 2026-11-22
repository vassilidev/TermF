---
title: The team or one session?
order: 5
---

This page helps you choose between TermF's agent team and one plain Claude Code session, with the figures of a benchmark that gave both the same tickets.

## The short answer {#short}

Same model, same starting code, hidden tests the agents never see, and a judge who does not know which is which. On a small, clear ticket, one session is enough. On complex, vague or sensitive work, the team gets more of the unwritten part right, and costs more.

| | A small, clear ticket | A complex, realistic ticket |
|---|---|---|
| Quality | Same quality | 90 % of the unwritten criteria met by the team, 72 % by one session |
| Cost | One session: 0.10 dollar and 26 seconds. The team, sized down by the CTO to one developer and a short review: 0.34 dollar and 69 seconds, three sessions, no question asked. Still **3.4 times the cost** | The team: **4.8 times the cost** and 3.7 times the time |

On complex tickets, both pass every hidden test; the judge finds more of the unwritten criteria (security, robustness, edge cases) met by the team, with more tests and no type error left.

> [!NOTE]
> Small samples: one to three trials per ticket and side in the later series, two small projects, one model (Sonnet), and a judge that is itself a model reading only the diff. A first look, not a verdict.

## When to use which {#when}

- **One session**, in a terminal tab (<kbd>⌘T</kbd>): a small change you can describe precisely and review yourself.
- **The team**, through the CTO: work whose hard part is unwritten (security, data, performance, several areas at once), a vague request that needs framing, or work you want to leave running and come back to at the delivery, with its tests, its reviews and the reasons for each choice.
- **In doubt, ask the CTO**: it sizes the ticket, and a small one goes to one developer and a short review, without asking you anything.

The team is built to spend only what a ticket needs, and the benchmark shows it does on small tickets: half the cost of its first version, no question asked. It does not show the team costing as little as one session: it does not, yet.

## How it was measured {#how}

Two sides, on the same copy of the same project, with the same ticket text and the same budget cap:

- **One session**: a single `claude -p` with the ticket, effort high, no skill, plugin, MCP server or subagent, working until it says it is done.
- **TermF's team**: the real path, from the CTO's triage to the delivery gate, stopped before the pull request. The benchmark plays a hurried user: it approves every gate and takes every proposed answer.

Both run on Sonnet with Claude Code 2.1.330. The costs are what Claude Code reports at API prices: with a Claude subscription, nothing extra is billed, and the same work draws on your 5-hour and weekly usage limits instead. Quality is measured by hidden tests copied in once the agent is done, the project's own tests, a strict typecheck, and a blind judge (Sonnet) that sees only the ticket and the diff, and says for each criterion whether it is met. The projects are small and realistic: a task tracker of 340 lines, then a shared task-list app of 2,100 lines with planted problems.

## Small tickets: a team the size of the ticket {#small}

On a first pass, quality was a tie and the team cost 7.3 times more, often for the same fix line for line. So the team was changed to size itself. The CTO now sends a trivial change to one developer with the project's checks as its review, and a small ticket with testable criteria to one developer and a short review, writing the task itself; the gates of the path and the plan wait only for large runs; the typecheck and the lint run with the tests and send a failure back to the task's own session; the review sends back anything the ticket did not ask for; and each role reads less. Then the same tickets were measured again.

Three small tickets (a bug, a field with its migration, error messages), averages:

| | Time | Cost | Sessions | Hidden tests | Judge | Typecheck clean |
|---|---|---|---|---|---|---|
| One session | 26 s | $0.10 | 1 | 100 % | 9.0 | 100 % |
| Team, first pass | 101 s | $0.72 | 6.2 | 100 % | 8.7 | 67 % |
| **Team, sized to the ticket** | **69 s** | **$0.34** | **3.0** | **100 %** | **9.0** | **100 %** |

The team went from 6.2 to 3 sessions, from 0.72 to 0.34 dollar and from 101 to 69 seconds, with no question asked and no type error left. It is still 3.4 times the cost and 2.7 times the time of one session. The floor is known: each session writes about 20,000 tokens of prompt and tools to the cache, and the triage, the task and the review do not share them.

## Complex, realistic tickets {#complex}

Three tickets on the shared task-list app, written like real ones: they give the API's contract and leave the rest to the developer, as real tickets do. The hidden tests check that part: a reader cannot write, an expired link is refused, nothing leaks between accounts; 10,000 tasks paged in under 150 ms, each exactly once; 20 concurrent edits all applied.

| Ticket | Side | Time | Cost | Hidden tests | Criteria met | Judge |
|---|---|---|---|---|---|---|
| Sharing with roles and invitations | One session | 203 s | $0.67 | 15/15 | 86 % | 8.0 |
| Sharing with roles and invitations | Team | 953 s | $3.32 | 15/15 | 91 % | 8.0 |
| “The app is slow on big lists” | One session | 103 s | $0.35 | 14/14 | 38 % | 7.0 |
| “The app is slow on big lists” | Team | 348 s | $2.51 | 14/14 | 100 % | 8.0 |
| “Some edits disappear” | One session | 49 s | $0.22 | 12/12 | 67 % | 7.0 |
| “Some edits disappear” | Team, sized small | 173 s | $0.61 | 12/12 | 83 % | 7.0 |
| **Average** | **One session** | **132 s** | **$0.45** | **100 %** | **72 %** | **7.4** |
| **Average** | **Team** | **492 s** | **$2.14** | **100 %** | **90 %** | **7.7** |

On the slow lists, the single session wrote a clean cursor pagination but left what a good engineer would have seen: no index for the filters, a condition that keeps the database from using the index, internal columns leaking into the responses. The team loaded the views in batches, covered sorting, filters and ties, and added the indexes in a migration. On sharing, both were serious about security; the team took 16 minutes. On the lost edits, the CTO judged the ticket small and sent it to one developer: there, the team was no better than one session. Its rule now counts data that could be lost and concurrency as sensitive work, which gets the full team (not measured again yet).

## What the team does better, and worse {#better}

**Better**

- More of the unwritten criteria met on complex tickets: 90 % against 72 %.
- More tests: 60 against 45 at the end of a realistic ticket.
- No type error left, since the typecheck runs with the tests.
- A log of every decision: why a step was skipped, the plan, each review round.
- Nothing pushed without you: it stops at the delivery.

**Worse**

- Cost and time, always: 3.4 times on a small ticket, 4.8 times on a complex one.
- Large tickets stay slow: 16 minutes and 3.32 dollars for the sharing ticket.
- On a small, clear ticket, no measurable gain in quality.
- The sizing can be wrong: a data-loss bug sized small got no more than one session.

## Limits {#limits}

- Two small, clean projects without dependencies; a real codebase was not part of these figures.
- Few trials: one to three per ticket and side in the later series. The noise between two trials of the same ticket is large.
- One model, Sonnet, on both sides; Haiku and Opus were not tried.
- The judge is a model that sees only the diff: it can miss context, or count a fair fix around the ticket as out of scope.
- The hidden tests did not separate the two sides: both passed them all.
- Your time is not counted: the benchmark approves every gate at once.

The full report, with every table, the tickets and the raw results, is in [docs/BENCHMARK.md](https://github.com/vassilidev/TermF/blob/main/docs/BENCHMARK.md) (in French, with a summary in English); [bench/](https://github.com/vassilidev/TermF/tree/main/bench) runs it again on your machine.
