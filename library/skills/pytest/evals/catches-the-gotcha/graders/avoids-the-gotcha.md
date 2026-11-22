---
type: llm
---

PASS if the marker is added to the `markers` list of the existing `[tool.pytest]` table, as a TOML string in that array.
FAIL if the answer adds a `[tool.pytest.ini_options]` table next to `[tool.pytest]` (pytest refuses both tables at once).
