---
name: django
description: >-
  Builds, changes and tests Django applications the way the installed Django version expects: models and
  migrations, querysets and N+1 queries, views, forms and templates, settings, background tasks, content
  security policy, the admin, and tests with Django's test client. Use when a task touches a Django project
  (manage.py, settings.py, models.py, migrations/, templates/). Not for FastAPI or Flask, and not for
  frontend code served by a separate JavaScript app.
paths: "**/manage.py, **/settings*.py, **/settings/**/*.py, **/models.py, **/models/**/*.py, **/migrations/*.py, **/views.py, **/forms.py, **/urls.py, **/admin.py, **/templates/**/*.html"
license: MIT
metadata:
  termf-stacks: "django"
  termf-roles: "back qa secops"
  termf-versions: ">=5.2"
  termf-verified: "2026-11-22 django@6.1.3"
  termf-sources: "https://docs.djangoproject.com/en/6.1/releases/6.1/ https://docs.djangoproject.com/en/6.0/releases/6.0/ https://www.djangoproject.com/download/ https://docs.djangoproject.com/en/6.1/ref/django-admin/ https://docs.djangoproject.com/en/6.1/topics/testing/overview/ https://docs.djangoproject.com/en/6.1/topics/security/ https://docs.djangoproject.com/en/6.1/howto/deployment/checklist/"
---
# Django

Django ships no agent skill, `llms.txt` or MCP server; its versioned docs are the reference.

1. Read the version (`python -m django --version`, or `Django` in the lockfile), the settings module in use (`DJANGO_SETTINGS_MODULE`, `manage.py`), the database engine and its version, and the test runner (`manage.py test`, or pytest with pytest-django). Done when you can name all four.
2. Open the docs of that version: `https://docs.djangoproject.com/en/<major.minor>/` and its release notes. Done when the page for the task is open.
3. Follow the project's structure: its apps, settings split, Django REST Framework or Ninja when present, its task backend, its template or API style. Done when the new code sits next to code of the same kind.
4. Model changes: edit the model, run `python manage.py makemigrations <app>`, read the generated file, and keep schema and data changes in separate migrations. Before changing a table that already holds data in production, read `references/migrations.md`. Done when `python manage.py makemigrations --check` reports nothing and the migration reverses.
5. Views, forms and querysets: validate input in forms or serializers, check permissions in the view, and load related rows in the same query. Before writing a list, an export or an admin page, read `references/queries.md`. Done when the query count of the page does not grow with the number of rows.
6. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Django 6.0 made `BigAutoField` the default of `DEFAULT_AUTO_FIELD`, and `startproject` no longer writes the setting. A project upgraded without the setting sees migrations that alter every primary key: set `DEFAULT_AUTO_FIELD = "django.db.models.AutoField"` to keep its keys, unless the task is that migration.
- `django.tasks` (6.0): `@task` and `.enqueue()` with the `TASKS` setting, but Django ships no worker; its built-in backends (immediate, dummy) are for development and tests. Enqueued work runs in production only with a third-party backend and its worker: check which one the project has before relying on it.
- 6.0 brings content security policy (`SECURE_CSP`, `SECURE_CSP_REPORT_ONLY`, `django.middleware.csp.ContentSecurityPolicyMiddleware`, nonce through the `csp()` context processor; 6.1 adds `{% csp_nonce_attr %}`) and template partials (`{% partialdef %}`, `{% partial %}`, `"page.html#name"` in `render()` and `{% include %}`). Use them instead of adding a package, unless the project already uses one.
- 6.1 fetch modes: `QuerySet.fetch_mode(models.FETCH_PEERS)` prefetches a related field for all rows at the first access; `FETCH_RAISE` raises `FieldFetchBlocked` on any lazy load (useful in tests).
- 6.1 database-level `on_delete` (`DB_CASCADE`, `DB_SET_NULL`, `DB_SET_DEFAULT`) skips `pre_delete`/`post_delete` signals: keep `CASCADE` where signals or Python-side cleanup matter.
- 6.1: `first()` and `last()` no longer add a primary-key ordering after `order_by()`; the `MAILERS` setting replaces the `EMAIL_*` settings (deprecated, opt-in until Django 2028).
- 6.1 requires PostgreSQL 15+, MySQL 8.4+, MariaDB 10.11+, SQLite 3.37+; Python 3.12 to 3.15. The next release is 6.2 LTS (April 2027), then "Django 2028" (calendar versioning): deprecation warnings are named `RemovedInDjango2028Warning`.
- `makemigrations --check` already implies `--dry-run`.

## Testing

Follow the skill `tests-from-criteria` (and `pytest` when the project runs pytest-django).

- A user flow is tested through the test client (`self.client` in `TestCase`, `client` fixture with pytest-django): request the URL, follow the form post, assert the response and the rows it left.
- Run against the database engine of production; SQLite behaves differently on constraints, JSON fields and locking. `TestCase` rolls back each test; use `captureOnCommitCallbacks` (or `TransactionTestCase`) when the code uses `transaction.on_commit`.
- Guard N+1 fixes with `assertNumQueries` (`django_assert_num_queries` in pytest-django).
- Tasks: set the immediate backend in test settings so enqueued work runs inside the test.
- One test while iterating: `python manage.py test orders.tests.RefundTests.test_refund_over_limit --keepdb`; `--parallel auto` for the whole suite.

## Verify

- The project's commands first (`Makefile`, CI steps, `pytest`).
- `python manage.py check`, `python manage.py makemigrations --check`, the test command; `python manage.py check --deploy --settings=<production settings>` when the task touches settings.

## Security

- Raw SQL (`raw()`, `cursor.execute`, `RawSQL`, `extra()`) takes `params`; never format values into the string, never quote a placeholder.
- `mark_safe`, `|safe`, `{% autoescape off %}` and `format_html` with pre-built strings skip escaping: use them only on content the code built itself, and sanitize stored HTML.
- `@csrf_exempt` only on endpoints authenticated by something other than the session cookie (signed webhooks).
- `ALLOWED_HOSTS` explicit; read the host with `request.get_host()`, never from `request.META["HTTP_HOST"]`.
- `DEBUG` off and `SECRET_KEY` from the environment outside development; rotate with `SECRET_KEY_FALLBACKS`.

## References

- Read `references/migrations.md` before changing a table that holds production data, adding an index or a constraint to a large table, or writing a data migration.
- Read `references/queries.md` before writing a list, a report, an export, an admin page or a bulk update.
