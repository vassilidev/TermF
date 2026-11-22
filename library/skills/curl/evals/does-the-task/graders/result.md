---
type: llm
---

PASS if the commands wait for `/health` on `$PORT` with retries that cover a refused connection (`--retry-connrefused`, or a loop with a limit), build the JSON body with a tool that escapes values (`jq -n --arg`, or an equivalent) instead of putting `$NAME` inside a hand-quoted JSON string, and make the POST fail on an error status (`--fail` or `--fail-with-body`), with a time limit on the calls.
FAIL if the JSON is written as a quoted string with `$NAME` and `$EMAIL` inside, if the POST exits 0 on a 4xx or 5xx answer, or if the wait is a fixed `sleep`.
