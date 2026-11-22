---
name: nextjs
description: >-
  Changes a Next.js app the way the installed version expects: App Router routes, layouts, Server
  Components and Server Actions, caching and revalidation, proxy (formerly middleware), metadata,
  next.config and upgrades. Sends the agent to the docs bundled with the installed Next.js and keeps
  the version 16 pitfalls that agents trained on older releases get wrong. Use when a task touches a
  Next.js project (next.config.*, app/, pages/, proxy.ts, middleware.ts). Not for React code outside
  Next.js, nor for styling alone.
paths: "**/next.config.*, **/app/**/page.*, **/app/**/layout.*, **/app/**/route.*, **/pages/_app.*, **/pages/_document.*, **/proxy.*, **/middleware.*"
license: MIT
metadata:
  termf-stacks: "nextjs"
  termf-roles: "front back seo qa"
  termf-versions: ">=15"
  termf-verified: "2026-11-22 next@16.4.6"
  termf-sources: "https://nextjs.org/docs/app/guides/ai-agents https://nextjs.org/docs/app/guides/upgrading/version-16 https://nextjs.org/docs/llms.txt https://nextjs.org/docs/app/guides/mcp https://nextjs.org/docs/app/guides/data-security https://github.com/vercel/next.js/tree/canary/skills"
---
# Next.js

1. Read the installed version in the lockfile entry for `next` (or `node_modules/next/package.json`), and keep it as the reference for every later step. Note the router of each route you touch: `app/` is the App Router, `pages/` the Pages Router, and both can coexist. Done when you can name the version and the file it came from.
2. Open the docs of that version before writing code. From 16.2 on they ship in the package: search `node_modules/next/dist/docs/` (resolved from the app's own folder in a monorepo), `01-app/` for the App Router, `02-pages/` for the Pages Router, and open the page of every API, file convention or config option you are about to use. For 16.1 and earlier, open the nextjs.org page with `.md` appended (index: `https://nextjs.org/docs/llms.txt`); the site documents the latest release, so read the version history table at the bottom of the page. Done when the page for each API in your change is open.
3. Use the tools the project already has (package manager, linter or Biome, test runner, data access layer, auth helper, cache tags), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. Make the change in the project's existing patterns and `'use client'` boundaries. Done when the change uses no API you did not read in step 2.
5. If a dev server runs, check the routes you touched on it. One may already run: `.next/dev/lock` gives its URL and PID, and a TermF run exports `PORT`; reuse it rather than starting a second one. When the session has the `next-devtools` MCP server or the `next-dev-loop` skill, read errors and compilation issues through them. Done when no new error shows for those routes.
6. Leave the managed `AGENTS.md` block alone: if `next dev` created or changed the block between `<!-- BEGIN:nextjs-agent-rules -->` and its end marker and the task did not ask for it, neither edit nor delete it (`next dev` writes it back) and mention it in your report so the user decides whether to commit it. Done when your report says whether `AGENTS.md` changed.
7. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- The `nextjs-agent-rules` block that `next dev` writes into `AGENTS.md` does not reach you when the repository has a `CLAUDE.md` that does not import `AGENTS.md` (Claude Code then reads `CLAUDE.md` only), or when `next dev` wrote it after your session started. Step 2 does its job.
- Next 16 removed synchronous access to `cookies()`, `headers()`, `draftMode()`, `params` and `searchParams`: await them, including `params` in route handlers, layouts, `default.js` and the image and icon metadata files. Type props with the global `PageProps<'/blog/[slug]'>`, `LayoutProps` and `RouteContext` that `next typegen` generates.
- `middleware.ts` is deprecated: the file is `proxy.ts` and the export `proxy`. Proxy always runs on Node.js and setting `runtime` there throws; a project that needs the edge runtime keeps `middleware`.
- Turbopack is the default for `next dev` and `next build`. A `webpack()` function in `next.config` (often added by a plugin) makes `next build` fail; the project chooses between `--webpack` and a Turbopack migration, so ask rather than pick.
- `revalidateTag(tag)` with one argument is deprecated and a type error: write `revalidateTag(tag, 'max')` for stale-while-revalidate. In a Server Action that must show the user's own write at once, use `updateTag(tag)`; `refresh()` refreshes the client router. `cacheLife` and `cacheTag` lost their `unstable_` prefix.
- `experimental.ppr`, `experimental.dynamicIO` and `experimental.useCache` are gone: `cacheComponents: true` replaces them. With it, uncached data outside `<Suspense>` fails the prerender, and the error lists labelled fixes (`[stream]`, `[cache]`, `[block]`) with a `/docs/messages/...` page written for agents: read it before choosing.
- `next lint` no longer exists and `next build` no longer lints: lint through the project's script, or ESLint (flat config) directly.
- Every parallel route slot (`@slot/`) needs a `default.js`, or the build fails.
- The App Router renders with the React canary bundled in Next, not with the `react` version in `package.json`.
- Next 16 needs Node.js 20.9+ and TypeScript 5.1+.

## Verify

- The project's own scripts from `package.json`: `lint`, `typecheck` or `type-check`, `test`, `build`.
- `npx next typegen && npx tsc --noEmit` for types, including the generated route types, when the project has no typecheck script.
- `npx eslint .`, or the project's Biome command, when the project has no lint script.
- `npx next build`; add `--debug-prerender` for source-mapped prerender errors and to keep checking the other routes after the first failure.

## Security

- Every `'use server'` function is a public endpoint: its arguments come from the client, so validate them, check the caller's authorization inside the function, and return only the fields the UI needs.
- Proxy is for optimistic redirects, not for authorization: check access where the data is read or written (the data access layer, the action, the route handler).
- Variables prefixed `NEXT_PUBLIC_` are inlined into the client bundle, so secrets never take that prefix. Mark server-only modules with `import 'server-only'`.
- Values captured by an inline Server Action travel to the client and back, encrypted; keep secrets out of those closures rather than relying on the encryption.
- React Server Components had critical advisories in December 2025 (CVE-2025-55182, then denial-of-service and source-exposure fixes). When you touch dependencies, keep `next` on a release the advisories list as fixed.

## References

- Bundled docs: `node_modules/next/dist/docs/01-app/02-guides/ai-agents.md` (agent tooling), `.../02-guides/upgrading/version-16.md` (every v16 change), `.../02-guides/data-security.md`, `.../02-guides/mcp.md`.
- Online: `https://nextjs.org/docs/llms.txt`, any docs page with `.md` appended, error pages under `https://nextjs.org/docs/messages/`.
- Official workflow skills (MIT, `skills/` in `vercel/next.js`): `next-dev-loop`, `next-cache-components-adoption`, `next-bundle-optimizer` and others. Suggest them to the user (`npx skills add vercel/next.js --skill next-dev-loop`) rather than installing them yourself.
- Upgrades are their own task, only when the ticket asks for one: `npx @next/codemod@canary upgrade latest`, or `npx next@canary upgrade --agent=latest`, which hands the migration guides to an agent.
