---
title: Create a project
order: 3
---

A project starts either from repositories you already have, which TermF reads to propose a team, ticket sources, a worktree recipe, CI and skills, or from scratch, with an idea the team frames and researches.

## From your repositories {#projects}

Creating a project is a full-screen setup, one step per screen, and the navigation steps aside until you are done. Nothing is written before the last step. Your answers are kept: **Finish later** (Escape) closes the setup, and you pick up where you stopped.

![New project, step 5 of 5, Your team and your tools: the agents' team, ticket sources, worktree setup, the checks before delivery and CI, each kept by default with Adjust](shot:setup "Creating a project: your team and your tools, all proposed. Sample data.")

1. Press **+** next to Projects, or <kbd>⌘O</kbd>, then **From my repositories**.
2. **Your repositories**: search the repositories of your machine and pick one or several. The project's name comes from them; change it if you like.
3. **What TermF understood**: two or three sentences per repository, its stack, its tests and its CI, read without changing anything. Claude Code reads the code at the same time, live; **See the details** shows everything found.
4. **A few questions**: what the code cannot settle, one card at a time. Each comes with a proposed answer and what in your repositories backs it, and **Why we ask**. Keep it, change it, **Skip this question**, or ask the CTO what it advises.
5. **Your team and your tools**: the team, ticket sources, worktree setup, the checks before delivery, CI, skills, MCP servers, databases, conventions and trust. Everything is kept by default; **Adjust** opens a line. **Create the project** opens its Home.

After creation, what is still being read, the proposed rules and the expert's memory follow on the Home. You can run the same setup again on an existing project from its **Configuration**: what is already in place is left alone.

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

1. Press **+** next to Projects, then **Start from scratch**. In **Your idea**, write it in a few sentences, or click one of the three examples.
2. TermF proposes a name from the idea and a folder beside your other repositories. Tick **Also create the repository on GitHub** if you want one (private by default).
3. **Check and create** shows the folder, the exact files of the first commit and every command, in order. Nothing runs before **Create the project**.
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
