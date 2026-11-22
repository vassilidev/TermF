---
description: Customizing JSON output in a Spring Boot 4 application.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Spring Boot 4.2 service must write every `java.time.Instant` as epoch milliseconds in its JSON responses, as our mobile clients expect. Today it writes ISO strings. Add the configuration class that does it.
