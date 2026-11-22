---
title: Security and privacy
order: 4
---

Agents that write code and run commands on your machine need guard rails: this page lists each one TermF puts in place, what it stops, and what it does not.

## Local-first {#local}

TermF has no server and no telemetry. It runs on your machine, and so does everything it keeps: projects, runs, logs, the project memory. Your code goes where Claude Code sends it, to the model, exactly as it would without TermF. The other connections are the ones you set up: GitHub or Jira for tickets, an MCP server if you add one, and the remote you pick for the [sync](sync.md).

## The run guard {#guard}

Every session of a run (the terminal of a ticket, each step of a team, and any step you take over) starts with `termf guard` as a Claude Code `PreToolUse` hook. A run does not start without it, and it is not optional.

It refuses:

- `git push` in its forms, `git -C . push` included;
- `gh pr`, GitHub API calls that are not reads, writes to GitHub issues, starting or cancelling CI;
- `termf deliver`, `close` and `run`;

also when they hide inside `sh -c`, `eval` or `$(…)`. The agent gets the reason, and tells you its work is ready:

```text
● Bash(git push -u origin termf/57-export-invoices-csv)
  ⎿  `git push -u origin termf/57-export-invoices-csv` is refused.
     In a TermF run, pushing, pull requests and writes to the
     tracker wait until the user delivers the run. Commit your
     work if you like, then tell the user it is ready.
```

```sh
# the hook itself; Claude Code calls it, not you
termf guard
```

> [!NOTE]
> Defense in depth, not a sandbox: a script the agent writes and then runs, a git alias or another tool still pass. Managed settings that allow only managed hooks, or disable all hooks, would turn it off.
>
> **Tried so far.** Its refusal of `git -C . push` was checked with a real Claude Code 2.1.330.

## Delivery in two steps {#delivery}

Delivering a run is yours. **Deliver…** shows the files to commit, the commits not pushed yet and a fingerprint of that preview; **Deliver** commits exactly that, pushes and opens the pull request. If the worktree changed in between, it is refused. See [delivery in two steps](runs.md#delivery).

## The isolated local API {#api}

The app, the `termf` command line and the agents talk over a local API. Since an agent can run `termf` too, the API cannot trust whoever calls it.

### Tokens and confirmations {#local-api}

- Each terminal and each headless session gets its own token, with only the rights it needs, revoked when its process ends.
- Your own token lives outside the agents' data folder, and the sessions TermF starts are denied reading it.
- Starting a run or a team, delivering, closing, approving a gate, accepting a permission, typing into a session or opening an agent from the command line shows a banner in the app and waits for your **Allow**. The banner gives up after five minutes, and when the caller has left.
- Some things are refused even if you allow them: an agent session without the guard, with every permission bypassed, or with TermF's own tools.

The banner cannot tell who is asking: your own command line and an agent that runs it look the same, which is why it says so.

A few commands do not wait for confirmation: opening a plain shell, sending to an agent, interrupting or stopping it, taking it over or handing it back, answering a team's questions, asking for changes, changing a run's rules or stopping a team, posting a review on a pull request, running a scheduled task, asking the expert, reindexing the memory, and locking or unlocking the vault.

Confirmation is on by default, `[api] confirm_cli = true`, read when TermF starts.

```sh
# an example of a command that waits for you in the app
termf deliver <run>
```

> [!NOTE]
> A process set on it can still find the token file by a pattern, then use everything but the reserved actions.
>
> **Tried so far.** Checked with a real Claude Code that an agent cannot print its own token; the banner, with a simulated backend.

## The CTO's session {#cto}

The CTO talks to you and reads your repositories, and that is all its session can do on its own. It runs restricted, with Read, Grep and Glob only: no shell, no file edits, no web, no sub-agents, and only TermF's MCP tools, whose every action waits on your approval card. Its token reaches only its own tools. What it reads from tickets and from other agents comes to it marked as data.

## Untrusted input and secrets {#input}

### Tickets are data, not orders {#security-input}

A ticket is written by someone else, and it may try to give orders. It reaches the agent as a framed file, marked untrusted, never as instructions; the reports of earlier steps are framed the same way in each brief.

The sessions TermF starts are denied `.env` files, private keys, `/proc/*/environ` and the usual ways of dumping the environment (`printenv`, `env`, `export`, `declare -p`…), along with TermF's own token folder. Add to a role's `deny` list for more.

> [!NOTE]
> The deny rules do not see every way around them, such as `python -c`, `sh -c` or a copy of the file. The tool sessions TermF runs for itself (scout, conventions, recipe, setup assistant) keep their own settings, without these denials.
>
> **Tried so far.** With a real Claude Code (Haiku), a session TermF started was refused `.env`, a private key, `cat .env`, `printenv` and `env`, with every permission it asked for granted.

### A minimal environment, keys on request {#agent-env}

Every agent TermF starts, in a terminal or headless, Claude Code, Codex and Gemini included, gets a short list of variables (the path, the language, the terminal, the proxies, what its own CLI needs to sign in) and nothing else, even after your shell profile has run. A terminal you open yourself is not filtered.

When a task needs an API key:

1. The agent asks for it by name. A card appears on the project's Home and in **Waiting for you**: who asks, why, a link to get one, a hidden field.
2. **Save and give**: the value goes to the vault as a `dev` secret of the project, and the project's file names it in `[agents.secrets]`.
3. The agent gets it when its step starts again, masked in everything the session logs or shows. A locked vault means the agent starts without it, and is told why.
4. The project's vault lists the keys given to agents, each with **Remove**.

A key is never given to the CTO or the expert, never from another project, and never a `prod` secret. Shipped roles that may ask: `motion` (media platforms) and `legal` (official legal APIs).

| Setting | What it does |
|---|---|
| `[agents] env_allow` | Names or prefixes of variables every agent may keep (also in **Settings › Agents' environment**) |
| `[agents.secrets]` | The vault keys given to agents, globally or per project |
| `[agents] env = "inherit"` | Gives back the whole environment, as before |

> [!NOTE]
> The list is a defense in depth, not a sandbox: a program the agent runs can still read files your user can read. On Windows, and for a command only your shell knows (a function), the shell profile is not filtered again after it runs. The cards were checked with a simulated backend.
>
> **Tried so far.** With a real Claude Code 2.1.330 on macOS, a terminal's hooks and status line still reach TermF through the allowlist; a real login shell whose profile exports a secret was filtered by the tests.

## Secrets at rest {#rest}

The [vault](vault.md) keeps secrets encrypted with Argon2id and XChaCha20-Poly1305, by scope, with prod asking the password again, an automatic lock after ten minutes and on sleep, and a hash-chained audit log; agents can never reveal a value. The settings [sync](sync.md) is encrypted end to end the same way, with masked file names and a recovery key. [SSH keys](ssh-keys.md) are read only from `~/.ssh`, the files TermF wrote and the ones you pick.

## Servers, databases and logs {#ops}

The [Ops](ops.md) connections are invisible to agents until you allow them, masked or in full. When an agent asks to run a recipe, an approval card shows you the exact command, the target and the environment, with a countdown; prod asks your master password. Your own clients run it (`ssh`, `psql`, `kubectl`…), keys are never read and secrets never go on the command line, a database opens read-only for a reading recipe, and the output is masked before the agent sees it.

After a prod recipe, the session needs your OK to reach the web or other tools, so data from production cannot quietly leave by another way; a session that read production data is also cut from Remote Control. The SQL console refuses anything that is not a read before it is sent, then runs the query in a read-only transaction. A production `.env` shows you its keys with the values masked; an agent only ever gets the keys.

## What it does not cover {#limits}

These guard rails are defense in depth on one machine, not an operating system boundary. A process set on getting around them still can. In plain words:

- A script an agent writes and then runs, a git alias, or a tool other than Bash can still push or read a secret. The deny rules do not see every form: `/usr/bin/env`, `sh -c`, `python -c`, `node -e`, `grep . .env`, a copy of the file, or `../.env`.
- Claude Code settings managed by an organisation that allow only managed hooks, or that disable all hooks, would turn the guard off. This has not been tried.
- A user process can still find TermF's token file by a pattern and use the API with every right but the reserved actions; it can also read another process's environment (`ps eww` on macOS, `/proc` on Linux).
- On Linux and Windows, a vault key remembered in the system keychain can be read by any process of the user; on macOS, the keychain keeps it to the app.
- The tool sessions TermF runs for itself, the scout, the conventions, the worktree recipe and the setup assistant, keep their own settings, without the secret denials.
- The interface is driven by end-to-end tests in the real app, but the confirmation banner itself has only been checked with a simulated backend. Windows and Linux are built and tested in CI but have not been run by hand.
- The minimal environment is not re-applied after your shell profile on Windows, or for a command only your shell knows (a function).

What was verified with a real Claude Code 2.1.330: the guard refusing `git -C . push`, an agent unable to print its own MCP token, trusting repositories, a session refused `.env`, a private key, `printenv` and `env` with every permission it asked for granted, and a terminal whose hooks still reach TermF through the minimal environment. In the real app, a terminal's own token is refused for a reserved action. Every known gap is listed in [docs/STATUS.md](https://github.com/vassilidev/TermF/blob/main/docs/STATUS.md) §4, in French.
