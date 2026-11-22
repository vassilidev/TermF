---
description: Validating a JSON request body in application code, which the language skills cover.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

In our Express service, validate the JSON body of `POST /orders` with zod (an array of items, each with a `sku` string and a positive integer `quantity`) and answer 400 with the validation errors when it is invalid.
