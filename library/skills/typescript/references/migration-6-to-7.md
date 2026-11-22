# Upgrading TypeScript 5.x → 6.0 → 7.0

Facts from the official release posts (TypeScript 6.0, 2026-03-23; TypeScript 7.0, 2026-07-08), checked on 2026-11-22. Go through 6.0 first: every option 7.0 removes is a deprecation error in 6.0, so 6.0 shows the whole list while the old behaviour still works.

## Order of work
1. **Check the tools before the compiler.** List the installed packages that load the TypeScript API: `typescript-eslint`, `vue-tsc`, `svelte-check`, `@astrojs/check`, `@angular/compiler-cli`, `ts-jest`, `ts-node`, any custom transformer or language-service plugin. Read their `peerDependencies` (`npm view <pkg> peerDependencies`). If one of them excludes 7, the project's `typescript` stays on 6.x; 7.0 can only come in as the side-by-side alias of step 6.
2. **Move to 6.0** (`typescript@^6`) with the project's package manager. Run the type-check: deprecation errors list what step 3 must change.
3. **Replace each deprecated option** with the table below, one at a time, re-running the type-check. Avoid `"ignoreDeprecations": "6.0"` except as a short bridge noted in the PR: 7.0 ignores it.
4. **Set the new defaults explicitly where the project relied on the old ones** (table "Defaults"). The usual three: `types` (list what the code uses), `rootDir` (when output paths moved), `strict` (only if the project was not strict and the task is not to make it strict).
5. **Compare with 7.0's type ordering** if declaration files or error output differ between machines: `--stableTypeOrdering` on 6.0 makes its ordering match 7.0 (up to 25 % slower; for diagnosis, not for keeping).
6. **Move to 7.0**:
   - No API consumer in step 1: `typescript@^7` and keep `tsc` in the scripts.
   - API consumers present: keep 6.0 under the `typescript` name and add 7.0 under another one, as the 7.0 post shows:
     ```json
     "devDependencies": {
       "typescript": "npm:@typescript/typescript6",
       "@typescript/native": "npm:typescript@^7.0.2"
     }
     ```
     `tsc` then runs 7.0 and `tsc6` runs 6.0; tools that `import "typescript"` get the 6.0 API.
7. **Editors**: VS Code uses the 7.0 language server through the `TypeScriptTeam.native-preview` extension; other editors talk to it over LSP. Not a repository change unless the project commits editor settings.
8. **Run everything the CI runs** (type-check, lint, build, tests). Done when they pass with no new suppression.

## Options that are errors in 7.0 (deprecated, or already unsupported, in 6.0)

| Option or syntax | Replacement |
|---|---|
| `target: "es5"` | `es2015` or later; the lowest supported target is ES2015. Older browsers need a separate transpiler step in the bundler. |
| `downlevelIteration` | Remove it (only meaningful for ES5). |
| `moduleResolution: "node"` / `"node10"` | `nodenext` (with `module: nodenext`) for code Node runs; `bundler` for code a bundler or Bun consumes. `bundler` now also works with `module: commonjs`. |
| `moduleResolution: "classic"` | Removed already in 6.0; same replacements. |
| `module: "amd"`, `"umd"`, `"system"`, `"none"` | Unsupported since 6.0. `esnext`, `nodenext` or `commonjs`; produce UMD or SystemJS output with the bundler. |
| `baseUrl` | Remove it and make each `paths` target relative to the tsconfig: `"@app/*": ["./src/app/*"]`. To keep bare imports resolving under `src/`, add `"*": ["./src/*"]`. |
| `outFile` | Removed in 6.0: concatenate with a bundler. |
| `alwaysStrict: false` | Remove it: every file is strict-mode code. |
| `esModuleInterop: false`, `allowSyntheticDefaultImports: false` | Rejected since 6.0. Remove them: the safe interop is always on. Default imports of CommonJS modules may change shape; check them. |
| `module Foo { … }` (namespace) | `namespace Foo { … }`. Ambient `declare module "pkg"` is unaffected. |
| `import x from "./a.json" assert { type: "json" }` | `with { type: "json" }`, also inside `import()`. |
| `/// <reference no-default-lib="true"/>` | `--noLib` or `--libReplacement`. |
| `tsc file.ts` while a `tsconfig.json` is present (TS5112) | Check the project through its tsconfig, or pass `--ignoreConfig` for a deliberate one-off file. |

## Defaults that changed in 6.0 (and stay in 7.0)

| Option | Old default | New default | Symptom when the project relied on the old one |
|---|---|---|---|
| `strict` | `false` | `true` | Many new errors in a loose project. Set `"strict": false` explicitly to keep the old checking, then tighten in a separate change. |
| `module` | depended on `target` | `esnext` | CommonJS output turned into ESM. Set `module` explicitly (`nodenext` for Node). |
| `target` | `es5` | latest year (ES2025 in 6.0; 7.0 takes the last stable year before `esnext`) | Newer syntax in the output. Set `target` to what the runtime supports. |
| `types` | every `@types/*` package | `[]` | "Cannot find name 'process' / 'describe' / 'vi'". List what the code uses: `["node"]`, `["vitest/globals"]`, `["jest"]`; `["*"]` restores the old behaviour. |
| `rootDir` | inferred from the input files | folder of the tsconfig | Output moved from `dist/` to `dist/src/`. Set `rootDir: "./src"`. |
| `noUncheckedSideEffectImports` | `false` | `true` | Error on `import "./styles.css"` or a polyfill import that does not resolve. Add the module declaration (`declare module "*.css";`) or fix the path. |
| `libReplacement` | `true` | `false` | Custom `@typescript/lib-*` replacement packages ignored. Turn it back on only if the project uses them. |
| `stableTypeOrdering` (7.0) | — | always on | Declaration files or error messages list union members in a different order than 5.x did. |

## Other 7.0 behaviour changes
- Parallel checking and emitting, tuned with `--checkers`, `--builders`, `--singleThreaded` (useful when a CI runner has few cores or little memory).
- A rebuilt `--watch` on the Parcel file watcher.
- JavaScript files (`checkJs`, JSDoc types) are analysed more like `.ts` files: JS projects that type-check through JSDoc can see new errors.
- Template literal inference keeps Unicode code points together (`"😀abc"` splits into `"😀"` and `"abc"`).
- No programmatic API in 7.0; the 7.0 post announces a new, different API for 7.1. Tools built on the 6.x API do not work against the 7.0 package.

## Sources
- [Announcing TypeScript 6.0](https://devblogs.microsoft.com/typescript/announcing-typescript-6-0/)
- [Announcing TypeScript 7.0](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)
- [TSConfig reference](https://www.typescriptlang.org/tsconfig/)
