import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { makeList, makeTask, signUp, start, type App } from "./helpers.ts";

let app: App;
before(async () => (app = await start()));
after(() => app.close());

test("adds comments and lists them oldest first, with their author", async () => {
  const { auth, user } = await signUp(app, "alice");
  const list = await makeList(app, auth);
  const task = await makeTask(app, auth, list.id);
  const first = await app.call("POST", `/api/tasks/${task.id}/comments`, { body: " Done by Friday? " }, auth);
  assert.equal(first.status, 201);
  assert.equal(first.body.body, "Done by Friday?");
  assert.deepEqual(first.body.author, { id: user.id, username: "alice" });
  await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "Yes" }, auth);
  const comments = await app.call("GET", `/api/tasks/${task.id}/comments`, undefined, auth);
  assert.deepEqual(
    comments.body.map((c: any) => c.body),
    ["Done by Friday?", "Yes"],
  );
});

test("a comment needs a body of at most 2000 characters", async () => {
  const { auth } = await signUp(app, "bob");
  const list = await makeList(app, auth);
  const task = await makeTask(app, auth, list.id);
  for (const body of [{}, { body: "  " }, { body: 3 }]) {
    const res = await app.call("POST", `/api/tasks/${task.id}/comments`, body, auth);
    assert.equal(res.status, 400);
    assert.deepEqual(res.body, { error: "Comment body can't be empty." });
  }
  const long = await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "x".repeat(2001) }, auth);
  assert.equal(long.status, 400);
});

test("comments go away with their task", async () => {
  const { auth } = await signUp(app, "carol");
  const list = await makeList(app, auth);
  const task = await makeTask(app, auth, list.id);
  await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "Soon gone" }, auth);
  await app.call("DELETE", `/api/tasks/${task.id}`, undefined, auth);
  assert.equal((await app.call("GET", `/api/tasks/${task.id}/comments`, undefined, auth)).status, 404);
});
