---
description: Extracting data from a JSON file, which the jq skill covers.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

From the shell, list the names of every package in the `dependencies` and `devDependencies` of our `package.json`, one per line, sorted.
