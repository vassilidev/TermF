---
title: SSH keys
order: 10
---

The SSH view lists your keys, the agent and your `~/.ssh/config`, and generates, converts, tests and rotates keys in every common format.

## The inventory

TermF reads `~/.ssh`, its own files and the files you pick, nothing else.

## Generate a key

Pick the use (SSH, signing, encryption, TLS), then the type: Ed25519, ECDSA, RSA or X25519.

Export as OpenSSH, PuTTY, JWK, PKCS#8, SEC1, PKCS#1 or SPKI, or convert an existing key. A private key is revealed on click, then masked again after a while, with the clipboard cleared.

## Config, repositories and rotation

1. Edit `~/.ssh/config` through a diff preview.
2. Assign one key per repository.
3. Test a key, rotate it, or move it to the trash.

```sh
# keys
termf ssh ls | gen | test

# agent, repositories, trash
termf ssh agent-add | assign | trash
```

Keys can also follow you to another machine with [encrypted sync](sync.md), once you turn that on.

> [!NOTE]
> Hardware keys (`-sk`), GitLab and certificate requests are not built. Clearing the clipboard is not verified on Windows and Linux.
>
> **Tried so far.** Every key TermF writes is read back by the real `ssh-keygen` (OpenSSH 10.2), and the other way round; generating a key is driven in the real app on macOS by an end-to-end test.
