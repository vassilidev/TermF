import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { makeList, makeTask, signUp, start, type App } from "./helpers.ts";

let app: App;
before(async () => (app = await start()));
after(() => app.close());

test("logs what happens to a list, newest first", async () => {
  const { auth, user } = await signUp(app, "alice");
  const list = await makeList(app, auth, "Home");
  const task = await makeTask(app, auth, list.id, { title: "Fix the door" });
  await app.call("PATCH", `/api/tasks/${task.id}`, { done: true, priority: 2 }, auth);
  await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "Done" }, auth);
  await app.call("DELETE", `/api/tasks/${task.id}`, undefined, auth);
  await app.call("PATCH", `/api/lists/${list.id}`, { name: "House" }, auth);
  const res = await app.call("GET", `/api/lists/${list.id}/activity`, undefined, auth);
  assert.equal(res.status, 200);
  assert.deepEqual(
    res.body.map((e: any) => e.action),
    ["list.renamed", "task.deleted", "comment.added", "task.updated", "task.created", "list.created"],
  );
  const updated = res.body.find((e: any) => e.action === "task.updated");
  assert.equal(updated.task_id, task.id);
  assert.deepEqual(updated.user, { id: user.id, username: "alice" });
  assert.deepEqual(updated.detail, { changes: ["done", "priority"] });
  assert.deepEqual(res.body[0].detail, { from: "Home", to: "House" });
});

test("?limit= keeps the latest entries", async () => {
  const { auth } = await signUp(app, "bob");
  const list = await makeList(app, auth);
  for (let i = 0; i < 4; i++) await makeTask(app, auth, list.id, { title: `t${i}` });
  const res = await app.call("GET", `/api/lists/${list.id}/activity?limit=2`, undefined, auth);
  assert.deepEqual(
    res.body.map((e: any) => e.detail.title),
    ["t3", "t2"],
  );
});

test("entries written together keep their order", async () => {
  const { auth } = await signUp(app, "carol");
  const list = await makeList(app, auth);
  const tasks = await Promise.all([1, 2, 3].map((i) => makeTask(app, auth, list.id, { title: `t${i}` })));
  const res = await app.call("GET", `/api/lists/${list.id}/activity`, undefined, auth);
  const created = res.body.filter((e: any) => e.action === "task.created").map((e: any) => e.task_id);
  assert.deepEqual(created, tasks.map((t) => t.id).sort((a, b) => b - a));
});
