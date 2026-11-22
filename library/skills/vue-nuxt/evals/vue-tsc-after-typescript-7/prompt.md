---
description: vue-tsc needs the TypeScript 6 API; the answer keeps or aliases TypeScript 6 instead of inventing a vue-tsc flag.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

We bumped our pnpm monorepo to `"typescript": "^7.0.2"`. The Vue 3.5 app's type check (`vue-tsc --build`, vue-tsc 3.3.14) now crashes with `ERR_PACKAGE_PATH_NOT_EXPORTED: Package subpath './lib/tsc' is not defined by "exports"`. The other packages are fine on TypeScript 7. How do we get the Vue app type-checked again?
