---
description: An aggregation that broke after the 9.0 upgrade; the outdated answer blames something else.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Since we upgraded to MongoDB 9.0, our nightly report aggregation fails with `ExceededMemoryLimit`. It worked fine on 8.0 with the same data. It groups 40 million events by user and day, then sorts. What changed and how do we fix it?
