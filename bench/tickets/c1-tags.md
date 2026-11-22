# Tags on tasks, with a filter

**Type:** feature (API, data, page, tests)

People want to group their tasks by tags such as `home` or `work`, and see only one group at a time.

## Acceptance criteria

1. Every task in the API has `tags`: an array of strings sorted alphabetically, `[]` by default.
2. `POST /api/tasks` and `PATCH /api/tasks/:id` accept `tags`. In a PATCH, `tags` replaces the whole list; a PATCH without `tags` leaves them as they are.
3. Tags are normalized: trimmed, lowercased, duplicates removed. After that, a valid tag is 1 to 30 characters among `a-z`, `0-9` and `-`, and a task has at most 10 tags. Otherwise the request answers `400` with `{"error": "invalid tags"}` and nothing changes.
4. `GET /api/tasks?tag=<name>` lists only the tasks that have this tag (the name is normalized the same way). It combines with `done`.
5. `GET /api/tags` lists the tags in use with their number of tasks, sorted by name: `[{"name": "home", "count": 2}, {"name": "work", "count": 1}]`. A tag no task uses any more is not listed.
6. Data: tags live in their own table(s), added by a new migration (not a text column on `tasks`); deleting a task removes its links to tags.
7. The page shows each task's tags; clicking a tag shows only the tasks with that tag (and there is a way back to all); the add form accepts comma-separated tags.
8. Tests cover the API; the README documents it.
