# Tasks: an optional due date

**Type:** feature (small)

People want to note when a task is due.

## Acceptance criteria

1. Every task in the API has a `due_date` field: a calendar date as a `YYYY-MM-DD` string, or `null` when it has none (the default).
2. `POST /api/tasks` accepts an optional `due_date`. `PATCH /api/tasks/:id` can set it, or clear it with `null`; a PATCH without `due_date` leaves it as it is.
3. A due date that is not a real `YYYY-MM-DD` date (wrong format such as `2026-1-5` or `tomorrow`, or a day that does not exist such as `2026-02-30`) answers `400` with `{"error": "due_date must be a date in YYYY-MM-DD format"}`, and nothing is saved.
4. Existing databases keep working: the column comes with a new migration, and the tasks already there get `due_date: null`.
5. The page shows a task's due date next to its title when it has one, and the add form has an optional date field.
6. Tests cover the new field; the README documents it.
