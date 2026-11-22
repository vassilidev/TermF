---
description: A Postgres 19 behaviour change; the outdated answer still expects NULL from json_array over no rows.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

We upgraded to PostgreSQL 19 and this query broke our API: users without tags used to get `tags: null`, which the frontend treats as "no tags yet", and now they get something else. What changed, and what is the smallest fix in SQL so the API keeps returning null?

```sql
SELECT u.id,
       JSON_ARRAY(SELECT t.name FROM tags t WHERE t.user_id = u.id) AS tags
FROM users u;
```
