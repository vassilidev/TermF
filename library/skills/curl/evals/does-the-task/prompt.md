---
description: Waiting for a local API, then posting JSON built from shell variables and failing on an error status.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

In this worktree our API listens on the port in `$PORT` and takes a few seconds to start after `npm run dev`. Write the shell commands that wait until it answers on `/health`, then create a user with `POST /users` whose `name` and `email` come from the shell variables `NAME` and `EMAIL` (a name can contain quotes). The commands must exit non-zero if the API answers with an error.
