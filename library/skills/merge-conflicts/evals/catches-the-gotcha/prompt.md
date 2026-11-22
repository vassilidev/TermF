---
description: A lockfile in conflict after a merge, which must be regenerated, not merged by hand.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

I merged `main` into my branch. `package.json` merged cleanly (main added `zod`, I added `papaparse`), but `pnpm-lock.yaml` is full of conflict markers. How do I fix the lockfile?
