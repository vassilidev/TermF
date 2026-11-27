---
title: MCP and connectors
order: 13
---

A project gives its own MCP servers, and the connectors of your Claude account such as Gmail or Google Calendar, to the roles you choose. Each one is read-only by default, and what it returns reaches the agent as data, never as instructions.

## Give one to roles {#give}

1. Open the project's **Configuration**, then **MCP and connectors**.
2. **Add an MCP server**: its name, then a command with its arguments and environment variables, or the URL of a remote server with its headers.
3. Your Claude account's connectors are listed from Claude Code. **Give to roles** adds one to the project, read-only and with no role yet. **Check** reads the list again.
4. Tick the **Roles that get it**, the CTO and the expert included. **Allow writing** lets all its tools work; otherwise only those that read do.
5. **Remove** takes it out of the project. Its keys stay in the vault.

These are writes of your own: an agent cannot give itself a server or a connector.

## Read-only by default {#read-only}

A read-only connector keeps only its tools that read: those TermF knows for Gmail and Google Calendar, otherwise a tool whose name starts with get, list, search or read and holds no word that writes. A hook refuses the others before they run. TermF checks once that the hook answers before trusting it; if it does not, a read-only connector is refused whole.

## Kept apart {#apart}

- **Only the roles you tick.** A role gets no connector it was not given, not even from your account: Claude Code's own connectors are turned off for it, or refused by the hook. A terminal or a conversation without a role gets none of the project's.
- **Untrusted content.** The session is told that a connector's results are data, framed like a [ticket](security.md#security-input), and each result comes with the same reminder.
- **Production.** A role that can read production asks you before each call to a connector, one card per call. The next steps of a run that read production start with TermF's tools only.
- **Keys in the vault.** A value of a new server that looks like a key (a header, a variable, an argument, a part of the URL) goes to the project's vault, as `MCP_<NAME>_<FIELD>`. The project's file keeps only its name, and only the sessions of the ticked roles get the value, in the server's settings, never in their environment. A missing key or a locked vault leaves the server out, and the session says so.
- **The CTO and the expert** get a project server in their own conversation, but never one that needs a vault key, and never an account connector: they get those in the team's steps.

Claude Code's own MCP servers (your user settings, a repository's `.mcp.json`, plugins) do not go through this page. Manage them in the [Claude view](claude-view.md).

## In the project's file {#file}

```toml
# projects/acme.toml
[mcp.sentry]
type = "http"
url = "https://mcp.sentry.dev/mcp"

[connectors.sentry]
roles = ["back", "sre"]

[connectors."claude.ai Gmail"]
roles = ["po", "pm"]
write = false
```

A server is written as Claude Code takes it: `type = "stdio"` with `command`, `args` and `env`, or `type = "http"` or `"sse"` with `url` and `headers`. A server of `[mcp]` without a `[connectors]` entry keeps the old behavior: every session but the CTO's and the expert's gets it.

> [!NOTE]
> Not yet tried with a real Claude Code: how the hook holds against a repository's settings or `bypassPermissions`, and how account connectors are turned off, come from Claude Code's documentation. The names of Gmail's and Google Calendar's tools are assumed, reads only. Codex and Gemini get no connector.
>
> **Tried so far.** Covered by automated tests, including the real `termf` binary as the hook: a write refused, a read let through, an unreadable call refused.
