---
description: A NestJS service (decorators, dependency injection, TypeScript) must not trigger the Angular skill.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

In my NestJS 11 API, write a `UsersService` with `findAll()` and `findOne(id)` backed by an in-memory array, and the matching `UsersController` with GET /users and GET /users/:id returning 404 when the user does not exist.
