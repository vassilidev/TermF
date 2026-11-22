---
title: Vault and passwords
order: 8
---

The vault keeps a project's secrets, logins with their two-factor codes and secure notes encrypted on your machine, and agents can never reveal a value.

![The vault: the project's logins, notes and secrets, with a GitHub login open on its two-factor code](shot:vault "The vault of a project. Sample data.")

## Create the vault

Create it with a master password, in the app or from a terminal:

```sh
termf vault init
```

It is encrypted with Argon2id and XChaCha20-Poly1305, and locks after 10 minutes and when the machine sleeps.

## Add and use items

1. Add logins (addresses, user name, password, two-factor key) and notes, for one project or for all of them.
2. Secrets live in `dev`, `staging` or `prod` scopes; `prod` asks the password again.
3. Search and filter logins, notes and secrets. Copy a password or the current two-factor code without opening the item; the clipboard is cleared after 30 seconds.
4. Generate passwords or passphrases.

A revealed password hides again after 30 seconds, a note after two minutes. Every reveal goes in a hash-chained audit log. A secret can also be a reference to 1Password, Bitwarden, Doppler, AWS Secrets Manager or Vault.

## Security keys, Touch ID and recovery

In **Security**:

- Add a security key (FIDO2) as a second factor, up to 10 keys.
- Turn on Touch ID or Windows Hello to open the vault for 1 to 30 days.
- Change the master password, or make a new recovery key at any time. The recovery key opens the vault if you lose the key or the password.

## Import and export

Import from Bitwarden, 1Password, Chrome or Edge, Firefox, Safari or KeePassXC. Export the vault, or one project, to an encrypted file. The vault also syncs item by item across your machines with [encrypted sync](sync.md).

## Agents and the vault

Agents never read a value. Per item, you choose whether agents may use it through the [Ops connections and recipes](ops.md). Keys you hand to an agent on request are listed in the project's vault, each with **Remove** (see [a minimal environment](security.md#agent-env)).

## Settings and commands

`[vault] auto_lock_minutes` sets the lock delay.

```sh
# the vault
termf vault init | unlock | lock | status

# logins and notes, from a terminal
termf vault items | item <id>

# secrets and the audit
termf vault set | ref | reveal | audit
```

> [!NOTE]
> Never tried with a real security key: the FIDO2 code is checked against a simulated key, and the Windows path is only built, never run. Touch ID and Windows Hello are a convenience, not a cryptographic lock like a security key: the app is not signed. Two-factor keys cannot be read from a QR code. On Linux and Windows, a key kept in the keychain can be read by any process of the user. Opening the vault with a passkey waits for a signed app.
>
> **Tried so far.** The command line was tried end to end in a throwaway home folder; creating the vault, adding a login, locking and unlocking it and its two-factor codes are driven in the real app on macOS by end-to-end tests.
