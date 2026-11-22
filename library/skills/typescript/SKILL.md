---
name: typescript
description: >-
  Writes and fixes TypeScript the way the installed compiler expects: types at trust boundaries,
  narrowing instead of casts, tsconfig changes, type errors, upgrades from TypeScript 5 or 6 to 7.
  Use when a task touches .ts, .tsx, .mts or .cts files, a tsconfig, a type-check failure, or the
  `typescript` dependency. Not for plain JavaScript runtime questions (see javascript-node), test
  runner setup (testing-js) or framework conventions (react, nextjs, angular, vue-nuxt,
  svelte-sveltekit).
paths: "**/*.ts, **/*.tsx, **/*.mts, **/*.cts, **/tsconfig*.json"
license: MIT
metadata:
  termf-stacks: "typescript"
  termf-roles: "front back qa"
  termf-versions: ">=5.8"
  termf-verified: "2026-11-22 typescript@7.0.2 (6.0.3 for 6.x)"
  termf-sources: "https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/ https://devblogs.microsoft.com/typescript/announcing-typescript-6-0/ https://www.typescriptlang.org/tsconfig/"
---
# TypeScript

TypeScript ships no documentation or skill for agents (no `llms.txt`, nothing in the package): the release posts linked below are the reference for the 6.0 and 7.0 changes.

## Gotchas
- TypeScript 7 (the `typescript` package from 7.0, July 2026) is a native compiler with no programmatic API: the package only holds the `tsc` binary. Tools that import `typescript` stay on 6.x, and their peer ranges say so: `typescript-eslint` (`<6.1.0`), `svelte-check` and `@astrojs/check` (`^5 || ^6`), `@angular/compiler-cli` (`>=6.0 <6.1`), `ts-jest` (`<7`); Vue, MDX, Astro and Svelte tooling need 6.0 per the 7.0 post. When one of them is installed, leave `typescript` on 6.x. To also get the 7.0 checker, the official side-by-side setup is two aliases: `"typescript": "npm:@typescript/typescript6"` (it only provides a `tsc6` binary) and `"@typescript/native": "npm:typescript@^7.0.2"` (provides `tsc`). With the first alias alone, scripts that call `tsc` stop working.
- New defaults since 6.0, kept in 7.0, apply to any tsconfig that does not set them:
  - `strict: true` and `module: esnext`; `target` is the latest standard year.
  - `types: []`: global types from `@types/*` are no longer picked up. A sudden "Cannot find name 'process'" or "'describe' is not defined" means `types` needs the entries the code uses, for example `["node"]` or `["vitest/globals"]`; `["*"]` restores the old behaviour.
  - `rootDir` is the folder of the tsconfig, so emitted files can move (`dist/src/…`) when it was inferred before.
  - `noUncheckedSideEffectImports: true`: a side-effect import (`import "./polyfill"`, `import "./styles.css"`) of a file the compiler cannot resolve is an error.
  - `esModuleInterop` and `allowSyntheticDefaultImports` can no longer be `false`.
- Removed in 7.0, deprecated in 6.0: `target: es5`, `downlevelIteration`, `moduleResolution` `node`/`node10`/`classic`, `module` `amd`/`umd`/`system`/`none`, `baseUrl`, `outFile`, `alwaysStrict: false`, the `module` keyword for namespaces, `assert` on imports. Replacements are in `references/migration-6-to-7.md`. `"ignoreDeprecations": "6.0"` silences them on 6.x only and is not a fix.
- `moduleResolution`: `nodenext` (with `module: nodenext`) for code that Node runs, `bundler` for code a bundler or Bun consumes. Under `nodenext`, relative imports carry the extension of the emitted file (`./user.js` from `user.ts`), or `.ts` with `rewriteRelativeImportExtensions`.
- `tsc some-file.ts` in a folder that has a `tsconfig.json` is an error since 6.0 (TS5112). Check the project with its tsconfig; pass `--ignoreConfig` only for a deliberate one-off file.
- Import attributes use `with`: `import data from "./data.json" with { type: "json" }`; `assert { … }` is an error.

## Procedure
1. **Read the version and the effective config.** Read `typescript` in `package.json` and its resolved version in the lockfile: an alias (`npm:@typescript/typescript6@…`) means the project runs 6.x under the `tsc6` name. Read `tsconfig.json` and every file in its `extends` chain, plus `references` (project references). Done when you know the major version and the values of `strict`, `module`, `moduleResolution`, `types`, `verbatimModuleSyntax` and `noUncheckedIndexedAccess` actually in force.
2. **Find the project's type-check command.** Use the script in `package.json` (`typecheck`, `check`, `lint`, `build`) or the framework wrapper the project already runs (`vue-tsc`, `svelte-check`, `ng build`, `next build`, `astro check`). Done when you have the command; run it once before editing so you know which errors already exist.
3. **Type the boundaries as `unknown` and parse them.** Anything that comes from outside the program (request body, query string, `JSON.parse`, `fetch().json()`, environment variables, `localStorage`, `postMessage`, files, third-party webhooks) enters as `unknown` and goes through the validation library the project already uses (zod, valibot, arktype, yup…). Derive the static type from the schema (`z.infer<typeof Schema>` or the library's equivalent) instead of declaring it twice. Without a library in the project, write a type guard (`function isOrder(x: unknown): x is Order`) that checks every field the code reads. Done when no external value reaches business code through `as` or an annotation alone.
4. **Fix errors at their cause.** Narrow with `typeof`, `in`, `instanceof`, discriminated unions and early returns; use `satisfies` to check an object literal while keeping its literal types; model optional data as optional. Reach for `as`, `!`, `any` or `@ts-ignore` only when the compiler is provably wrong (a library typing bug), and then write `// @ts-expect-error <reason>` so the line errors once the bug is fixed. Done when the diff adds no `any`, no `as` on external data and no `@ts-ignore`.
5. **Match the module settings.** With `verbatimModuleSyntax`, type-only imports are written `import type { X }` or `import { type X }`; under `nodenext`, relative imports carry their extension; path aliases only resolve if both `paths` and the bundler or runtime know them (Node itself ignores `paths`). Done when the type-check and the build or run both resolve every import.
6. **Change tsconfig only when the task asks for it.** A new option changes how every file is checked. For a version upgrade or a deprecation error, follow `references/migration-6-to-7.md`. Done when the type-check passes with the same or fewer suppressions.
7. **Run Verify.** Done when it is clean.

## Verify
- The project's script first: `npm run typecheck` (or `pnpm`, `yarn`, `bun` per the lockfile), `npm run lint`, `npm run build`.
- Otherwise: `npx tsc --noEmit` for a single tsconfig, `npx tsc -b` when the root tsconfig has `references`; `npx tsc6 --noEmit` in a project that aliases 6.x; the framework wrapper for `.vue`, `.svelte` or Angular templates, since plain `tsc` does not check them.
- `git diff` shows no new `any`, `as unknown as`, `@ts-ignore` or `eslint-disable` added to get green.

## Security
- Types are erased at run time: an annotation on `req.body`, a `JSON.parse` result or an env variable validates nothing. Parse at the boundary (step 3) before the value reaches a query, a file path or a shell argument.
- `as` and `!` on external data turn a missing field into a crash or a silent `undefined` in an authorisation check; narrow and handle the missing case.
- `Record<string, T>` built from untrusted keys can be polluted through `__proto__` or `constructor`; build such maps with `new Map()` or `Object.create(null)`, or validate the keys.
- A new `@types/*` or validation package is a dependency like any other: check the exact name on the registry (typosquats target `@types`) and that the lockfile changes only by what you added.

## References
- Read `references/migration-6-to-7.md` when upgrading TypeScript, when a deprecated or removed option errors, or when a tool refuses TypeScript 7.
- [Announcing TypeScript 7.0](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/), [Announcing TypeScript 6.0](https://devblogs.microsoft.com/typescript/announcing-typescript-6-0/), [TSConfig reference](https://www.typescriptlang.org/tsconfig/).
- Running `.ts` files directly with Node (type stripping) is in skill `javascript-node`.
