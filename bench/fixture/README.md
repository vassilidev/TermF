# Tasks

A small task tracker: a JSON API over SQLite and a one-page interface. No dependencies: Node.js 22.18 or later runs
the TypeScript sources directly and brings SQLite (`node:sqlite`).

```sh
npm start        # http://localhost:3000, data in tasks.db (PORT and DB_PATH change them)
npm test         # the API tests (node:test)
```

## API

Every body is JSON. A task is `{ "id": 1, "title": "Buy milk", "done": false, "created_at": "2026-01-31T09:00:00.000Z" }`.

| Method and path | What it does | Answers |
|---|---|---|
| `GET /api/tasks` | Lists the tasks, oldest first; `?done=true` or `?done=false` keeps the done or the open ones | 200, an array of tasks |
| `POST /api/tasks` | Creates a task from `{ "title": "…" }` | 201, the task |
| `GET /api/tasks/:id` | Reads a task | 200, or 404 `{ "error": "task not found" }` |
| `PATCH /api/tasks/:id` | Changes `title` and / or `done` | 200, the task, or 404 |
| `DELETE /api/tasks/:id` | Deletes a task | 204, or 404 |

Errors answer `{ "error": "<message>" }`.

## Layout

- `src/db.ts`: opens the database and runs the migrations (`PRAGMA user_version` counts those applied).
- `src/tasks.ts`: the queries.
- `src/app.ts`: the HTTP server and its routes.
- `public/`: the page.
- `test/`: the API tests, against a server on a random port with an in-memory database.
