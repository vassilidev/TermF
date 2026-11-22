---
name: pest
description: >-
  Writes, fixes and runs PHP tests with Pest or PHPUnit the way the installed versions expect: tests
  derived from acceptance criteria, Laravel feature tests over HTTP with RefreshDatabase, factories and
  actingAs, datasets, expectations, architecture tests, browser tests, parallel runs, coverage and
  mutation testing. Use when a task adds or changes files under tests/, tests/Pest.php, phpunit.xml or
  test dependencies of a PHP project. Not for application code (php-laravel) and not for JavaScript tests.
paths: "**/tests/Pest.php, **/tests/**/*Test.php, **/phpunit.xml, **/phpunit.xml.dist"
license: MIT
metadata:
  termf-stacks: "pest phpunit"
  termf-roles: "qa back"
  termf-versions: ">=3"
  termf-verified: "2026-11-22 pest@5.3.7 phpunit@13.4.8 laravel@13.43.0 laravel-boost@2.10.4"
  termf-sources: "https://pestphp.com/llms.txt https://pestphp.com/docs/pest5-now-available/llms.txt https://pestphp.com/docs/cli-api-reference/llms.txt https://phpunit.de/supported-versions.html https://github.com/sebastianbergmann/phpunit/blob/main/ChangeLog-12.0.md https://github.com/sebastianbergmann/phpunit/blob/main/ChangeLog-13.0.md https://laravel.com/docs/13.x/testing https://laravel.com/docs/13.x/database-testing https://laravel.com/docs/13.x/dusk"
---
# Pest and PHPUnit

When Laravel Boost is installed, its testing guidelines and its `testing-best-practices` skill come first (they sit in the `<laravel-boost-guidelines>` block and `.claude/skills/`); Boost documents Pest up to 4.x, so the Pest 5 facts below still apply.

1. Read the versions in `composer.lock` (`pestphp/pest`, `phpunit/phpunit`, `pestphp/pest-plugin-laravel`, `pestphp/pest-plugin-browser`) and the PHP constraint in `composer.json`. Pest when `tests/Pest.php` exists, else PHPUnit. Read `phpunit.xml` for the test environment and `.env.testing` if present. Done when you can name the runner, its version and the test database.
2. Open the docs of that version: `https://pestphp.com/llms.txt` (each page has its own `/docs/<page>/llms.txt`), `https://laravel.com/docs/<major>.x/testing.md` and `http-tests.md`. Done when the page for the task is open.
3. Take each test from an acceptance criterion or a real flow, following the skill `tests-from-criteria`; write down what each one protects. Done when every planned test names its criterion.
4. Choose the highest level the project can run:
   - a user flow: a browser test (`visit('/')`, Pest 4+ with `pest-plugin-browser`) when the project has it, else Dusk when it has Dusk;
   - an endpoint: a feature test over HTTP (`actingAs($user)->postJson(...)`, then assert the status, the JSON and the rows with `assertDatabaseHas`), on the real test database through `RefreshDatabase`, records from factories;
   - real rules (pricing, parsing): unit tests, with `#[UnitTest]` (Laravel 13) or in `tests/Unit` so the app does not boot.
   Mock or fake only what leaves the app: `Http::fake()`, `Mail::fake()`, `Queue::fake()`, `Notification::fake()`. Done when no test mocks Eloquent or the class it checks.
5. Create files with `php artisan make:test <Name> --pest` (`--unit` for a unit test) in a Laravel app. Run the new test narrowly, then break or revert the change it protects and see it fail for the right reason, then restore. Done when it fails without the change and passes with it.
6. Run `## Verify` and put each command with its result in your report.

## Gotchas

- Pest 5 (current) needs PHP 8.4 and PHPUnit 13; Pest 4 runs on PHP 8.3 with PHPUnit 12, and `pest-plugin-laravel` 5 needs Laravel 13.23 or later. A Laravel 13 app on PHP 8.3 stays on Pest 4: keep the major the project has.
- PHPUnit 12 removed doc-comment annotations: `@test`, `@dataProvider`, `@depends` become attributes (`#[Test]`, `#[DataProvider('cases')]`). It also removed `getMockForAbstractClass()`, `getMockForTrait()`, `addMethods()`, and expectations on stubs (`createStub()` has no `expects()`; use `createMock()` when the call itself is the behavior).
- PHPUnit 13 (PHP 8.4+) removed `assertContainsOnly()`, `Assert::isType()`, `#[RunClassInSeparateProcess]` and `#[CoversNothing]` on methods, and hard-deprecated the `any()` matcher.
- Pest 5 adds `--tia` (runs only the tests impacted by the change; needs PCOV or Xdebug; local only, CI runs the full suite), `--shard=1/5`, and `--evals`, which calls a real model and costs money: never run it unless the task says so.
- Pest 4 added browser testing (`composer require pestphp/pest-plugin-browser --dev`, then Playwright from npm) and renamed snapshots (`--update-snapshots`); Laravel's docs now recommend it over Dusk for new projects. Pest 3 added `--mutate` and architecture presets (`arch()->preset()->laravel()`, `->security()`).
- `RefreshDatabase` migrates once when the schema changed, then wraps each test in a transaction: code that commits through another connection, or relies on `afterCommit`, needs `DatabaseTruncation` instead.
- With `laravel/pao` installed, Pest and PHPUnit print compact JSON when run by an agent: that JSON is the result.

## Verify

- The project's Composer scripts and CI steps first.
- One file or test while iterating: `php artisan test --compact tests/Feature/ProjectRenameTest.php` or `--filter='renames the project'` (`vendor/bin/pest` takes the same arguments).
- Before reporting: the whole suite (`php artisan test`, `--parallel` when the project uses it); `--coverage --min=<n>` only when the project enforces a minimum.

## Security

- Tests run against the testing environment only: check `phpunit.xml` sets `APP_ENV=testing` and a test database, so `RefreshDatabase` never wipes a development or shared database.
- Outside HTTP calls are faked (`Http::fake()` with `Http::preventStrayRequests()`), so a test can neither hit a real API nor leak a token.
- `--agent='<php snippet>'` (Pest 5) runs code inside the real suite configuration: wrap it in single quotes and run only snippets you wrote for the task.

## References

- `https://pestphp.com/docs/browser-testing/llms.txt` before writing a browser test; `https://pestphp.com/docs/arch-testing/llms.txt` before an architecture test.
