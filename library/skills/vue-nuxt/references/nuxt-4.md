# Nuxt 3 → 4, and Nuxt 5 defaults

Source: https://nuxt.com/docs/4.x/getting-started/upgrade (read 2026-11-22, `nuxt@4.6.4`). Re-read it for anything not listed here; the Nuxt MCP tool `get-documentation-page` serves the same page.

## Contents
1. Order of work
2. Folders
3. Data fetching
4. TypeScript
5. Other breaking changes
6. Nuxt 5 defaults (opt-in)
7. Done when

## 1. Order of work
1. Upgrade: `npx nuxt upgrade` (latest release), then install with the project's package manager.
2. Codemods: `npx codemod@0.18.7 nuxt/4/migration-recipe`. The upgrade guide pins `0.18.7` because of a known issue with the latest codemod release; review the diff file by file.
3. Walk sections 2 to 5 for what the codemod leaves, then `npx nuxt typecheck`, `npx nuxt build` and the tests.

## 2. Folders
- `srcDir` defaults to `app/`: `assets`, `components`, `composables`, `layouts`, `middleware`, `pages`, `plugins`, `utils`, `app.vue`, `app.config.ts`, `error.vue` move under `app/`.
- At the root: `server/` (`serverDir` = `<rootDir>/server`), `shared/`, `public/`, `layers/`, `modules/`, `content/`, `nuxt.config.ts`.
- `shared/` holds code used by both app and server; it cannot import Vue or Nitro code. Only `shared/utils/` and `shared/types/` are auto-imported; other files are imported through the `#shared` alias.
- Keeping the Nuxt 3 layout is possible by setting `srcDir` back; follow what the project chose rather than moving files unasked.
- `index` files in sub-folders of `app/middleware/` are now registered as middleware.

## 3. Data fetching (`useFetch`, `useAsyncData`)
- `data` and `error` default to `undefined` instead of `null`: update `=== null` checks.
- `data` is a `shallowRef`: deep mutations do not trigger updates; pass `deep: true` or replace the value.
- Calls with the same key share `data`, `error` and `status`; options that conflict for one key (different `transform`, `pick`, `default`…) warn. Give distinct keys to distinct requests.
- `getCachedData(key, nuxtApp, ctx)` runs on every fetch; `ctx.cause` is `'initial'`, `'refresh:hook'`, `'refresh:manual'` or `'watch'`.
- `pending` is true only while `status` is `'pending'` (no longer true before an `immediate: false` call runs).
- Reactive keys are supported; with `immediate: false`, a key change refetches only after the first fetch.
- `dedupe` accepts `'cancel'` or `'defer'`; booleans are gone.
- Clearing `data` (`clear()`, `clearNuxtData`) resets it to the `default` you gave; `clearNuxtState` resets `useState` to its initial value unless `{ reset: false }`.
- Prerendered pages share fetched data: keys must identify the data uniquely.

## 4. TypeScript
- Project references: Nuxt generates separate configs for app, server, shared and node contexts; the root `tsconfig.json` references them. Type-check with `npx nuxt typecheck` (options: `--checker=vue-tsc|golar`, build mode detected automatically).
- `noUncheckedIndexedAccess` is on by default: array and record access returns `T | undefined`.
- `vue-tsc` needs the TypeScript 6 API: keep `typescript` 6.x (or alias it to `@typescript/typescript6` in a TypeScript 7 workspace).

## 5. Other breaking changes
- Layer modules load before the project's own modules.
- `definePageMeta` data is read from the route, not duplicated in `route.meta`; page meta is scanned after `pages:extend` (use `pages:resolved` to override).
- Component names are normalized (they follow Nuxt's naming pattern): update `findComponent` lookups in `@vue/test-utils` tests and `<KeepAlive>` that relies on a name. Page component names now match route names (a `<KeepAlive>` `include` of `'index'` becomes the route name).
- Unhead v2: `vmid`, `hid`, `children` and `body` are removed from head tags; tag sorting and template params changed.
- `error.data` is already parsed in error pages: drop `JSON.parse`.
- Only component CSS is inlined by default.
- The SPA loading template sits next to `#__nuxt`, not inside it.
- `window.__NUXT__` is removed: use `useNuxtApp().payload`.
- `builder:watch` gets absolute paths.
- Lodash-based EJS templates are no longer compiled by Nuxt.
- Top-level `generate` config is removed: use `nitro.prerender`.
- `@nuxt/rspack-builder` is replaced by the Rsbuild builder in later 4.x releases (the guide says 4.7+).

## 6. Nuxt 5 defaults (opt-in)
- Nuxt 5 is in development. From Nuxt 4.2, `future: { compatibilityVersion: 5 }` in `nuxt.config` switches many defaults to Nuxt 5 behaviour (Vite Environment API, case-sensitive routing, normalized page names, typed pages and `$fetch`, server-side error boundaries, no Options API in the client bundle, stricter side-effect imports…). The full list is in the upgrade guide.
- Nitro 3 is separate from that flag: Nuxt uses Nitro 3 when the `nitro` package is a dependency and Nitro 2 when `nitropack` is (both listed: Nitro 3).
- Server code that imports from `nuxt/server` (Nuxt 4.6+) runs on both Nuxt 4 and Nuxt 5.
- Turn these on only when the task asks; when the project already has them, follow the Nuxt 5 behaviour described in the guide.

## 7. Done when
No component under the old root folders is left orphaned, `null` checks on `data`/`error` are updated, `npx nuxt typecheck` and `npx nuxt build` pass, and the tests of the main pages pass.
