---
description: Reading a field into a shell variable, where a missing field becomes the word null and exit 0.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

In a bash deploy script, read the `version` field of `package.json` into a variable `VERSION` with jq, and make the script stop with an error if the field is missing. Just the lines I need.
