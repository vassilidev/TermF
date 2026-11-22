# Edits sometimes disappear

**Type:** bug

> From support, three reports this week:
>
> "We share one account at home. Yesterday I renamed a task while my partner ticked it done on her phone: now it
> has my new name but it is not done any more." Another user sees the opposite: a due date or a priority they set
> is back to its old value a moment later. It happens on busy days, and more with the page open on several
> devices. Nobody could reproduce it by hand.

## Expected

1. No change to a task is ever lost silently.
2. Tasks get a `version`: an integer, `1` for a new task (and for every task that already exists), that goes up by
   one each time the task is changed. Every task the API answers carries it.
3. `PATCH /api/tasks/:id` may send the `version` it read. If the task changed since, it answers `409` with
   `{"error": "version conflict", "task": <the task as it is now>}` and changes nothing.
4. A `PATCH` without `version` still works, and never drops a change made at the same time to another field of the
   task.
5. A test reproduces the problem, and passes once it is fixed.
6. The page sends the version it read and, on a conflict, shows the task as it is now. The README documents
   `version` and the `409`.
