# SvelteKit 2 → 3 migration

Source: https://svelte.dev/docs/kit/migrating-to-sveltekit-3 (read 2026-11-22, `@sveltejs/kit@3.4.0`). Re-read the page for anything not listed here.

## Contents
1. Order of work
2. Minimum versions
3. Configuration
4. Module renames
5. Navigation and paths
6. Errors, responses, cookies
7. Server-only files, params, service worker
8. Adapters
9. Done when

## 1. Order of work
1. Upgrade to the latest Kit 2.x and fix its deprecation warnings first: they point at most of what breaks.
2. Raise the minimum versions (section 2) in `package.json`, install with the project's package manager.
3. Run the codemod: `npx sv migrate sveltekit-3`. It does most renames; review its diff file by file.
4. Walk sections 3 to 8 for what the codemod leaves, then run `check`, `build` and the tests.

## 2. Minimum versions
Node 22.17, TypeScript 6, Svelte 5.57.1, Vite 8.0.12, `@sveltejs/vite-plugin-svelte` 7; adapter-cloudflare needs `wrangler` ^4.67.0. Keep `typescript` on 6.x: Kit 3 and `svelte-check` do not accept TypeScript 7.

## 3. Configuration
- Delete `svelte.config.js`: its options move into `sveltekit({ ... })` in `vite.config.*`. What was under `kit` becomes top-level options of the plugin; `compilerOptions` sits beside them.
- Removed options: `files.lib`, `experimental.handleRenderingErrors`, `experimental.instrumentation`, `vitePlugin` (pass its options such as `inspector` directly), `preloadStrategy` (always `modulepreload`), `prerender.origin` (→ `paths.origin`), `csrf.checkOrigin` (→ `csrf.trustedOrigins`), `experimental.tracing` (→ top-level `tracing`).
- `alias` is deprecated: use subpath imports in `package.json` `imports`.
- `version.pollInterval` now defaults to one hour.
- `tsconfig.json` extends `"$app/tsconfig"` (instead of `./.svelte-kit/tsconfig.json`) and lists its own `include` and `exclude`. A service worker gets its own tsconfig extending `$app/tsconfig/service-worker`, and the root tsconfig excludes `src/service-worker`.

## 4. Module renames
| Kit 2 | Kit 3 |
|---|---|
| `import x from '$lib/x'` | `import x from '#lib/x.js'`, with `"#lib": "./src/lib/index.js"` and `"#lib/*": "./src/lib/*"` in `package.json` `imports` |
| `$app/stores` (`$page`, `$navigating`, `$updated`) | `$app/state` (`page`, `navigating`, `updated`, no `$` prefix) |
| `$app/environment` | `$app/env` |
| `$env/static/private`, `$env/dynamic/public`… (deprecated) | `$app/env/private`, `$app/env/public` |
| `$service-worker` `version` / `assets`, `immutable`, `prerendered` / `resolved` | `$app/env` / `$app/manifest` / `$app/paths` |
| `json(...)`, `text(...)` from `@sveltejs/kit` (deprecated) | `Response.json(...)`, `new Response(text)` |
| `defineParams`, env types, hook types (`Handle`) from `@sveltejs/kit` | `@sveltejs/kit/params`, `@sveltejs/kit/env`, `@sveltejs/kit/hooks` |
| `defineEnvVars` from `@sveltejs/kit/hooks` | `@sveltejs/kit/env` |
| Remote function types (`RemoteQuery`, `RemoteForm`, `RemoteCommand`) | `$app/server` |
| `import '@sveltejs/kit/node/polyfills'` | removed: delete the import |
| `await getRequest(...)`, `await setResponse(...)` (`@sveltejs/kit/node`) | synchronous: drop the `await` |

## 5. Navigation and paths
- `invalidateAll()` → `refreshAll()` (it keeps `page.state`); `invalidate`/`refreshAll` no longer abort a navigation in flight.
- `goto` options: `invalidateAll` → `refreshAll`, `replaceState` → `replace`, `keepFocus: true` + `noScroll: true` → `reset: false`. `goto` rejects for a URL that matches no route: external URLs go through `window.location.href`.
- Shallow routing: `pushState` / `replaceState` are deprecated → `goto(url, { shallow: true, state })` (`replace: true` to replace); `persistState: true` keeps `page.state` after a reload; shallow navigations now fire `beforeNavigate`, `onNavigate`, `afterNavigate` (filter on `shallow`).
- `preloadData` returns `{ type: 'error', status, error }` on failure: add that branch.
- `$app/paths`: `base`, `assets` and `resolveRoute` are removed. `base + resolveRoute('/blog/[slug]', { slug })` → `resolve('/blog/[slug]', { slug })`; `assets + '/logo.png'` → `asset('logo.png')`. Only route IDs start with `/`. Types `Pathname` → `Path`, `Asset` → `AssetPath`.
- `page.url` is readonly: `new URL(page.url.href)` before editing.
- Enhanced forms whose `action` points to another page now navigate there after submit. A link to the current page calls `refreshAll()`.
- `data-sveltekit-*` attributes: `"off"` → `"false"`.

## 6. Errors, responses, cookies
- `App.Error` always has `status`. `error(status, message)`: `message` must be a string; extra fields in a third argument.
- `handleValidationError` is removed: validation errors reach `handleError` with `kind: 'validation'`. `handleError` now receives every error, including `error(...)` ones, and may return a `status`.
- Form action responses use the status given to `fail(...)`.
- Empty 2xx responses from `+server.*` have no body; `resolve` in `handle` always returns a `Promise<Response>`.
- External `redirect(...)` needs `{ external: true }` or an allowlist.
- CSRF is always on: cross-origin form posts are refused unless their origin is in `csrf.trustedOrigins`; cross-origin mutating requests without `Content-Type` are refused.
- Cookies use cookie v2: ASCII names only, default `path` is `'/'`, types `CookieSerializeOptions` → `SerializeOptions`, `CookieParseOptions` → `ParseOptions`.
- `isHttpError` / `isRedirect` instead of `instanceof` checks.

## 7. Server-only files, params, service worker
- Any file with a `server` segment in its name (`db.server.ts`, `server.ts`) and any `server` directory (except under `src/routes` and `static`) is server-only.
- Param matchers in `src/params/` become one `src/params.ts` built with `defineParams`.
- `src/instrumentation.server.*` is picked up automatically; OpenTelemetry is opt-in through `tracing.server`.
- Service workers register as `type: 'module'` and can import `$app/env`, `$app/paths`, `$app/service-worker`.
- Remote functions (still experimental: `experimental.remoteFunctions` plus `compilerOptions.experimental.async`): form fields use `field.as(...)`; `event.url`, `event.params`, `event.route` throw inside `query`, pass values as arguments.

## 8. Adapters
- adapter-node: bundled with rolldown; static assets come from a build-time list (files added later are not served); only `GET`/`HEAD` serve them.
- adapter-cloudflare: `env` and `ctx.waitUntil` come from `cloudflare:workers`; run `wrangler types`.
- adapter-vercel: the `edge` runtime is gone.
- adapter-netlify: needs Netlify CLI 17.31+; the publish directory is an adapter option.

## 9. Done when
`svelte.config.js` is gone, no import of `$app/stores`, `$app/environment` or `$lib` remains (`grep -rn` over `src`), `check` and `build` pass, and the end-to-end tests of the main routes pass.
