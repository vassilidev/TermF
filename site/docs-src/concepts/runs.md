---
title: Runs and their steps
order: 4
---

A run is a ticket being worked on, in its own worktree, from the moment you ask the CTO to the pull request; this page follows one step by step, with every place where it waits for you.

TermF is not a model. It is the program around Claude Code that keeps the order of things: it starts each agent, gives it a brief, collects its report, holds the run at the gates and keeps the log. Agents never talk to each other directly. They talk through TermF, so everything they do is visible, can be interrupted, and has a cost you can see.

## The run page {#runs}

Each run has one page, from launch to merge, with or without the team. It shows where the run is, what waits for you and what happened, and it survives restarts of the app.

1. The steps sit in a band under the title: triage, path, framing, plan, tasks, review, delivery. Click one to see its work; the page opens where the team is, or where it stopped.
2. **Live** gives each agent at work its own lane. The agents that wait for you come first, and you answer in their lane: questions, a plan to approve, a blocked step with its output. The same cards wait on the project's Home.
3. On the right, **Next step** says what your answer starts. Below it come the ticket, CI, the run's **Changes** (each file with its lines added and removed, and its diff) and the budget with where it is heading. The cost and the duration sit in the header.
4. After a restart, **Resume** picks up the same conversation, with the same environment, ports and guard.
5. Once the pull request is merged, the run closes itself if nothing would be lost (see [after the merge](#after)).

```sh
# the runs and their state
termf runs

# resume after a restart
termf run resume <run>

# close a run (waits for your confirmation in the app)
termf close <run>
```

## You ask the CTO {#ask}

Every project opens on its Home, and the Home opens on a conversation with [the CTO](team.md#cto). Ask it for a piece of work in your own words; it answers with a card asking your approval to start a team run.

You can also start from the ticket itself: **Tickets** lists the inbox of every [source](tickets.md) the project follows, and **With the team** sits next to each ticket. Pasting a URL or a key works too, and so does the command line, which waits for your confirmation in the app:

```sh
# start the team on a ticket
termf team run <ticket>

# or on a request written in your own words
termf team run --request <text>
```

## The CTO picks the path {#triage}

Every team run starts with a triage, the CTO on opus. It reads the request and the project and sizes the work, never more than it needs:

- a trivial change goes to one developer, with the project's checks as its review;
- a small ticket whose criteria are already testable, to one developer and a short review, with the one task written by the CTO itself: no framing, no plan by the lead;
- medium work, to the project's workflow with only the steps it needs;
- large, vague or sensitive work (authentication, payments, personal data, data that could be lost, concurrency), to the full team with its gates.

It returns the kind of work (bug, feature, epic, refactor, infra, research, SEO, docs, brand, copy, motion…), the size (S, M, L), the [workflow](#workflows), the steps to add or skip, the effort per step, the squads and the budget, with its rulings, their reasons and the cost of being wrong. If something real is unclear, it asks before it routes. TermF checks everything it proposes before using it.

On a large run, the path is the first gate: nothing starts until you **Approve the path**, **Change** it (workflow, optional steps, squads, run budget) or **Refuse** it. A smaller run goes on by itself, and you see it at the delivery.

| Setting | What it does |
|---|---|
| `[team] triage` | Turns the triage off |
| `[team] approve_route` | Makes every run wait at the path |
| `[team] plan_gate` | When the plan waits for you: `"large"` (default), `"always"` or `"off"` |

They work per project or for every project. The `hotfix` workflow never waits at the path. The CTO's budget advice covers every step that will run, and can only lower the run's budget.

```sh
# the decision, its reason, steps, squads and budget
termf team route <run>
```

> [!WARNING]
> Even sized down, a team run costs more than one Claude Code session: about 3.4 times on the benchmark's small tickets, for the same quality. Every gate also wakes the CTO for one turn. See [the team or one session?](team-vs-solo.md)

> [!NOTE]
> **Tried so far.** The triage ran with a real Claude Code (Haiku) through automated tests, and on Sonnet in the benchmark against a plain session: the small tickets went to one developer and a short review, three sessions, no question asked.

## The product owner frames it {#frame}

Unless the ticket already lists testable acceptance criteria, the product owner reads the ticket and the code, changes nothing, and writes the acceptance criteria, each with its source, and the list of what is out of scope. It explores the implications and the side effects, settles what the evidence settles, and turns the rest into questions for you.

When a ticket needs more, the CTO can add steps that ship turned off: a product manager writes a short PRD for a vague epic, an architect settles a design decision that crosses modules, a UX designer specifies the screens and their states before anyone builds them. The product owner, the product manager and the architect ask the [project expert](team.md#expert) before searching on their own.

## Questions, with proposed answers {#questions}

Whether they come from the triage, the product manager, the product owner, the plan or a step that got stuck, questions reach you in one form. Each has a theme (product, scope, interface, technical, security, delivery), the reason it is asked, its options and a proposed answer already filled in.

![A run's page: the plan's questions in the lane of the agent that asks, each with its reason and a proposed answer already selected, and the next step on the right](shot:run "The plan's questions, answers proposed. Sample data.")

### Answer the questions {#questionnaire}

1. The run stops on the lane of the agent that asks, and on the Home; the proposed answers are already selected.
2. Change what is wrong, add another answer or a note, and send them all at once, or answer in free text.
3. The step runs again with your answers, and they travel into every later brief of the run.

```sh
# each question with its reason, options and proposal
termf team status <run>

# answer a step's questions from a terminal
termf team answer <run> <step> <id>=<answer>…
```

> [!NOTE]
> The review's questions are written in English for the CTO and the API, and shown translated in the French interface.
>
> **Tried so far.** Run with a real Claude Code (Haiku) through the triage; the form was checked with a simulated backend.

## The lead plans, you approve {#plan}

The lead turns the criteria into tasks, one per role: what each task covers, which files it expects to touch, what it depends on and its size. Tasks of the same level can run together; a task that needs another waits for it. The size sets the effort of the agent that will do it, medium, high or extra high, within the range its role allows.

On a large run, the plan is the second gate: it waits for your approval in the run and on the Home. If something is off, **Ask for changes**: the lead plans again with your feedback, in a fresh session. A smaller plan goes on without you (`[team] plan_gate`), and for a small ticket the CTO writes the one task itself.

## The team works in parallel {#tasks}

Each task gets an agent of its role: a fresh Claude Code session, started by TermF with a brief that holds the criteria, your answers, the plan and the reports so far. The ticket itself reaches it as a framed file marked as untrusted data, never as instructions. A fresh session per step keeps each context small.

Every task works in a [worktree](git.md#worktrees) of its own. When it is done, TermF commits it and merges it into the run's worktree; a conflict goes back to the agent. Two tasks expected to touch the same files never run at the same time, and at most four agents work at once in the default workflow.

Each agent shows its state and its cost, and you can [watch it work](team.md#live). `@back` sends a role a message between turns.

## Budgets, stop and take over {#budgets}

A run never spends without a limit, and you can step in at any time.

1. The run starts with its budget, 15 dollars in the default workflow. Each step gets the smaller of its own budget and its share of what is left.
2. When it is spent, the agents stop and the run waits for you.
3. **Emergency stop** interrupts every agent of the run, stops them and kills their commands. You can start the team again afterwards.
4. Between two turns, **Take over** opens a step's conversation in a terminal with the same model, effort and permission mode; **Hand back** returns it to the team. There is never more than one process on a conversation.

After a restart of TermF, a working team is paused: nothing spends without you. The budget is the workflow's `budget_usd`, or the one you set at launch or when you change the path.

With a Claude subscription, the dollars are an API equivalent: nothing extra is billed, a run draws on your 5-hour and weekly [usage limits](claude-view.md#usage), shown at the foot of the navigation.

```sh
# stop one team, or every team
termf team stop [<run>]

# take a conversation into a terminal, and give it back
termf agent takeover <id>
termf agent handback <id>
```

> [!NOTE]
> A shell command started before a restart is not always killed. Take over is refused for the CTO and the expert, and once the budget is spent.
>
> **Tried so far.** With a real Claude Code: a budget too small, the emergency stop and a restart in the middle of a team run; a conversation taken into a real terminal, a message typed there, then handed back, the next turn knowing what was said.

## Runs across repositories {#multi-repo}

A run's plan can name any repository of the project. Each one joins the run at its first task, with its own worktree and branch, its own checks and its own base update. At the delivery, each repository gets its pull request, linked to the others.

Every repository a run touches needs an `origin` on GitHub; otherwise the plan step stops and says why.

> [!NOTE]
> Linked pull requests work on GitHub only.
>
> **Tried so far.** With a real Claude Code and the real GitHub: two pull requests, one per repository, opened and citing each other.

## Checks and rules {#checks}

When the tasks are merged, the repository's own checks run on the run's worktree: the lint, tests and build you declared for it, plus the typecheck and the lint the repository configures. A failure goes back to the task that caused it, in its own session, twice at most, then to you.

Then come the project's rules. Three ship with TermF, off until you switch them on:

- **scope-strict**: do only what the ticket asks; in doubt, stop and ask. Its check flags files changed outside the role's paths or the plan.
- **no-text-changes**: no change to a visible or editorial text that the ticket did not ask for. Its check reads the diff for changed texts.
- **visual-review**: a before and after [review](visual-review.md) of every changed screen, as a gate.

Each rule has a level, from advise (instructions in the briefs only) to block (a deviation fails the step). Set it in the library, in the project's `[rules]`, per ticket (`--rule visual-review=block`) or per task. You can write your own rules, with a check of your own.

### Workflows {#workflows}

A workflow is a list of typed steps (agents, gates, checks, rules, review, delivery) written in TOML. Six ship in `library/workflows/`:

| Workflow | What it runs |
|---|---|
| `default` | Frame, plan (a gate on large runs), tasks, checks, rules, then the code review and the security review side by side, the latest base, delivery (gate). Up to four agents at once, a 15 dollar budget |
| `direct` | One task, one short review |
| `trivial` | One task, the project's checks as its review |
| `hotfix` | No path or plan gate, one review round on bugs only |
| `squads` | The CTO plans every [squad](team.md#squads) |
| `kickoff` | A new project, from the idea to its skeleton |

Product manager, architect, UX, SEO, research, competition and reverse engineering steps ship turned off, for the CTO to add.

The project picks a default workflow (`[workflows] default`); the CTO may route a run to another and add or skip optional steps. Write your own workflows globally, per project or per repository: a variant writes only what changes, starting from another with `extends`, such as a different budget or a step turned off.

```sh
# change a run's rules
termf team rules <run> …

# why an item applies, and at which level
termf library explain <name>
```

> [!NOTE]
> A repository's workflow cannot set commands, a budget or rules. The text-changes check reads the diff line by line, not with a parser. Rules with a custom check, and the secrets check, have not been run yet.
>
> **Tried so far.** Gates and rules were tested with a fake Claude Code; whole runs through the default workflow ran with a real Claude Code (Haiku) up to a real pull request.

## Review and security {#review}

The reviewer reads the run's integrated diff against the criteria, the contracts between tasks and the project's conventions. The security reviewer goes over the same diff for vulnerabilities, abuse cases and vulnerable dependencies, in two passes, finding then confirming, so false positives are dropped before they reach you.

### The review loop {#code-review}

1. After the tasks and the checks, the reviewer reads the run's whole diff. Each finding has its file and line, the scenario that breaks and the fix. A change the ticket did not ask for is a finding too.
2. Important findings go back to their task, in the task's own session; the reviewer looks again, up to two rounds by default. Nits are reported, not fixed.
3. A finding with no clear owner, or still open after the last round, becomes a question for you. Leaving it as it is asks for a reason, which stays in the run's report.
4. The security reviewer (`secops`, opus at extra high effort, citing the OWASP Top 10 categories) follows, in two passes.

Every workflow has a review but `trivial`, where the project's checks are the review; the one of `direct` is short, without the security review. The CTO cannot skip either review.

| Setting | What it does |
|---|---|
| `[team] review` | Turns the review off |
| `[team] review_rounds` | 1 to 10 rounds |
| `[team] review_nits` | `"report"` the nits (default), or `"fix"` them |

```sh
# the run's events, the review rounds included
termf team events <run>
```

> [!NOTE]
> A finding is tied to a task by its id or by the role of a single task, never by its file. The reviewer is asked to check each finding on its own; TermF does not check them.
>
> **Tried so far.** The review loop ran with a real Claude Code (Haiku) through automated tests: findings sent back to their task, a second round, the review again after the integrator's merge, and a last finding escalated to the CTO, then to you.

## The visual review, when it is on {#visual}

When the visual-review rule is on and the interface changed, the run stops once more. TermF starts the project's dev server for the run and for its base, captures each changed screen and shows you each one before and after, to approve, ask for changes or refuse. See [Visual review](visual-review.md).

## The latest base, merged in {#base}

Main rarely waits for a run. Just before the delivery, TermF fetches the base branch and merges it into the run's work, never with a rebase and never with a push. It runs in the default, hotfix and squads workflows; there is nothing to do.

### The merge integrator {#integrator}

1. Base unchanged: the step is done. Clean merge: the checks run again.
2. Conflict: the merge waits in a worktree of its own and the merge integrator, an agent of its own, resolves each conflict so the result does what both sides meant. TermF refuses a leftover conflict marker, then the checks and the code review read the merge.
3. If the integrator is stuck, it asks the CTO first, then you.
4. If the pull request falls behind after the delivery, **Update with the base** on the run page runs the same step again, and you deliver again, without a force-push.

A conflict between the run's own tasks stays with the agent of the task. The integrator's model and effort are in `[roles.integrator]`.

> [!NOTE]
> Accepting a run as it is, without the base, is flagged in the definition of done.
>
> **Tried so far.** The resolution ran once with a real Claude Code (Haiku) on a deliberate conflict; the rest is covered by automated tests with a fake Claude Code and a fake GitHub CLI.

## You deliver {#deliver}

The last gate is the delivery, and it always happens in two steps. No agent can do it for you: every session of a run carries TermF's [guard](security.md#guard), which refuses `git push`, pull requests and tracker writes.

### Deliver in two steps {#delivery}

1. When the team is done, the run is ready to deliver; approve that gate.
2. **Deliver…** shows exactly what will be sent: the files to commit, the commits not pushed yet, and a fingerprint of that preview.
3. **Deliver** commits that, pushes the branch and opens the pull request, with `Closes #N` for a GitHub issue; on GitLab, a merge request.
4. If the worktree changed since the preview, the delivery is refused and you look again.

Optional tracker writes (a comment, a transition), set per source with `comment` and `transition`, are tried once each.

```sh
# deliver from a terminal (waits for your confirmation in the app)
termf deliver <run>
```

> [!NOTE]
> Delivery by the team itself is not built: after the team's gate, you deliver from the run.
>
> **Tried so far.** Tried against the real GitHub on a sandbox repository: the push, the pull request with `Closes #N`, the comment on the issue.

## After the merge {#after}

TermF checks the pull request's state 30 seconds after start, then every 15 minutes. Once it is merged, the run is closed if nothing would be lost: its worktree, its local branch, its database copy and its ports go away. A pull request closed without a merge, or a session still busy, is only flagged **Ready to clean up**. `[tickets] cleanup = "auto" | "ask" | "off"` changes this (auto by default).

> [!NOTE]
> The cleanup after a merged pull request has not been tried against the real GitHub (a closed one has). A `claude --resume` started outside TermF on the same conversation is not detected.
>
> **Tried so far.** Resume ran with a real Claude Code 2.1.330; runs, delivery and cleanup against the real GitHub on a sandbox repository; the page with a simulated backend.

## What has been tried {#tried}

TermF is in early development, and this is what is known to work. Whole team runs have gone from a ticket to a real pull request on GitHub with a real Claude Code, on Haiku, through automated tests: the triage, the gates, the plan, the tasks, the checks, the review and the security review, the latest base and the delivery; so have the CTO's tools and the expert, a stop and a restart in the middle of a run. On Sonnet, the team was measured against a plain Claude Code session. Every view of the app is driven by end-to-end tests in the real app, with a fake Claude Code, but little has been tried by hand on real projects; Windows and Linux have been built and tested in CI but not run by hand. The full list of what is not verified is in [docs/STATUS.md](https://github.com/vassilidev/TermF/blob/main/docs/STATUS.md), in French.
