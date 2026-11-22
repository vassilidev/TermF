---
description: A read-through cache for a slow endpoint, which needs TTLs, the existing prefix and no KEYS.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Node service already uses ioredis with keys prefixed `shop:`. Add a cache in front of `getProduct(id)` (it hits Postgres and takes 300 ms), and a function that clears every cached product when we run a bulk price update. Show the code.
