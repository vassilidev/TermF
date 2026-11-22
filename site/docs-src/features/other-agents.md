---
title: Codex and Gemini
order: 14
---

TermF opens Codex CLI and Gemini CLI in your terminals and conversations beside Claude Code, and a team role can run on one of them.

## Open a session

1. Install Codex CLI or Gemini CLI as usual. TermF finds them; there is nothing to configure.
2. Pick it in the new session launcher (<kbd>⌘T</kbd>) or in the conversation form.

It opens like Claude Code: in a terminal tab with its real state (working, waiting, done), or as a conversation without a terminal. Your library's instructions reach it without writing anything in your repository.

## Give a team role to one of them

Set the role's agent in the project's file:

```toml
[roles.front]
agent = "codex"
```

The same table takes the model and permission mode for that role. The CTO, the expert and tickets stay on Claude Code.

TermF's tools reach them through `termf mcp`. Gemini gets rules that deny reading secret files.

```sh
# conversations, from a terminal
termf agent ls | send | events | stop
```

> [!NOTE]
> These adapters are new and have not been tried with the real Codex or Gemini yet: they were built from their official documentation. There is no budget, cost or context gauge for them, since they do not report it. A team task on Codex or Gemini is refused when it needs what only Claude Code offers (TermF's guard, permissions asked through TermF).
>
> **Tried so far.** Covered by automated tests with fake Codex and Gemini programs.
