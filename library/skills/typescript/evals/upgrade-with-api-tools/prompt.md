---
description: TypeScript 7 is not installed as `typescript` when typescript-eslint and svelte-check need the 6.x API; the official side-by-side aliases are used instead.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---
I want the faster TypeScript 7 compiler in our SvelteKit app. When I change `"typescript": "^6.0.3"` to `"^7.0.2"`, `npm install` fails with ERESOLVE: typescript-eslint@8.73.0 wants `typescript@">=4.8.4 <6.1.0"` and svelte-check@4.7.8 wants `"^5.0.0 || ^6.0.0"`. Our scripts: `"check": "svelte-check --tsconfig ./tsconfig.json"`, `"lint": "eslint ."`, `"typecheck": "tsc --noEmit"`.

Should I just use `--legacy-peer-deps`? What's the right package.json setup? Show the devDependencies and scripts.
