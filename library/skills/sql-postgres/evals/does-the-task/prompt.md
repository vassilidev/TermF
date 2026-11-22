---
description: A foreign key and an index on a large, busy table, which must not lock writes.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

On our Postgres 18 production database, `orders` (see migrations/) has 80 million rows and takes writes all day. I need a new nullable column `customer_id bigint` referencing `customers(id)`, with an index on it. Write the next SQL migration(s) so we don't block checkout while they run.
