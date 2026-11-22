import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { makeList, makeTask, signUp, start, type App } from "./helpers.ts";

let app: App;
before(async () => (app = await start()));
after(() => app.close());

test("creates a list and lists it with its counts", async () => {
  const { auth, user } = await signUp(app, "alice");
  const created = await app.call("POST", "/api/lists", { name: "  Groceries " }, auth);
  assert.equal(created.status, 201);
  assert.equal(created.body.name, "Groceries");
  assert.equal(created.body.owner_id, user.id);
  const task = await makeTask(app, auth, created.body.id, { title: "Milk" });
  await makeTask(app, auth, created.body.id, { title: "Eggs" });
  await app.call("PATCH", `/api/tasks/${task.id}`, { done: true }, auth);
  const lists = await app.call("GET", "/api/lists", undefined, auth);
  assert.equal(lists.status, 200);
  assert.equal(lists.body.length, 1);
  assert.equal(lists.body[0].task_count, 2);
  assert.equal(lists.body[0].open_count, 1);
});

test("lists come sorted by name", async () => {
  const { auth } = await signUp(app, "bob");
  for (const name of ["work", "Errands", "books"]) await makeList(app, auth, name);
  const lists = await app.call("GET", "/api/lists", undefined, auth);
  assert.deepEqual(
    lists.body.map((l: any) => l.name),
    ["books", "Errands", "work"],
  );
});

test("a list needs a name of at most 100 characters", async () => {
  const { auth } = await signUp(app, "carol");
  for (const name of [undefined, "", "   ", 12]) {
    const res = await app.call("POST", "/api/lists", { name }, auth);
    assert.equal(res.status, 400);
    assert.deepEqual(res.body, { error: "name is required" });
  }
  const long = await app.call("POST", "/api/lists", { name: "x".repeat(101) }, auth);
  assert.deepEqual(long.body, { error: "name is too long (max 100 characters)" });
});

test("renames a list", async () => {
  const { auth } = await signUp(app, "dave");
  const list = await makeList(app, auth, "Old");
  const renamed = await app.call("PATCH", `/api/lists/${list.id}`, { name: "New" }, auth);
  assert.equal(renamed.status, 200);
  assert.equal(renamed.body.name, "New");
  assert.equal((await app.call("GET", `/api/lists/${list.id}`, undefined, auth)).body.name, "New");
});

test("deleting a list deletes its tasks", async () => {
  const { auth } = await signUp(app, "erin");
  const list = await makeList(app, auth);
  const task = await makeTask(app, auth, list.id);
  assert.equal((await app.call("DELETE", `/api/lists/${list.id}`, undefined, auth)).status, 204);
  assert.equal((await app.call("GET", `/api/lists/${list.id}`, undefined, auth)).status, 404);
  assert.equal((await app.call("GET", `/api/tasks/${task.id}`, undefined, auth)).status, 404);
});

test("someone else's list is a 404, as if it did not exist", async () => {
  const owner = await signUp(app, "frank");
  const other = await signUp(app, "grace");
  const list = await makeList(app, owner.auth, "Private");
  for (const [method, path, body] of [
    ["GET", `/api/lists/${list.id}`, undefined],
    ["PATCH", `/api/lists/${list.id}`, { name: "Mine now" }],
    ["DELETE", `/api/lists/${list.id}`, undefined],
    ["GET", `/api/lists/${list.id}/tasks`, undefined],
    ["POST", `/api/lists/${list.id}/tasks`, { title: "Sneaky" }],
    ["GET", `/api/lists/${list.id}/activity`, undefined],
  ] as const) {
    const res = await app.call(method, path, body, other.auth);
    assert.equal(res.status, 404, `${method} ${path}`);
    assert.deepEqual(res.body, { error: "list not found" });
  }
  assert.deepEqual((await app.call("GET", "/api/lists", undefined, other.auth)).body, []);
  assert.equal((await app.call("GET", `/api/lists/${list.id}`, undefined, owner.auth)).body.name, "Private");
});
