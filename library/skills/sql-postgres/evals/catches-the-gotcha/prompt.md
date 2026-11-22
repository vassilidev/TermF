---
description: A Postgres 19 behaviour change; the outdated answer still expects NULL from JSON_ARRAY over no rows.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

We upgraded to PostgreSQL 19 and the query in queries/user_tags.sql broke our API: users without tags used to get `tags: null`, which the frontend treats as "no tags yet", and now they get something else. What changed, and what is the smallest fix in SQL so the API keeps returning null?
