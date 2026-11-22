import { test } from "node:test";
import assert from "node:assert/strict";
import { insertUser, shippedDb, signIn, signUp, start, type App, type Auth } from "./lists-helpers.ts";

async function setUp(path?: string) {
  const app = await start(path);
  const alice = await signUp(app, "alice");
  const list = (await app.call("POST", "/api/lists", { name: "Home" }, alice.auth)).body;
  // Opens the connections first: otherwise their set-up spreads a burst of requests out and hides the race.
  await Promise.all(range(25).map(() => app.call("GET", "/api/me", undefined, alice.auth)));
  return { app, alice, list, ...client(app, alice.auth, list.id) };
}

function client(app: App, auth: Auth, listId: number) {
  return {
    make: async (fields: Record<string, unknown> = {}) => {
      const res = await app.call("POST", `/api/lists/${listId}/tasks`, { title: "Task", ...fields }, auth);
      assert.equal(res.status, 201, `create: ${res.status}`);
      return res.body;
    },
    get: async (id: number) => (await app.call("GET", `/api/tasks/${id}`, undefined, auth)).body,
    patch: (id: number, body: Record<string, unknown>) => app.call("PATCH", `/api/tasks/${id}`, body, auth),
    /** How many `task.updated` entries the activity log holds for this task. */
    updates: async (taskId: number) => {
      const res = await app.call("GET", `/api/lists/${listId}/activity?limit=200`, undefined, auth);
      assert.equal(res.status, 200, `activity: ${res.status}`);
      return res.body.filter((e: any) => e.action === "task.updated" && e.task_id === taskId).length;
    },
  };
}

const range = (n: number) => Array.from({ length: n }, (_, i) => i);

test("a new task has version 1, and every task the API answers carries its version", async () => {
  const { app, alice, list, make, get } = await setUp();
  const task = await make();
  const read = await get(task.id);
  const listed = await app.call("GET", `/api/lists/${list.id}/tasks`, undefined, alice.auth);
  await app.close();
  assert.equal(task.version, 1);
  assert.equal(read.version, 1);
  const items = Array.isArray(listed.body) ? listed.body : listed.body?.items;
  assert.equal(items?.find((t: any) => t.id === task.id)?.version, 1);
});

test("every change adds exactly one to the version", async () => {
  const { app, make, get, patch } = await setUp();
  const task = await make({ title: "Draft" });
  const versions = [];
  for (const body of [{ title: "Final" }, { done: true }, { priority: 2, due_date: "2026-06-01" }, { tags: ["home"] }, { notes: "Before noon" }]) {
    const res = await patch(task.id, body);
    assert.equal(res.status, 200, `${JSON.stringify(body)}: ${res.status}`);
    versions.push(res.body.version);
  }
  const read = await get(task.id);
  await app.close();
  assert.deepEqual(versions, [2, 3, 4, 5, 6]);
  assert.equal(read.version, 6);
});

test("a PATCH with the version it read applies; with a stale one it is a 409 with the current task and changes nothing", async () => {
  const { app, make, get, patch } = await setUp();
  const task = await make({ title: "Draft" });
  const fresh = await patch(task.id, { title: "Final", version: 1 });
  const stale = await patch(task.id, { done: true, version: 1 });
  const after = await get(task.id);
  const retried = await patch(task.id, { done: true, version: stale.body?.task?.version });
  await app.close();
  assert.equal(fresh.status, 200);
  assert.equal(fresh.body.version, 2);
  assert.equal(stale.status, 409);
  assert.equal(stale.body.error, "version conflict");
  assert.equal(stale.body.task.id, task.id);
  assert.equal(stale.body.task.version, 2);
  assert.equal(stale.body.task.title, "Final");
  assert.equal(stale.body.task.done, false);
  assert.equal(after.version, 2);
  assert.equal(after.done, false);
  assert.equal(retried.status, 200);
  assert.equal(retried.body.version, 3);
  assert.equal(retried.body.done, true);
});

test("concurrent PATCHes of different fields, without a version, all apply", async () => {
  const { app, alice, make, get, patch, updates } = await setUp();
  const task = await make({ title: "Start" });
  for (let round = 1; round <= 3; round++) {
    const want: Record<string, unknown> = {
      title: `Round ${round}`,
      done: round % 2 === 1,
      priority: round,
      due_date: `2026-07-0${round}`,
      assignee_id: round % 2 === 1 ? alice.id : null,
      tags: [`r${round}`],
      notes: `Note ${round}`,
    };
    const replies = await Promise.all(Object.entries(want).map(([field, value]) => patch(task.id, { [field]: value })));
    for (const res of replies) assert.equal(res.status, 200, `round ${round}: ${res.status}`);
    const now = await get(task.id);
    const got: Record<string, unknown> = { ...now, assignee_id: now.assignee?.id ?? null };
    for (const [field, value] of Object.entries(want)) assert.deepEqual(got[field], value, `round ${round}: ${field} was lost`);
    assert.equal(now.version, 1 + 7 * round, `round ${round}: version`);
  }
  const entries = await updates(task.id);
  await app.close();
  assert.equal(entries, 21, "activity entries");
});

test("of concurrent PATCHes sent with the same version, exactly one applies and the others are 409s with the current task", async () => {
  const { app, make, get, patch } = await setUp();
  const task = await make();
  const replies = await Promise.all(range(20).map((i) => patch(task.id, { title: `Writer ${i}`, version: 1 })));
  const final = await get(task.id);
  await app.close();
  const winners = replies.filter((r) => r.status === 200);
  const losers = replies.filter((r) => r.status === 409);
  assert.equal(winners.length, 1, `${winners.length} PATCHes applied`);
  assert.equal(losers.length, 19, `statuses: ${replies.map((r) => r.status).join(",")}`);
  assert.equal(final.version, 2);
  assert.equal(final.title, winners[0].body.title);
  for (const res of losers) {
    assert.equal(res.body.error, "version conflict");
    assert.equal(res.body.task.version, 2);
    assert.equal(res.body.task.title, final.title);
  }
});

test("the version moves only on a change: never on a 409, a 400 or a 404", async () => {
  const { app, make, get, patch, updates } = await setUp();
  const task = await make({ title: "A" });
  assert.equal((await patch(task.id, { title: "B" })).status, 200);
  const refused: [Record<string, unknown>, number[]][] = [
    [{ title: "C", version: 1 }, [409]],
    [{ priority: 9 }, [400]],
    [{ title: "" }, [400]],
    [{ title: "D", version: 1.5 }, [400, 409]],
    [{ title: "E", version: "abc" }, [400, 409]],
    [{ title: "F", version: 0 }, [400, 409]],
    [{ title: "G", version: -1 }, [400, 409]],
    [{ title: "H", version: 3 }, [400, 409]],
  ];
  for (const [body, statuses] of refused) {
    const res = await patch(task.id, body);
    assert.ok(statuses.includes(res.status), `${JSON.stringify(body)}: ${res.status}`);
  }
  const missing = await patch(999_999, { title: "Nobody" });
  const missingVersioned = await patch(999_999, { title: "Nobody", version: 1 });
  const read = await get(task.id);
  const entries = await updates(task.id);
  await app.close();
  assert.equal(missing.status, 404);
  assert.equal(missingVersioned.status, 404);
  assert.equal(read.title, "B");
  assert.equal(read.version, 2);
  assert.equal(entries, 1, "activity entries");
});

test("the activity log has one entry per change applied, none for a refused one", async () => {
  const { app, make, patch, updates, get } = await setUp();
  const task = await make();
  const sameVersion = await Promise.all(range(10).map((i) => patch(task.id, { notes: `Note ${i}`, version: 1 })));
  const free = await Promise.all(range(6).map((i) => patch(task.id, { title: `Title ${i}` })));
  const entries = await updates(task.id);
  const read = await get(task.id);
  await app.close();
  const applied = [...sameVersion, ...free].filter((r) => r.status === 200).length;
  assert.equal(applied, 7, "applied PATCHes");
  assert.equal(entries, 7, "activity entries");
  assert.equal(read.version, 8);
});

test("50 concurrent requests on the same tasks: no error, no change lost", async () => {
  const { app, alice, list, make, get } = await setUp();
  const tasks = await Promise.all(range(3).map((i) => make({ title: `Shared ${i}` })));
  const requests = range(50).map((i) => {
    const task = tasks[i % 3];
    switch (i % 5) {
      case 0:
        return app.call("GET", `/api/tasks/${task.id}`, undefined, alice.auth);
      case 1:
        return app.call("PATCH", `/api/tasks/${task.id}`, { title: `Title ${i}` }, alice.auth);
      case 2:
        return app.call("PATCH", `/api/tasks/${task.id}`, { notes: `Note ${i}`, version: 1 }, alice.auth);
      case 3:
        return app.call("POST", `/api/tasks/${task.id}/comments`, { body: `Comment ${i}` }, alice.auth);
      default:
        return app.call("POST", `/api/lists/${list.id}/tasks`, { title: `New ${i}` }, alice.auth);
    }
  });
  const replies = await Promise.all(requests);
  const after = await Promise.all(tasks.map((t) => get(t.id)));
  await app.close();
  replies.forEach((res, i) => assert.ok([200, 201, 409].includes(res.status), `request ${i}: ${res.status}`));
  tasks.forEach((task, k) => {
    const mine = (kind: number) => replies.filter((_, i) => i % 3 === k && i % 5 === kind);
    const applied = [...mine(1), ...mine(2)].filter((r) => r.status === 200).length;
    assert.equal(after[k].version, 1 + applied, `task ${k}: version`);
    assert.equal(after[k].comment_count, mine(3).length, `task ${k}: comments`);
  });
  assert.equal(replies.filter((r, i) => i % 5 === 4 && r.status === 201 && r.body.version === 1).length, 10);
});

test("tasks that existed before the change get version 1", async () => {
  let listId = 0;
  const path = shippedDb((db) => {
    const owner = insertUser(db, "alice");
    listId = Number(db.prepare("INSERT INTO lists (owner_id, name) VALUES (?, 'Old')").run(owner).lastInsertRowid);
    for (const title of ["Old one", "Old two"]) {
      db.prepare("INSERT INTO tasks (list_id, title, position, created_by, updated_at) VALUES (?, ?, 0, ?, '2026-01-01T00:00:00.000Z')").run(listId, title, owner);
    }
  });
  const app = await start(path);
  const auth = await signIn(app, "alice");
  const listed = await app.call("GET", `/api/lists/${listId}/tasks`, undefined, auth);
  const items = Array.isArray(listed.body) ? listed.body : listed.body?.items;
  const first = items?.[0];
  const res = first ? await app.call("PATCH", `/api/tasks/${first.id}`, { done: true, version: 1 }, auth) : undefined;
  await app.close();
  assert.equal(listed.status, 200);
  assert.deepEqual(
    items.map((t: any) => t.version),
    [1, 1],
  );
  assert.equal(res?.status, 200);
  assert.equal(res?.body.version, 2);
});

test("creating, commenting and deleting keep working; a deleted task is a 404, with or without a version", async () => {
  const { app, alice, make, get, patch } = await setUp();
  const task = await make({ title: "Short-lived", priority: 1 });
  const comment = await app.call("POST", `/api/tasks/${task.id}/comments`, { body: "Hello" }, alice.auth);
  const read = await get(task.id);
  const del = await app.call("DELETE", `/api/tasks/${task.id}`, undefined, alice.auth);
  const plain = await patch(task.id, { title: "Ghost" });
  const versioned = await patch(task.id, { title: "Ghost", version: read.version });
  await app.close();
  assert.equal(task.version, 1);
  assert.equal(comment.status, 201);
  assert.equal(read.comment_count, 1);
  assert.equal(read.priority, 1);
  assert.equal(del.status, 204);
  assert.equal(plain.status, 404);
  assert.equal(versioned.status, 404);
});

test("clients that read, change and send back with the version all get their change in, after retrying", async () => {
  const { app, make, get, patch } = await setUp();
  const task = await make();
  await Promise.all(
    range(10).map(async (i) => {
      for (let attempt = 0; attempt < 100; attempt++) {
        const current = await get(task.id);
        const res = await patch(task.id, { tags: [...(current.tags ?? []), `c${i}`], version: current.version });
        if (res.status === 200) return;
        assert.equal(res.status, 409, `client ${i}: ${res.status}`);
      }
      assert.fail(`client ${i} never got its change in`);
    }),
  );
  const final = await get(task.id);
  await app.close();
  assert.deepEqual([...final.tags].sort(), range(10).map((i) => `c${i}`).sort());
  assert.equal(final.version, 11);
});

test("a PATCH racing a DELETE answers 200, 404 or 409, never an error", async () => {
  const { app, alice, make, patch, get } = await setUp();
  for (let i = 0; i < 5; i++) {
    const task = await make({ title: `Doomed ${i}` });
    const replies = await Promise.all([
      patch(task.id, { title: `Renamed ${i}` }),
      app.call("DELETE", `/api/tasks/${task.id}`, undefined, alice.auth),
      patch(task.id, { done: true, version: 1 }),
    ]);
    assert.ok([200, 404].includes(replies[0].status), `rename: ${replies[0].status}`);
    assert.equal(replies[1].status, 204);
    assert.ok([200, 404, 409].includes(replies[2].status), `versioned: ${replies[2].status}`);
    assert.equal(await get(task.id).then((t) => t?.error), "task not found");
  }
  await app.close();
});
