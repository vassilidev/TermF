---
description: A Compose start-up order question, which the docker skill covers.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

In our compose.yaml the `api` service starts before `postgres` accepts connections and crashes on boot. Fix the file so `api` waits for the database.
