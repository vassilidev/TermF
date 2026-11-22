---
title: The termf CLI
order: 1
---

`termf` drives everything the app does from any terminal: sessions, tickets, runs, the team, agents, the library, keys, sync, the vault, CI, schedules, worktrees, Ops and visual reviews.

## Install it

`termf` ships inside the app. Put it on your `PATH` in one click from **Settings › What you need**, or by hand (see [Installation](installation.md#cli)). It lives in `TermF.app/Contents/MacOS/` on macOS, in `/usr/bin` with the deb and rpm, next to the app on Windows.

```sh
# every command, with its options
termf help
```

It talks to the running app through its [local API](security.md#local-api). Commands that start a run or a team, deliver or close, approve a gate, accept a permission, type into a session or open an agent wait for your confirmation in the app (`[api] confirm_cli`). A few others do not, such as `termf team answer`: the list is in [Security](security.md#local-api).

## Sessions

```sh
# list sessions
termf ls

# open a session in a folder (a shell without --agent)
termf open <path> [--agent <name>]

# type text then Enter in a session (id or part of its title)
termf say <session> <text>...

# print status changes, live
termf watch
```

## Tickets and runs

```sh
# GitHub issues assigned to you, or what the project's sources list
termf inbox [--project <id> [--refresh]]

# work on a ticket (URL, owner/repo#42, repo#42, #42, ACME-123) in a new worktree
termf run <ticket> [--project <id>] [--repo <name>]

# resume the run's Claude Code conversation after a restart
termf run resume <run>

# list runs, or show one run and its events
termf runs [<run>]

# show what the run would send, then commit, push and open the pull request
termf deliver <run> [--yes]

# remove the run's worktree (--force drops work not pushed)
termf close <run> [--force]
```

## The team

```sh
# the teams of ticket runs: state, workflow, cost, what waits
termf team ls

# start the run's team and stop once its plan is made
termf team plan <run> [--workflow <name>] [--rule <rule>=<level>]... [--budget <usd>]

# start the run's team, or go on with it (rule levels: off, advise, warn, block)
termf team run <run> [--workflow <name>] [--rule <rule>=<level>]... [--budget <usd>]

# start the team on a request written here, in a new run
termf team run --request <text> [--project <id>] [--repo <name>] [--workflow <name>] [--budget <usd>]

# the CTO's routing: workflow, steps, squads, budget, why
termf team route <run>

# answer a step's questions at once
termf team answer <run> <step> <question>=<answer>... [--note <text>]

# its plan, its steps, what waits for you, what the rules found
termf team status <run>

# what happened, step by step
termf team events <run> [--after <n>]

# let a gate pass, or run a step again with your feedback
termf team approve <run> <step>
termf team revise <run> <step> <feedback>...

# rule levels for the ticket, or one task of the plan
termf team rules <run> [--task <id>] <rule>=<level>...

# emergency stop of one team, or of every team at work
termf team stop [<run>]

# the squads and staff of a project, each role with its settings and their layers
termf team squads [<project>]
```

## Agents

```sh
# headless agent sessions: state, cost, context
termf agent ls

# a headless Claude Code session in a folder (default: here), driven by TermF
termf agent start [--prompt <text|->] [--dir <path>] [--title <t>] [--project <id>] [--agent <name>]
                  [--role <name>] [--model <m>] [--effort <e>] [--permission-mode <p>] [--budget <usd>]

# its next message, once its turn is over
termf agent send <id> <text>...

# what it said and did
termf agent events <id> [--after <n>]

# stop its turn, answer a permission request, end its process
termf agent interrupt <id>
termf agent allow|deny <id> <request> [<message>...]
termf agent stop <id>

# go on with the conversation in a tile, then give it back
termf agent takeover <id>
termf agent handback <id>
```

## The expert and the project memory

```sh
# ask the project's expert (budget: $1 at most)
termf ask <question>... [--project <id>] [--budget <usd>]

# search the project memory
termf memory search <query>... [--project <id>] [--source <s>]... [--limit <n>]

# bring the memory up to date
termf memory reindex [--project <id>]
```

## Projects, library and config

```sh
# trust the project's repositories in Claude Code, or stop
termf project trust <id> [--off]

# the library: each item's layer, state and why (kinds: skill, command, role, preference, rule, workflow)
termf library ls [--kind <kind>] [--repo <path>] [--project <id>]
termf library explain <name> [--kind <kind>] [--repo <path>] [--project <id>]
termf library enable|disable <name> [--kind <kind>] [--project <id>]

# import skills, agents and commands from ~/.claude
termf library import-claude [--repo <path>] [--project <id>] [--overwrite]

# the library scout: a sourced critique of your items
termf library scout [--item <kind>/<name>]... [--project <id>] [--model <name>] [--budget <usd>]

# your config and library in one archive
termf config export [-o <file>]
termf config import <file> [--dry-run] [--replace <item>]... [--replace-all]
```

## Claude Code

```sh
# a setting in each scope's file (masked), and which wins
termf claude settings get <key> [--scope <s>] [--repo <path>]
termf claude settings set <key> <value> [--scope user|project|local] [--repo <path>]
termf claude settings unset <key> [--scope user|project|local] [--repo <path>]

# MCP servers and plugins
termf claude mcp add <name> <json> [--scope local|project|user] [--repo <path>]
termf claude mcp remove <name> [--scope <s>] [--repo <path>]
termf claude plugin install|remove <plugin> [--scope user|project|local] [--accept-command <sha256>]
termf claude plugin enable|disable <plugin> [--scope <s>]

# saved conversations of the repository and its worktrees
termf claude sessions [--repo <path>]
```

## Worktrees, CI and visual reviews

```sh
# prepare a worktree with its project's recipe, or undo it
termf worktree setup [<path>]
termf worktree teardown [<path>]

# GitHub Actions runs, jobs and logs
termf ci runs [--repo <path>] [--branch <name>] [--status <status>] [--pr <n>] [--page <n>]
termf ci view <run> [--repo <path>]
termf ci logs <job> [--repo <path>] [--errors]
termf ci rerun <run> [--failed] [--repo <path>]
termf ci cancel <run> [--repo <path>]
termf ci dispatch <workflow> [--ref <branch>] [--input key=value]... [--repo <path>]

# capture and compare the screens before and after
termf review start [--run <run>] [--repo <path> --base <ref> --head <ref>] [--route <path>]...
termf review show <id> | --run <run>
termf review decide <id> <screen>|all approve|reject|changes [--comment <text>] [--pin x,y,<text>]...
termf review pr <id> [--yes]
```

## Scheduler

```sh
# scheduled tasks, their schedule and next run
termf schedule list

# a Claude Code task run on a schedule (local time)
termf schedule add <name> --cron '<min hour day month weekday>' --prompt <text|-> [--dir <path>]
                   [--tile] [--model <m>] [--effort <e>] [--permission-mode <p>] [--budget <usd>]
                   [--no-catchup] [--disable] [--machine <name>|any]
termf schedule edit <id> [--name <n>] [same options] [--headless] [--enable] [--catchup]
termf schedule rm <id>

# run it now, see past runs
termf schedule run <id> [--scheduled]
termf schedule history [<id>] [--limit <n>] [--run <run>]

# also run a headless task while TermF is closed (launchd, systemd, Task Scheduler)
termf schedule install|uninstall <id>
```

## Ops

```sh
# the project's connections, and what works for each
termf ops connections [--project <id>]
termf ops test <connection> [--project <id>]

# the recipes, and run one without an agent (output masked and audited)
termf ops recipes [--project <id>]
termf ops run <recipe> --connection <name> [--<param> <value>]... [--project <id>]
```

## Vault

```sh
# create, open, close the vault
termf vault init
termf vault unlock | lock | status

# secrets: names only, store (read on stdin), reference, reveal, remove
termf vault ls [--scope dev|staging|prod] [--project <id>]
termf vault set <name> [--scope <s>] [--project <id>] [--description <text>]
termf vault ref <name> <uri> [--scope <s>] [--project <id>] [--description <text>]
termf vault reveal <name> [--scope <s>] [--project <id>]
termf vault rm <name> [--scope <s>] [--project <id>]

# logins and notes
termf vault items [--project <id>]
termf vault item <id> [password|totp|seed|notes]

# the audit log, and checking its hash chain
termf vault audit [--limit <n>] [--jsonl]
termf vault audit verify

# export and import
termf vault export -o <file> [--project <id>]
termf vault import <file> [--project <id>] [--restore]
```

`termf vault ref` takes `op://`, `bw://`, `doppler://`, `aws-sm://` and `vault://` references. `termf vault import` reads a TermF export, or a Bitwarden, 1Password, Chrome, Edge, Firefox, Safari, KeePassXC or KeePass export.

## SSH keys

```sh
# keys, agent keys and config hosts
termf ssh ls

# create a key in ~/.ssh (asks for a passphrase)
termf ssh gen <name> [--type ed25519|ecdsa|rsa] [--comment <text>]

# check that a host accepts your key
termf ssh test <host>

# the SSH agent
termf ssh agent-add <key>
termf ssh agent-rm <key>

# make a repository (default: here) use only this key, or the default keys again
termf ssh assign <key> [<repo>]
termf ssh assign --none [<repo>]

# move a key to ~/.ssh/.termf-trash
termf ssh trash <key>
```

## Sync

```sh
# through git or a folder
termf sync setup --git <url> | --folder <path> [--name <device>] [--scopes config,projects,conventions,vault]

# through S3 (curl 8.3 or newer) or WebDAV
termf sync setup --s3 <endpoint> --bucket <name> [--region <region>] [--prefix <path>] | --webdav <url>

# state, sync now, settle a conflict
termf sync status
termf sync push | pull
termf sync resolve <path> --local|--remote

# machines, passwords, scopes
termf sync devices
termf sync change-password
termf sync recover
termf sync scopes <list>
termf sync forget <device>
termf sync disable
```

## Used by Claude Code, not by you

These commands are called by Claude Code sessions TermF starts:

| Command | Role |
|---|---|
| `termf hook <event>` | Forwards a Claude Code hook to TermF (JSON on stdin) |
| `termf statusline [<file>]` | The status line of TermF sessions: reports the context, then runs yours |
| `termf mcp` | The MCP server of TermF's tools, for the sessions TermF starts |
| `termf guard` | The [run guard](security.md#guard): refuses pushes and tracker writes in a run's session |

> [!NOTE]
> **Tried so far.** The command line is covered by automated tests; several groups (CI, SSH keys, the vault, the scheduler) were also tried end to end with the real tools, as each feature page says.
