import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "../src/app.ts";

let server: any;
let base = "";

before(async () => {
  server = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  base = `http://127.0.0.1:${server.address().port}`;
});

after(() => server.close());

async function call(method: string, path: string, body?: unknown) {
  const res = await fetch(base + path, {
    method,
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  return { status: res.status, body: res.status === 204 ? null : await res.json() };
}

test("creates a task and lists it", async () => {
  const created = await call("POST", "/api/tasks", { title: "Write the report" });
  assert.equal(created.status, 201);
  assert.equal(created.body.title, "Write the report");
  assert.equal(created.body.done, false);
  const list = await call("GET", "/api/tasks");
  assert.ok(list.body.some((t: any) => t.id === created.body.id));
});

test("reads a task by id", async () => {
  const created = await call("POST", "/api/tasks", { title: "Call the plumber" });
  const read = await call("GET", `/api/tasks/${created.body.id}`);
  assert.equal(read.status, 200);
  assert.deepEqual(read.body, created.body);
});

test("updates the title and marks a task done", async () => {
  const created = await call("POST", "/api/tasks", { title: "Buy milk" });
  const updated = await call("PATCH", `/api/tasks/${created.body.id}`, { title: "Buy oat milk", done: true });
  assert.equal(updated.status, 200);
  assert.equal(updated.body.title, "Buy oat milk");
  assert.equal(updated.body.done, true);
});

test("deletes a task", async () => {
  const created = await call("POST", "/api/tasks", { title: "Old task" });
  assert.equal((await call("DELETE", `/api/tasks/${created.body.id}`)).status, 204);
  assert.equal((await call("GET", `/api/tasks/${created.body.id}`)).status, 404);
});

test("an unknown task is a 404", async () => {
  const res = await call("GET", "/api/tasks/9999");
  assert.equal(res.status, 404);
  assert.deepEqual(res.body, { error: "task not found" });
});

test("lists only the done tasks with ?done=true", async () => {
  const task = await call("POST", "/api/tasks", { title: "Finished" });
  await call("PATCH", `/api/tasks/${task.body.id}`, { done: true });
  const list = await call("GET", "/api/tasks?done=true");
  assert.ok(list.body.length > 0);
  assert.ok(list.body.every((t: any) => t.done));
});

test("serves the page", async () => {
  const res = await fetch(base + "/");
  assert.equal(res.status, 200);
  assert.match(await res.text(), /<h1>Tasks<\/h1>/);
});
