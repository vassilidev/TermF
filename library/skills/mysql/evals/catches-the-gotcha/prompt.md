---
description: An upgrade from 8.0 to 9.7; the outdated answer keeps mysql_native_password.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

We are moving our MySQL 8.0 server to 9.7 LTS. Look at our my.cnf and the migration that creates the app user: what do we need to change, and can we upgrade straight from 8.0 to 9.7?
