# Lists

Task lists: a JSON API over SQLite with accounts, tags, comments, an activity log and email reminders, and a small
page. No dependencies: Node.js 22.18 or later runs the TypeScript sources directly and brings SQLite
(`node:sqlite`).

```sh
npm start        # http://localhost:3000, data in lists.db (PORT and DB_PATH change them)
npm test         # the tests (node:test)
```

`npm start` also runs the job runner every minute (reminders and digests, see below).

## API

Every body is JSON. Errors answer `{ "error": "<message>" }`. Every route but signing up and signing in needs
`Authorization: Bearer <token>`; without a valid one it answers `401 { "error": "authentication required" }`.

### Accounts

| Method and path | Body | Answers |
|---|---|---|
| `POST /api/users` | `{ "username", "password", "email"? }` | 201 `{ "id", "username", "email" }`; 400, or 409 if the username is taken |
| `POST /api/sessions` | `{ "username", "password" }` | 201 `{ "token", "user" }`; 401 `{ "error": "invalid credentials" }` |
| `DELETE /api/sessions` | | 204: the token stops working |
| `GET /api/me` | | 200, the signed-in user |

A username is lowercased, then 3 to 32 characters among `a-z`, `0-9` and `_`. A password has at least 10
characters. The email is optional: without one, no reminder nor digest is sent.

### Lists

A list belongs to the user who created it, and only they see it: someone else's list answers 404, as one that does
not exist.

| Method and path | Body | Answers |
|---|---|---|
| `GET /api/lists` | | 200, the user's lists by name, each with `task_count` and `open_count` |
| `POST /api/lists` | `{ "name" }` | 201, the list |
| `GET /api/lists/:id` | | 200, the list |
| `PATCH /api/lists/:id` | `{ "name" }` | 200, the list |
| `DELETE /api/lists/:id` | | 204: the list, its tasks, their comments and the list's activity are deleted |
| `GET /api/lists/:id/tags` | | 200, `[{ "name", "count" }]`: the tags used in the list |
| `GET /api/lists/:id/activity` | | 200, the latest entries, newest first; `?limit=` (default 50, at most 200) |

### Tasks

A task is:

```json
{
  "id": 12, "list_id": 3, "title": "Pay rent", "notes": "", "done": false, "position": 4, "priority": 2,
  "due_date": "2026-04-01", "assignee": { "id": 1, "username": "alice" }, "tags": ["home"], "comment_count": 2,
  "created_by": 1, "created_at": "2026-03-01T09:00:00.000Z", "updated_at": "2026-03-02T10:00:00.000Z"
}
```

| Method and path | Body | Answers |
|---|---|---|
| `GET /api/lists/:id/tasks` | | 200, the tasks of the list (filters and sorts below) |
| `POST /api/lists/:id/tasks` | `{ "title", "notes"?, "done"?, "priority"?, "due_date"?, "assignee_id"?, "position"?, "tags"? }` | 201, the task, at the end of the list unless `position` is given |
| `GET /api/tasks/:id` | | 200, the task |
| `PATCH /api/tasks/:id` | any of the fields above | 200, the task |
| `DELETE /api/tasks/:id` | | 204 |
| `GET /api/tasks/:id/comments` | | 200, `[{ "id", "task_id", "author", "body", "created_at" }]`, oldest first |
| `POST /api/tasks/:id/comments` | `{ "body" }` | 201, the comment |

Fields: `title` 1 to 200 characters; `notes` up to 5,000; `done` a boolean; `priority` 0 (low) to 3 (urgent);
`due_date` `YYYY-MM-DD` or null; `assignee_id` a user who can see the list, or null; `position` a
whole number, 0 or more; `tags` up to 10 names of 1 to 30 letters, digits, `-` or `_`, stored in lowercase. A
comment has 1 to 2,000 characters.

`GET /api/lists/:id/tasks` takes:

- `?done=true` or `?done=false`: only the done, or the open tasks;
- `?tag=<name>`: only the tasks with this tag;
- `?assignee=<user id>` or `?assignee=none`: only the tasks assigned to this user, or to nobody;
- `?sort=`: `position` (the default, ascending), `due_date` (soonest first, tasks without one last), `priority`
  (highest first) or `created` (newest first). Ties go by id: ascending, except for `created` where the newest
  comes first.

### Activity

Every change to a list or its tasks adds an entry `{ "id", "action", "task_id", "user", "detail", "created_at" }`.
Actions: `list.created`, `list.renamed`, `task.created`, `task.updated` (`detail.changes` names the fields that
changed), `task.deleted`, `comment.added`.

## Reminders and digests

`src/jobs.ts` holds a small job runner over the `jobs` table; `runDueJobs(db, now)` runs what is due.

- A task with a due date and an assignee gets a reminder on the morning of its due date (08:00 UTC), unless it is
  done by then. Changing the due date, the assignee or `done` replans it; deleting the task cancels it.
- From 07:00 UTC, each user with an email gets one digest a day listing their open tasks due that day or overdue:
  those assigned to them, and the unassigned ones of their lists.
- A job that fails is retried with a growing delay, five times at most.

Emails are written to the `outbox` table; sending them is the job of the mail relay, which is not part of this
repository.

## Layout

- `src/db.ts`: opens the database and runs the migrations (`PRAGMA user_version` counts those applied).
- `src/http.ts`: the router, JSON bodies, errors.
- `src/auth.ts`: accounts and sessions (scrypt, hashed bearer tokens).
- `src/lists.ts`, `src/tasks.ts`, `src/comments.ts`, `src/activity.ts`: the resources.
- `src/jobs.ts`: reminders, digests and the job runner.
- `src/app.ts`: the routes and the HTTP server; `src/main.ts` starts it.
- `public/`: the page (`app.js` starts it; `api.js`, `auth.js`, `lists.js`, `tasks.js`).
- `test/`: the tests, against a server on a random port with an in-memory database.

## Known issues

- Sessions never expire: a token works until its user signs out.
- Moving a task does not renumber the others, so two tasks can share a position (ties go by id). See the skipped
  test in `test/tasks.test.ts`.
- Tags are never deleted, even when no task carries them any more.
- Error messages are not all worded the same way.
