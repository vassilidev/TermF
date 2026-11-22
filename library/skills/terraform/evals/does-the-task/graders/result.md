---
type: llm
---

PASS if `environment` has a `validation` block that only accepts dev, staging and prod, the password variable is marked `ephemeral` (or at least `sensitive`) with no default, and the resource receives it through the write-only argument `password_wo` together with `password_wo_version`, with an explanation that write-only values are not stored in state.
FAIL if the password has a default value, is passed through the regular `password` argument while claiming it stays out of the state, or if `environment` is not validated.
