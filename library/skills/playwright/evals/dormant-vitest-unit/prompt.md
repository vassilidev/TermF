---
description: A pure-function unit test with Vitest is not a browser test.
max_turns: 8
allowed_tools: [Read, Glob, Grep, Skill]
---
Add Vitest unit tests for this helper (vitest 5.1, `npm test` runs `vitest run`). Cases from the spec: spaces become dashes, accents are stripped, the result is lower case, and an empty string stays empty.

```ts
// src/slugify.ts
export function slugify(input: string): string {
  return input.normalize('NFD').replace(/[̀-ͯ]/g, '').trim().toLowerCase().replace(/\s+/g, '-');
}
```
