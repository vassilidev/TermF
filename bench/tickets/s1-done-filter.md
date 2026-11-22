# Filtering by `done=false` shows the completed tasks

**Type:** bug

On the page, choosing "Show: Open" lists the tasks that are done, and hides the open ones. The API does the same:

1. Create two tasks, mark one of them done.
2. `GET /api/tasks?done=false`
3. Expected: only the open task. Actual: only the done one.

`?done=true` looks right.

## Acceptance criteria

1. `GET /api/tasks?done=false` returns only the tasks that are not done.
2. `GET /api/tasks?done=true` returns only the done tasks; without `done`, every task is listed as before.
3. Any other value of `done` (for example `?done=yes` or `?done=`) answers `400` with `{"error": "done must be true or false"}`.
4. A test reproduces the bug and passes with the fix.
