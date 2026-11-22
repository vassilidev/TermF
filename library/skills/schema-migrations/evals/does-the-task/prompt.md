---
description: A column rename on a large table of a live Rails app, which must not break the running code.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Rails 8.1 app on PostgreSQL 18 deploys with rolling restarts, and `users` has 30 million rows. Rename the column `users.name` to `full_name` (see db/migrate and app/models). Write the migration(s) and tell me what ships in which deploy.
