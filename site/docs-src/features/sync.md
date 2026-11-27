---
title: Encrypted sync
order: 9
---

Sync keeps your settings, projects, conventions, vault and, if you choose, your SSH keys and your dev config the same on every machine, encrypted end to end. With a private GitHub repository as the remote, it is a backup too.

## Set it up

1. Set a sync password. TermF gives you a recovery key: **Copy** it or **Download** it as a file, and keep it off this computer.
2. Pick where it syncs: a git remote, a synced folder, an S3 bucket (any compatible service) or a WebDAV server.
3. Choose what syncs: config, projects, conventions, the vault, SSH keys, dev config.

From then on it syncs in the background, and turning on a choice later syncs at once. What a pull changes applies at once.

## What each choice carries

| Choice | What it carries |
|---|---|
| Config | `termf.toml`, layouts, schedules, the global library, the library scout's results, refusals and journal, [your environment](environment.md) (`env.toml` and its scripts) |
| Projects | Each project with its repositories, Ops connections, saved SQL queries, Forge account and ticket sources, its library and its recipes |
| Conventions | The conventions of each project |
| Vault | The vault, sealed by its own master password, merged item by item |
| SSH keys | The key pairs of `~/.ssh`, all of `~/.ssh/config` and `~/.ssh/known_hosts` |
| Dev config | Claude Code and git, as below |

**Dev config** brings what it takes to code the same way on another machine:

- From `~/.claude`: `CLAUDE.md`, `settings.json` (with your enabled plugins and their marketplaces), `keybindings.json`, and the `skills`, `agents`, `commands`, `output-styles` and `hooks` folders.
- Claude Code's auto memory of each project in your home folder. It lands on another machine only when the project is at the same place there, under its own home folder.
- The MCP servers of `~/.claude.json`, and only them: the rest of that file (your account, history, each project's state) stays.
- `~/.gitconfig`, and the global excludes file it names when it is in your home folder.

It never carries your credentials, your conversations, caches, todos, shell snapshots or the plugins' downloaded copies: Claude Code installs the plugins again from their marketplaces. Paths into your home folder follow each machine's home folder. A link (a dotfiles setup) is neither followed nor written through.

## Several machines {#machines}

Machines that each have their own settings, keys and hosts combine; nothing is replaced.

- TermF's settings and projects merge key by key, even the first time two machines meet. `~/.ssh/config` merges host by host and option by option, with the comments of each line; `~/.gitconfig` section by section; Claude Code's settings and the MCP servers of `~/.claude.json` key by key.
- Two different SSH keys with the same name are both kept: yours stays, the other machine's lands beside it as `<name>-2`, known to this machine only.
- `known_hosts` keeps the lines of every machine, yours first, without duplicates; none is ever removed.
- What belongs to one machine stays on it, never sent, never taken: an absolute path outside your home folder (such as `/opt/homebrew/bin/gpg`), a server started from such a path, `UseKeychain`, a git credential helper.
- A real disagreement shows side by side, limited to what differs. **Keep this version** keeps the other side's additions too.

## A new machine {#new-machine}

1. Set up sync with the same place and password, or let the [first launch](first-launch.md) do it.
2. **See what will change**, before **Join the sync**: for each choice, what is in conflict, merged, kept both, added here or sent to the others. Nothing is written until you join.
3. A project's repository missing on this machine: **Clone here** fetches it.
4. A lost device: **Forget** removes it; it can no longer read what follows.

## Always in sight {#status}

- A line above **This machine**, in the navigation, shows the sync's state and its last run. Its menu has the last successful sync, **Sync now** and **Open sync**.
- **This machine and the sync**, on the sync's page, says **Same as the sync**, or what is left to send, receive, combine or settle, for each choice; a click shows the files. It is a dry run: nothing is written, here or on the remote.
- The same line shows whether the computer is kept awake, and why; its menu changes the mode in one click.

## Undo the last sync {#undo}

Before it writes, each sync keeps a copy of the files it changes, the last 10, in `.sync/backups/` of TermF's folder. **Undo the last sync** puts them back byte for byte, except those you changed since, and holds them in the conflicts until you choose. SSH keys are not in it: a sync only ever adds them.

## Defaults

- Encrypted with Argon2id and XChaCha20-Poly1305; file names are masked.
- `local.toml` never leaves the machine, nor does the history of conversations and runs: each machine keeps its own.
- The vault, SSH keys and dev config stay off until you turn them on; SSH keys also ask you to confirm a warning.
- A file changed on two machines is never overwritten: what differs waits for you, side by side.

```sh
# the sync
termf sync setup | status | push | pull

# what syncs, and forget a device
termf sync scopes config,projects,conventions,vault,ssh-keys,dev-config
termf sync forget <device>

# keys and passwords
termf sync recover | change-password
```

> [!NOTE]
> Not yet used between two real machines, or against a real S3 or WebDAV service: the tests use local stand-ins and throwaway home folders. Settings read at start (the API confirmation) and agents already running keep their old values until restart.
>
> **Tried so far.** Tested with automated tests, including two machines syncing through a local S3 server that checks the signature as AWS does, and through WebDAV.
