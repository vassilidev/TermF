import { test } from "node:test";
import assert from "node:assert/strict";
import { expireInvitations, insertUser, publicText, shippedDb, signIn, signUp, start, storedText, tempDb, type App, type Auth, type Reply } from "./lists-helpers.ts";

type User = { id: number; auth: Auth };

async function setUp(path?: string) {
  const app = await start(path);
  const owner = await signUp(app, "olivia");
  const list = (await app.call("POST", "/api/lists", { name: "Family" }, owner.auth)).body;
  const task = (await app.call("POST", `/api/lists/${list.id}/tasks`, { title: "Buy bread" }, owner.auth)).body;
  await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "Wholemeal" }, owner.auth);
  return { app, owner, list, task };
}

type Ctx = Awaited<ReturnType<typeof setUp>>;

async function invite(app: App, owner: User, listId: number, role: unknown, extra: Record<string, unknown> = {}) {
  const res = await app.call("POST", `/api/lists/${listId}/invitations`, { role, ...extra }, owner.auth);
  assert.equal(res.status, 201, `invite as ${role}: ${res.status}`);
  return res.body;
}

const accept = (app: App, token: string, auth: Auth = {}) => app.call("POST", `/api/invitations/${encodeURIComponent(token)}/accept`, undefined, auth);

/** A new account that joins the list of `ctx` with `role`. */
async function join(ctx: Ctx, username: string, role: "editor" | "viewer"): Promise<User> {
  const user = await signUp(ctx.app, username);
  const res = await accept(ctx.app, (await invite(ctx.app, ctx.owner, ctx.list.id, role)).token, user.auth);
  assert.ok(res.status >= 200 && res.status < 300, `accept as ${role}: ${res.status}`);
  return user;
}

const ok = (res: Reply) => res.status >= 200 && res.status < 300;
const denied = (res: Reply) => res.status === 403 || res.status === 404;
const refused = (res: Reply) => res.status >= 400 && res.status < 500;

async function members(ctx: Ctx): Promise<string[]> {
  const res = await ctx.app.call("GET", `/api/lists/${ctx.list.id}/members`, undefined, ctx.owner.auth);
  assert.equal(res.status, 200, `members: ${res.status}`);
  return res.body.map((m: any) => `${m.username}:${m.role}`).sort();
}

async function sharedWith(app: App, auth: Auth, listId: number): Promise<any[]> {
  const res = await app.call("GET", "/api/lists", undefined, auth);
  assert.equal(res.status, 200, `lists: ${res.status}`);
  const lists = Array.isArray(res.body) ? res.body : res.body?.items;
  return lists.filter((l: any) => l.id === listId);
}

test("the owner invites with a link; whoever accepts it becomes a member with its role", async () => {
  const ctx = await setUp();
  const { app, owner, list } = ctx;
  const inv = await invite(app, owner, list.id, "editor");
  const eve = await signUp(app, "eve");
  const accepted = await accept(app, inv.token, eve.auth);
  const evesLists = await sharedWith(app, eve.auth, list.id);
  const evesList = await app.call("GET", `/api/lists/${list.id}`, undefined, eve.auth);
  const ownersLists = await sharedWith(app, owner.auth, list.id);
  const all = await members(ctx);
  const raw = (await app.call("GET", `/api/lists/${list.id}/members`, undefined, owner.auth)).body;
  await app.close();
  for (const key of ["id", "token", "url", "role", "expires_at"]) assert.ok(key in inv, `no ${key} in the invitation`);
  assert.equal(typeof inv.token, "string");
  assert.ok(inv.token.length >= 20, "the token is short");
  assert.ok(inv.url.includes(inv.token), "the url does not carry the token");
  assert.equal(inv.role, "editor");
  const hours = (Date.parse(inv.expires_at) - Date.now()) / 3_600_000;
  assert.ok(hours > 71.5 && hours <= 72.1, `expires in ${hours} hours`);
  assert.ok(ok(accepted), `accept: ${accepted.status}`);
  assert.equal(evesLists.length, 1);
  assert.equal(evesLists[0].role, "editor");
  assert.equal(evesLists[0].name, "Family");
  assert.equal(evesList.status, 200);
  assert.equal(evesList.body.role, "editor");
  assert.equal(ownersLists[0].role, "owner");
  assert.deepEqual(all, ["eve:editor", "olivia:owner"]);
  assert.ok(raw.every((m: any) => typeof m.id === "number"), "a member has no id");
  assert.ok(raw.some((m: any) => m.id === eve.id && m.role === "editor"), "the member's id is not their user id");
});

test("expires_in_hours sets the expiry, up to 720 hours", async () => {
  const { app, owner, list } = await setUp();
  const inv = await invite(app, owner, list.id, "viewer", { expires_in_hours: 5 });
  const hours = (Date.parse(inv.expires_at) - Date.now()) / 3_600_000;
  assert.ok(hours > 4.9 && hours <= 5.01, `expires in ${hours} hours`);
  for (const bad of [0, -1, 721, 100000, "abc"]) {
    const res = await app.call("POST", `/api/lists/${list.id}/invitations`, { role: "viewer", expires_in_hours: bad }, owner.auth);
    if (res.status === 201) {
      const h = (Date.parse(res.body.expires_at) - Date.now()) / 3_600_000;
      assert.ok(h > 0 && h <= 720.01, `expires_in_hours ${bad}: expires in ${h} hours`);
    } else {
      assert.equal(res.status, 400, `expires_in_hours ${bad}: ${res.status}`);
    }
  }
  await app.close();
});

test("an invitation's role is viewer or editor, nothing else", async () => {
  const { app, owner, list } = await setUp();
  for (const role of ["owner", "admin", "", null, 1, undefined, ["editor"]]) {
    const res = await app.call("POST", `/api/lists/${list.id}/invitations`, { role }, owner.auth);
    assert.equal(res.status, 400, `role ${JSON.stringify(role)}: ${res.status}`);
  }
  await app.close();
});

test("an editor changes tasks and comments, but manages neither the list nor its members", async () => {
  const ctx = await setUp();
  const { app, owner, list, task } = ctx;
  const eddie = await join(ctx, "eddie", "editor");
  const vera = await join(ctx, "vera", "viewer");
  const created = await app.call("POST", `/api/lists/${list.id}/tasks`, { title: "Eddie's" }, eddie.auth);
  assert.equal(created.status, 201);
  for (const [method, path, body] of [
    ["GET", `/api/lists/${list.id}/tasks`, undefined],
    ["PATCH", `/api/tasks/${task.id}`, { title: "Buy rye bread", done: true }],
    ["POST", `/api/tasks/${task.id}/comments`, { body: "On it" }],
    ["GET", `/api/lists/${list.id}/activity`, undefined],
    ["DELETE", `/api/tasks/${created.body.id}`, undefined],
  ] as const) {
    const res = await app.call(method, path, body, eddie.auth);
    assert.ok(ok(res), `editor ${method} ${path}: ${res.status}`);
  }
  for (const [method, path, body] of [
    ["PATCH", `/api/lists/${list.id}`, { name: "Eddie's now" }],
    ["DELETE", `/api/lists/${list.id}`, undefined],
    ["POST", `/api/lists/${list.id}/invitations`, { role: "editor" }],
    ["GET", `/api/lists/${list.id}/invitations`, undefined],
    ["DELETE", `/api/lists/${list.id}/members/${vera.id}`, undefined],
    ["DELETE", `/api/lists/${list.id}/members/${owner.id}`, undefined],
  ] as const) {
    const res = await app.call(method, path, body, eddie.auth);
    assert.ok(denied(res) || (path.endsWith(`/members/${owner.id}`) && refused(res)), `editor ${method} ${path}: ${res.status}`);
  }
  const after = await app.call("GET", `/api/lists/${list.id}`, undefined, owner.auth);
  const read = await app.call("GET", `/api/tasks/${task.id}`, undefined, owner.auth);
  const all = await members(ctx);
  await app.close();
  assert.equal(after.body.name, "Family");
  assert.equal(read.body.title, "Buy rye bread");
  assert.equal(read.body.comment_count, 2);
  assert.deepEqual(all, ["eddie:editor", "olivia:owner", "vera:viewer"]);
});

test("a viewer reads everything and changes nothing", async () => {
  const ctx = await setUp();
  const { app, owner, list, task } = ctx;
  const vera = await join(ctx, "vera", "viewer");
  for (const path of [`/api/lists/${list.id}`, `/api/lists/${list.id}/tasks`, `/api/tasks/${task.id}`, `/api/tasks/${task.id}/comments`, `/api/lists/${list.id}/activity`]) {
    const res = await app.call("GET", path, undefined, vera.auth);
    assert.equal(res.status, 200, `viewer GET ${path}: ${res.status}`);
  }
  for (const [method, path, body] of [
    ["POST", `/api/lists/${list.id}/tasks`, { title: "Vera's" }],
    ["PATCH", `/api/tasks/${task.id}`, { title: "Changed", done: true }],
    ["PATCH", `/api/tasks/${task.id}`, { assignee_id: vera.id }],
    ["DELETE", `/api/tasks/${task.id}`, undefined],
    ["POST", `/api/tasks/${task.id}/comments`, { body: "Hi" }],
    ["PATCH", `/api/lists/${list.id}`, { name: "Vera's now" }],
    ["DELETE", `/api/lists/${list.id}`, undefined],
    ["POST", `/api/lists/${list.id}/invitations`, { role: "viewer" }],
    ["DELETE", `/api/lists/${list.id}/members/${owner.id}`, undefined],
  ] as const) {
    const res = await app.call(method, path, body, vera.auth);
    assert.ok(denied(res) || (path.endsWith(`/members/${owner.id}`) && refused(res)), `viewer ${method} ${path}: ${res.status}`);
  }
  const read = await app.call("GET", `/api/tasks/${task.id}`, undefined, owner.auth);
  const tasks = await app.call("GET", `/api/lists/${list.id}/tasks`, undefined, owner.auth);
  const after = await app.call("GET", `/api/lists/${list.id}`, undefined, owner.auth);
  await app.close();
  assert.equal(read.body.title, "Buy bread");
  assert.equal(read.body.done, false);
  assert.equal(read.body.assignee, null);
  assert.equal(read.body.comment_count, 1);
  assert.equal((Array.isArray(tasks.body) ? tasks.body : tasks.body.items).length, 1);
  assert.equal(after.body.name, "Family");
});

test("someone who is not a member gets a 404 on every route of the list, even through task ids", async () => {
  const ctx = await setUp();
  const { app, owner, list, task } = ctx;
  const work = (await app.call("POST", "/api/lists", { name: "Work" }, owner.auth)).body;
  const sam = await signUp(app, "sam");
  const workInvite = await invite(app, owner, work.id, "editor");
  assert.ok(ok(await accept(app, workInvite.token, sam.auth)), "sam joins Work");
  for (const [method, path, body] of [
    ["GET", `/api/lists/${list.id}`, undefined],
    ["PATCH", `/api/lists/${list.id}`, { name: "Sam's" }],
    ["DELETE", `/api/lists/${list.id}`, undefined],
    ["GET", `/api/lists/${list.id}/tasks`, undefined],
    ["POST", `/api/lists/${list.id}/tasks`, { title: "Sam's" }],
    ["GET", `/api/lists/${list.id}/activity`, undefined],
    ["GET", `/api/lists/${list.id}/tags`, undefined],
    ["GET", `/api/lists/${list.id}/members`, undefined],
    ["GET", `/api/lists/${list.id}/invitations`, undefined],
    ["POST", `/api/lists/${list.id}/invitations`, { role: "editor" }],
    ["DELETE", `/api/lists/${list.id}/members/${owner.id}`, undefined],
    ["GET", `/api/tasks/${task.id}`, undefined],
    ["PATCH", `/api/tasks/${task.id}`, { title: "Sam's" }],
    ["DELETE", `/api/tasks/${task.id}`, undefined],
    ["GET", `/api/tasks/${task.id}/comments`, undefined],
    ["POST", `/api/tasks/${task.id}/comments`, { body: "Hi" }],
  ] as const) {
    const res = await app.call(method, path, body, sam.auth);
    assert.equal(res.status, 404, `non-member ${method} ${path}: ${res.status}`);
  }
  const visible = await sharedWith(app, sam.auth, list.id);
  const read = await app.call("GET", `/api/tasks/${task.id}`, undefined, owner.auth);
  await app.close();
  assert.equal(visible.length, 0);
  assert.equal(read.body.title, "Buy bread");
});

test("an expired invitation is refused", async () => {
  const path = tempDb();
  const ctx = await setUp(path);
  const { app, owner, list } = ctx;
  const inv = await invite(app, owner, list.id, "editor");
  assert.ok(expireInvitations(path) > 0, "no expiry column found");
  const eve = await signUp(app, "eve");
  const res = await accept(app, inv.token, eve.auth);
  const read = await app.call("GET", `/api/lists/${list.id}`, undefined, eve.auth);
  const all = await members(ctx);
  await app.close();
  assert.ok(refused(res), `accept: ${res.status}`);
  assert.equal(read.status, 404);
  assert.deepEqual(all, ["olivia:owner"]);
});

test("an unknown or revoked invitation is refused, and accepting needs to be signed in", async () => {
  const ctx = await setUp();
  const { app, owner, list } = ctx;
  const eve = await signUp(app, "eve");
  const live = await invite(app, owner, list.id, "viewer");
  for (const token of ["nope", "x".repeat(64), `${live.token}x`, "%00"]) {
    const res = await accept(app, token, eve.auth);
    assert.ok(refused(res), `token ${token.slice(0, 12)}: ${res.status}`);
  }
  const anonymous = await accept(app, live.token);
  assert.equal(anonymous.status, 401);
  const revoked = await invite(app, owner, list.id, "editor");
  const before = await app.call("GET", `/api/lists/${list.id}/invitations`, undefined, owner.auth);
  const del = await app.call("DELETE", `/api/lists/${list.id}/invitations/${revoked.id}`, undefined, owner.auth);
  const after = await app.call("GET", `/api/lists/${list.id}/invitations`, undefined, owner.auth);
  const res = await accept(app, revoked.token, eve.auth);
  const read = await app.call("GET", `/api/lists/${list.id}`, undefined, eve.auth);
  const all = await members(ctx);
  await app.close();
  assert.equal(before.status, 200);
  assert.ok(before.body.some((i: any) => i.id === revoked.id), "the pending invitation is not listed");
  assert.equal(del.status, 204);
  assert.ok(!after.body.some((i: any) => i.id === revoked.id), "the revoked invitation is still listed");
  assert.ok(refused(res), `accept a revoked invitation: ${res.status}`);
  assert.equal(read.status, 404);
  assert.deepEqual(all, ["olivia:owner"]);
});

test("accepting again changes nothing: no second membership, no other role", async () => {
  const ctx = await setUp();
  const { app, owner, list } = ctx;
  const editorLink = await invite(app, owner, list.id, "editor");
  const viewerLink = await invite(app, owner, list.id, "viewer");
  const eve = await signUp(app, "eve");
  assert.ok(ok(await accept(app, editorLink.token, eve.auth)), "first accept");
  const replies = [
    await accept(app, editorLink.token, eve.auth),
    await accept(app, viewerLink.token, eve.auth),
    await accept(app, viewerLink.token, owner.auth),
    await accept(app, editorLink.token, owner.auth),
  ];
  const all = await members(ctx);
  const evesLists = await sharedWith(app, eve.auth, list.id);
  const ownersLists = await sharedWith(app, owner.auth, list.id);
  const rename = await app.call("PATCH", `/api/lists/${list.id}`, { name: "Still mine" }, owner.auth);
  await app.close();
  for (const res of replies) assert.ok(res.status < 500, `accept again: ${res.status}`);
  assert.deepEqual(all, ["eve:editor", "olivia:owner"]);
  assert.equal(evesLists.length, 1);
  assert.equal(evesLists[0].role, "editor");
  assert.equal(ownersLists.length, 1);
  assert.equal(ownersLists[0].role, "owner");
  assert.equal(rename.status, 200);
});

test("invitation tokens are not stored in clear, nor answered again", async () => {
  const path = tempDb();
  const ctx = await setUp(path);
  const { app, owner, list } = ctx;
  const inv = await invite(app, owner, list.id, "editor");
  const pending = await app.call("GET", `/api/lists/${list.id}/invitations`, undefined, owner.auth);
  await app.close();
  assert.equal(pending.status, 200);
  assert.ok(!pending.text.includes(inv.token), "the pending invitations answer the token");
  assert.ok(!storedText(path).includes(inv.token), "the token is stored in clear");
});

test("removing a member cuts their access at once", async () => {
  const ctx = await setUp();
  const { app, owner, list, task } = ctx;
  const eve = await join(ctx, "eve", "editor");
  assert.equal((await app.call("GET", `/api/tasks/${task.id}`, undefined, eve.auth)).status, 200);
  const removed = await app.call("DELETE", `/api/lists/${list.id}/members/${eve.id}`, undefined, owner.auth);
  assert.equal(removed.status, 204);
  for (const [method, path, body] of [
    ["GET", `/api/lists/${list.id}`, undefined],
    ["GET", `/api/lists/${list.id}/tasks`, undefined],
    ["GET", `/api/tasks/${task.id}`, undefined],
    ["PATCH", `/api/tasks/${task.id}`, { title: "Too late" }],
    ["POST", `/api/tasks/${task.id}/comments`, { body: "Too late" }],
    ["GET", `/api/lists/${list.id}/activity`, undefined],
  ] as const) {
    const res = await app.call(method, path, body, eve.auth);
    assert.ok(denied(res), `removed member ${method} ${path}: ${res.status}`);
  }
  const visible = await sharedWith(app, eve.auth, list.id);
  const all = await members(ctx);
  await app.close();
  assert.equal(visible.length, 0);
  assert.deepEqual(all, ["olivia:owner"]);
});

test("the owner cannot be removed", async () => {
  const ctx = await setUp();
  const { app, owner, list } = ctx;
  await join(ctx, "eve", "editor");
  const res = await app.call("DELETE", `/api/lists/${list.id}/members/${owner.id}`, undefined, owner.auth);
  const read = await app.call("GET", `/api/lists/${list.id}`, undefined, owner.auth);
  const rename = await app.call("PATCH", `/api/lists/${list.id}`, { name: "Renamed" }, owner.auth);
  const all = await members(ctx);
  await app.close();
  assert.ok(refused(res), `remove the owner: ${res.status}`);
  assert.equal(read.status, 200);
  assert.equal(read.body.role, "owner");
  assert.equal(rename.status, 200);
  assert.deepEqual(all, ["eve:editor", "olivia:owner"]);
});

test("a task can be assigned to a member of its list, not to anyone else", async () => {
  const ctx = await setUp();
  const { app, owner, task } = ctx;
  const eve = await join(ctx, "eve", "editor");
  const vera = await join(ctx, "vera", "viewer");
  const sam = await signUp(app, "sam");
  const toVera = await app.call("PATCH", `/api/tasks/${task.id}`, { assignee_id: vera.id }, owner.auth);
  const bySelf = await app.call("PATCH", `/api/tasks/${task.id}`, { assignee_id: eve.id }, eve.auth);
  const toSam = await app.call("PATCH", `/api/tasks/${task.id}`, { assignee_id: sam.id }, owner.auth);
  const read = await app.call("GET", `/api/tasks/${task.id}`, undefined, owner.auth);
  await app.close();
  assert.equal(toVera.status, 200);
  assert.equal(toVera.body.assignee?.id, vera.id);
  assert.equal(bySelf.status, 200);
  assert.equal(bySelf.body.assignee?.id, eve.id);
  assert.equal(toSam.status, 400);
  assert.equal(read.body.assignee?.id, eve.id);
});

test("lists made before sharing still belong to their owners", async () => {
  let listId = 0;
  const path = shippedDb((db) => {
    const owner = insertUser(db, "olivia");
    insertUser(db, "eve");
    listId = Number(db.prepare("INSERT INTO lists (owner_id, name) VALUES (?, 'Old')").run(owner).lastInsertRowid);
    db.prepare("INSERT INTO tasks (list_id, title, position, created_by, updated_at) VALUES (?, 'Old task', 0, ?, '2026-01-01T00:00:00.000Z')").run(listId, owner);
  });
  const app = await start(path);
  const olivia = await signIn(app, "olivia");
  const eve = await signIn(app, "eve");
  const mine = await sharedWith(app, olivia, listId);
  const tasks = await app.call("GET", `/api/lists/${listId}/tasks`, undefined, olivia);
  const inv = await app.call("POST", `/api/lists/${listId}/invitations`, { role: "viewer" }, olivia);
  const outsider = await app.call("GET", `/api/lists/${listId}`, undefined, eve);
  const joined = inv.status === 201 ? await accept(app, inv.body.token, eve) : undefined;
  const all = await app.call("GET", `/api/lists/${listId}/members`, undefined, olivia);
  await app.close();
  assert.equal(mine.length, 1);
  assert.equal(mine[0].role, "owner");
  assert.equal(tasks.status, 200);
  assert.equal(inv.status, 201);
  assert.equal(outsider.status, 404);
  assert.ok(joined && ok(joined), `accept: ${joined?.status}`);
  assert.deepEqual(all.body.map((m: any) => `${m.username}:${m.role}`).sort(), ["eve:viewer", "olivia:owner"]);
});

test("the page shows the sharing: members, invitations, accepting a link", () => {
  const page = publicText();
  assert.ok(page.includes("/members"), "the page never calls /members");
  assert.ok(page.includes("/invitations"), "the page never calls /invitations");
  assert.ok(/accept/.test(page), "the page never accepts an invitation");
});
