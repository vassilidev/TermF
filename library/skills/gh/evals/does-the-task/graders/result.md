---
type: llm
---

PASS if the answer lists the checks with `gh pr checks 128` (with `-R acme/web` or from the clone), finds the failing run's id (from the check's link, `gh run list` or `--json`), and reads its failed steps with `gh run view <run-id> --log-failed`, without rerunning or cancelling anything.
FAIL if it reads the whole log without `--log-failed`, starts with `gh run rerun`, or sends the user to the web page only.
