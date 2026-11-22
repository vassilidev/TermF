---
description: Waiting for a server that is still starting, where --retry alone gives up on a refused connection.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

Our dev server takes about ten seconds to start. Give me a single curl command for a script that waits until http://127.0.0.1:5173/ answers, trying for at most 30 seconds, and fails if it never does.
