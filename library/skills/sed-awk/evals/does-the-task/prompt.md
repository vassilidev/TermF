---
description: A replacement across many files that has to run both on a Mac and on Linux CI.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

About forty Markdown files under `docs/` link to `https://old.acme.dev/`. Give me one shell command that replaces it with `https://docs.acme.dev/` in all of them, in place. It has to work on my Mac and in our Ubuntu CI job.
