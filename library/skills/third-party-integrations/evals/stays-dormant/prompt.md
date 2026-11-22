---
description: Pagination on the project's own API, which is back-end work with no third party.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Express API returns every order at once from GET /api/orders. Add cursor-based pagination (limit and cursor query parameters, a next cursor in the response) backed by our Postgres orders table.
