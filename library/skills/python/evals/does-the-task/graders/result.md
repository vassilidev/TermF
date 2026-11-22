---
type: llm
---

PASS if the dependency is added with `uv add pyyaml` (or `uv add PyYAML`), the lockfile is left to uv, and the file is parsed with `yaml.safe_load` (or a SafeLoader).
FAIL if it uses `pip install`, edits uv.lock or the dependency list by hand without uv, or parses the file with `yaml.load` and an unsafe or full loader.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
