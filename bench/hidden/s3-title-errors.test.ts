import { test } from "node:test";
import assert from "node:assert/strict";
import { start } from "./helpers.ts";

const REQUIRED = { error: "title is required" };

test("AC1: a missing title is a 400 and creates nothing", async () => {
  const app = await start();
  const res = await app.call("POST", "/api/tasks", {});
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(res.status, 400);
  assert.deepEqual(res.body, REQUIRED);
  assert.equal(list.body.length, 0);
});

test("AC1: an empty or blank title is a 400 and creates nothing", async () => {
  const app = await start();
  for (const title of ["", "   ", "\t\n"]) {
    const res = await app.call("POST", "/api/tasks", { title });
    assert.equal(res.status, 400, JSON.stringify(title));
    assert.deepEqual(res.body, REQUIRED);
  }
  const list = await app.call("GET", "/api/tasks");
  await app.close();
  assert.equal(list.body.length, 0);
});

test("AC1: a title that is not a string is a 400", async () => {
  const app = await start();
  for (const title of [42, null, ["a"], { a: 1 }, true]) {
    const res = await app.call("POST", "/api/tasks", { title });
    assert.equal(res.status, 400, JSON.stringify(title));
    assert.deepEqual(res.body, REQUIRED);
  }
  await app.close();
});

test("AC2: PATCH with a blank title is a 400 and changes nothing", async () => {
  const app = await start();
  const { id } = (await app.call("POST", "/api/tasks", { title: "Keep me" })).body;
  const res = await app.call("PATCH", `/api/tasks/${id}`, { title: "  ", done: true });
  const read = await app.call("GET", `/api/tasks/${id}`);
  await app.close();
  assert.equal(res.status, 400);
  assert.deepEqual(res.body, REQUIRED);
  assert.equal(read.body.title, "Keep me");
  assert.equal(read.body.done, false);
});

test("AC2: PATCH without a title still works", async () => {
  const app = await start();
  const { id } = (await app.call("POST", "/api/tasks", { title: "Toggle me" })).body;
  const res = await app.call("PATCH", `/api/tasks/${id}`, { done: true });
  await app.close();
  assert.equal(res.status, 200);
  assert.equal(res.body.done, true);
  assert.equal(res.body.title, "Toggle me");
});

test("AC3: 201 characters is a 400, 200 is fine", async () => {
  const app = await start();
  const long = await app.call("POST", "/api/tasks", { title: "a".repeat(201) });
  const max = await app.call("POST", "/api/tasks", { title: "a".repeat(200) });
  const padded = await app.call("POST", "/api/tasks", { title: `  ${"b".repeat(200)}  ` });
  await app.close();
  assert.equal(long.status, 400);
  assert.deepEqual(long.body, { error: "title must be at most 200 characters" });
  assert.equal(max.status, 201);
  assert.equal(padded.status, 201, "the limit counts after trimming");
});

test("AC3: titles are saved trimmed", async () => {
  const app = await start();
  const created = await app.call("POST", "/api/tasks", { title: "  Buy milk \n" });
  const { id } = created.body;
  const patched = await app.call("PATCH", `/api/tasks/${id}`, { title: " Buy oat milk " });
  await app.close();
  assert.equal(created.body.title, "Buy milk");
  assert.equal(patched.body.title, "Buy oat milk");
});

test("AC4: a body that is not JSON is a 400, on POST and PATCH", async () => {
  const app = await start();
  const post = await app.call("POST", "/api/tasks", "{title: oops");
  const { id } = (await app.call("POST", "/api/tasks", { title: "Fine" })).body;
  const patch = await app.call("PATCH", `/api/tasks/${id}`, "not json");
  await app.close();
  assert.equal(post.status, 400);
  assert.deepEqual(post.body, { error: "invalid JSON body" });
  assert.equal(patch.status, 400);
  assert.deepEqual(patch.body, { error: "invalid JSON body" });
});

test("AC5: an odd body never leaks an internal message", async () => {
  const app = await start();
  for (const body of ["null", "[1, 2]", "\"text\"", "7"]) {
    const res = await app.call("POST", "/api/tasks", body);
    assert.ok([400, 500].includes(res.status), `${body}: ${res.status}`);
    if (res.status === 500) assert.deepEqual(res.body, { error: "internal error" });
    assert.doesNotMatch(res.text, /Cannot read|TypeError|SQLite|undefined/i, body);
  }
  await app.close();
});
