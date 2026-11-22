---
description: Reshaping a JSON file into TSV lines, then editing one entry chosen by a shell variable.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

`releases.json` in this repository lists our releases. With jq, (1) print one tab-separated line per release that is not a draft: its tag, then the total size of its assets; (2) give me the shell line that marks as draft the release whose tag is in the shell variable `$TAG`, writing the change back to `releases.json`.
