---
description: After moving to TypeScript 6+, missing globals are fixed by listing them in `types`, not by reinstalling @types packages.
max_turns: 12
allowed_tools: [Read, Glob, Grep, Skill]
---

I bumped TypeScript in our API repo and now `npm run typecheck` fails everywhere with things like:

```
src/server.ts:3:20 - error TS2580: Cannot find name 'process'.
src/orders.test.ts:5:1 - error TS2582: Cannot find name 'describe'.
```

package.json devDependencies: `"typescript": "^6.0.3"`, `"@types/node": "^24.10.0"`, `"vitest": "^5.1.1"`. Vitest runs with `globals: true`. Our tsconfig.json:

```json
{
  "compilerOptions": {
    "module": "nodenext",
    "moduleResolution": "nodenext",
    "noEmit": true,
    "skipLibCheck": true
  },
  "include": ["src"]
}
```

@types/node is clearly installed. What's going on and what's the fix? Show the tsconfig change.
