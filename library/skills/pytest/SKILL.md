---
name: pytest
description: >-
  Writes, fixes and runs Python tests with pytest the way the installed pytest and plugins expect: tests
  derived from acceptance criteria, fixtures and conftest.py, parametrize, tmp_path and monkeypatch,
  markers, async tests, Django and FastAPI test clients, pytest configuration, narrow, last-failed and
  parallel runs. Use when a task adds or changes Python tests, conftest.py or the [tool.pytest] section.
  Not for JavaScript tests, and not for suites run only by unittest or Django's own runner without pytest.
paths: "**/test_*.py, **/*_test.py, **/conftest.py, **/tests/**/*.py, **/pytest.toml, **/pytest.ini"
license: MIT
metadata:
  termf-stacks: "pytest"
  termf-roles: "qa back"
  termf-versions: ">=8"
  termf-verified: "2026-11-22 pytest@9.2.0 pytest-asyncio@1.5.0 pytest-django@4.14.0 pytest-xdist@3.8.0"
  termf-sources: "https://docs.pytest.org/en/stable/changelog.html https://docs.pytest.org/en/stable/deprecations.html https://docs.pytest.org/en/stable/reference/customize.html https://docs.pytest.org/en/stable/how-to/subtests.html https://pytest-asyncio.readthedocs.io/en/stable/concepts.html https://pytest-django.readthedocs.io/en/latest/database.html"
---
# pytest

1. Read the pytest version in the lockfile (`uv.lock`, `poetry.lock`, requirements) and find its configuration: the first of `pytest.toml`, `.pytest.toml`, `pytest.ini`, `.pytest.ini`, `pyproject.toml`, `tox.ini`, `setup.cfg` that has a pytest section wins, and files are never merged. Note the plugins installed (pytest-asyncio and its mode, pytest-django, pytest-xdist, an HTTP mocking library). Done when you can name the version, the config file and the plugins.
2. Take each test from an acceptance criterion or a real user flow, following the skill `tests-from-criteria`; write down for each one what it protects. Done when every planned test names its criterion.
3. Choose the highest level the project can run: the framework's test client for an endpoint (Django `client`, FastAPI `TestClient`), the real test database for queries (pytest-django `django_db`, or the project's database fixture), a plain function test only for real logic (rules, calculations, parsing). Mock only what leaves the project (third-party HTTP, clock, email), with the mocking library it already uses. Done when no test mocks the code it is meant to check.
4. Reuse the fixtures of the nearest `conftest.py` before writing new ones; a new shared fixture goes in the `conftest.py` of the folder that needs it. Files and environment through `tmp_path` and `monkeypatch`, so a test leaves nothing behind. Done when the test runs alone and in any order.
5. Run the new test narrowly, then break or revert the change it protects and see it fail for the right reason, then restore. Done when it fails without the change and passes with it.
6. Run `## Verify` and put each command with its result in your report.

## Gotchas

- pytest 9 reads native TOML: `[tool.pytest]` in `pyproject.toml`, or `[pytest]` in `pytest.toml`, with real TOML lists (`addopts = ["-ra", "-q"]`). The legacy `[tool.pytest.ini_options]` still works, but both tables at once is an error: add settings to the table the project already has.
- `strict = true` (pytest 9) turns on strict markers, strict config, strict parametrize ids and strict xfail: a new marker must be registered in `markers` before use, or the run fails.
- pytest 9.2: `indirect`, `ids` and `scope` of `@pytest.mark.parametrize` are keyword-only (a positional value raises `TypeError`). pytest 9.1 deprecates generators as parametrize values (pass a list) and class-scoped fixtures written as instance methods (`@classmethod`). `yield_fixture` is scheduled for removal: `@pytest.fixture` with `yield`.
- pytest 9 errors on a sync test that requests an async fixture and on marks applied to fixtures. The `subtests` fixture is built in (`def test_x(subtests): with subtests.test(i=i): ...`); the pytest-subtests plugin is no longer needed.
- pytest-asyncio 1.x removed the `event_loop` fixture. In its default strict mode, an async test runs only with `@pytest.mark.asyncio` (or `asyncio_mode = "auto"` in the config); a shared loop is `@pytest.mark.asyncio(loop_scope="module")`. Without the mark the plugin leaves the test alone and pytest fails it ("async def functions are not natively supported").
- pytest-django blocks database access unless the test has `@pytest.mark.django_db` (`transaction=True` when the code commits or uses `on_commit`).

## Verify

- The project's own test command first (`Makefile`, `noxfile.py`, `tox.ini`, CI steps), through its manager (`uv run pytest`, `poetry run pytest`).
- While iterating: `pytest tests/test_orders.py::test_refund_over_limit -x`, then `pytest --lf` for what failed last.
- Before reporting: the whole suite; `pytest -n auto` only when pytest-xdist is installed.

## Security

- Tests never reach a real service or use a real credential: fake secrets through `monkeypatch.setenv`, outside HTTP through the project's mocking library or a local server.
- Files go under `tmp_path`, never into the repository or the home folder.
- A fixture file loaded with `pickle` runs code: keep fixtures in JSON or YAML (`yaml.safe_load`).

## References

- `https://docs.pytest.org/en/stable/how-to/usage.html` for selecting tests (node ids, `-k`, `-m`), `https://docs.pytest.org/en/stable/deprecations.html` before removing a warning filter.
