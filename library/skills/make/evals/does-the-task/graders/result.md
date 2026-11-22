---
type: llm
---

PASS if the new target is added to `.PHONY`, its recipe lines are indented with a tab, every dollar meant for the shell is written `$$` (for example `$$(git describe --tags --abbrev=0)`), and a shell variable is set and used on the same recipe line (joined with `;` or `&&`, or continued with `\`), or the tag comes from a make variable instead.
FAIL if a shell variable is set on one recipe line and used on the next, if a shell substitution is written with a single `$`, or if the existing targets are changed.
