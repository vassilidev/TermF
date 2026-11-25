---
title: Library and skills
order: 11
---

The library holds what your agents get (roles, rules, skills, preferences, workflows and commands) in layers you can see and change, matched to the stack of each repository.

## The library of roles, rules and skills {#library}

Each item lives in a layer: shipped, packages, global, project, repository. The most specific layer wins, and the library tells you why each item applies.

![The Library: what the project's sessions get, filtered by kind, with the accessibility skill open on why it is on, layer by layer, and what it costs](shot:library "What the sessions of a project get, and why. Sample data.")

1. Open **Library**. The list is what the project's sessions get: on in this project first, then what a repository offers, then what is off. Filter by kind (skills, commands, roles, preferences, rules, workflows, plugins) or search.
2. Open an item to see why it is on or off, layer by layer, when the agent reads it, what it costs in tokens and who uses it.
3. Switch it on or off for **This project only** or for **Every project**; **Undo** puts it back. **Copy to edit it here** puts a copy of a shipped item in the project's library, where it replaces the shipped one.
4. A repository can neither switch itself on nor hide another level.
5. What a session gets goes in as a Claude Code plugin and a system prompt file.
6. Import your own items from `~/.claude` with **Import from Claude Code**, or start a new skill with **Write with Claude**.
7. **To do** gathers what the scout, the conventions and the updates propose, each in one gesture.

Shipped items are off until switched on, except the skills that match your stack or are always on. Your global and project items are on.

### Move your setup to another machine

Export your setup to one archive, with a preview and a choice per item, and import it elsewhere. The archive holds no secret.

```sh
# the library from a terminal
termf library ls | explain | enable | disable

# your setup in one archive
termf config export | import --dry-run
```

> [!NOTE]
> The archive is not signed. The shipped rules are written in French, the rest in English.
>
> **Tried so far.** The library view was checked by eye in the app; switching a skill off for a project is driven in the real app on macOS by an end-to-end test.

## Skills by stack {#skills}

An agent works better when it knows the framework and its version. TermF reads each repository's stack and turns on the matching skills: 38 ship, from React and Laravel to Terraform and Kubernetes.

1. TermF reads the manifests and lockfiles: frameworks, test runners, databases, infrastructure, and their versions.
2. Each skill names the stacks it is for; the matching ones switch on. `tests-from-criteria` is always on.
3. A **Stack** card goes into every session's system prompt.
4. Skills shipped inside npm packages, a Python virtualenv or Composer vendors switch on once you trust the repository.

Vendors' MCP servers are offered, and added only on your click. You can switch any skill off, or on for a stack it does not match.

> [!NOTE]
> A manifest changed without a commit is seen at the next commit or restart. Package skills work only for npm, a virtualenv and Composer for now. The skills have not yet run with a real Claude Code.
>
> **Tried so far.** Tested with automated tests and a fake Claude Code.

## Skills for command-line tools {#cli-skills}

Seven skills ship for the tools every role uses: `git`, `gh`, `curl`, `jq`, `sed` and `awk`, `make` and `lsof`. They favour safe options and machine-readable output, nothing interactive, commands that work on macOS and Linux, and guard rails before anything destructive.

- Every role gets them, the CTO and the reviewers included, and sessions without a role too.
- `git`, `curl`, `jq`, `sed` and `awk`, and `lsof` are always on; `gh` turns on for a repository with a GitHub remote, `make` for one with a Makefile.
- Their short cards go into the Stack section of each session that gets them.
- They follow the [run guard](security.md#guard): no push, no pull request, reads through `gh api` GET. A command that throws work away or writes to GitHub runs only when you or the brief asked for it.
- `psql`, `mysql`, `docker`, `kubectl` and `terraform` stay in their stack skills.

> [!NOTE]
> A GitHub Enterprise host or an SSH alias is not recognised as GitHub.
>
> **Tried so far.** Checked by the catalogue's automated controls, like the stack skills.

## Language servers and evals {#language-servers}

When the language server of your stack is installed (rust-analyzer, typescript-language-server, pyright, gopls, intelephense, ruby-lsp, jdtls, kotlin-lsp, sourcekit-lsp, csharp-ls), TermF declares it to the Claude Code sessions it starts, so the compiler's errors show up as the agent edits.

1. Install the language server of your stack as you usually would; TermF finds it on your `PATH`.
2. The project's **Configuration** shows each server: declared to the sessions, turned off for the project, or not installed, with a link to install it.
3. `[lsp] enabled = false` in the project's file turns them off.

The skills and key roles that ship with TermF also have evals: test cases a model plays and another judges. Every pull request checks that each case is readable by Claude Code; running them costs money and is never automatic. For contributors:

```sh
cd app && pnpm evals
```

> [!NOTE]
> Only the servers of the official Claude Code plugins are known. The project's card was checked with a simulated backend.
>
> **Tried so far.** A real Claude Code loaded TermF's language server declaration and received the diagnostics of a Swift file; the evals ran for real on Haiku, judged by Sonnet.

## Conventions in plain words {#conventions}

Write your team's rules as sentences. Claude Code reads the repositories, read-only, and proposes for each sentence a rule, a check, a skill or a tool setting, for you to review one by one.

1. Write the conventions in plain words.
2. Claude Code reads the repositories and proposes an item for each.
3. Review each proposal; its impact is measured in a sandbox before anything is applied.
4. The change is journaled; each sentence keeps an on and off switch and a level.

It runs on opus at high effort; `[conventions]` sets the model and effort.

> [!NOTE]
> The impact is measured on macOS only (its sandbox); elsewhere it says “not measured”. Hooks it writes are shell scripts, not for Windows.
>
> **Tried so far.** Checked by eye in the app; not yet run with opus on a big project.

## The library scout {#scout}

The scout reads each item of your library against the web, with sources, and proposes changes you review like the conventions.

1. A Claude Code with only web search and fetch reads your items.
2. It writes a sourced critique and proposals, item by item.
3. You review them; anything that can run code needs your explicit confirmation.
4. What you refuse is remembered.

It runs on sonnet with a three-dollar budget per run; `[scout]` sets the model and budget.

```sh
# start a review
termf library scout
```

> [!NOTE]
> Not yet run with a real Claude Code. A run cannot be cancelled once started.
>
> **Tried so far.** Tested with a fake Claude Code.
