---
description: A new Makefile target that needs shell variables, $$ and one shell per recipe line.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Add a `release-notes` target to the Makefile of this repository: it finds the latest git tag, then writes the one-line log of the commits since that tag to `NOTES.md`. Reply with the full updated Makefile.
