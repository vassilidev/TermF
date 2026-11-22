---
title: Create a project
order: 3
---

A project starts either from repositories you already have, which TermF reads to propose a team, ticket sources, a worktree recipe, CI and skills, or from scratch, with an idea the team frames and researches.

## From your repositories {#projects}

Creating a project is a full-screen setup, one step per screen. Nothing is written before you click **Create**, and Escape drops everything.

![New project: what TermF proposes after reading the repositories, the agents' team, ticket sources, worktree setup, CI/CD and skills](shot:setup "Creating a project: the proposal, one block at a time. Sample data.")

1. Press **+** next to Projects, or <kbd>⌘O</kbd>. The repositories of your machine come first, as cards sorted by activity, with their remote, branch, uncommitted changes and stack. Search or filter, and pick one or several.
2. The name comes from them (one repository, its name; several, their GitHub owner or their common folder). Change it if you like.
3. As you pick, TermF shows what it detects in each: stack and versions, dev, build, test and lint commands, CI, ticket sources, monorepo parts, database, tests and infrastructure.
4. It proposes the team, the ticket sources, the worktree recipe, CI and the stack's skills. Accept each one, or set it aside to customise later.
5. The summary shows the exact project file before anything is written. **Create** opens the project's Home, with the CTO ready.

After creation, Claude Code reads the repositories read-only in the background and proposes conventions, through the setup assistant. You can run the setup assistant again from the project's **Configuration**, and add repositories later with the same cards.

### Trust in Claude Code

Trusting the repositories in Claude Code is opt-in, explained repository by repository: the box is unchecked. You can change it later:

```sh
# trust the project's repositories in Claude Code, or stop
termf project trust <id> [--off]
```

> [!NOTE]
> A repository added to the project later is not trusted by itself. The setup was tried on throwaway repositories and in the real app by an end-to-end test; the owner has started trying it by hand.
>
> **Tried so far.** Detection and the proposal are covered by automated tests on throwaway repositories; trust was checked with a real Claude Code. Creating a project was driven in the real app on macOS by an end-to-end test.

## From scratch {#greenfield}

Start from a few sentences of your idea. TermF creates the folder, its first commit and, if you like, the GitHub repository, then hands the idea to the CTO. The `kickoff` workflow frames it, researches what exists and who else does it, and brings you a decision note before the architecture and the skeleton are built.

![New project, Start from scratch: the idea in a few sentences, three examples, and what the team does next, framing, research, your decision, the skeleton](shot:greenfield "Starting a project from an idea. Sample data.")

1. Press **+** next to Projects, then **Start from scratch**. Write your idea, or click one of the three examples.
2. TermF proposes a name from the idea and a folder beside your other repositories. Tick **Also create the repository on GitHub** if you want one (private by default).
3. The summary shows the folder, the exact files of the first commit and every command, in order. Nothing runs before **Create**.
4. The project opens on the CTO, which proposes the `kickoff` workflow: framing, then the researcher and the competitive analyst side by side, then a decision note with its options, trade-offs and sources.
5. Approve the recommendation or ask for another option. The architect and the lead then plan the skeleton, its tooling, a first test, CI and the README.

### What it creates

- A `README.md` with your idea and a `.gitignore`, committed on `main` with your own git identity.
- No GitHub repository unless you tick it. A GitHub failure keeps the project, and says so. Without a remote, a run's delivery merges its branch into `main` on your machine, without a pull request.
- `kickoff` has a 25 dollar budget and two gates: the decision and the plan.

The researcher, the competitive analyst and the reverse engineer read only: they change no file and source every claim. They are also optional steps of the default workflow, for the CTO to turn on when a ticket asks who else does it or how something works.

> [!NOTE]
> The first message to the CTO and the `kickoff` run have never run with a real Claude Code; the three research roles have not been evaluated yet. Creating the GitHub repository has only been tried with a fake GitHub CLI.
>
> **Tried so far.** Covered by automated tests on throwaway folders with a fake GitHub CLI; the screens were checked with a simulated backend.

Next: [your first task for the CTO](first-task.md).
