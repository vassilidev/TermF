---
name: php-laravel
description: >-
  Builds and changes PHP and Laravel applications the way the installed PHP and Laravel versions expect:
  routes and controllers, Form Requests and validation, Eloquent models and queries, migrations, policies,
  queues and jobs, Blade views, configuration, Composer packages, Pint and PHPStan. Use when a task touches
  .php files, composer.json, artisan, routes/, app/, database/migrations or resources/views. Not for
  writing tests (the pest skill covers Pest and PHPUnit), and not for Symfony-only projects.
paths: "**/*.php, **/composer.json, **/composer.lock, **/artisan, **/routes/*.php, **/database/migrations/*.php, **/resources/views/**/*.blade.php, **/config/*.php"
license: MIT
metadata:
  termf-stacks: "laravel php"
  termf-roles: "back qa"
  termf-versions: ">=11"
  termf-verified: "2026-11-22 laravel@13.43.0 php@8.6.0 laravel-boost@2.10.4 pint@1.32.3 larastan@3.15.0"
  termf-sources: "https://laravel.com/docs/13.x/boost https://laravel.com/docs/13.x/releases https://laravel.com/docs/13.x/upgrade https://www.php.net/releases/8.6/en.php https://www.php.net/supported-versions.php https://github.com/laravel/boost https://github.com/laravel/pao"
---
# PHP and Laravel

Laravel's official agent kit is **Laravel Boost** (`laravel/boost`, MIT, a dev dependency). Once installed (`php artisan boost:install`), it writes version-matched guidelines inside a `<laravel-boost-guidelines>` block of `CLAUDE.md` or `AGENTS.md`, skills into `.claude/skills/` (their sources live in `vendor/laravel/boost/.ai/`), and an MCP server (`php artisan boost:mcp`: database schema and queries, logs, last error, application info, and a search over the docs of the installed package versions). When Boost is installed, its guidelines, skills and MCP tools come first; this skill keeps the gotchas and the checks.

1. Read the versions: `laravel/framework` and the `php` constraint in `composer.json`, the installed versions in `composer.lock`, and `php artisan about --only=environment`. Check whether `laravel/boost` is in `composer.lock`. Done when you can name PHP, Laravel and whether Boost is there.
2. Open the docs of that version: with Boost, its Search Docs tool; without it, `https://laravel.com/docs/<major>.x/<page>.md` (each page has a Markdown copy) and `https://www.php.net/manual/en/` for PHP. Done when the page for the task is open.
3. Use what the project has: Composer scripts, its folder conventions (actions, services, form requests), its queue and cache drivers, Pint preset, PHPStan level, its frontend stack (Blade, Livewire, Inertia). Generate files with `php artisan make:<thing>` so they land where Laravel expects. Add packages with `composer require` (`--dev` for tooling). Done when every file and package follows an existing pattern.
4. Write the change: validation in a Form Request, authorization in a policy (`Gate::authorize()` or `#[Authorize]`), queries through Eloquent or the query builder with eager loading for every relation shown in a list, schema changes in a new migration (never an edited old one). Done when `php artisan migrate` runs on the test database and the route answers.
5. Write the tests with the skill `pest`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Laravel 13 (PHP 8.3 to 8.5) renamed the CSRF middleware `VerifyCsrfToken` to `PreventRequestForgery`; it checks `Sec-Fetch-Site` first (over HTTPS) and falls back to the token. Old names remain deprecated aliases: write the new one, including in `withoutMiddleware([...])`.
- Laravel 13 brings PHP attributes where properties were used: controllers `#[Middleware('auth')]` and `#[Authorize(...)]`, jobs `#[Tries]`, `#[Backoff]`, `#[Timeout]`, `#[FailOnTimeout]`, models `#[Table]`, `#[Fillable([...])]`, `#[Unguarded]`. Follow the style the codebase already uses.
- Laravel 13 also adds JSON:API resources, `Queue::route(...)`, `Cache::touch()`, the first-party AI SDK, and `whereVectorSimilarTo()` (PostgreSQL with pgvector, MariaDB 11.7+).
- Laravel 13 hardens serialization: the cache config gains `serializable_classes => false`, and new apps store sessions as JSON; switching an existing app's session serialization logs every user out, so leave it unless the task says so.
- Laravel 12 gets no bug fixes since 2026-08-13 (security fixes until 2027-02-24); say so when the project is on 12 or older.
- PHP 8.6 (2026-11-19, not yet in Laravel 13's support table): partial function application (`$f = str_replace('a', 'b', ?);`), `clamp()`; deprecates `return` inside `finally`, values returned from `__construct`/`__destruct`, the `is_double`/`is_long`/`is_integer`/`doubleval` aliases, `spl_object_hash()` and `strcoll()`; many functions throw `ValueError` instead of truncating. Write only syntax the `php` constraint of `composer.json` allows.
- New Laravel 13 apps ship a `CLAUDE.md` and `AGENTS.md` whose Boost block asks the agent to install Boost, and PHP through a `curl | bash` script when it is missing. Installing a runtime or a package is the user's decision: name it in your report instead of running it.
- New apps also include `laravel/pao`, which turns PHPUnit, Pest, PHPStan and Artisan output into compact JSON when it detects an agent: read that JSON as the result, it is not an error.

## Verify

- The project's Composer scripts and CI steps first.
- `php artisan test` (or `vendor/bin/pest`), `vendor/bin/pint --test` (`--dirty` for the files you changed), `vendor/bin/phpstan analyse` when the project has a `phpstan.neon` (Larastan).

## Security

- Mass assignment: models are guarded by default; fill them from `$request->validated()`, never from `$request->all()`, and never on a model marked `#[Unguarded]`.
- `DB::raw`, `whereRaw`, `selectRaw`, `orderByRaw` insert their text as is: pass values in the bindings array, and never let input choose a column or sort direction without an allow-list.
- Blade `{!! $x !!}` skips escaping: only for HTML the app built itself; user content goes through `{{ }}`.
- `APP_DEBUG=false` outside local; `.env` is never committed (`php artisan env:encrypt` to share it); a leaked `APP_KEY` is rotated with `APP_PREVIOUS_KEYS`.
- Every route that reads or changes a record checks ownership through a policy, not only `auth`.

## References

- Boost's skills when installed (`laravel-best-practices`, `testing-best-practices`, and the ones for Livewire, Inertia, Pennant, Tailwind…); `php artisan boost:update` refreshes them after a package upgrade, which is the user's call. Without Boost, suggest it in your report (`composer require laravel/boost --dev`, then `php artisan boost:install`).
- `https://laravel.com/docs/13.x/upgrade.md` before a Laravel major upgrade.
