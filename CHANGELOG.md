# Changelog

What changed from one release of TermF to the next, by feature. Every feature has its page on the
[website](https://vassilidev.github.io/TermF/features.html), with its defaults and its limits.

## 0.4.0

TermF 0.4.0 lets the agents act: on any screen, the agent does what the screen offers, with your OK, and never sees
a secret. Conversations take images and queued messages, tickets come in full on a kanban, the database studio draws
charts and the whole schema, and Ops adds Redis, Nginx, Laravel, server health and Laravel Forge.

### Agents that act in TermF
- **They act, you approve.** In the ⌘I panel and in the CTO's and the expert's conversations, an agent no longer
  gives you values to type: it proposes the action of the screen you are on, and one card shows what it will do.
  Nothing changes without your OK.
- **Secrets never go through the agent.** A password or a token is typed in the card and goes straight to the vault,
  or to the system keychain for a ticket token. The agent can only say where to find it.
- **Every screen.** Ops connections and saved queries, the database studio, ticket sources, CI/CD (deploy tools,
  runs, workflows, redeploys, webhooks), the vault, the library and its plugins, Git branches and cleanup, settings,
  the scheduler, the project's configuration, the setup wizard and the Design system scan. Not only adding: changing
  and removing too.
- **A delete says what it loses**, in the card. An action that needs something first, such as a vault that does not
  exist yet or is locked, proposes that first.
- **What stays yours**: typing in terminals, answering an agent's own requests, revealing secrets, SSH keys, what the
  agents may do or reach, and installs. A query run by an agent shows its rows to you, not to the agent.
- **Empty screens that lead somewhere.** Tickets offers to find and connect your ticket tool with the CTO, and CI/CD
  without a workflow leads to connecting your deploy tool.

### Conversations
- **A queue while the agent works.** A message typed during a turn waits above the field, each one removable, and
  goes as one message when the turn ends.
- **Images in a message.** Paste, drop or attach PNG, JPEG, GIF or WebP images in any conversation, the DBA's
  included. TermF stores only how many there were, never the images. Codex and Gemini read text only, and TermF says
  so.
- **A question while a card waits.** During an approval request, the field stays open: ask about the action, and the
  agent answers before asking again. *Ask for help* on the card writes the question for you.
- **Topic done.** One click in the header, or *New topic* in ⌘K, archives the conversation and starts an empty one
  with the same role and settings. The CTO and the expert keep the project's memory, not the thread. Undo brings the
  old one back.
- **Archives.** Archived conversations sit folded under the list: read one, reopen it, or delete it with a card that
  says what goes.

### Tickets
- **Connect Jira in one go.** *Connect Jira step by step* offers Claude Code's Atlassian server first if you have
  one. Otherwise it finds the sites in your commits or guesses them from your email, checks your email and token and
  keeps the token in the keychain, then lets you tick the spaces, with those your commits cite already ticked.
- **Several Jira sites and spaces** in one project. The old single-site setting still reads, and is saved in the new
  form next time.
- **Browse every ticket**, grouped by space, with filters (space, status, assignee, current sprint, search) and pages
  of 50. The Home still shows what waits for you.
- **The whole ticket in TermF.** The description rendered, never raw JSON. For Jira: the attachments, the status you
  change in place, the assignee, priority, sprint, sub-tasks and linked tickets. The comments, a field to add one,
  then start a run or *Ask the CTO*.
- **A kanban for every source.** *Browse* switches between a list and a *Board* whose columns are the source's real
  statuses: Jira workflows, a GitHub Project's Status field, Linear and Azure DevOps states. Drag a card to change
  its status in the source: it moves at once, and comes back with the reason if the tool refuses.
- **When the source has no board**, as with GitHub Issues without a Project or GitLab, *Organize with AI* asks the
  CTO to propose columns and set them up. *Sort with AI* proposes moves, a priority and the tickets to start, applied
  with your OK.

### The database studio
- **The DBA in the same conversation as everywhere else**: the same messages, approval cards and queue, and under an
  answer with a query, its check, *Put in the editor*, *Run* and *Save*.
- **Charts of a result.** *Table* or *Chart*: a line along a date, bars by a text, or your choice of bars, line,
  area or pie (only when it reads well), with a tooltip and a legend. Export as PNG or SVG.
- **The whole schema.** Tables as cards, foreign keys drawn from line to line, laid out for you, with search, zoom
  and *Fit*. Click a table for its indexes and links, *Query this table* or *Ask the DBA*. Foreign keys are read on
  Postgres, MySQL, MariaDB and SQLite.

### Ops
- **Redis**, direct or through a server's SSH tunnel, with the password in the vault: keys by pattern (never `KEYS`),
  each key's type, expiry and value, and `INFO` in plain words. Deletes, expiries and flushes go through a card that
  says what is lost, then the environment's name to type.
- **Nginx** on an SSH server: running or not, `nginx -t` and its messages, the enabled sites, and the access and
  error logs live, with 5xx as errors. *Reload* only after a passing `nginx -t`.
- **Laravel** on an SSH server with the site's folder: `laravel.log` live with exceptions folded, failed jobs to
  retry or forget, Horizon, the schedule and `php artisan about`.
- **Server health**, read only: CPU, load, memory, disks, running services, listening ports and when the SSL
  certificates served by nginx expire.
- **Laravel Forge accounts.** One account per project, its token in the vault. *Import from Forge* lists the servers
  and sites to tick, then creates each server's SSH connection, each site's database (password in the vault) and a
  Forge deploy target in CI/CD. For each site: its deployments, their logs, and *Redeploy* after a confirmation.
- Agents do these too, each through its card: Redis writes, the Nginx reload, Laravel jobs, the Forge account, the
  import and a redeploy.

### Fixes from real use
- Long lines, such as a refused command, wrap: the conversation no longer scrolls sideways in the ⌘I panel.
- Text cut short by an ellipsis shows in full on hover, on every screen.
- In Conversations beside another view, the approval card no longer pushes the thread sideways, answers wrap, and
  the field keeps its width with the buttons below it when space is short.
- A long path in SSH keys › Hosts no longer crushes the other columns, and the Git cleanup proposal in ⌘I wraps its
  buttons.
- ⌘I and Ops with no project yet offer *New project* instead of pointing at an empty navigation.
- The swipe review shows your first answer at once.
- Run › *Say something to the team* uses the shared chat, with its queue and cards: no hand-made chat is left.
- The deletes and edits that were missing: an SSH host (the card shows the lines removed from `~/.ssh/config`), a
  Forge account, a webhook, a copy in the Library.
- In a narrow window, lists stack above their detail instead of crushing it: contents, the SQL studio, tickets.

### The website and the README
- **The launch film v6**: 49 seconds instead of 76, on what is new, in light and dark.
- **New docs pages** for agents that act, Jira sites and spaces, the ticket board, a ticket in full, Redis, Nginx,
  Laravel, server health and Laravel Forge, with new screenshots and front page cards.
- **A short README**, in English and French, with release and Claude Code badges.

Not tried against the real thing yet: real Jira sites, Redis, Nginx, Laravel and the Forge API, and images with the
real Claude Code.

## 0.3.0

TermF 0.3.0 redesigns the whole app: one design system on every screen, an agent one keystroke away with ⌘I, a
guided path from the first launch to a project's first run, a team that asks you less, and a database studio.

### A redesigned interface
- **A new design system.** New colors in light and dark, Instrument Sans for text and JetBrains Mono for code, and
  shared components: buttons, lists, side panels, wizards, and toasts with Undo.
- **Ask from any screen.** ⌘I, or the Ask bar at the bottom of each view, opens a panel beside it with the agent that
  fits the screen: the CTO on the Home, the lead on a run, devops on CI/CD and Ops, the DBA in the database studio,
  secops in the vault, the expert in Git and Files. What you are looking at goes along as context. Answers stream in,
  and nothing changes without your OK, with Undo after.
- **Today, on the Home.** One sentence on where things stand, what waits for you first, the agents at work in plain
  words, the runs, the setup and the tickets to pick up.
- **One wizard to create a project.** Five steps instead of two wizards: how to start, your repositories, what TermF
  understood in a few sentences per repository, a few questions one at a time with a proposed answer and its proof
  (or the CTO's advice), then the team and the tools, kept by default.
- **A first-launch guide.** Welcome, Claude Code checked live (installed, signed in, how you pay), your first project,
  the CTO, and where to find help. Nothing is written in `~/.claude`.
- **A library you never get lost in.** Search first, filters by type, what is really active in this project and why,
  the repository layer in view, one switch per item with Undo, and for each item its layers, when the agent reads it
  and what it costs in context.
- **A desktop-grade workspace.** A side navigation you resize or fold to icons (⌘B), splits in one keystroke (⌘D,
  ⌘⇧D), right-click menus on tabs and terminals, a tab dragged into a split, and native File and View menus.
- **The Run page, live and guided.** One lane per agent, those waiting for you first, with the answer in place; the
  work so far folded into a sentence and a failure shown with its output; the next step and what your answer starts;
  the changed files with their diff; the cost projected at the current pace.
- **A visual CI/CD.** Each branch's last runs at a glance, the job graph updated live, the first error with its lines
  and *Why?* to ask devops, webhooks with their deliveries, and deployments through the Laravel Forge, Vercel,
  Netlify, Fly.io and Railway command lines, linked to their pull request and run, redeployed after your approval.
- **Git cleanup and a git expert.** *Clean up* lists the branches and worktrees that can go and what each would lose.
  What loses nothing goes at once, with Undo; anything else asks once and names the loss. The git expert explains a
  conflict or a commit and proposes a cleanup you apply in one click.
- **Every other screen** on the design system: loading shapes, empty states that offer the next action, a toast after
  each action, the Ask bar with the right agent. A terminal selection or lines of a file go to an agent; Design system
  warnings are grouped by kind; vault secrets are added, replaced and removed in the app, with the audit log in plain
  words; Ops recipes are cards; deleting a project sits in a danger zone that says what goes and what stays.
- **Narrow windows.** Every screen works at 1024 or 900 pixels wide: side panels push the content while it keeps room,
  and otherwise cover it and fold away.

### A practical team
- **Fewer prompts, same guard rails.** In a run, commands that only read, and the project's own checks, tests and
  lint, run without asking inside the step's worktree. Writes, redirections, subshells and paths outside the worktree
  still ask. One switch per project turns it off.
- **Skills used.** Each step's brief names up to six skills that fit the ticket, with when to use them.
- **Roles on demand.** The CTO can add a role the project's team lacks, such as front-end for a screen or docs for
  documentation, and the run's path says so.
- **Visual review of a new page.** With nothing to compare to, the review captures the new page alone and says so.
- **Pull requests that tell the work.** The plan, the acceptance criteria checked, what each task did, the checks run
  and the visual review.
- **Talk to the team** from the Run page: to the CTO, who leads the run, or to the agent of a step.

### A database studio
- **Any database in seconds.** Paste a URL, pick one your repositories declare (`.env` files, Laravel, Rails, Prisma,
  Compose), or go through SSH. The password goes to the vault, and a failed test says what is missing.
- **More engines, read-only for sure.** SQLite, MariaDB and ClickHouse join Postgres and MySQL. SQL Server, MongoDB and
  Redis are refused, with the reason: none of them can promise a read-only session.
- **Schema, saved queries and exports.** The schema beside the editor, with completion; saved queries, which can
  become a recipe for agents; copy or export to CSV, TSV, JSON, NDJSON, Markdown, SQL `INSERT` and Excel.
- **Ask the DBA.** It writes, explains or optimizes a query from the schema, and the query is checked before it runs.

### The Design system view
- **The project's real fonts**, from its `@font-face` files and `@fontsource` packages (Google Fonts and Bunny Fonts
  only on a click), a type specimen, and the base components redrawn from the project's tokens with their variants,
  in light or dark. One navigation: colors, typography, spacing, shadows and motion, components, inconsistencies and
  the brand guide.

### The watch and conventions, as actions
- **A To do tab** at the top of the Library: every question, proposal and update from the watch and the conventions,
  one action per line (answer, review, turn on for this project, run the watch, infer from the code).
- **The watch** says why, what, where and its sources for each proposal; it can run every Monday, be cancelled, or be
  handed to the team as a pull request.
- **Conventions** inferred from the code, Undo, a cost cap (2 dollars by default), and the repositories' files handed
  to the team.

### Fixes from trying it in the real window
Each area was walked through in the real app, with the real Claude Code, `git` and `gh` in a throwaway home. What
broke is fixed, with a test:
- The CTO's triage really sizes the team: every ticket used to fall back to the full workflow.
- A small run no longer loops over its acceptance criteria; a web page made by the back-end agent no longer skips the
  visual review; a closed run no longer shows as waiting for you.
- New projects get their checks, so runs no longer end on "no command known"; Laravel's test command is found; the
  project analysis shows each read live instead of a spinner.
- Deleting claude.ai artifacts no longer asks again for each one.
- The expert reindexes the project before each question, so it finds what was just committed; conventions show their
  progress live.
- File search no longer shows the previous results while a new search runs.

### Security
- The `.claude/settings.local.json` that TermF writes is kept out of git, so a repository's personal settings never
  sit ready to commit.
- A project check runs without asking only with plain paths: a setting that swaps the program it runs still asks.
- Database passwords stay in the vault, never in the project file or a log; agents see data only through your
  approval card.
- The vault asks for the current master password before changing it.
- In the Ask panel, a secret sends only its name.
- `cookie` 0.18.3, for RUSTSEC-2026-0594.

### The website
- **A short front page**, "Stop using Claude Code (alone).", with the launch film, now on the redesigned app, and
  **Laravel-style docs** with search.
- A benchmarks page, a roomier layout, release notes on the download page shown as headings and lists, and
  screenshots and docs of the redesigned app, with new pages for the Ask panel and the database studio.

Still not signed by Apple or Microsoft: see the README for the first launch. Not tried against the real thing yet:
deployments with real Forge, Vercel, Netlify, Fly.io or Railway accounts; the database studio against real Postgres,
MySQL, MariaDB and ClickHouse servers, and the DBA, the watch and the skills of a run with the real Claude Code.

## 0.2.0

The first public release: TermF installs in one command and keeps itself up to date.

### The team
- **A proportionate team.** The CTO sizes each ticket: a trivial one gets one developer and the project's checks, a
  small one one developer and a short review, with no question and no gate before the delivery; the full team, with its
  gates, only for large, vague or sensitive work. Typecheck and lint run with the tests, a failure goes back to the
  task's own session, and the review sends back any change the ticket did not ask for.
- **Team or one session? Measured.** A benchmark against a plain Claude Code session on the same tickets, published in
  full: see `docs/BENCHMARK.md` and the website.
- **29 roles.** Brand designer, UX writer and motion designer; database, docs, integrations, site reliability,
  accessibility, AI, product analytics and compliance specialists; researcher, competitive analyst and reverse engineer.
- **Bigger projects.** Each squad's lead plans its share of a run, a squad editor, and runs across several
  repositories with one linked pull request in each.
- **The base kept up to date.** Before the delivery, the latest main is merged into the run; the merge integrator
  resolves conflicts.
- **Discipline.** A definition of done checked at the delivery, paths a role may not touch, a decision log every step
  reads, and the CTO asked before you.
- **The agents' browser.** Front, QA, UX and SEO agents open their own pages in a throwaway headless Chrome.
- **The agents' work, live.** The CTO's answer word by word, each tool in plain words with its state, a run's
  *Right now* and *What's happening* panels, and the last action on each agent tile; every view shows the shape of what
  is loading.
- **The agents' work, live.** The CTO's answer word by word, each tool in plain words with its state, a run's
  *Right now* and *What's happening* panels, and the last action on each agent tile; every view shows the shape of what
  is loading.
- **Your sessions' loops, visible.** A badge on each tile running a `/loop` or a scheduled task, its detail, and one
  click to turn it into a Scheduler task that outlives the session.
- **Codex and Gemini.** Codex CLI and Gemini CLI in terminals, conversations and team roles (not tried with the real
  tools yet).

### Projects and tickets
- **A project from scratch.** Describe an idea; TermF creates the folder, the first commit and, if you like, the GitHub
  repository, then the team frames it, researches it, and you settle the decision before the skeleton is built
  (`kickoff` workflow).
- **The project setup.** Pick repositories as cards; TermF detects stack, commands, CI and tickets, and proposes the
  team, ticket sources, worktree recipe, CI and skills.
- **More ticket sources.** Linear, GitLab (delivered as a merge request) and Azure DevOps, beside GitHub Issues, GitHub
  Projects, Jira Cloud and MCP.
- **Git in the project.** Branch graph, worktrees, stage by file, hunk or line, commit, stash, merge, rebase,
  cherry-pick and revert with conflicts, tags, history and blame.

### Your machine
- **A password vault per project.** Logins with two-factor codes, secure notes and secrets; a security key (FIDO2),
  Touch ID or Windows Hello; synced item by item; encrypted export; import from Bitwarden, 1Password, browsers and
  KeePassXC.
- **Agents get a minimal environment.** Only the variables they need; a key from the vault on request, through a card
  you answer.
- **Servers, databases and logs.** Ops connections and recipes with approval cards and masked output, a read-only SQL
  console, live logs, and the production `.env` read, compared and written into a run's worktree.
- **Encrypted sync** through S3 and WebDAV too, SSH keys and the vault included; forget a lost device; clone a
  project's missing repository.
- **The Design system view** and the **visual review page** (side by side, slider, overlay, heatmap, pins, swipe).
- **Remote Control** from any Claude Code session, with its link and QR code; a menu bar icon and a dock badge for
  what waits for you.
- **Usage and limits.** With a Claude subscription, amounts read as an API equivalent, the 5-hour and weekly limits
  are in view, and the CTO checks them before it starts a team.
- **Language servers** for the agents, and evals for the shipped skills and roles.
- **Skills for command-line tools**: git, gh, curl, jq, sed and awk, make, lsof.

### Install and the app
- **One command to install**, checked against the release's `SHA256SUMS`; **automatic updates**, signed, on a stable
  or a beta channel; `termf` on your `PATH` in one click; the tools TermF needs listed on first launch.
- **A native app on every system.** Laid-out `.dmg`, Windows installers in English and French, `.deb`, `.rpm` and
  AppImage with a desktop entry, a native macOS menu, a window that remembers its place.
- **Lighter and faster.** Hidden terminals let go of their display, views load on demand, and the idle app stays
  close to zero CPU: on a Mac, a cold start in about 0.4 second and about 100 MB at rest (release build).
- **Tested end to end.** The real app driven on macOS and Linux, one scenario per feature; a final pass over the
  interface (foldable navigation, settings in reach, hints, examples for the CTO).

Not signed by Apple or Microsoft yet: see the README for the first launch. Not tried against the real thing yet:
Remote Control with a claude.ai account, a hardware security key, the Linear, GitLab and Azure DevOps services, real
servers and databases for Ops, Codex and Gemini, and installing an update.

## 0.1.0

A first pre-release: the CTO and a team of 13 roles with squads, triage, questions with proposed answers, code and
security review; tickets from GitHub and Jira with runs in their own worktrees and a two-step delivery; terminals in
tabs and splits; the layered library and 38 stack skills; the Claude Code view, the scheduler, CI/CD, SSH keys,
encrypted sync, the vault's engine, visual review captures, the project expert and the `termf` command line.
