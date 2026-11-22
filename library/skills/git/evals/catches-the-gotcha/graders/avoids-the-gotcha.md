---
type: llm
---

PASS if the answer sets the changes aside without relying on the top of the stash, which every worktree of the repository shares: a work-in-progress commit or a branch, a separate worktree, or a stash entry found by its own message and applied by its `stash@{n}`, and it says why.
FAIL if it recommends a plain `git stash` followed by `git stash pop` without warning that another worktree's entry may be on top.
