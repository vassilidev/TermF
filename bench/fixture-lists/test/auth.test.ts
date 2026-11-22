import { after, before, test } from "node:test";
import assert from "node:assert/strict";
import { PASSWORD, signUp, start, type App } from "./helpers.ts";

let app: App;
before(async () => (app = await start()));
after(() => app.close());

test("signing up answers the account, without its password", async () => {
  const res = await app.call("POST", "/api/users", { username: "Alice_1", password: PASSWORD, email: "alice@example.com" });
  assert.equal(res.status, 201);
  assert.equal(typeof res.body.id, "number");
  assert.equal(res.body.username, "alice_1");
  assert.equal(res.body.email, "alice@example.com");
  assert.ok(!JSON.stringify(res.body).includes(PASSWORD));
});

test("invalid usernames, short passwords and bad emails are refused", async () => {
  for (const username of ["ab", "a".repeat(33), "bad name", "bad-name", 42, undefined]) {
    const res = await app.call("POST", "/api/users", { username, password: PASSWORD });
    assert.equal(res.status, 400);
    assert.deepEqual(res.body, { error: "invalid username" });
  }
  const short = await app.call("POST", "/api/users", { username: "shorty", password: "123456789" });
  assert.deepEqual(short.body, { error: "password too short" });
  const email = await app.call("POST", "/api/users", { username: "mailer", password: PASSWORD, email: "nope" });
  assert.deepEqual(email.body, { error: "invalid email" });
});

test("a username is taken whatever its case", async () => {
  await app.call("POST", "/api/users", { username: "carol", password: PASSWORD });
  const again = await app.call("POST", "/api/users", { username: "CAROL", password: PASSWORD });
  assert.equal(again.status, 409);
  assert.deepEqual(again.body, { error: "username taken" });
});

test("signing in gives a token; a wrong password or user is the same 401", async () => {
  await app.call("POST", "/api/users", { username: "dave", password: PASSWORD });
  const ok = await app.call("POST", "/api/sessions", { username: "Dave", password: PASSWORD });
  assert.equal(ok.status, 201);
  assert.ok(ok.body.token.length >= 32);
  assert.equal(ok.body.user.username, "dave");
  const wrong = await app.call("POST", "/api/sessions", { username: "dave", password: "not the password" });
  const nobody = await app.call("POST", "/api/sessions", { username: "nobody", password: PASSWORD });
  assert.equal(wrong.status, 401);
  assert.deepEqual(wrong.body, { error: "invalid credentials" });
  assert.deepEqual(nobody, wrong);
});

test("the API needs a valid bearer token", async () => {
  const { auth } = await signUp(app, "erin");
  const token = auth.Authorization.slice("Bearer ".length);
  const attempts: Record<string, string>[] = [{}, { Authorization: token }, { Authorization: "Bearer nope" }, { Authorization: `Basic ${token}` }];
  for (const headers of attempts) {
    for (const path of ["/api/me", "/api/lists"]) {
      const res = await app.call("GET", path, undefined, headers);
      assert.equal(res.status, 401, `${path} with ${JSON.stringify(headers)}`);
      assert.deepEqual(res.body, { error: "authentication required" });
    }
  }
});

test("GET /api/me answers the signed-in user", async () => {
  const { user, auth } = await signUp(app, "frank", "frank@example.com");
  const me = await app.call("GET", "/api/me", undefined, auth);
  assert.equal(me.status, 200);
  assert.deepEqual(me.body, user);
});

test("signing out ends the token", async () => {
  const { auth } = await signUp(app, "grace");
  assert.equal((await app.call("DELETE", "/api/sessions", undefined, auth)).status, 204);
  assert.equal((await app.call("GET", "/api/me", undefined, auth)).status, 401);
});
