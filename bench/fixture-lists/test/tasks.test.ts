import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { makeList, makeTask, signUp, start, type App, type Auth } from "./helpers.ts";

let app: App;
before(async () => (app = await start()));
after(() => app.close());

let users = 0;
async function setUp(): Promise<{ auth: Auth; user: any; list: any }> {
  const { auth, user } = await signUp(app, `user_${++users}`);
  return { auth, user, list: await makeList(app, auth) };
}

const titles = (res: { body: any[] }) => res.body.map((t) => t.title);

test("creates a task with its defaults, at the end of the list", async () => {
  const { auth, user, list } = await setUp();
  const first = await app.call("POST", `/api/lists/${list.id}/tasks`, { title: " Buy milk " }, auth);
  assert.equal(first.status, 201);
  assert.equal(first.body.title, "Buy milk");
  assert.equal(first.body.list_id, list.id);
  assert.equal(first.body.done, false);
  assert.equal(first.body.priority, 0);
  assert.equal(first.body.due_date, null);
  assert.equal(first.body.assignee, null);
  assert.deepEqual(first.body.tags, []);
  assert.equal(first.body.comment_count, 0);
  assert.equal(first.body.created_by, user.id);
  assert.equal(first.body.position, 0);
  const second = await makeTask(app, auth, list.id);
  assert.equal(second.position, 1);
});

test("refuses invalid fields", async () => {
  const { auth, list } = await setUp();
  const cases: [Record<string, unknown>, string][] = [
    [{}, "title is required"],
    [{ title: "   " }, "title is required"],
    [{ title: "x".repeat(201) }, "title is too long (max 200 characters)"],
    [{ title: "t", done: "yes" }, "done must be true or false"],
    [{ title: "t", priority: 4 }, "priority must be 0, 1, 2 or 3"],
    [{ title: "t", due_date: "2026-02-30" }, "due_date must be a date as YYYY-MM-DD, or null"],
    [{ title: "t", due_date: "tomorrow" }, "due_date must be a date as YYYY-MM-DD, or null"],
    [{ title: "t", tags: "home" }, "tags must be a list of at most 10 names"],
    [{ title: "t", tags: ["no spaces"] }, "a tag is 1 to 30 letters, digits, - or _"],
    [{ title: "t", position: -1 }, "position must be a whole number, 0 or more"],
  ];
  for (const [body, error] of cases) {
    const res = await app.call("POST", `/api/lists/${list.id}/tasks`, body, auth);
    assert.equal(res.status, 400, JSON.stringify(body));
    assert.deepEqual(res.body, { error });
  }
  const bad = await app.call("POST", `/api/lists/${list.id}/tasks`, "{not json", auth);
  assert.equal(bad.status, 400);
});

test("reads, changes and deletes a task", async () => {
  const { auth, list } = await setUp();
  const task = await makeTask(app, auth, list.id, { title: "Call the plumber" });
  const read = await app.call("GET", `/api/tasks/${task.id}`, undefined, auth);
  assert.deepEqual(read.body, task);
  const changed = await app.call("PATCH", `/api/tasks/${task.id}`, { title: "Call the electrician", done: true, priority: 2, due_date: "2026-03-01", notes: "Before noon" }, auth);
  assert.equal(changed.status, 200);
  assert.equal(changed.body.title, "Call the electrician");
  assert.equal(changed.body.done, true);
  assert.equal(changed.body.priority, 2);
  assert.equal(changed.body.due_date, "2026-03-01");
  assert.equal(changed.body.notes, "Before noon");
  const cleared = await app.call("PATCH", `/api/tasks/${task.id}`, { due_date: null }, auth);
  assert.equal(cleared.body.due_date, null);
  assert.equal(cleared.body.title, "Call the electrician");
  assert.equal((await app.call("DELETE", `/api/tasks/${task.id}`, undefined, auth)).status, 204);
  const gone = await app.call("GET", `/api/tasks/${task.id}`, undefined, auth);
  assert.equal(gone.status, 404);
  assert.deepEqual(gone.body, { error: "task not found" });
});

test("tags are trimmed, lowercased, deduplicated and sorted; ?tag= filters on them", async () => {
  const { auth, list } = await setUp();
  const task = await makeTask(app, auth, list.id, { title: "Paint", tags: ["Home", "weekend", " home"] });
  assert.deepEqual(task.tags, ["home", "weekend"]);
  await makeTask(app, auth, list.id, { title: "Report", tags: ["work"] });
  await makeTask(app, auth, list.id, { title: "Nothing" });
  assert.deepEqual(titles(await app.call("GET", `/api/lists/${list.id}/tasks?tag=HOME`, undefined, auth)), ["Paint"]);
  const retagged = await app.call("PATCH", `/api/tasks/${task.id}`, { tags: ["work"] }, auth);
  assert.deepEqual(retagged.body.tags, ["work"]);
  assert.deepEqual(titles(await app.call("GET", `/api/lists/${list.id}/tasks?tag=work`, undefined, auth)), ["Paint", "Report"]);
  const tags = await app.call("GET", `/api/lists/${list.id}/tags`, undefined, auth);
  assert.deepEqual(tags.body, [
    { name: "home", count: 0 },
    { name: "weekend", count: 0 },
    { name: "work", count: 2 },
  ]);
});

test("a task can be assigned to the list's owner only", async () => {
  const { auth, user, list } = await setUp();
  const other = await signUp(app, "outsider");
  const task = await makeTask(app, auth, list.id, { assignee_id: user.id });
  assert.deepEqual(task.assignee, { id: user.id, username: user.username });
  const refused = await app.call("PATCH", `/api/tasks/${task.id}`, { assignee_id: other.user.id }, auth);
  assert.equal(refused.status, 400);
  assert.deepEqual(refused.body, { error: "the assignee must have access to the list" });
  const unassigned = await app.call("PATCH", `/api/tasks/${task.id}`, { assignee_id: null }, auth);
  assert.equal(unassigned.body.assignee, null);
});

test("?done= and ?assignee= filter the tasks", async () => {
  const { auth, user, list } = await setUp();
  const done = await makeTask(app, auth, list.id, { title: "Done", done: true, assignee_id: user.id });
  await makeTask(app, auth, list.id, { title: "Open" });
  assert.deepEqual(titles(await app.call("GET", `/api/lists/${list.id}/tasks?done=true`, undefined, auth)), ["Done"]);
  assert.deepEqual(titles(await app.call("GET", `/api/lists/${list.id}/tasks?done=false`, undefined, auth)), ["Open"]);
  assert.deepEqual(titles(await app.call("GET", `/api/lists/${list.id}/tasks?assignee=${user.id}`, undefined, auth)), ["Done"]);
  assert.deepEqual(titles(await app.call("GET", `/api/lists/${list.id}/tasks?assignee=none`, undefined, auth)), ["Open"]);
  assert.equal((await app.call("GET", `/api/lists/${list.id}/tasks?done=maybe`, undefined, auth)).status, 400);
  assert.equal((await app.call("GET", `/api/lists/${list.id}/tasks?assignee=me`, undefined, auth)).status, 400);
  assert.equal(done.done, true);
});

test("sorts by position, due date, priority or creation", async () => {
  const { auth, list } = await setUp();
  const a = await makeTask(app, auth, list.id, { title: "a", due_date: "2026-05-02", priority: 1 });
  await makeTask(app, auth, list.id, { title: "b", priority: 3 });
  await makeTask(app, auth, list.id, { title: "c", due_date: "2026-05-01", priority: 1 });
  await makeTask(app, auth, list.id, { title: "d", due_date: "2026-05-02" });
  const sorted = async (sort: string) => titles(await app.call("GET", `/api/lists/${list.id}/tasks?sort=${sort}`, undefined, auth));
  assert.deepEqual(await sorted("position"), ["a", "b", "c", "d"]);
  assert.deepEqual(await sorted("due_date"), ["c", "a", "d", "b"]);
  assert.deepEqual(await sorted("priority"), ["b", "a", "c", "d"]);
  assert.deepEqual(await sorted("created"), ["d", "c", "b", "a"]);
  await app.call("PATCH", `/api/tasks/${a.id}`, { position: 2 }, auth);
  assert.deepEqual(await sorted("position"), ["b", "a", "c", "d"]);
  const unknown = await app.call("GET", `/api/lists/${list.id}/tasks?sort=title`, undefined, auth);
  assert.equal(unknown.status, 400);
  assert.deepEqual(unknown.body, { error: "sort must be one of position, due_date, priority, created" });
});

test("a task shows how many comments it has", async () => {
  const { auth, list } = await setUp();
  const task = await makeTask(app, auth, list.id);
  await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "First" }, auth);
  await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "Second" }, auth);
  assert.equal((await app.call("GET", `/api/tasks/${task.id}`, undefined, auth)).body.comment_count, 2);
  const [listed] = (await app.call("GET", `/api/lists/${list.id}/tasks`, undefined, auth)).body;
  assert.equal(listed.comment_count, 2);
});

test("someone else's task is a 404", async () => {
  const owner = await setUp();
  const other = await setUp();
  const task = await makeTask(app, owner.auth, owner.list.id, { title: "Mine" });
  for (const [method, path, body] of [
    ["GET", `/api/tasks/${task.id}`, undefined],
    ["PATCH", `/api/tasks/${task.id}`, { title: "Taken" }],
    ["DELETE", `/api/tasks/${task.id}`, undefined],
    ["GET", `/api/tasks/${task.id}/comments`, undefined],
    ["POST", `/api/tasks/${task.id}/comments`, { body: "Hi" }],
  ] as const) {
    const res = await app.call(method, path, body, other.auth);
    assert.equal(res.status, 404, `${method} ${path}`);
    assert.deepEqual(res.body, { error: "task not found" });
  }
  assert.equal((await app.call("GET", `/api/tasks/${task.id}`, undefined, owner.auth)).body.title, "Mine");
});

test("a PATCH that changes nothing leaves the task as it was", async () => {
  const { auth, list } = await setUp();
  const task = await makeTask(app, auth, list.id, { title: "Same", tags: ["a"] });
  const res = await app.call("PATCH", `/api/tasks/${task.id}`, { title: "Same", tags: ["a"] }, auth);
  assert.equal(res.status, 200);
  assert.deepEqual(res.body, task);
});

// TODO(#41): moving a task should shift the ones after it instead of leaving two tasks on the same position.
test.skip("moving a task renumbers the positions of the others", async () => {
  const { auth, list } = await setUp();
  const tasks = [];
  for (const title of ["a", "b", "c"]) tasks.push(await makeTask(app, auth, list.id, { title }));
  await app.call("PATCH", `/api/tasks/${tasks[2].id}`, { position: 0 }, auth);
  const res = await app.call("GET", `/api/lists/${list.id}/tasks`, undefined, auth);
  assert.deepEqual(
    res.body.map((t: any) => [t.title, t.position]),
    [["c", 0], ["a", 1], ["b", 2]],
  );
});
