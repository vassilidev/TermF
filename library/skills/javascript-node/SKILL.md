---
name: javascript-node
description: >-
  Writes and changes Node.js code and tooling the way the installed Node expects: ES modules or
  CommonJS, running TypeScript files directly, scripts, environment files, the built-in test runner,
  child processes, file paths, ESLint flat config, the project's package manager. Use when a task
  touches server or CLI JavaScript, package.json scripts or engines, .js/.mjs/.cjs files, a
  `node:` import, eslint.config.*, or a Node upgrade. Not for browser-only UI code (react, vue-nuxt,
  svelte-sveltekit, angular), type design (typescript) or test design (testing-js,
  tests-from-criteria).
paths: "**/*.js, **/*.mjs, **/*.cjs, **/*.mts, **/*.cts, **/package.json, **/eslint.config.*, **/.nvmrc, **/.node-version"
license: MIT
metadata:
  termf-stacks: "node"
  termf-roles: "back front devops qa"
  termf-versions: ">=20"
  termf-verified: "2026-11-22 node@26.13.1 eslint@10.15.0"
  termf-sources: "https://nodejs.org/llms.txt https://nodejs.org/docs/latest/api/typescript.md https://nodejs.org/docs/latest/api/cli.md https://nodejs.org/en/blog/announcements/evolving-the-nodejs-release-schedule https://eslint.org/blog/2026/02/eslint-v10.0.0-released/"
---
# Node.js

1. Read the Node version in `.nvmrc`, `.node-version`, the `volta` key or `engines.node` of `package.json` (the CI setup step otherwise), compare it with `node --version`, and keep it as the reference for every later step. Done when you can name the version and the file it came from.
2. Open the docs of that version before writing code: Node publishes an index for agents at `https://nodejs.org/llms.txt`, and every API page exists in Markdown (`https://nodejs.org/docs/latest/api/<module>.md`, or `docs/latest-v24.x/…` for an older line); nothing ships inside the `node` binary. Done when the page of the module you touch is open.
3. Use the tools the project already has (the package manager named by the lockfile — `pnpm-lock.yaml`, `yarn.lock`, `bun.lock`, `package-lock.json` — or by `packageManager`, its test runner, linter and TypeScript runner), because a second tool splits the project. Done when every command you plan to run comes from the project.
4. **Prefer what Node already provides** over a new dependency, within the project's Node major: global `fetch`, `node:test` + `node:assert`, `--env-file`, `--watch`, `util.parseArgs`, `util.styleText`, `fs.promises.glob`. Add a package only when the built-in does not cover the need, and with the project's package manager so the lockfile stays the single one. Done when every new import resolves to a built-in (`node:` prefix) or a dependency listed in package.json.
5. **Write the module in the file's own system.** Read the `"type"` of the nearest package.json and match the file's `import`/`require` style; in ESM, write relative imports with their extension. For `.ts` run by plain `node`, keep to erasable syntax (Gotchas) and the tsconfig settings Node recommends: `module: nodenext`, `erasableSyntaxOnly`, `verbatimModuleSyntax`, `rewriteRelativeImportExtensions`. Done when `node <entry>` (or the project's start script) loads the file without a module error.
6. **Handle async and errors explicitly.** Await every promise or return it; wrap rethrown errors with `new Error(message, { cause })`; close servers, timers and handles you open so tests and scripts exit. Done when the process exits on its own after the work and no unhandled rejection is logged.
7. **Test with the project's runner.** Follow skill `tests-from-criteria` for what to test and skill `testing-js` for Vitest or Jest. In a project with no runner, use `node --test` (it finds `*.test.*`, `*_test.*`, `test-*`, `test/**` files, `.ts` included). Done when the new tests pass and were seen failing without the change.
8. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- Release lines on 2026-11-22: Node 26 is Active LTS ("Lithium", since 2026-10-28), 24 and 22 are in maintenance, Node 20 reached end of life on 2026-04-30. From Node 27 there is one major a year, in April, and every major becomes LTS; 27 is in alpha now and not meant for production.
- Running `.ts` files with plain `node` works (type stripping: on by default since 22.18 / 23.6, stable since 24.12 / 25.2) but only for erasable syntax: `enum`, namespaces with runtime code, parameter properties and import aliases (`import x = …`) throw `ERR_UNSUPPORTED_TYPESCRIPT_SYNTAX`; decorators are a parse error; `.tsx` is unsupported; `tsconfig.json` is ignored, so `paths` aliases fail (use `#` subpath imports from `package.json`); type-only imports need `import type`; relative imports keep the `.ts` extension; files under `node_modules` are refused. Nothing is type-checked: run `tsc` as well. `--experimental-transform-types` was removed in Node 26.
- A `.js` file follows the nearest `package.json` `"type"` (`"module"` = ESM; missing or `"commonjs"` = CommonJS); `.mjs`/`.mts` are always ESM, `.cjs`/`.cts` always CommonJS. ESM has no `__dirname`/`__filename` (`import.meta.dirname`, `import.meta.filename`), no directory-index or extensionless resolution, and imports need the full file name.
- `require()` of an ES module works without a flag since 20.19 / 22.12 (stable since 25.4), unless the module graph uses top-level `await` (`ERR_REQUIRE_ASYNC_MODULE`): a CommonJS project can load an ESM-only package with `require`, so converting the project to ESM is not needed for that.
- `node --run <script>` starts package.json scripts faster but skips `pre`/`post` scripts and the package-manager environment variables; use the package manager (`npm run`, `pnpm run`…) when a script relies on them.
- `--env-file=.env` fails when the file is missing (`--env-file-if-exists` does not), and variables already set in the environment win over the file. It replaces `dotenv` for scripts the project starts with `node`.
- Corepack is not bundled since Node 25: `corepack enable` fails on Node 26 unless the project installs corepack itself. Use the package manager that the lockfile and the `packageManager` field name, as installed on the PATH.
- ESLint 10 (2026-02-06) reads only `eslint.config.{js,mjs,cjs,ts,mts,cts}`: `.eslintrc*` and `.eslintignore` are silently ignored, `/* eslint-env */` comments are errors, the config is looked up from each linted file's folder (a nested `eslint.config.*` now applies to its subtree), and Node `^20.19 || ^22.13 || >=24` is required. A project still on `.eslintrc` is on ESLint 9 or older: keep that version unless the task is the migration (`npx @eslint/migrate-config .eslintrc.json`).

## Verify

- Project scripts first, through its package manager: `test`, `lint`, `typecheck`, `build`, `start`.
- Without scripts: `node --test`, `npx eslint .`, `node --check <file.js>` for a syntax check (not available for `.ts`), `npx tsc --noEmit` when the files are TypeScript.
- Install from the lockfile without changing it: `npm ci`, `pnpm install --frozen-lockfile`, `yarn install --immutable` (Yarn 2+), `bun install --frozen-lockfile`.

## Security

- Run processes with `execFile` or `spawn` and an argument array; `exec`, `execSync` or `shell: true` with any interpolated value is command injection. Validate an executable name against an allowlist when it comes from input.
- Resolve user-supplied paths against a base folder and check the result stays inside it (`const root = path.resolve(base); const p = path.resolve(root, input); if (p !== root && !p.startsWith(root + path.sep)) refuse`); reject absolute paths and NUL bytes before touching the filesystem.
- Merging untrusted JSON into objects (`Object.assign`, deep-merge helpers, `obj[key] = value` with keys from input) can set `__proto__` or `constructor.prototype`; use `Map`, `Object.create(null)` or schema validation that rejects unknown keys.
- `eval`, `new Function` and the `node:vm` module are not sandboxes: the Node docs state `node:vm` is not a security mechanism. Untrusted code goes in a separate process with the permission model (`--permission`, stable since 22.13 / 23.5) or not at all.
- Read secrets from the environment (`process.env`, `--env-file` for local runs); keep `.env` files out of git and out of logs, error messages and responses.
- New dependencies: check the exact package name and its install scripts before adding it; the lockfile diff should change only by what you added.

## References

- [nodejs.org/llms.txt](https://nodejs.org/llms.txt) (index of the API pages in Markdown), [Modules: TypeScript](https://nodejs.org/docs/latest/api/typescript.md), [Command-line API](https://nodejs.org/docs/latest/api/cli.md), [Test runner](https://nodejs.org/docs/latest/api/test.md), [Packages and `"type"`](https://nodejs.org/docs/latest/api/packages.md).
- [Release schedule announcement](https://nodejs.org/en/blog/announcements/evolving-the-nodejs-release-schedule); dates per line in [nodejs/Release `schedule.json`](https://github.com/nodejs/Release/blob/main/schedule.json).
- [ESLint v10.0.0 released](https://eslint.org/blog/2026/02/eslint-v10.0.0-released/) and its migration guide `https://eslint.org/docs/latest/use/migrate-to-10.0.0`.
