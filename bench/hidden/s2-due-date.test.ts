import { test } from "node:test";
import assert from "node:assert/strict";
import { legacyDb, page, start } from "./helpers.ts";

const ERROR = { error: "due_date must be a date in YYYY-MM-DD format" };

test("AC1: a new task has due_date null", async () => {
  const app = await start();
  const res = await app.call("POST", "/api/tasks", { title: "No date" });
  await app.close();
  assert.equal(res.status, 201);
  assert.equal(res.body.due_date, null);
});

test("AC2: POST keeps the due date and GET returns it", async () => {
  const app = await start();
  const created = await app.call("POST", "/api/tasks", { title: "Taxes", due_date: "2026-05-31" });
  const read = await app.call("GET", `/api/tasks/${created.body.id}`);
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(created.status, 201);
  assert.equal(created.body.due_date, "2026-05-31");
  assert.equal(read.body.due_date, "2026-05-31");
  assert.equal(list.body[0].due_date, "2026-05-31");
});

test("AC2: PATCH sets, keeps and clears the due date", async () => {
  const app = await start();
  const { id } = (await app.call("POST", "/api/tasks", { title: "Dentist" })).body;
  const set = await app.call("PATCH", `/api/tasks/${id}`, { due_date: "2026-03-01" });
  const kept = await app.call("PATCH", `/api/tasks/${id}`, { title: "Dentist at 9" });
  const cleared = await app.call("PATCH", `/api/tasks/${id}`, { due_date: null });
  await app.close();
  assert.equal(set.body.due_date, "2026-03-01");
  assert.equal(kept.body.due_date, "2026-03-01");
  assert.equal(cleared.status, 200);
  assert.equal(cleared.body.due_date, null);
});

test("AC3: a wrong format is a 400 with the message", async () => {
  const app = await start();
  for (const due_date of ["2026-1-5", "tomorrow", "2026/01/05", "2026-01-05T10:00:00Z", 20260105]) {
    const res = await app.call("POST", "/api/tasks", { title: "x", due_date });
    assert.equal(res.status, 400, `due_date ${due_date}`);
    assert.deepEqual(res.body, ERROR);
  }
  await app.close();
});

test("AC3: a day that does not exist is a 400, a leap day is fine", async () => {
  const app = await start();
  for (const due_date of ["2026-02-30", "2025-02-29", "2026-04-31", "2026-13-01", "2026-00-10"]) {
    const res = await app.call("POST", "/api/tasks", { title: "x", due_date });
    assert.equal(res.status, 400, `due_date ${due_date}`);
  }
  const leap = await app.call("POST", "/api/tasks", { title: "x", due_date: "2028-02-29" });
  await app.close();
  assert.equal(leap.status, 201);
});

test("AC3: a refused due date saves nothing", async () => {
  const app = await start();
  await app.call("POST", "/api/tasks", { title: "Bad", due_date: "soon" });
  const { id } = (await app.call("POST", "/api/tasks", { title: "Good", due_date: "2026-06-01" })).body;
  const patch = await app.call("PATCH", `/api/tasks/${id}`, { title: "Changed", due_date: "2026-02-30" });
  const read = await app.call("GET", `/api/tasks/${id}`);
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(patch.status, 400);
  assert.equal(read.body.title, "Good");
  assert.equal(read.body.due_date, "2026-06-01");
  assert.equal(list.body.length, 1);
});

test("AC4: an existing database is migrated, its tasks get due_date null", async () => {
  const path = legacyDb([
    { title: "From before", done: false, created_at: "2025-01-01T00:00:00.000Z" },
    { title: "Old and done", done: true, created_at: "2025-01-02T00:00:00.000Z" },
  ]);
  const app = await start(path);
  const list = await app.call("GET", "/api/tasks");
  const added = await app.call("POST", "/api/tasks", { title: "New", due_date: "2026-01-01" });
  await app.close();
  assert.deepEqual(
    list.body.map((t: any) => [t.title, t.due_date]),
    [
      ["From before", null],
      ["Old and done", null],
    ],
  );
  assert.equal(added.status, 201);
});

test("AC5: the page knows the due date and has a date field", () => {
  const html = page();
  assert.match(html, /due_date/);
  assert.match(html, /type\s*=\s*["']?date|\.type\s*=\s*["']date/);
});
