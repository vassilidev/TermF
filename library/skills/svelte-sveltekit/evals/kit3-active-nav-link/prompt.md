---
description: In a SvelteKit 3 project, reads the current path from $app/state instead of the removed $app/stores.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Here is my package.json:

```json
{
  "type": "module",
  "imports": { "#lib": "./src/lib/index.js", "#lib/*": "./src/lib/*" },
  "devDependencies": {
    "@sveltejs/adapter-auto": "^8.0.1",
    "@sveltejs/kit": "^3.4.0",
    "@sveltejs/vite-plugin-svelte": "^7.3.1",
    "svelte": "^5.59.1",
    "svelte-check": "^4.7.8",
    "typescript": "^6.0.3",
    "vite": "^8.3.8"
  }
}
```

Write `src/routes/+layout.svelte`: a nav with links to /, /blog and /about, where the link of the current page gets `aria-current="page"`, and a small "Loading…" text while a navigation is in progress. Give me the full file in your answer.
