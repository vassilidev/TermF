---
description: A GNU sed one-liner that fails on macOS, with GNU-only escapes the BSD sed reads as letters.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

This works in our Ubuntu CI but fails on my Mac: `sed -i 's/\s\+$//' src/*.py`. Fix it so the same command strips trailing whitespace on both.
