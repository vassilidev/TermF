---
title: Encrypted sync
order: 9
---

Sync keeps your settings, projects, conventions, vault and, if you choose, your SSH keys the same on every machine, encrypted end to end.

## Set it up

1. Set a sync password. TermF gives you a recovery key: keep it.
2. Pick where it syncs: a git remote, a synced folder, an S3 bucket (any compatible service) or a WebDAV server.
3. Choose what syncs: config, projects, conventions, the vault, SSH keys.

From then on it syncs in the background. Changes merge three ways, key by key for TOML files; conflicts show side by side. What a pull changes applies at once.

## A new machine

1. Set up sync with the same place and password.
2. A project's repository missing on this machine: **Clone here** fetches it.
3. A lost device: **Forget** removes it; it can no longer read what follows.

## Defaults

- Encrypted with Argon2id and XChaCha20-Poly1305; file names are masked.
- `local.toml` never leaves the machine.
- SSH keys stay off until you turn them on and confirm the warning.

```sh
# the sync
termf sync setup | status | push | pull

# what syncs, and forget a device
termf sync scopes | forget <device>

# keys and passwords
termf sync recover | change-password
```

> [!NOTE]
> Not yet used between two real machines, or against a real S3 or WebDAV service: the tests use local stand-ins. Settings read at start (the API confirmation) and agents already running keep their old values until restart.
>
> **Tried so far.** Tested with automated tests, including two machines syncing through a local S3 server that checks the signature as AWS does, and through WebDAV.
