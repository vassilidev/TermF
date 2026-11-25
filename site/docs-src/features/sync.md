---
title: Encrypted sync
order: 9
---

Sync keeps your settings, projects, conventions, vault and, if you choose, your SSH keys and your dev config the same on every machine, encrypted end to end. With a private GitHub repository as the remote, it is a backup too.

## Set it up

1. Set a sync password. TermF gives you a recovery key: keep it.
2. Pick where it syncs: a git remote, a synced folder, an S3 bucket (any compatible service) or a WebDAV server.
3. Choose what syncs: config, projects, conventions, the vault, SSH keys, dev config.

From then on it syncs in the background. Changes merge three ways, key by key for TOML files; conflicts show side by side. What a pull changes applies at once.

## What each choice carries

| Choice | What it carries |
|---|---|
| Config | `termf.toml`, layouts, schedules, the global library, the library scout's results, refusals and journal |
| Projects | Each project with its repositories, Ops connections, saved SQL queries, Forge account and ticket sources, its library and its recipes |
| Conventions | The conventions of each project |
| Vault | The vault, sealed by its own master password, merged item by item |
| SSH keys | The key pairs of `~/.ssh` and all of `~/.ssh/config` |
| Dev config | Claude Code and git, as below |

**Dev config** brings what it takes to code the same way on another machine:

- From `~/.claude`: `CLAUDE.md`, `settings.json` (with your enabled plugins and their marketplaces), `keybindings.json`, and the `skills`, `agents`, `commands`, `output-styles` and `hooks` folders.
- Claude Code's auto memory of each project in your home folder. It lands on another machine only when the project is at the same place there, under its own home folder.
- The MCP servers of `~/.claude.json`, and only them: the rest of that file (your account, history, each project's state) stays.
- `~/.gitconfig`, and the global excludes file it names when it is in your home folder.

It never carries your credentials, your conversations, caches, todos, shell snapshots or the plugins' downloaded copies: Claude Code installs the plugins again from their marketplaces. Paths into your home folder follow each machine's home folder. A link (a dotfiles setup) is neither followed nor written through.

## A new machine

1. Set up sync with the same place and password.
2. A project's repository missing on this machine: **Clone here** fetches it.
3. A lost device: **Forget** removes it; it can no longer read what follows.

## Defaults

- Encrypted with Argon2id and XChaCha20-Poly1305; file names are masked.
- `local.toml` never leaves the machine, nor does the history of conversations and runs: each machine keeps its own.
- The vault, SSH keys and dev config stay off until you turn them on; SSH keys also ask you to confirm a warning.
- A file changed on two machines is never overwritten: you choose, side by side.

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
