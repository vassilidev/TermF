# Creating a task without a title answers a 500 with a database error

**Type:** bug / error message

`POST /api/tasks` with `{}` answers `500` and `{"error": "Provided value cannot be bound to SQLite parameter 1."}`. With `{"title": ""}` it creates an empty task. A body that is not JSON also answers a 500 with the parser's message. On the page, adding an empty task does nothing visible.

## Acceptance criteria

1. `POST /api/tasks` with a missing, empty or blank (only spaces) `title`, or a `title` that is not a string, answers `400` with `{"error": "title is required"}`, and no task is created.
2. The same rule applies to `PATCH /api/tasks/:id` when the body has a `title`.
3. A title longer than 200 characters (after trimming) answers `400` with `{"error": "title must be at most 200 characters"}`. Titles are saved trimmed.
4. A request body that is not valid JSON answers `400` with `{"error": "invalid JSON body"}`.
5. An unexpected error answers `500` with `{"error": "internal error"}` and never shows the internal message to the client (log it on the server instead).
6. When adding a task fails, the page shows the error message from the API.
7. Tests cover these cases.
