---
description: Setting changes aside in one of several worktrees of a repository, where the stash is shared.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

I'm an agent working in one of several git worktrees of the same repository; other agents work in the other worktrees at the same time. I have uncommitted changes on my branch `task/7` and need to set them aside for a minute to run the tests on the clean branch, then get them back. Which git commands should I run?
