# Replace the done flag with a three-state status

**Type:** refactor with a schema migration

"Done or not" is not enough: people want to see what is in progress. A task gets a status `todo`, `doing` or `done`, which replaces the `done` flag in the data. Clients that still send or read `done` must keep working for one release.

## Acceptance criteria

1. Every task in the API has `status`: `"todo"`, `"doing"` or `"done"`, `"todo"` by default.
2. In the database, a `status` column of `tasks` replaces `done`: a new migration adds it, fills it, and drops the `done` column.
3. Existing databases are migrated on startup without losing anything: `done = 1` becomes `done`, `done = 0` becomes `todo`; ids, titles and creation dates are kept.
4. `POST /api/tasks` and `PATCH /api/tasks/:id` accept `status`; any other value answers `400` with `{"error": "status must be todo, doing or done"}`.
5. `GET /api/tasks?status=<value>` lists only the tasks in that status (an unknown value answers the same `400`).
6. Compatibility for one release: responses still include `done` (`true` exactly when the status is `done`); `PATCH` still accepts `done: true` (status `done`) and `done: false` (status `todo`); `GET /api/tasks?done=true|false` keeps working (`false` means not done: `todo` or `doing`).
7. The page shows and changes the status with a select instead of the checkbox, and can filter by status.
8. Tests cover the migration and the API; the README documents `status` and says `done` is deprecated.
