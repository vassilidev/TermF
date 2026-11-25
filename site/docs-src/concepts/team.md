---
title: The CTO and the team
order: 3
---

You talk to one agent, the CTO; it brings in a team of {roles} agents, each a Claude Code session with one job, its own model and its own permissions, sized to each ticket.

## Talk to the CTO {#cto}

Each project has one permanent conversation with the CTO, on the project's Home. You ask it where something lives, where a run is at, or for a piece of work in your own words, and it turns that into the right run for the team. It triages every team run, relays what the team needs from you, and never writes code.

1. Write to the CTO on the project's Home, or press <kbd>⌘⇧H</kbd> from anywhere.
2. It reads the repositories, the runs, the tickets and what waits for you, then answers in place.
3. When something needs doing, it asks through a card: start a team run, answer a question, approve a step, stop a run.
4. Approve or deny the card. Write `@front` (or any role) to reach a role at work, `@expert` for the [project expert](#expert).
5. Done with a subject? **Topic done** archives the thread and starts an empty one. The CTO keeps the project's memory, not the thread. See [In a conversation](ask-panel.md#conversation).

### What it can do

It reads the project's repositories with Read, Grep and Glob. It has no shell and no network, and it acts only through TermF's own tools: it can read the project, the runs, the tickets and what waits for you, and it can start a team run, answer a question, request changes, approve, stop, or send a message to a role. It can also do what the screen you ask from offers, such as connecting Jira or moving a ticket: see [Agents that act for you](agent-actions.md). Every one of these actions waits on your approval card.

### What it costs

The conversation has no budget of its own, unlike a run. Every gate of a run wakes the CTO for one turn, so it can tell you what the gate is about. It runs on opus at high effort by default; change it in `[roles.cto]`.

```sh
# where a run is at, from any terminal
termf team status <run>

# the emergency stop
termf team stop
```

> [!NOTE]
> Notices waiting for the CTO do not survive a restart of the app (they stay on screen). Text from tickets and agents reaches it as data; each action still waits on your card.
>
> **Tried so far.** Its tools were run with a real Claude Code (Haiku) through automated tests; a message to the CTO is driven in the real app on macOS by an end-to-end test, with a fake Claude Code.

## Sized to the ticket {#sized}

The team spends only what a ticket needs. At the [triage](runs.md#triage), the CTO sizes the request:

| The request | Who works on it |
|---|---|
| A trivial change | One developer, with the project's checks as its review |
| A small ticket whose criteria are already testable | One developer and a short review, the task written by the CTO itself |
| Medium work | The project's workflow, with only the steps it needs |
| Large, vague or sensitive work (authentication, payments, personal data, data that could be lost, concurrency) | The full team |

Only a large run waits for you at the path and the plan; a smaller one asks you nothing until the delivery, unless something real needs you.

> [!WARNING]
> It is not free. On a small ticket, the sized-down team still costs about 3.4 times one Claude Code session, for the same quality. On complex tickets, it met more of the unwritten criteria, 90 % against 72 %, for 4.8 times the cost. See [the team or one session?](team-vs-solo.md)

## The roles {#roles}

{roles} roles ship with TermF: {role_names}. Each card says what the agent does, when it is called, what it never does, the Claude model it runs on, its effort, and what it may touch. Most planners and reviewers run on opus, most builders on sonnet.

{{roles}}

### How a role works in a run

1. The plan gives each task a role and a size.
2. TermF starts a fresh Claude Code session for the step, with the role's instructions and a brief: the task, the criteria, your answers, the earlier reports.
3. The size sets the effort, medium, high or extra high, within the role's range. Turns are capped at 60, 120 or 240 (`[team] max_turns`).
4. The step ends with a report in a fixed shape, and the next step starts from it.

Steps run in `acceptEdits` mode; the roles that only read cannot edit a file.

## Change a role {#role-file}

Every role is a Markdown file in `library/roles/`, in the format of Claude Code's own agents. This is the start of the front-end role:

```text
---
name: front
description: Front-end developer. Use for interface work — screens,
  components, styles, client-side logic, accessibility. …
runtime: claude-code
model: sonnet
effort: high
effort_range: [medium, xhigh]
---
You are the front-end developer of the team. You build the interface
the task asks for, with the project's own components and conventions.
```

You can change a role for every project (`termf.toml`), for one project, for one squad (`[squads.roles.<role>]`) or for one repository, without touching its prompt, in `[roles.<role>]`: its model, effort and range, its permission mode, and its lists of skills, preferences, write paths and denied tools, where `+x` adds and `-x` removes.

```toml
[roles.front]
model = "opus"
effort_range = ["high", "xhigh"]
skills = ["+accessibility"]
```

You can also write your own roles in the [library](library.md).

```sh
# every role of a project, with its settings and where each comes from
termf team squads
```

> [!NOTE]
> A repository can only pick a model or an effort and add to what is denied; it can never widen what a role may do. The descriptions of the shipped roles are in English even in the French interface.
>
> **Tried so far.** Whole team runs ran with a real Claude Code (Haiku on every role) through automated tests, from the triage to a real pull request; the key roles have evals, played and judged by a model.

## Squads for bigger projects {#squads}

A small project needs nothing: one implicit squad has every role, with no limit. A bigger one can split into squads, a team per area of the project, each with its lead, its members and how many of each, its repositories, its paths and how many tasks it runs at once. The roles that serve every squad, like the product owner or the security reviewer, form the staff.

![The Team tab of a project's Configuration: two squads, billing and web, the staff, and the roles with their model and layer](shot:team "Squads, staff and roles of a project. Sample data.")

1. Create squads in the project's **Configuration › Team**, or accept the ones the setup assistant proposes.
2. A run that spans one squad is planned by its lead. One that spans several is planned by the CTO in batches, with the `squads` workflow, and each squad's lead splits its batch into tasks that TermF grafts into the run.
3. Each task stays inside its squad's repositories and paths, and the scheduler respects each squad's limits.

```toml
[team]
staff = ["po", "secops"]

[[squads]]
id = "billing"
roles = { back = 2 }
repos = ["billing-service"]
paths = ["src/**", "tests/**"]
max_parallel = 2
```

`[[squads]]` takes `id`, `lead`, `roles` with counts, `repos`, `paths`, `domains` and `max_parallel`.

```sh
# the squads, the staff and each member's settings
termf team squads [<project>]
```

> [!NOTE]
> A large batch's sub-plan waits for your approval by default (`[team] subplan_gate`). Splitting into batches stays the CTO's choice: it may plan small parts itself. The squad editor was checked with a simulated backend only.
>
> **Tried so far.** With a real Claude Code, two squads over two repositories: the CTO (Sonnet) gave each squad a batch, each lead (Haiku) returned its sub-plan, grafted into the run, up to the delivery gate.

## The project expert {#expert}

The expert answers questions about the project (why it is built this way, where something lives, what was decided last month) from a local memory of it, then from the code, and gives its sources. The team calls it instead of searching on its own.

1. TermF indexes the project's memory on your machine: the Claude Code conversations of its repositories and worktrees, the agents' conversations and run logs, the library, `CLAUDE.md` files and rules, the docs, the last 2,000 commits; for claude.ai artifacts, the title and the link only.
2. Secrets are masked before anything is written.
3. Ask on the Home, switching the conversation from **CTO** to **Expert**, or with `termf ask`. The product owner, product manager, architect and CTO ask it too.
4. It searches the memory, reads the code if needed, and answers with its sources.

It is on by default, indexed a minute after TermF starts, every 15 minutes and before each question. It is read-only, with no shell and no web, and a question from an agent is capped at one dollar. `[memory] enabled` turns it off, and `exclude` leaves out a source.

```sh
# ask from any terminal
termf ask <question>

# search the index
termf memory search <query>

# index again now
termf memory reindex
```

> [!NOTE]
> The index stays on your machine, but the passages the expert reads go to the model, as in any Claude Code session. Not yet tried on a big project: the time of the first indexing and the size of the base are not measured.
>
> **Tried so far.** Run with a real Claude Code (Haiku) through automated tests, the memory served by a fake TermF.

## Brand, copy and motion {#creative}

Some tickets are not code: a logo, the words of a new screen, a launch video. Three roles ship for them, each with skills of its own (`brand-identity`, `logo-export`, `ux-writing`, `motion-video` and `stock-media`).

- **Brand designer** (opus, high): compares three directions, lets you choose through the questionnaire, then delivers the guidelines printed to PDF, the design system as code, the SVG logos and every export.
- **UX writer**, `copy` (opus, medium): writes the texts the ticket asks for, in every language of the project, with options in the questionnaire when the text is not given, and never rewrites an existing one.
- **Motion designer** (sonnet, high): gets its storyboard approved through the questionnaire before anything is rendered, then works from real captures of the product.

The plan gives them a task only when the request calls for it; no shipped workflow includes them. The setup assistant proposes brand and copy for a project with a front end, and motion for one with a public site, as staff. Media come from sources without a key (Openverse, Wikimedia Commons): TermF puts no key in an agent's environment, and a key you set yourself in the session's environment (Pexels, Pixabay, Unsplash…) is used by its name, never shown.

> [!NOTE]
> Never run in a real run nor evaluated yet. The image and video tools they call (rsvg-convert, resvg, Inkscape, ImageMagick, ffmpeg) have not been tried, and no media API has been called.
>
> **Tried so far.** Shipped with their skills and evaluation cases; nothing run for real yet.

## The specialists {#specialists}

Costly bugs hide in the fields a generalist forgets: a migration that locks a table, a webhook received twice, a screen a keyboard cannot reach, a consent nobody asked for. Specialists ship for these fields: the database engineer, the technical writer, the integration engineer, the site reliability engineer, the accessibility auditor, the AI engineer, the product analyst and the compliance analyst.

- The plan gives a specialist a task when the request touches its field, like any role. No shipped workflow includes them, except the database engineer's review.
- The database engineer reviews, as a step of the default workflow, every run whose tasks changed a migration; the step is skipped otherwise.
- The accessibility auditor opens the changed screens in [the agents' browser](#agent-browser) and audits them against WCAG 2.2 AA and the RGAA. It changes nothing: the front-end developer fixes.
- The compliance analyst sources each obligation from an official text, to be checked by a legal professional. It changes nothing either.
- With squads, a specialist must be in the staff or in a squad to get a task.
- They never use a real account, key or production system of a provider.

> [!NOTE]
> Never run in a real run nor evaluated yet. The worktrees of tasks and reviews are not prepared yet, so a database engineer's task that must run its migrations may not reach the run's database (not tried).
>
> **Tried so far.** Shipped with their instructions; nothing run for real yet.

## The agents' browser {#agent-browser}

The front, QA, UX and SEO agents open your pages and check them themselves. The browser gives a session a headless Chrome on its worktree's own dev server: it opens a page at a desktop, tablet or mobile size, in the light or dark theme, reads it, clicks, fills forms, waits, takes screenshots and reads the console and the network. It cannot reach your accounts or the rest of the web.

1. An agent opens a page of its worktree. The first time, TermF starts the repository's dev server on a port of its own.
2. It reads the page as an accessibility tree, clicks, fills fields, waits for a text and takes screenshots.
3. It reads the console and the network, including the requests TermF blocked.
4. When the session ends, Chrome, its throwaway profile and the dev server go away.

It is on for front, QA, UX and SEO, in team steps and in direct conversations. It uses the system's Chrome, Chromium or Edge, headless, with a throwaway profile: never your keychain, your extensions or your sync. Only localhost and the origins you allow are reachable; downloads are refused, and no tool runs JavaScript the agent wrote.

| Setting | What it does |
|---|---|
| `[roles.<role>] allowedTools = ["+mcp__termf__browser_*"]` | Gives the browser to another role |
| `[browser] origins` | Allows more origins, a staging site for example |
| `[browser] tiles = true` | Offers it to the project's Claude Code terminals too, with a permission prompt |

> [!NOTE]
> Tried for real on macOS only. On Windows, the throwaway profile and the dev server's log may stay in the temporary folder. A page an agent checked does not count as a [visual review](visual-review.md), which needs a before.
>
> **Tried so far.** Run with a real Chrome on macOS, and with a real Claude Code (Haiku) in the QA role, which opened a page, clicked and read the console.

## Watch the agents work, live {#live}

TermF shows the work as it happens, in plain words: you always know who is doing what, and since when.

1. In a conversation with the CTO or any agent, the answer arrives word by word; *Thinking…* shows before the first word.
2. Each tool appears in plain words with its state: *Reads src/app.ts*, *Runs npm test*, *Searches the web: …*, running, done or failed, with its detail one click away.
3. On a run's page, **Right now** lists every step at work (role, current action, for how long, usage so far), and **What's happening** tells the whole run in order.
4. Each agent tile shows its last action under its name; each running run on the home page says what its agent is doing.

Every view shows the shape of what is loading instead of a blank screen or a spinner, after a short delay so fast loads do not flicker. Values from the vault are masked in everything shown live. Motion follows your system's *reduce motion* setting.

> [!NOTE]
> Codex and Gemini tools are shown by their own names (*Uses …*). Checked with the end-to-end tests and a fake Claude Code that streams word by word, not yet with a real long team run.
>
> **Tried so far.** An end-to-end test drives the real app on macOS: the CTO answers word by word and shows the tool it is using.

## What has been tried {#tried}

With a real Claude Code, on Haiku, through automated tests: whole team runs from the triage to a real pull request on GitHub, a vague request turned into questions, a run across two repositories with each lead's sub-plan, the review loop up to its escalation, a stop and a restart in the middle of a task; the CTO's tools; the expert answering through `ask_expert`. On Sonnet, the [benchmark](team-vs-solo.md) against a plain session. Every view is driven in the real app by end-to-end tests with a fake Claude Code; little has been tried by hand yet. Brand, copy, motion, the specialists and the research roles have not run in a real run yet.

Next: what happens step by step, in [Runs and their steps](runs.md).
