---
name: vue-nuxt
description: >-
  Writes and changes Vue 3 single-file components and Nuxt 4 apps the way the installed versions expect:
  script setup with TypeScript, props and v-model macros, template refs, composables, Pinia stores, Nuxt
  pages, layouts, server routes, data fetching with useFetch and useAsyncData, runtime config, and type
  checking with vue-tsc or nuxt typecheck. Use when a task touches .vue files, nuxt.config, app/ or server/
  of a Nuxt project, or a Vue + Vite app. Not for React, Svelte or Angular, nor for Vue 2 code.
paths: "**/*.vue, **/nuxt.config.*, **/app.config.*, **/app/**, **/server/**, **/shared/**, **/composables/**, **/stores/**"
license: MIT
metadata:
  termf-stacks: "vue nuxt"
  termf-roles: "front qa"
  termf-versions: ">=3.4"
  termf-verified: "2026-11-22 vue@3.5.46 nuxt@4.6.4 vue-tsc@3.3.14"
  termf-sources: "https://vuejs.org/llms.txt https://nuxt.com/llms.txt https://nuxt.com/docs/4.x/getting-started/upgrade https://nuxt.com/docs/4.x/guide/ai/mcp https://nuxt.com/docs/4.x/api/commands/typecheck https://github.com/vuejs/language-tools/issues/5381"
---
# Vue and Nuxt

1. Read the installed versions of `vue`, `nuxt`, `vue-tsc`, `typescript`, `pinia` and `vue-router` in the lockfile (or `package.json`), and in a Nuxt app whether `app/` exists and what `nuxt.config.*` sets (`future.compatibilityVersion`, `srcDir`, modules); keep them as the reference for every later step. Vue 3.5 is the stable line (3.6 with Vapor mode is a release candidate) and Nuxt 4 changed folders, data-fetching defaults and the TypeScript setup. Done when you can name the Vue minor, the Nuxt major, the file they came from, and where pages and components live.
2. Open the docs of those versions before writing code. The packages ship no docs; the official agent resources are the Nuxt MCP server at `https://nuxt.com/mcp` (tools `list-documentation-pages`, `get-documentation-page`, `get-changelog`, `list-modules`…), https://nuxt.com/llms.txt and https://vuejs.org/llms.txt, whose entries link to Markdown pages. Use the MCP tools when the session has them. Done when the page for every API you will touch is open.
3. Use the tools the project already has (package manager from the lockfile, `vue-tsc` or `nuxt typecheck`, Vitest with `@nuxt/test-utils`, Pinia, its UI library and CSS approach, auto-imports or explicit imports), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. Find a component, page or store close to the task and copy its shape (`<script setup lang="ts">` or not, setup or options stores). Done when you can name the file you are modelling on.
5. Write the component: `defineProps<T>()` with a TypeScript type, `defineEmits<T>()`, `defineModel()` for `v-model` on a component (Vue 3.4+), `useTemplateRef<T>('name')` for `ref="name"` (3.5+), `useId()` for ids that must match between server and client (3.5+), `computed` for derived state rather than a `watch` that writes a ref. In Nuxt, put code shared by app and server in `shared/` (only `shared/utils` and `shared/types` are auto-imported; the rest through `#shared`). Done when every prop, emit and model is typed.
6. Fetch data the Nuxt way: in a page or component setup, `useFetch('/api/x')` or `useAsyncData(key, () => $fetch(...))`, so the server result is reused on hydration; in event handlers, call `$fetch` directly. Handle `status` and `error`. Done when the page renders on the server without a second client request.
7. Server routes go in `server/api/` or `server/routes/`, one file per route with a method suffix (`users.post.ts`); validate input with `readValidatedBody` / `getValidatedQuery` and the project's schema library; throw `createError({ status, statusText })` (the Nuxt 4 docs' form; older code uses `statusCode` / `statusMessage`, keep the project's). Done when the route has a test or a request that shows its answer.
8. When the task upgrades Nuxt 3 to 4, or the project opted into Nuxt 5 defaults, follow `references/nuxt-4.md`. Done when its "Done when" holds.
9. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Nuxt 4 keeps app code in `app/` (`app/pages`, `app/components`, `app/composables`, `app/layouts`, `app/middleware`, `app/plugins`, `app/utils`, `app/app.vue`), while `server/`, `shared/`, `public/`, `layers/` and `modules/` stay at the root. A Nuxt 3 project, or one that kept the old `srcDir`, has them at the root: look before creating a file.
- Nuxt 4 `useFetch` and `useAsyncData` start `data` and `error` as `undefined`, not `null`; `data` is a `shallowRef`, so mutating a nested field does not re-render (replace the value or pass `deep: true`); calls with the same key share their refs, so one key with different options conflicts; `pending` is true only while `status` is `'pending'`; `dedupe` takes `'cancel'` or `'defer'`.
- `vue-tsc` 3.x drives TypeScript's JavaScript API, which TypeScript 7 does not ship: keep `typescript` on 6.x in Vue packages (create-vue pins `~6.0.0`), or alias `typescript` to `@typescript/typescript6` when the workspace moved to 7. Plain `tsc` never checks `.vue` files.
- Nuxt 4 TypeScript uses project references (generated configs for app, server, shared and node code) and turns on `noUncheckedIndexedAccess`, so `items[0]` is `T | undefined`. Check with `npx nuxt typecheck`, which runs `vue-tsc` (or Golar) with those configs.
- Destructured props stay reactive since Vue 3.5 (`const { size = 'md' } = defineProps<Props>()` also gives the default); watch one with a getter, `watch(() => size, ...)`, because `watch(size, ...)` receives a plain value.
- Nuxt 5 is not released: a project with `future.compatibilityVersion: 5` (Nuxt 4.2+) has opted into Nuxt 5 defaults, so follow them, and leave the flag alone when the task does not ask for it.
- Removed in Nuxt 4: top-level `generate` (use `nitro.prerender`), `window.__NUXT__` (use `useNuxtApp().payload`), boolean `dedupe`.

## Verify

- The project's scripts, with its package manager: typically `typecheck` or `type-check`, `build`, `test` (Vitest), `lint`, its end-to-end script.
- Nuxt without scripts: `npx nuxt typecheck`, `npx nuxt build`, `npx vitest run`.
- Vue + Vite without scripts: `npx vue-tsc --build`, `npx vite build`, `npx vitest run`.

## Security

- `v-html` renders raw HTML: only sanitized or trusted content, because user or network content there is an XSS.
- Nuxt `runtimeConfig`: secrets go in its top level (server only) and are set at run time with `NUXT_*` environment variables; only `runtimeConfig.public` reaches the browser, so a secret read in a component or put in `public` ships to every visitor.
- Server routes under `server/api` are public HTTP endpoints and page middleware does not run for them: check the user in server middleware or in each handler, and validate body and query with a schema.
- State defined at module level (`const user = ref()` outside `setup` or a composable call) is shared by every request during SSR: use `useState` or a Pinia store for per-request state.
- A server-side `$fetch` to an absolute URL built from user input is an SSRF: allow-list the hosts.

## References

- Read `references/nuxt-4.md` when the project is on Nuxt 3 and the task upgrades it, when Nuxt 3 code must work under Nuxt 4, or when the project opted into Nuxt 5 defaults.
- Vue docs for agents: https://vuejs.org/llms.txt (and `llms-full.txt`); Nuxt: https://nuxt.com/llms.txt.
- Nuxt MCP for Claude Code, the user's choice since it calls nuxt.com: `claude mcp add --transport http nuxt https://nuxt.com/mcp`.
- Community Vue skills (MIT, not official): `vuejs-ai/skills`, to import from its repository when the user wants them.
