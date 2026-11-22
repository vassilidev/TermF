---
name: fastapi
description: >-
  Builds, changes and tests FastAPI applications the way the installed FastAPI version expects: path
  operations, Annotated dependencies, Pydantic v2 models, routers, streaming and Server-Sent Events,
  request limits, security, TestClient and async tests. Use when a task touches a FastAPI app (imports
  from fastapi, APIRouter, [tool.fastapi] in pyproject.toml). Not for Django or Flask, and it defers to
  the official FastAPI skill shipped inside the installed package.
paths: "**/*.py, **/pyproject.toml"
license: MIT
metadata:
  termf-stacks: "fastapi"
  termf-roles: "back qa"
  termf-versions: ">=0.128"
  termf-verified: "2026-11-22 fastapi@0.165.0"
  termf-sources: "https://fastapi.tiangolo.com/release-notes/ https://fastapi.tiangolo.com/ai-development/ https://github.com/fastapi/fastapi/tree/master/fastapi/.agents/skills/fastapi https://fastapi.tiangolo.com/tutorial/testing/ https://fastapi.tiangolo.com/advanced/async-tests/ https://fastapi.tiangolo.com/fastapi-cli/"
---
# FastAPI

FastAPI ships its official skill inside the package since 0.133.1 (MIT): `<site-packages>/fastapi/.agents/skills/fastapi/SKILL.md`, with references on dependencies, path operations, Pydantic, responses, streaming and more. It matches the installed version exactly, so it comes first; this skill adds the version gotchas, the tests and the checks.

1. Read the FastAPI version in the lockfile (or `uv pip show fastapi`) and the app entrypoint in `[tool.fastapi] entrypoint` of `pyproject.toml`. Done when you can name both.
2. Read the official skill of that version: if it is not already loaded, open the file printed by `uv run python -c "import fastapi, pathlib; print(pathlib.Path(fastapi.__file__).parent / '.agents/skills/fastapi/SKILL.md')"` (or with the project's own runner), then the reference it points to for the task. Below 0.133.1 the file does not exist: use `https://fastapi.tiangolo.com/release-notes/` for that version. Done when the skill or the notes for the task are open.
3. Write the change as the official skill says (the `Annotated` style, one dependency alias such as `CurrentUserDep = Annotated[User, Depends(get_current_user)]` reused everywhere) and as the surrounding routers do. Done when `fastapi dev` starts and the endpoint answers.
4. Write the tests of `## Testing`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- Pydantic v2 only: `pydantic.v1` support was dropped in 0.128.0. Python 3.11 or later since 0.162.6; `fastapi-slim` is gone (`fastapi` or `"fastapi[standard]"`).
- With a return type or `response_model`, Pydantic serializes the JSON in Rust (0.130.0); `ORJSONResponse` and `UJSONResponse` are deprecated (0.131.0). Declare the return type instead of choosing a faster response class.
- Server-Sent Events are built in (0.135.0): `from fastapi.sse import EventSourceResponse, ServerSentEvent`, `response_class=EventSourceResponse` and `yield`; JSON Lines and binary streaming also work with `yield` (0.134.0). No extra SSE package.
- 0.137.0 reworked `APIRouter`: `router.routes` is a tree, not a flat list of `APIRoute`; code that walks it needs a rewrite.
- `Depends(f, max_concurrency=n)` (0.163.0) limits per worker process, and each `Depends()` object is its own limit: declare it once in an `Annotated` alias.
- Request bodies are unlimited unless `max_body_size` is set on the app, router or route (0.164.0); multipart limits are `form_limits` (0.165.0).
- OpenTelemetry is native (0.142.0), but exporters configure themselves only when opted in (0.143.0: `FASTAPI_OTEL_AUTO_CONFIGURE=true` or `FastAPI(telemetry={"auto_configure": True})`).

## Testing

Follow the skills `tests-from-criteria` and `pytest`.

- An endpoint is tested through `TestClient(app)` (needs `httpx`), with plain `def` tests; async tests use `httpx.AsyncClient(transport=ASGITransport(app=app), base_url="http://test")` with `@pytest.mark.anyio`, and that client does not run lifespan events.
- Replace with `app.dependency_overrides` only what leaves the project (a payment API, an email sender); keep the real database session on a test database, because the queries are what breaks.

## Verify

- The project's commands first, else `uv run pytest`, `uv run ruff check`, `uv run ruff format --check`, and `fastapi dev` to try the endpoint by hand.

## Security

- CORS: `allow_credentials=True` cannot go with `["*"]` for origins, methods or headers; list the origins.
- Compare secrets with `secrets.compare_digest` on bytes.
- Tokens with PyJWT and passwords with `pwdlib[argon2]` (`PasswordHash.recommended()`), as the current docs do; `python-jose` and `passlib` are no longer what they show.
- Set `max_body_size` on endpoints that accept uploads or large bodies.
- `fastapi run` listens on `0.0.0.0`; `fastapi dev` on `127.0.0.1`. Keep `dev` for local work.

## References

- The official skill and its references in the installed package (step 2). For FastAPI Cloud deployment, the official `fastapicloud/skills` plugin.
