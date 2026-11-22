---
description: Registering a marker in a pytest 9 native TOML configuration.
max_turns: 6
allowed_tools: [Read, Glob, Grep, Skill]
---

Our pyproject.toml has:

```toml
[tool.pytest]
strict = true
testpaths = ["tests"]
markers = ["slow: tests that take more than a second"]
```

I want to tag some tests with `@pytest.mark.integration` and run them separately. What do I change in the config? Show the exact TOML.
