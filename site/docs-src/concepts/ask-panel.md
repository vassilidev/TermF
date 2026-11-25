---
title: Ask from any screen
order: 2
---

Press <kbd>⌘I</kbd> on any screen to ask an agent about what you see: a panel opens with the agent that fits the screen, and what it shows already attached.

![The Ask panel open on CI/CD: the DevOps engineer answers, the failed run and the line of its first error are attached, and a question is typed](shot:ask "Asking about a failed run, from the CI/CD view. Sample data.")

## Open the panel {#open}

1. Press <kbd>⌘I</kbd> (<kbd>Ctrl+I</kbd> on Linux and Windows), click the bar at the bottom of a view, or pick **Ask the AI** in the palette (<kbd>⌘K</kbd>).
2. With <kbd>⌘I</kbd>, the text you selected on the screen comes along as well.
3. Write, then send with <kbd>↵</kbd>. The answer streams in the panel, and the view stays where it was.
4. <kbd>⌘I</kbd> again, or <kbd>Esc</kbd>, closes the panel and brings you back where you were.

## Who answers {#roles}

Each screen has its own agent. **Who answers** at the top of the panel changes it, and `@role` at the start of a message sends it to someone else.

| Screen | Who answers |
|---|---|
| Home, Tickets, Terminals, and any other screen | The CTO |
| A run's page | The tech lead |
| CI/CD, Ops, SSH keys | The DevOps engineer |
| The SQL tab of Ops | The database engineer |
| Vault | The security reviewer |
| Design system, Visual review | The UX designer |
| Git, Files | The project expert |
| Library | The role of the item you opened, if it has one |

The CTO and the [project expert](team.md#expert) answer in their own conversation. Any other role gets a direct conversation, kept per project, role and what the screen is about. On a run's page, the role at work on the run gets your message at its step.

## What the agent sees {#context}

The screen sends what you are looking at as chips above the message, and you can remove any of them before you send:

- the run, on its page;
- the run shown and, once read, the line of its first error, on CI/CD;
- the open file, or the lines you selected, in Files;
- the connection and the query, in Ops;
- the item, in the Vault and the Library;
- the open commit, a stopped merge and its conflicts, or what can be cleaned up, in Git.

A vault item or an Ops connection is sent by its name only, never its value. Each chip carries 4,000 characters at most.

## Nothing changes without you {#approval}

When the agent wants to use a tool outside TermF, a card says in plain words what it wants to do, shows the lines of a file it would change, and keeps the raw call under **Details**. **Deny**, with a reason if you like, or **Allow**.

- An allowed change to a file of the project's repositories can be undone from its notice, or with <kbd>⌘Z</kbd>, as long as nobody changed the file since.
- <kbd>Esc</kbd> in the message box stops the agent's turn.
- Every conversation also opens in **Conversations**, marked with the screen it came from.

## Where the screens help {#shortcuts}

Some screens write the question for you, to read before you send it:

- a run's page: **Ask the lead** under a blocked step;
- CI/CD: **Why?** on the **First error** card of a failed run;
- Git: **Ask the git expert** in the clean-up, **Explain this conflict**, **Explain this commit**;
- Ops: **Ask devops** when the project has no connection yet.

> [!NOTE]
> The answers come from Claude Code: the panel needs it installed and signed in, like the rest of the team.
>
> **Tried so far.** Who answers and what each screen sends are covered by automated tests; the screens were checked with a simulated backend.
