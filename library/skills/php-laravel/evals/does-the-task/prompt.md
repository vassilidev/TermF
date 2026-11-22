---
description: An update endpoint with validation and authorization in a Laravel 13 app.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our Laravel 13 app has `Project` models owned by a user (`projects.user_id`). Add `PATCH /projects/{project}` so the owner can rename a project (name required, max 120 characters). Other users must not be able to rename it. Show the route, the controller method and any other class you add.
