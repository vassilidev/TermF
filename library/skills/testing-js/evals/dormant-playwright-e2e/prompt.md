---
description: A browser end-to-end test belongs to the Playwright skill, not to the Vitest/Jest one.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---

We use @playwright/test 1.65. Write me an end-to-end test that signs in at /login with the test account from `process.env.E2E_USER` / `E2E_PASSWORD` and checks that the dashboard heading "Your projects" shows up.
