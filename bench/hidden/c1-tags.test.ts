import { test } from "node:test";
import assert from "node:assert/strict";
import { legacyDb, page, sql, start, tempDb } from "./helpers.ts";

const INVALID = { error: "invalid tags" };

test("AC1: a new task has no tags", async () => {
  const app = await start();
  const res = await app.call("POST", "/api/tasks", { title: "Plain" });
  await app.close();
  assert.deepEqual(res.body.tags, []);
});

test("AC2-3: tags are normalized, deduplicated and sorted", async () => {
  const app = await start();
  const created = await app.call("POST", "/api/tasks", { title: "Tidy", tags: [" Work", "home", "work", "HOME "] });
  const read = await app.call("GET", `/api/tasks/${created.body.id}`);
  await app.close();
  assert.equal(created.status, 201);
  assert.deepEqual(created.body.tags, ["home", "work"]);
  assert.deepEqual(read.body.tags, ["home", "work"]);
});

test("AC2: PATCH replaces the tags, and keeps them without tags", async () => {
  const app = await start();
  const { id } = (await app.call("POST", "/api/tasks", { title: "Swap", tags: ["a", "b"] })).body;
  const kept = await app.call("PATCH", `/api/tasks/${id}`, { done: true });
  const replaced = await app.call("PATCH", `/api/tasks/${id}`, { tags: ["c"] });
  const emptied = await app.call("PATCH", `/api/tasks/${id}`, { tags: [] });
  await app.close();
  assert.deepEqual(kept.body.tags, ["a", "b"]);
  assert.deepEqual(replaced.body.tags, ["c"]);
  assert.deepEqual(emptied.body.tags, []);
});

test("AC3: invalid tags are a 400 and create nothing", async () => {
  const app = await start();
  const cases = [["bad tag"], [""], ["   "], ["x".repeat(31)], ["émoji"], ["under_score"], [1], "home", { a: 1 }];
  for (const tags of cases) {
    const res = await app.call("POST", "/api/tasks", { title: "x", tags });
    assert.equal(res.status, 400, JSON.stringify(tags));
    assert.deepEqual(res.body, INVALID);
  }
  const eleven = Array.from({ length: 11 }, (_, i) => `t${i}`);
  const tooMany = await app.call("POST", "/api/tasks", { title: "x", tags: eleven });
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(tooMany.status, 400);
  assert.equal(list.body.length, 0);
});

test("AC3: limits: 30 characters and 10 tags are fine, duplicates count once", async () => {
  const app = await start();
  const long = await app.call("POST", "/api/tasks", { title: "x", tags: ["y".repeat(30), "a-1"] });
  const ten = Array.from({ length: 10 }, (_, i) => `t${i}`);
  const max = await app.call("POST", "/api/tasks", { title: "x", tags: [...ten, "T0", " t1 "] });
  await app.close();
  assert.equal(long.status, 201);
  assert.equal(max.status, 201);
  assert.equal(max.body.tags.length, 10);
});

test("AC3: an invalid PATCH changes nothing", async () => {
  const app = await start();
  const { id } = (await app.call("POST", "/api/tasks", { title: "Stay", tags: ["home"] })).body;
  const res = await app.call("PATCH", `/api/tasks/${id}`, { title: "Moved", tags: ["no spaces allowed"] });
  const read = await app.call("GET", `/api/tasks/${id}`);
  await app.close();
  assert.equal(res.status, 400);
  assert.deepEqual(res.body, INVALID);
  assert.equal(read.body.title, "Stay");
  assert.deepEqual(read.body.tags, ["home"]);
});

test("AC4: ?tag= filters, normalized, and combines with done", async () => {
  const app = await start();
  const a = (await app.call("POST", "/api/tasks", { title: "A", tags: ["work"] })).body;
  const b = (await app.call("POST", "/api/tasks", { title: "B", tags: ["work", "home"] })).body;
  await app.call("POST", "/api/tasks", { title: "C", tags: ["home"] });
  await app.call("PATCH", `/api/tasks/${b.id}`, { done: true });
  const work = await app.call("GET", "/api/tasks?tag=work");
  const upper = await app.call("GET", "/api/tasks?tag=%20WORK%20");
  const doneWork = await app.call("GET", "/api/tasks?tag=work&done=true");
  const none = await app.call("GET", "/api/tasks?tag=garden");
  await app.close();
  assert.deepEqual(work.body.map((t: any) => t.id), [a.id, b.id]);
  assert.deepEqual(upper.body.map((t: any) => t.id), [a.id, b.id]);
  assert.deepEqual(doneWork.body.map((t: any) => t.id), [b.id]);
  assert.deepEqual(none.body, []);
});

test("AC5: /api/tags counts the tags in use, sorted by name", async () => {
  const app = await start();
  const a = (await app.call("POST", "/api/tasks", { title: "A", tags: ["work", "home"] })).body;
  await app.call("POST", "/api/tasks", { title: "B", tags: ["home"] });
  const c = (await app.call("POST", "/api/tasks", { title: "C", tags: ["garden"] })).body;
  const first = await app.call("GET", "/api/tags");
  await app.call("DELETE", `/api/tasks/${a.id}`);
  await app.call("PATCH", `/api/tasks/${c.id}`, { tags: [] });
  const after = await app.call("GET", "/api/tags");
  await app.close();
  assert.equal(first.status, 200);
  assert.deepEqual(first.body, [
    { name: "garden", count: 1 },
    { name: "home", count: 2 },
    { name: "work", count: 1 },
  ]);
  assert.deepEqual(after.body, [{ name: "home", count: 1 }]);
});

test("AC6: tags have their own table, from a new migration", async () => {
  const path = tempDb();
  const app = await start(path);
  await app.call("POST", "/api/tasks", { title: "A", tags: ["home"] });
  await app.close();
  const tables = sql(path, "SELECT name FROM sqlite_master WHERE type = 'table'").map((t) => t.name);
  const columns = sql(path, "PRAGMA table_info(tasks)").map((c) => c.name);
  const [{ user_version }] = sql(path, "PRAGMA user_version");
  assert.ok(tables.some((t) => /tag/i.test(t)), `tables: ${tables}`);
  assert.ok(!columns.some((c) => /tag/i.test(c)), `tasks columns: ${columns}`);
  assert.ok(user_version > 1, `user_version ${user_version}`);
});

test("AC6: an existing database is migrated and keeps its tasks", async () => {
  const path = legacyDb([{ title: "From before", done: true, created_at: "2025-01-01T00:00:00.000Z" }]);
  const app = await start(path);
  const list = await app.call("GET", "/api/tasks");
  const tagged = await app.call("PATCH", `/api/tasks/${list.body[0].id}`, { tags: ["old"] });
  await app.close();
  assert.equal(list.body.length, 1);
  assert.equal(list.body[0].title, "From before");
  assert.deepEqual(list.body[0].tags, []);
  assert.deepEqual(tagged.body.tags, ["old"]);
});

test("AC7: the page shows tags and filters by them", () => {
  const html = page();
  assert.ok(/tags/.test(html), "the page lacks it");
  // The filter may ask the API (`?tag=`) or filter on the page: a click on a tag is enough.
  assert.ok(/[?&]tag=|addEventListener\(\s*["']click|onclick/.test(html), "the page lacks it");
});
