---
name: testing-js
description: >-
  Writes and runs JavaScript and TypeScript unit, integration and component tests with the
  project's runner, Vitest or Jest, at the installed version: config, mocks at the boundaries,
  fake timers, async assertions, Testing Library queries by role, upgrades (Vitest 4 to 5, Jest 29
  to 30), failing or flaky tests. Use when a task adds or fixes *.test.* / *.spec.* files, touches
  vitest.config.* or jest.config.*, or runs the test suite. Not for browser end-to-end tests
  (playwright), not for deciding what deserves a test (tests-from-criteria), not for Python or PHP
  tests.
paths: "**/*.test.*, **/*.spec.*, **/__tests__/**, **/vitest.config.*, **/vitest.workspace.*, **/jest.config.*, **/vite.config.*"
license: MIT
metadata:
  termf-stacks: "vitest jest"
  termf-roles: "qa front back"
  termf-versions: "vitest>=4 jest>=29"
  termf-verified: "2026-11-22 vitest@5.1.1 jest@30.5.2"
  termf-sources: "https://vitest.dev/llms.txt https://vitest.dev/guide/migration.md https://vitest.dev/guide/learn/writing-tests-with-ai.md https://jestjs.io/docs/upgrading-to-jest30 https://testing-library.com/docs/queries/about/"
---
# JavaScript tests: Vitest and Jest

Follow skill `tests-from-criteria` for what to test and at which level; this skill is how to do it in Vitest or Jest. Vitest publishes `https://vitest.dev/llms.txt` (and `llms-full.txt`); every page also exists in Markdown, for example `https://vitest.dev/guide/migration.md` and the guide `https://vitest.dev/guide/learn/writing-tests-with-ai.md`. Neither package ships docs for agents, and Jest has no `llms.txt`: use `https://jestjs.io/docs/` for the installed major.

## Gotchas
- Vitest starts in watch mode and the process never exits. Run `vitest run` (or `--run`); when the `test` script is a bare `vitest`, pass it through: `npm test -- --run`. Vitest tries to detect agents and then disables watch and switches to the `minimal` reporter (failures only), but the official guide calls that detection fragile.
- Vitest 5 (5.0 on 2026-09-03; 5.1.1 current) needs Node ≥ 22.12 and Vite ≥ 6.4 (`vite` is now a peer dependency; Yarn does not install it by itself). Behaviour changes that break tests written for 4.x:
  - `clearMocks` is on by default: call history is cleared before every test, so calls recorded at module top level or in `beforeAll` are gone when the test asserts on them. Implementations are kept.
  - `vi.mock`, `vi.unmock` and `vi.hoisted` inside a function, block or `describe` now throw: they are hoisted anyway, so write them at the top level; `vi.doMock` is the non-hoisted variant.
  - An unawaited `expect(p).resolves`, `.rejects` or `toMatchFileSnapshot` fails the test: always `await` them.
  - `test.sequential`, `describe.sequential` and the `sequential` option are removed: use `{ concurrent: false }`.
  - `toThrow("")` matches any error, `expect.poll` fails at its timeout, `-t`/`testNamePattern` matches the full name joined with `>`, config files are no longer looked up in parent folders, and the `json`/`junit` reporters write files under `.vitest/` instead of stdout.
  - Fake timers also mock `Temporal` when it exists.
- Vitest 4 changes still met in older configs: `test.workspace` and `vitest.workspace.*` became `test.projects`; the browser provider is an object from its own package (`playwright()` from `@vitest/browser-playwright`); `poolOptions`, `maxThreads`, `maxForks` became top-level options and `maxWorkers`.
- `vitest run --since-last-run` (5.1) reruns only the tests whose loaded files changed since their last run, plus those that failed. It misses files a test never loaded (a new `.env`, a new `__mocks__` file) and checkouts that keep old modification times: run the full suite before calling the work done.
- Vitest is not Jest: `vi.fn`/`vi.mock`, not `jest.*`; a `vi.mock` factory returns an object of named exports (`{ default: … }` for a default export); `vi.importActual` is async; files in `__mocks__` load only when `vi.mock()` is called; there is no `done` callback; `mockReset` restores the original implementation instead of an empty function.
- Without `globals: true`, Testing Library does not clean the DOM between Vitest tests: the project needs `afterEach(cleanup)` in a setup file, or rendered components leak into the next test.
- Jest 30 (30.0 in June 2025; 30.5.2 current) removed the matcher aliases: `toBeCalled` → `toHaveBeenCalled`, `toBeCalledWith` → `toHaveBeenCalledWith`, `lastCalledWith` → `toHaveBeenLastCalledWith`, `toReturn…` → `toHaveReturned…`, `toThrowError` → `toThrow`. Also: `--testPathPattern` is `--testPathPatterns`, `jest --init` and `jest.genMockFromModule` are gone (`jest.createMockFromModule`), `jest.mock` paths are case-sensitive, non-enumerable properties are ignored by object equality, minimums are Node 18 and TypeScript 5.4, `jest-environment-jsdom` runs jsdom 26.
- With React 19, `act` comes from `react`; `react-dom/test-utils` is removed.

## Procedure
1. **Read the runner and its setup.** In package.json: `vitest` or `jest` and their versions, the `test` scripts. The config: `vitest.config.*` (or the `test` key of `vite.config.*`), `jest.config.*` or the `"jest"` key. Note `globals`, `environment` (`node`, `jsdom`, `happy-dom`, browser mode), `setupFiles`, `projects`, `restoreMocks`, path aliases. Done when you can run a single test file without watch mode and know which environment it gets.
2. **Decide the tests** with skill `tests-from-criteria`. Done when each criterion has a test and a level.
3. **Copy the neighbours.** Same folder layout and file suffix, same `test`/`it` and `describe` style, same imports (explicit `import { test, expect, vi } from "vitest"` unless `globals` is on), same fixtures (`test.extend` in Vitest) and helpers. Done when the new file would not stand out in a review of the folder.
4. **Drive UI through Testing Library like a user.** Query with `screen.getByRole("button", { name: "Save" })` first, then `getByLabelText` for form fields, then `getByText`; `getByTestId` only when nothing accessible identifies the element. Interact with `const user = userEvent.setup()` and `await user.click(…)`; wait for appearance with `findBy…`; assert what the user sees. Done when no test reaches into component state or CSS classes.
5. **Fake only the boundaries.** Network with the project's tool (MSW handlers if present), time with `vi.useFakeTimers()` / `vi.setSystemTime()` (or `jest.useFakeTimers()`), randomness with a seeded stub. In Vitest, prefer `vi.mock(import("./mailer.js"), …)` to the string form: the path is type-checked and follows renames. Restore spies (`restoreMocks: true` or `vi.restoreAllMocks()` in `afterEach`; Vitest's default is `false`). Done when no module of the project itself is mocked unless it wraps an external service.
6. **Make async explicit.** `await` every promise and async assertion; wait for a state with `await expect.poll(…)`, `vi.waitFor` or `findBy…`, not with timers or sleeps. Done when the test passes repeated runs without retries (`vitest run <file> --repeats 10`).
7. **Run narrow, then wide.** One file, then the suite the CI runs for this package; see each new test fail without the change (skill `tests-from-criteria`, step 5). Done when the suite is green and no existing test, snapshot or threshold was loosened.

## Verify
- The project's scripts first: `npm test`, `npm run test:unit`, `npm run coverage` (or `pnpm`/`yarn`/`bun` per the lockfile); add `-- --run` when the script starts Vitest in watch mode.
- Vitest: `npx vitest run <file>`, `npx vitest run -t "<test name>"`, `npx vitest run --project <name>` in a multi-project config.
- Jest: `npx jest <file>`, `npx jest -t "<test name>"`, `npx jest --testPathPatterns "<regex>"`.
- Coverage only when the project measures it (`vitest run --coverage` needs the installed `@vitest/coverage-v8` or `-istanbul`); keep its thresholds as they are.

## Security
- Neither runner blocks the network: an unmocked `fetch` in a test reaches the real service. Fake HTTP at the boundary and point integration tests at the run's local services only.
- Fixtures and snapshots end up in git: fake data only (`alice@example.test`, `test-token`), and read the snapshot diff for secrets or personal data before keeping it.
- `vitest --ui` and the Vitest API server can run code on the machine; since 5.0 the UI needs the token Vitest prints. Keep them on localhost and out of CI.
- Updating snapshots (`-u`, `--update`) to make a red test green hides a regression; update only when the change is the intended behaviour and say so in the report.

## References
- [vitest.dev/llms.txt](https://vitest.dev/llms.txt), [Migrating to Vitest 5](https://vitest.dev/guide/migration.md) (links to the 4.0 and 3.0 guides), [Writing tests with AI](https://vitest.dev/guide/learn/writing-tests-with-ai.md), [Migrating from Jest](https://vitest.dev/guide/migration/jest.md).
- [Upgrading to Jest 30](https://jestjs.io/docs/upgrading-to-jest30).
- [Testing Library: query priority](https://testing-library.com/docs/queries/about/).
- Browser end-to-end tests: skill `playwright`.
