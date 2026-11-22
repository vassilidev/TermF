# Changelog

What changed from one release of TermF to the next, by feature. Every feature has its page on the
[website](https://vassilidev.github.io/TermF/features.html), with its defaults and its limits.

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
