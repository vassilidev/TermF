import { test } from "node:test";
import assert from "node:assert/strict";
import { legacyDb, page, sql, start, tempDb } from "./helpers.ts";

const INVALID = { error: "status must be todo, doing or done" };

test("AC1: a new task is todo, and not done", async () => {
  const app = await start();
  const res = await app.call("POST", "/api/tasks", { title: "New" });
  await app.close();
  assert.equal(res.status, 201);
  assert.equal(res.body.status, "todo");
  assert.equal(res.body.done, false);
});

test("AC4: POST and PATCH take a status", async () => {
  const app = await start();
  const created = await app.call("POST", "/api/tasks", { title: "Start", status: "doing" });
  const patched = await app.call("PATCH", `/api/tasks/${created.body.id}`, { status: "done" });
  const back = await app.call("PATCH", `/api/tasks/${created.body.id}`, { status: "doing" });
  await app.close();
  assert.equal(created.body.status, "doing");
  assert.equal(created.body.done, false);
  assert.equal(patched.body.status, "done");
  assert.equal(patched.body.done, true);
  assert.equal(back.body.done, false);
});

test("AC4: another status is a 400 and changes nothing", async () => {
  const app = await start();
  const post = await app.call("POST", "/api/tasks", { title: "x", status: "blocked" });
  const { id } = (await app.call("POST", "/api/tasks", { title: "Keep" })).body;
  const patch = await app.call("PATCH", `/api/tasks/${id}`, { title: "Changed", status: "DONE" });
  const read = await app.call("GET", `/api/tasks/${id}`);
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(post.status, 400);
  assert.deepEqual(post.body, INVALID);
  assert.equal(patch.status, 400);
  assert.deepEqual(patch.body, INVALID);
  assert.equal(read.body.title, "Keep");
  assert.equal(read.body.status, "todo");
  assert.equal(list.body.length, 1);
});

test("AC5: ?status= filters, an unknown one is a 400", async () => {
  const app = await start();
  const a = (await app.call("POST", "/api/tasks", { title: "A", status: "doing" })).body;
  await app.call("POST", "/api/tasks", { title: "B" });
  const doing = await app.call("GET", "/api/tasks?status=doing");
  const bad = await app.call("GET", "/api/tasks?status=later");
  await app.close();
  assert.deepEqual(doing.body.map((t: any) => t.id), [a.id]);
  assert.equal(bad.status, 400);
  assert.deepEqual(bad.body, INVALID);
});

test("AC6: PATCH still takes done", async () => {
  const app = await start();
  const { id } = (await app.call("POST", "/api/tasks", { title: "Old client", status: "doing" })).body;
  const done = await app.call("PATCH", `/api/tasks/${id}`, { done: true });
  const undone = await app.call("PATCH", `/api/tasks/${id}`, { done: false });
  await app.close();
  assert.equal(done.body.status, "done");
  assert.equal(done.body.done, true);
  assert.equal(undone.body.status, "todo");
  assert.equal(undone.body.done, false);
});

test("AC6: ?done=true and ?done=false still filter", async () => {
  const app = await start();
  const todo = (await app.call("POST", "/api/tasks", { title: "T" })).body;
  const doing = (await app.call("POST", "/api/tasks", { title: "D", status: "doing" })).body;
  const done = (await app.call("POST", "/api/tasks", { title: "X", status: "done" })).body;
  const open = await app.call("GET", "/api/tasks?done=false");
  const closed = await app.call("GET", "/api/tasks?done=true");
  await app.close();
  assert.deepEqual(open.body.map((t: any) => t.id), [todo.id, doing.id]);
  assert.deepEqual(closed.body.map((t: any) => t.id), [done.id]);
});

test("AC2: the done column is replaced by status", async () => {
  const path = tempDb();
  const app = await start(path);
  await app.call("POST", "/api/tasks", { title: "A", status: "doing" });
  await app.close();
  const columns = sql(path, "PRAGMA table_info(tasks)").map((c) => c.name);
  const [{ user_version }] = sql(path, "PRAGMA user_version");
  assert.ok(columns.includes("status"), `columns: ${columns}`);
  assert.ok(!columns.includes("done"), `columns: ${columns}`);
  assert.ok(user_version > 1);
  assert.deepEqual(sql(path, "SELECT status FROM tasks"), [{ status: "doing" }]);
});

test("AC3: an existing database is migrated without losing anything", async () => {
  const path = legacyDb([
    { title: "Open from before", done: false, created_at: "2025-01-01T00:00:00.000Z" },
    { title: "Done from before", done: true, created_at: "2025-01-02T03:04:05.678Z" },
  ]);
  const app = await start(path);
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.deepEqual(
    list.body.map((t: any) => [t.id, t.title, t.status, t.done, t.created_at]),
    [
      [1, "Open from before", "todo", false, "2025-01-01T00:00:00.000Z"],
      [2, "Done from before", "done", true, "2025-01-02T03:04:05.678Z"],
    ],
  );
});

test("AC3: the migrated database keeps working after a restart", async () => {
  const path = legacyDb([{ title: "Once", done: true, created_at: "2025-01-01T00:00:00.000Z" }]);
  const first = await start(path);
  await first.call("POST", "/api/tasks", { title: "Twice", status: "doing" });
  await first.close();
  const second = await start(path);
  const list = await second.call("GET", "/api/tasks");
  await second.close();
  assert.deepEqual(
    list.body.map((t: any) => t.status),
    ["done", "doing"],
  );
});

test("AC7: the page uses a status select", () => {
  const html = page();
  assert.ok(/status/.test(html), "the page lacks it");
  assert.ok(/<select|createElement\(\s*["']select/.test(html), "the page lacks it");
  assert.ok(/doing/.test(html), "the page lacks it");
});
