---
type: llm
---

PASS if the tests go through the real router or handler with net/http/httptest, cover the three criteria (including the exact boundary amount == paid giving 201) as table cases run with t.Run, and say to run them with -race.
FAIL if they call internal helpers instead of the HTTP handler, skip the boundary, or assert only that no error occurred.
The run starts in an empty folder: judge the code and commands given in the reply; not finding or editing project files is not a failure.
