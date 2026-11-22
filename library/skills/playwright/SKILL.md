---
name: playwright
description: >-
  Writes, runs and repairs Playwright end-to-end tests (@playwright/test) the way the installed
  version expects: user-facing locators, web-first assertions, signed-in state, traces, axe scans,
  and the dev server port of the run. Use when a task adds or changes browser or e2e tests,
  playwright.config, a page object, or a flaky or failing Playwright test. Not for unit or component
  tests run by Vitest or Jest (testing-js), nor for deciding what deserves a test
  (tests-from-criteria).
paths: "playwright.config.*, **/e2e/**, **/playwright/**, **/*.spec.ts, **/*.spec.js"
license: MIT
metadata:
  termf-stacks: "playwright"
  termf-roles: "qa front"
  termf-versions: ">=1.56"
  termf-verified: "2026-11-22 @playwright/test@1.65.1"
  termf-sources: "https://playwright.dev/docs/release-notes https://playwright.dev/docs/best-practices https://playwright.dev/docs/auth https://playwright.dev/docs/accessibility-testing https://playwright.dev/docs/test-agents https://github.com/microsoft/playwright"
---
# Playwright

1. Read the installed version in `@playwright/test` in `package.json` and its lockfile entry, and keep it as the reference for every later step. Done when you can name the version and the file it came from.
2. Open the docs of that version before writing code: from 1.62 the installed package ships its own agent skills, read in place. `node_modules/playwright-core/lib/tools/skills/playwright-cli/SKILL.md` (browser driving, `references/test-generation.md` for plan, generate and heal, `references/playwright-tests.md` for running and debugging), `playwright-trace/SKILL.md` (reading a trace from the terminal) and `playwright-component-testing/SKILL.md`; their commands run as `npx playwright cli …` and `npx playwright trace …`. Done when the page for the task is open.
3. Use the tools the project already has (package manager, its e2e script, `playwright.config.*` with its `testDir`, `projects`, `use.baseURL`, `webServer`, `retries` and `trace`, `@axe-core/playwright` when installed), because a second tool splits the project. Done when every command you plan to run comes from the project, and you know the test folder, the browsers and how the app is started.
4. Point the config at the run's server. In a TermF run the dev server listens on `PORT` (set for the session and for the project's `dev` command). Read it instead of a hard-coded port:
   ```ts
   const port = process.env.PORT ?? '3000'; // keep the project's current default
   export default defineConfig({
     use: { baseURL: `http://localhost:${port}` },
     webServer: { command: 'npm run dev', url: `http://localhost:${port}`, reuseExistingServer: !process.env.CI },
   });
   ```
   Keep the project's own `command` and other fields; only the port changes. `reuseExistingServer` lets the test reuse the server the run already started. Done when `npx playwright test --list` loads the config without error.
5. Write each flow from the criterion (what to test comes from skill `tests-from-criteria`). One test per user flow, named after the behaviour. Navigate with relative URLs (`page.goto('/account')`). Locate as the user perceives the page: `getByRole` with `name`, then `getByLabel`, `getByPlaceholder`, `getByText`, and `getByTestId` only when nothing visible names the element. Narrow with `.filter({ hasText })` or by chaining from a region (`page.getByRole('dialog').getByRole('button', { name: 'Save' })`). Assert the outcome the user sees (text, URL, state) with web-first assertions. Done when no step uses CSS or XPath selectors, `waitForTimeout`, or `{ force: true }`.
6. Sign in once. Follow the auth guide: a `setup` project matching `*.setup.ts` logs in through the UI, saves `storageState` to `playwright/.auth/<role>.json`, and the test projects declare `dependencies: ['setup']` and `use: { storageState }`. Check that `playwright/.auth` is in `.gitignore`. Done when only the setup test types a password.
7. Control what the project does not own. Mock third-party calls (payment, analytics, maps) with `page.route(…, route => route.fulfill(…))`; keep the app's own API real. Seed data through the app's API or the run's test database in `beforeEach`. Done when each test runs alone and in any order.
8. Add an accessibility scan where the criteria touch a page's markup. With `@axe-core/playwright` already in the project: `const results = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze(); expect(results.violations).toEqual([]);`. Exclude a known issue with `.exclude(selector)` and say so in the report. Keyboard and focus checks belong in the test as real key presses (skill `accessibility`). Done when the scan is part of the test and any exclusion is named in the report.
9. Prove and stabilise. See each new test fail without the change (skill `tests-from-criteria`), then run it several times (`--repeat-each=5`) to catch flakiness. For a failure, read the trace first (`playwright-trace` skill from the terminal, or `npx playwright show-trace <zip>`), then change the test or report the app bug. Done when it is green five times in a row and was seen red.
10. Run the commands of `## Verify` and put each one with its result in your report. Done when they pass, or when the failure is reported with its output.

## Gotchas

- 1.65 is tested on Node 22, 24 and 26 only; `engines` still says `>=20`, so an old CI image installs and then fails in odd ways. Check `node -v` and the CI image before blaming the test.
- 1.65 compiles test files with the project's `useDefineForClassFields`: a page object field initialised from a constructor parameter property (`button = this.page.getByRole(…)` next to `constructor(private page: Page)`) now throws. Assign locators inside the constructor body.
- 1.65: actions wait until the target is no longer inert, so a click behind an open modal `<dialog>` times out instead of clicking through. Close the dialog in the test, as the user would.
- 1.65: text the browser does not render is left out of accessible names and aria snapshots. A role locator or an aria snapshot that relied on hidden text fails after the upgrade: match the name a user actually hears, and report markup whose visible and accessible names differ.
- 1.65: a sharded run where no shard finds a test fails with "No tests found". Pass `--pass-with-no-tests` only when an empty selection is expected (a tag filter in CI).
- 1.64: elements inside a hidden iframe count as hidden for actions, `isVisible()` and `toBeVisible()`.
- 1.63: `@playwright/experimental-ct-react` / `-vue` are no longer updated. New component tests follow `playwright-component-testing/SKILL.md` (story gallery and the `mount` fixture).
- `npx playwright init-agents --loop=claude` writes `.claude/agents/*.md`, a seed test, and overwrites `.mcp.json` with only its own server. `npx playwright init-skills` copies the three skills into `.claude/skills/`. Run them only when the task asks for it; otherwise read the files from `node_modules`.
- `expect(await locator.isVisible()).toBe(true)` checks once and never waits. Use `await expect(locator).toBeVisible()` and the other web-first assertions; a missing `await` on them passes silently, which `@typescript-eslint/no-floating-promises` catches.

## Verify

- The project's e2e script from `package.json` first (`test:e2e`, `e2e`…); otherwise `PLAYWRIGHT_HTML_OPEN=never npx playwright test`, narrowed with a file path or `-g "<title>"`.
- `npx playwright test --trace on` locally to get a trace for every test while debugging; CI keeps the project's setting (`on-first-retry` is the documented default).
- `npx tsc --noEmit` when the project is in TypeScript; the project's lint script if it has the floating-promises rule.
- Browsers missing: `npx playwright install chromium --with-deps` (only the browsers the config uses).

## Security

- Storage state files hold live session cookies and tokens: keep them in the git-ignored `playwright/.auth/`, made from a test account of the local server, never a real user.
- Credentials come from the environment or the project's test env file (`process.env.E2E_PASSWORD`), never as literals in a test or in `storageState`.
- `baseURL` points at the local dev server of the run. A production or shared staging URL turns a test run into real orders, emails and data changes.
- Traces, videos, HAR files and `playwright-report/` contain cookies, form values and API responses. Keep `test-results/` and `playwright-report/` out of git and treat them as sensitive when attached to a PR or a CI artifact.
- Mocked third parties keep real payment, email and SMS providers out of the run; a test that needs one is a manual check to report.

## References

- In the installed package: `node_modules/playwright-core/lib/tools/skills/` (`playwright-cli`, `playwright-trace`, `playwright-component-testing`), `node_modules/playwright/lib/agents/` (Test Agents: planner, generator, healer, since 1.56).
- [Best practices](https://playwright.dev/docs/best-practices), [Locators](https://playwright.dev/docs/locators), [Authentication](https://playwright.dev/docs/auth), [Accessibility testing](https://playwright.dev/docs/accessibility-testing), [Release notes](https://playwright.dev/docs/release-notes).
