---
description: "`node --run` does not run pre/post scripts; the answer says so and keeps the seed step running."
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Since we switched our CI step from `npm test` to `node --run test` (Node 26) to save a few seconds, the integration tests fail because the test database is empty. Nothing else changed. Scripts in package.json:

```json
"scripts": {
  "pretest": "node scripts/seed-test-db.js",
  "test": "vitest run"
}
```

Why did it break and what should the CI step be?
