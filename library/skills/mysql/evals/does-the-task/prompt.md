---
description: Two schema changes on a busy InnoDB table, which should come out online and explicit.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

On MySQL 8.4, the `events` table (InnoDB, 60 million rows, constant inserts) needs a new nullable column `source VARCHAR(32)` and an index on `(account_id, created_at)`. Write the ALTER statements so inserts keep flowing, and make sure MySQL errors out instead of copying the table if I got something wrong.
