---
description: A filtered read through gh api, which turns into a POST when the filters are passed as fields.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

With `gh api`, list every closed issue labelled `bug` in acme/api (there are a few hundred), and print only their numbers and titles. Give me the one command.
