---
title: Git and worktrees
order: 1
---

The Git view gives you the branch graph and every everyday git action inside the project, and each run gets its own worktree, ready to run.

## Git in the project {#git}

The branches of a project, its worktrees and what the agents did are one look away, and everyday git does not need a separate app. The project's Git view shows each repository's branches with their upstream and pull request, the worktrees with the run that owns each, and the commit graph. You stage by file, hunk or line, commit, stash, merge, rebase and resolve conflicts, tag, and read a file's history and blame.

![The Git view: local and remote branches, worktrees and stashes, and the commit graph of every branch](shot:git "The Git view of a repository. Sample data.")

### How to use it

1. Open **Git** among the project's views, or from the palette. Pick the repository when the project has several.
2. Read the branches, local and remote, ahead and behind, the worktrees named after their run, and the graph of the last commits.
3. Open a worktree's changes: stage or discard by file, hunk or line, write the message and commit. Amend only when the commit is not pushed yet.
4. Merge, rebase, cherry-pick or revert from a branch or a commit. On a conflict, keep one side, or open the file in TermF's editor, then continue, skip or abort.
5. Stash, tag, and open a file's history or blame. A commit opens with its message, files and diffs.

Under the branch's state, **Next step** proposes one thing at a time: pull, commit, push or clean up. The project expert answers from the Git view, with the open commit or a conflict attached (see [Ask from any screen](ask-panel.md)).

### Clean up branches and worktrees {#cleanup}

**Clean up**, at the top of the view, with the number of suggestions, lists what can go and why: merged, its pull request merged, gone from the remote, no commit for 90 days, its run finished, detached, or its folder gone.

![Clean up branches and worktrees: four worktrees and branches ticked, each with its reason and what it would lose, and one button to clean them up](shot:cleanup "What can go, why, and what it would lose. Sample data.")

1. Each line says what you would lose: **Nothing to lose**, or the commits and uncommitted changes. The suggestions, which lose nothing, are ticked at first.
2. A worktree whose run is still at work, or whose pull request is open, stays.
3. One button cleans up everything ticked, worktrees first, then branches. When something would be lost, one dialog lists it first.
4. **Undo** (<kbd>⌘Z</kbd>) brings the branches back to their commit and the worktrees back on their branch. A closed run and lost changes do not come back.
5. **Ask the git expert** proposes a plan as a card: **Apply** it, change it, or ignore it.

### Defaults

- The view reads git when it shows, after each action and when the window comes back to the front, never by polling.
- Pull only fast-forwards. Force-push only with `--force-with-lease`, never on the default branch, and with the branch name to type.
- Every destructive action says what will be lost first: a reset, a dropped stash, a deleted branch or tag.
- Pull requests come from the GitHub CLI when it is signed in; without it, the rest works the same.

There is nothing to set: the view follows the repository's own git configuration, its push remote, upstreams and SSH command.

> [!NOTE]
> The Git view does not change what agents may do: `termf guard` still keeps them from pushing. It has not been used by hand on a real project yet.
>
> **Tried so far.** Tried with the real git on throwaway repositories (accented paths, CRLF, detached HEAD, submodules), and driven in the real app on macOS by end-to-end tests: a commit, a stash, a merge conflict resolved.

## Worktrees ready to run {#worktrees}

Two tickets at once in one checkout do not mix. Each run gets a git worktree, and the project's recipe makes it ready to run: dependencies installed without being copied raw, `.env` files pointed at it, its own copy of the local database and its own ports, so several runs work side by side.

### How it works

1. The run's worktree is created inside the repository, so Claude Code's trust carries over.
2. The recipe copies the ignored files it needs, clones the dependency folders copy-on-write, then installs offline first.
3. It repoints the `.env` files, copies the local database for the worktree, and gives it its own ports and Compose project name.
4. When the run closes, the database copy is dropped and the ports go back.

### Defaults

- Worktrees live in `<repo>/.claude/worktrees/termf-<run>-…`, added once to `.git/info/exclude`.
- Without a saved recipe, the recipe detected from the stack runs.
- Local databases: Postgres, MySQL or MariaDB, and SQLite. A remote database is refused.
- The preparation agent is off.

### Settings

- **Library › Worktrees**: edit the recipe, or **Propose with Claude**, read-only.
- The preparation agent: a Claude Code in Claude Code's sandbox, writing only in the worktree and caches.
- `[tickets] worktrees = "data"` keeps the worktrees in TermF's data folder instead.

```sh
# prepare a worktree
termf worktree setup [<path>]

# undo it
termf worktree teardown [<path>]
```

> [!NOTE]
> Tools that ignore git's ignore files can see the worktrees from the main checkout, and from a worktree, tools that walk up the folders see the main copy: use `worktrees = "data"` then. The worktrees of the team's tasks are not prepared by the recipe yet.
>
> **Tried so far.** Covered by automated tests; not yet tried by hand on a real project.

Next: how a run uses its worktree, in [Runs and their steps](runs.md#tasks).
