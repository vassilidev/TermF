---
description: Folding the last commits into an earlier one from an agent's shell, where an editor would hang.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

You are running in a non-interactive shell (no terminal, no editor). On my branch `feat/export` the last three commits are "Add CSV export", "wip" and "fix typo". Fold "wip" and "fix typo" into "Add CSV export" and keep that message. The branch is not pushed yet. Give me the exact commands to run.
