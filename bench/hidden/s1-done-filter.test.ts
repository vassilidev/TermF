import { test } from "node:test";
import assert from "node:assert/strict";
import { start } from "./helpers.ts";

async function seeded() {
  const app = await start();
  const open = (await app.call("POST", "/api/tasks", { title: "Open one" })).body;
  const done = (await app.call("POST", "/api/tasks", { title: "Done one" })).body;
  await app.call("PATCH", `/api/tasks/${done.id}`, { done: true });
  return { app, open, done };
}

test("AC1: ?done=false lists only the open tasks", async () => {
  const { app, open } = await seeded();
  const res = await app.call("GET", "/api/tasks?done=false");
  await app.close();
  assert.equal(res.status, 200);
  assert.deepEqual(res.body.map((t: any) => t.id), [open.id]);
});

test("AC2: ?done=true lists only the done tasks", async () => {
  const { app, done } = await seeded();
  const res = await app.call("GET", "/api/tasks?done=true");
  await app.close();
  assert.deepEqual(res.body.map((t: any) => t.id), [done.id]);
});

test("AC2: without done every task is listed", async () => {
  const { app } = await seeded();
  const res = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(res.body.length, 2);
});

test("AC3: ?done=yes is a 400 with the message", async () => {
  const { app } = await seeded();
  const res = await app.call("GET", "/api/tasks?done=yes");
  await app.close();
  assert.equal(res.status, 400);
  assert.deepEqual(res.body, { error: "done must be true or false" });
});

test("AC3: an empty ?done= is a 400 with the message", async () => {
  const { app } = await seeded();
  const res = await app.call("GET", "/api/tasks?done=");
  await app.close();
  assert.equal(res.status, 400);
  assert.deepEqual(res.body, { error: "done must be true or false" });
});

test("AC3: ?done=FALSE or ?done=1 is refused too", async () => {
  const { app } = await seeded();
  const upper = await app.call("GET", "/api/tasks?done=FALSE");
  const one = await app.call("GET", "/api/tasks?done=1");
  await app.close();
  assert.equal(upper.status, 400);
  assert.equal(one.status, 400);
});
