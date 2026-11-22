import { test } from "node:test";
import assert from "node:assert/strict";
import { legacyDb, page, sql, start, storedText, tempDb } from "./helpers.ts";

const AUTH = { error: "authentication required" };
const NOT_FOUND = { error: "task not found" };

type App = Awaited<ReturnType<typeof start>>;

async function signUp(app: App, username: string, password = "correct horse") {
  const created = await app.call("POST", "/api/users", { username, password });
  assert.equal(created.status, 201, `sign up ${username}: ${created.text}`);
  const session = await app.call("POST", "/api/sessions", { username, password });
  assert.equal(session.status, 201, `sign in ${username}: ${session.text}`);
  return { Authorization: `Bearer ${session.body.token}` };
}

test("AC1: sign-up answers the id and the lowercased username, nothing else", async () => {
  const app = await start();
  const res = await app.call("POST", "/api/users", { username: "Alice_1", password: "0123456789" });
  await app.close();
  assert.equal(res.status, 201);
  assert.deepEqual(Object.keys(res.body).sort(), ["id", "username"]);
  assert.equal(typeof res.body.id, "number");
  assert.equal(res.body.username, "alice_1");
});

test("AC1: invalid usernames and short passwords are refused", async () => {
  const app = await start();
  for (const username of ["ab", "a".repeat(33), "bad name", "bad-name", "", 42, undefined]) {
    const res = await app.call("POST", "/api/users", { username, password: "long enough pass" });
    assert.equal(res.status, 400, `username ${username}`);
    assert.deepEqual(res.body, { error: "invalid username" });
  }
  const short = await app.call("POST", "/api/users", { username: "bob", password: "123456789" });
  const exact = await app.call("POST", "/api/users", { username: "bob", password: "1234567890" });
  await app.close();
  assert.equal(short.status, 400);
  assert.deepEqual(short.body, { error: "password too short" });
  assert.equal(exact.status, 201);
});

test("AC1: a username is taken whatever its case", async () => {
  const app = await start();
  await app.call("POST", "/api/users", { username: "carol", password: "long enough pass" });
  const again = await app.call("POST", "/api/users", { username: "CAROL", password: "another long one" });
  await app.close();
  assert.equal(again.status, 409);
  assert.deepEqual(again.body, { error: "username taken" });
});

test("AC2: sign-in gives a token; a wrong password or user is the same 401", async () => {
  const app = await start();
  await app.call("POST", "/api/users", { username: "dave", password: "long enough pass" });
  const ok = await app.call("POST", "/api/sessions", { username: "DAVE", password: "long enough pass" });
  const wrong = await app.call("POST", "/api/sessions", { username: "dave", password: "long enough pasS" });
  const nobody = await app.call("POST", "/api/sessions", { username: "erin", password: "long enough pass" });
  await app.close();
  assert.equal(ok.status, 201);
  assert.equal(typeof ok.body.token, "string");
  assert.ok(ok.body.token.length >= 32);
  assert.equal(wrong.status, 401);
  assert.deepEqual(wrong.body, { error: "invalid credentials" });
  assert.deepEqual(nobody.body, wrong.body);
  assert.equal(nobody.status, 401);
});

test("AC3: every task route needs a valid bearer token", async () => {
  const app = await start();
  const auth = await signUp(app, "frank");
  const { id } = (await app.call("POST", "/api/tasks", { title: "Mine" }, auth)).body;
  const token = auth.Authorization.slice("Bearer ".length);
  const headers = [{}, { Authorization: token }, { Authorization: "Bearer" }, { Authorization: "Bearer nope" }, { Authorization: `Basic ${token}` }];
  for (const h of headers) {
    for (const [method, path, body] of [
      ["GET", "/api/tasks", undefined],
      ["POST", "/api/tasks", { title: "x" }],
      ["GET", `/api/tasks/${id}`, undefined],
      ["PATCH", `/api/tasks/${id}`, { title: "y" }],
      ["DELETE", `/api/tasks/${id}`, undefined],
    ] as const) {
      const res = await app.call(method, path, body, h);
      assert.equal(res.status, 401, `${method} ${path} with ${JSON.stringify(h)}`);
      assert.deepEqual(res.body, AUTH);
    }
  }
  const still = await app.call("GET", `/api/tasks/${id}`, undefined, auth);
  await app.close();
  assert.equal(still.body.title, "Mine");
});

test("AC4: a user never sees nor changes another user's task", async () => {
  const app = await start();
  const grace = await signUp(app, "grace");
  const heidi = await signUp(app, "heidi");
  const task = (await app.call("POST", "/api/tasks", { title: "Grace's" }, grace)).body;
  await app.call("POST", "/api/tasks", { title: "Heidi's" }, heidi);
  const list = await app.call("GET", "/api/tasks", undefined, heidi);
  const read = await app.call("GET", `/api/tasks/${task.id}`, undefined, heidi);
  const patch = await app.call("PATCH", `/api/tasks/${task.id}`, { title: "Stolen", done: true }, heidi);
  const del = await app.call("DELETE", `/api/tasks/${task.id}`, undefined, heidi);
  const own = await app.call("GET", `/api/tasks/${task.id}`, undefined, grace);
  const ownList = await app.call("GET", "/api/tasks", undefined, grace);
  await app.close();
  assert.deepEqual(list.body.map((t: any) => t.title), ["Heidi's"]);
  for (const res of [read, patch, del]) {
    assert.equal(res.status, 404);
    assert.deepEqual(res.body, NOT_FOUND);
  }
  assert.equal(own.body.title, "Grace's");
  assert.equal(own.body.done, false);
  assert.deepEqual(ownList.body.map((t: any) => t.title), ["Grace's"]);
});

test("AC5: signing out ends the token", async () => {
  const app = await start();
  const auth = await signUp(app, "ivan");
  const out = await app.call("DELETE", "/api/sessions", undefined, auth);
  const after = await app.call("GET", "/api/tasks", undefined, auth);
  await app.close();
  assert.equal(out.status, 204);
  assert.equal(after.status, 401);
  assert.deepEqual(after.body, AUTH);
});

test("AC6: neither the password nor the token is stored in clear", async () => {
  const path = tempDb();
  const app = await start(path);
  const password = "s3cret-Pa55word!";
  await app.call("POST", "/api/users", { username: "judy", password });
  const session = await app.call("POST", "/api/sessions", { username: "judy", password });
  await app.close();
  assert.equal(session.status, 201);
  const stored = storedText(path);
  assert.ok(!stored.includes(password), "the password is stored in clear");
  assert.ok(!stored.includes(session.body.token), "the token is stored in clear");
});

test("AC6: no response carries a password or a hash", async () => {
  const app = await start();
  const password = "another-s3cret!";
  const created = await app.call("POST", "/api/users", { username: "ken", password });
  const session = await app.call("POST", "/api/sessions", { username: "ken", password });
  const auth = { Authorization: `Bearer ${session.body.token}` };
  const task = await app.call("POST", "/api/tasks", { title: "Hi" }, auth);
  const list = await app.call("GET", "/api/tasks", undefined, auth);
  await app.close();
  for (const res of [created, session, task, list]) {
    assert.ok(!res.text.includes(password));
    assert.doesNotMatch(res.text, /password|hash|salt/i);
  }
  assert.deepEqual(Object.keys(session.body), ["token"]);
});

test("AC7: tasks from before accounts stay stored but nobody sees them", async () => {
  const path = legacyDb([
    { title: "Legacy one", done: false, created_at: "2025-01-01T00:00:00.000Z" },
    { title: "Legacy two", done: true, created_at: "2025-01-02T00:00:00.000Z" },
  ]);
  const app = await start(path);
  const auth = await signUp(app, "leo");
  const list = await app.call("GET", "/api/tasks", undefined, auth);
  const read = await app.call("GET", "/api/tasks/1", undefined, auth);
  await app.close();
  assert.deepEqual(list.body, []);
  assert.equal(read.status, 404);
  const titles = sql(path, "SELECT title FROM tasks ORDER BY id").map((r) => r.title);
  assert.deepEqual(titles.slice(0, 2), ["Legacy one", "Legacy two"]);
});

test("AC8: the page signs in and sends the token", () => {
  const html = page();
  assert.match(html, /Authorization/);
  assert.match(html, /Bearer/);
  assert.match(html, /\/api\/sessions/);
  assert.match(html, /\/api\/users/);
});
