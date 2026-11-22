---
name: python
description: >-
  Writes, changes and checks Python code the way the project's Python version and tools expect: modules
  and packages, pyproject.toml and dependencies with uv, poetry or pip, typing, ruff lint and format, type
  checking, scripts and command-line tools. Use when a task touches .py files, pyproject.toml, uv.lock,
  poetry.lock, requirements files or .python-version. Not for writing tests (the pytest skill) and not for
  Django or FastAPI specifics, which their own skills cover.
paths: "**/*.py, **/pyproject.toml, **/uv.lock, **/poetry.lock, **/requirements*.txt, **/.python-version"
license: MIT
metadata:
  termf-stacks: "python"
  termf-roles: "back qa devops"
  termf-versions: ">=3.10"
  termf-verified: "2026-11-22 python@3.15.2 uv@0.13.9 ruff@0.17.6 ty@0.0.107 mypy@2.4.1"
  termf-sources: "https://docs.python.org/3.15/whatsnew/3.15.html https://docs.python.org/3.14/whatsnew/3.14.html https://docs.astral.sh/uv/llms.txt https://docs.astral.sh/ruff/llms.txt https://docs.astral.sh/ty/llms.txt https://github.com/astral-sh/ruff/blob/main/changelogs/0.16.x.md https://github.com/astral-sh/agent-plugins"
---
# Python

Astral publishes official skills for uv, ruff and ty in [astral-sh/agent-plugins](https://github.com/astral-sh/agent-plugins) (MIT or Apache-2.0, plugin `astral`); Python itself ships none. When the `astral` plugin is loaded, follow its skills for those tools; this skill keeps the version floor, the gotchas and the checks.

1. Read the Python floor in `requires-python` (`[project]` of `pyproject.toml`) and the interpreter pinned in `.python-version` (uv reads it first). The floor decides which syntax you may write, not the interpreter on this machine. Done when you can name both.
2. Read the tools from the files present: `uv.lock` means uv, `poetry.lock` means Poetry, `requirements*.txt` (with `.in` files for pip-tools) means pip; linter, formatter and type checker from `[tool.ruff]`, `[tool.mypy]`, `[tool.pyright]`, `[tool.ty]` or their own files. Done when every command you plan to run comes from that list.
3. Open the docs of that version: `https://docs.python.org/3.<minor>/` (no `llms.txt` for Python), and `https://docs.astral.sh/uv/llms.txt`, `.../ruff/llms.txt`, `.../ty/llms.txt` for the tools. Done when the page for the task is open.
4. Change dependencies only through the project's manager, because a hand edit leaves the lockfile out of date: `uv add <pkg>`, `uv add --dev <pkg>` (it writes `[dependency-groups]`), `uv remove`; `poetry add`; for pip, the requirements file and the project's pin tool. Never edit `uv.lock` or `poetry.lock` by hand, and never `pip install` into a uv or Poetry environment. Done when the manifest and the lockfile changed together.
5. Write the code for the floor of step 1, with type hints in the style the project uses, and run it through the manager (`uv run <cmd>`, `poetry run <cmd>`). Done when the type checker the project configures reports nothing new.
6. Write tests with the skill `pytest`, then run `## Verify` and put each command with its result in your report. Done when they pass, or the failure is reported with its output.

## Gotchas

- New syntax is gated by the floor: 3.12 generics (`class Box[T]:`, `type Alias = ...`), 3.14 t-strings and `except A, B:` without brackets, 3.15 `lazy import x` (module level only; inside a function, class or `try` it is a `SyntaxError`). In a project with `requires-python = ">=3.12"`, none of the 3.14 or 3.15 forms may appear.
- Python 3.15 opens files in UTF-8 when no encoding is given (PEP 686). Code that must also run on older versions passes `encoding="utf-8"` explicitly, since older versions use the locale encoding.
- Python 3.15 adds `frozendict` and `sentinel()` as builtins, the `profiling` package (`profile` is deprecated, `cProfile` stays), and removes the keyword form of `typing.NamedTuple`, `TypedDict` calls without fields, the `sre_*` modules, `http.server.CGIHTTPRequestHandler` and `importlib` `load_module()`. The asyncio policy system (`set_event_loop_policy`) is deprecated and leaves in 3.16: choose the loop with `asyncio.run(main(), loop_factory=...)`.
- `uv run` locks and syncs before it runs. In checks, use `uv sync --locked` (or `uv run --locked`) so a stale lockfile fails instead of being rewritten silently. `uv init` makes a packaged project with the `uv_build` backend.
- Ruff 0.16 turned on 413 rules by default (59 before) and formats Python code blocks in Markdown; ruff 0.17 assumes Python 3.11 when `requires-python` and `target-version` are both absent. On a project without an explicit `select`, an upgrade brings many findings: fix those in the lines you touch, and leave a repository-wide cleanup to its own task.
- The ruff formatter replaces Black but does not sort imports: sorting is lint rule `I001`, fixed by `ruff check --fix`.
- mypy 2.0 turns on `--local-partial-types` and `--strict-bytes` by default. ty is in beta (0.0.x): run it only when the project configures it.

## Verify

- The project's own commands first: `Makefile`, `justfile`, `noxfile.py`, `tox.ini`, CI workflow steps.
- With uv: `uv sync --locked`, `uv run ruff check`, `uv run ruff format --check`, the configured type checker (`uv run mypy .`, `uv run pyright`, or `uvx ty check`), `uv run pytest`. With Poetry or pip, the same tools through `poetry run` or the active virtual environment.

## Security

- `subprocess` with a list of arguments; `shell=True` with any outside input is shell injection.
- `pickle`, `marshal` and `shelve` run code while loading: never load them from untrusted data; use JSON.
- YAML from outside: `yaml.safe_load`, never `yaml.load` with the full loader.
- Archives: before 3.14, pass `filter="data"` to `tarfile` extraction (3.14 made it the default); check zip member paths before extracting.
- Tokens and passwords come from `secrets`, never `random`.
- Ruff's default rule set has almost none of the `S` (bandit) security rules: name them in your report when a project handles untrusted input and does not select them.

## References

- When the `astral` plugin is not loaded and the task is about uv workspaces, Python version management or ruff configuration, read the matching page from `https://docs.astral.sh/uv/llms.txt` or `https://docs.astral.sh/ruff/llms.txt`, and suggest the plugin in your report (`/plugin marketplace add astral-sh/agent-plugins`).
