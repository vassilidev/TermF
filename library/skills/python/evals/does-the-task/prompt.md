---
description: A small feature in a uv project that reads user-supplied YAML.
max_turns: 10
allowed_tools: [Read, Glob, Grep, Skill]
---

Our CLI is a uv project (pyproject.toml with requires-python = ">=3.12", uv.lock committed). Add a `--config PATH` option to `acme/cli.py` that loads a YAML file given by the user and returns its `targets` list. PyYAML is not a dependency yet. Show the commands you run and the code.
