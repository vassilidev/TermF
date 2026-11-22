---
name: svelte-sveltekit
description: >-
  Writes and changes Svelte 5 components and SvelteKit routes the way the installed versions expect:
  runes, snippets, load functions, form actions, endpoints, hooks, configuration, and the SvelteKit 2 to 3
  upgrade ($lib to #lib, $app/stores to $app/state, svelte.config.js moved into vite.config). Use when a task
  touches .svelte or .svelte.ts files, src/routes, or a SvelteKit config. Not for React, Vue or Angular
  components, nor for plain Vite apps without Svelte.
paths: "**/*.svelte, **/*.svelte.ts, **/*.svelte.js, **/+page.*, **/+layout.*, **/+server.*, **/+error.svelte, **/hooks.server.*, **/hooks.client.*, **/svelte.config.*"
license: MIT
metadata:
  termf-stacks: "svelte sveltekit"
  termf-roles: "front qa"
  termf-versions: ">=5"
  termf-verified: "2026-11-22 svelte@5.59.1 @sveltejs/kit@3.4.0"
  termf-sources: "https://svelte.dev/docs/kit/migrating-to-sveltekit-3 https://svelte.dev/docs/kit/configuration https://svelte.dev/docs/ai/overview https://svelte.dev/docs/ai/cli https://svelte.dev/llms.txt https://github.com/sveltejs/ai-tools"
---
# Svelte and SvelteKit

1. Read the installed versions of `svelte`, `@sveltejs/kit` and `typescript` in the lockfile (or `package.json`), check whether `svelte.config.js` exists and what the `sveltekit(...)` call in `vite.config.*` holds, and keep them as the reference for every later step: Kit 3 (October 2026) moved the configuration and renamed modules, so Kit 2 code breaks under Kit 3 and the reverse. Done when you can name both majors, the file they came from, and where the configuration lives.
2. Open the docs of those versions before writing code. The packages ship no docs; the official agent resources are the Svelte MCP server (`list-sections`, then `get-documentation` for every section whose `use_cases` match the task), the same commands from the shell (`npx -y @sveltejs/mcp list-sections`, `npx -y @sveltejs/mcp get-documentation '<section>'`), and https://svelte.dev/docs/kit/llms.txt and https://svelte.dev/docs/svelte/llms.txt. When the session has the Svelte MCP tools or the `svelte` plugin skills (`svelte-code-writer`, `svelte-core-bestpractices`), follow them first. Done when the page for every API you will touch is open.
3. Use the tools the project already has (package manager from the lockfile, `svelte-check`, Vitest or Playwright, its UI kit and CSS approach), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. Find a route or component that already does something close (data loading in `+page.server.ts` `load`, form actions, remote functions, `.svelte.ts` state classes) and copy its shape. Done when you can name the file you are modelling on.
5. Write the change. Route files: `+page.svelte`, `+page.ts` (universal `load`), `+page.server.ts` (server `load`, `actions`), `+layout*`, `+server.ts` (endpoint), `+error.svelte`; hooks in `src/hooks.server.ts` and `src/hooks.client.ts`. Server-only code goes in a file or folder whose name has a `server` segment. Lib imports use the project's form (`$lib/<path>` on Kit 2, `#lib/<path>.js` on Kit 3). Done when every import resolves to a module of the installed versions.
6. Run the autofixer on every changed `.svelte` file: `npx -y @sveltejs/mcp svelte-autofixer <file> --svelte-version <svelte version from step 1>` (add `--async` when the project turned on experimental async), fix, and run it again until it returns no issues and no suggestions. Done when it is clean, or, offline, when the report says it could not run.
7. When the task upgrades Kit 2 to 3, follow `references/kit-3-migration.md`. Done when its "Done when" holds.
8. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Kit 3 has no `svelte.config.js`: options go into `sveltekit({ ... })` in `vite.config.*`, the former `kit.*` options become top-level plugin options and Svelte `compilerOptions` sit beside them. A Kit 2 project keeps `svelte.config.js`, so check which file exists before adding an option.
- Kit 3 renames, each an error rather than a warning: `$lib/x` → `#lib/x.js` (subpath import declared in `package.json` `imports`, extension included); `$app/stores` removed → `$app/state` (`page`, `navigating`, `updated`, read without `$`); `$app/environment` → `$app/env`; `$env/*` deprecated → `$app/env/private`, `$app/env/public`; `invalidateAll` → `refreshAll`; `base`, `assets`, `resolveRoute` removed → `resolve('/blog/[slug]', { slug })` and `asset('logo.png')` (no leading slash for assets).
- Keep `typescript` on 6.x in a Kit 3 project, even when other packages of a monorepo moved to TypeScript 7: `@sveltejs/kit` 3 declares `typescript ^6.0.0` and `svelte-check` accepts `^5 || ^6`.
- `page.url` is a readonly URL in Kit 3: copy it (`new URL(page.url.href)`) before changing search params, then `goto` the copy.
- `error(404, 'Not found')` takes a string as second argument in Kit 3, extra fields go in a third; `json()` and `text()` are deprecated, return `Response.json(data)` or `new Response(text)`.
- Shallow routing in Kit 3 is `goto(url, { shallow: true, state })`; `goto` options `replaceState` → `replace`, `invalidateAll` → `refreshAll`, `keepFocus` + `noScroll` → `reset: false`. `goto` rejects a URL that matches no route, so external URLs go through `window.location.href`.
- In a Svelte 5 project, new code uses `let { title, count = 0 } = $props()`, `$state`, `$derived` (an `$effect` that writes state loops and runs late), `onclick={...}` and `{#snippet}` with `{@render children()}`. A file still in legacy syntax stays as it is unless the task asks to migrate it.
- `await` in components and remote functions are experimental: they need `compilerOptions.experimental.async: true` (and `experimental.remoteFunctions: true`). Use them only when the project already turned them on.

## Verify

- The project's scripts, with its package manager: usually `check` (`svelte-kit sync && svelte-check --tsconfig ./tsconfig.json`), `build` (`vite build`), `test` (Vitest) and its end-to-end script (Playwright).
- Without a `check` script: `npx svelte-kit sync && npx svelte-check --tsconfig ./tsconfig.json`, then `npx vite build`.
- `npx -y @sveltejs/mcp svelte-autofixer <file> --svelte-version <version>` on each changed `.svelte` file.

## Security

- `{@html ...}` renders raw HTML: pass only content you sanitized or that the project already trusts, because anything from users or the network there is an XSS.
- Form actions, `+server.ts` endpoints and remote functions are public HTTP endpoints, and a layout `load` does not run for them: check the user in `hooks.server.ts` or in each action and endpoint, and validate input with the project's schema library (Zod, Valibot…).
- Read secrets through `$app/env/private` (Kit 3) or `$env/static/private` / `$env/dynamic/private` (Kit 2), only in server-only files; when Kit refuses a server-only import in client code, move the code to the server instead of working around the check.
- Module-level state in server code (a module-level `$state` in a `.svelte.ts` file included) is shared by every request and every user during SSR: keep per-user data in `event.locals`, `load` data or component context.
- CSRF protection is always on in Kit 3: accept a known cross-origin form post by listing its origin in `csrf.trustedOrigins`, and keep the check on (Kit 2: `csrf.checkOrigin`).
- An external `redirect` in Kit 3 needs `{ external: true }` or an allowlist: keep the allowlist to the hosts the feature needs, against open redirects.

## References

- Read `references/kit-3-migration.md` when the project is on SvelteKit 2 and the task upgrades it, or when Kit 2 code (`$app/stores`, `$lib`, `svelte.config.js`, `invalidateAll`) must run under Kit 3.
- SvelteKit 3 migration guide: https://svelte.dev/docs/kit/migrating-to-sveltekit-3 — configuration: https://svelte.dev/docs/kit/configuration
- Svelte AI tools (MCP, CLI, plugins): https://svelte.dev/docs/ai/overview; Claude Code plugin (MIT): `/plugin marketplace add sveltejs/ai-tools`, then `/plugin install svelte`.
- Docs for agents: https://svelte.dev/llms.txt (full, medium, small and per-package variants).
