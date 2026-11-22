import { test } from "node:test";
import assert from "node:assert/strict";
import { copyFileSync } from "node:fs";
import { indexedFirstColumns, insertUser, publicText, random, shippedDb, signIn, sql, start, tempDb, type App, type Auth } from "./lists-helpers.ts";

// Calibrated on an Apple M4 Pro: a page of 200 answers in 4 to 6 ms with paging and indexes (the fastest of three
// tries, under load), while the shipped endpoint takes 2 to 9 s for the whole list.
const PAGE_BOUND_MS = 150;

type Db = Parameters<Parameters<typeof shippedDb>[0]>[0];

interface Row {
  id: number;
  list_id: number;
  title: string;
  done: number;
  position: number;
  priority: number;
  due_date: string | null;
  assignee_id: number | null;
  created_at: string;
}

function insertList(db: Db, owner: number, name: string): number {
  return Number(db.prepare("INSERT INTO lists (owner_id, name) VALUES (?, ?)").run(owner, name).lastInsertRowid);
}

/** Inserts `count` tasks in `list`, drawn by `draw`, with tags and comments; ties on every sort key are frequent. */
function insertTasks(db: Db, list: number, count: number, tagNames: string[], rnd: () => number, draw: (i: number) => Partial<Row>) {
  const tags = tagNames.map((name) => Number(db.prepare("INSERT INTO tags (list_id, name) VALUES (?, ?)").run(list, name).lastInsertRowid));
  const task = db.prepare(
    `INSERT INTO tasks (list_id, title, done, position, priority, due_date, assignee_id, created_by, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  );
  const tag = db.prepare("INSERT INTO task_tags (task_id, tag_id) VALUES (?, ?)");
  const comment = db.prepare("INSERT INTO comments (task_id, author_id, body) VALUES (?, ?, ?)");
  for (let i = 0; i < count; i++) {
    const t = draw(i);
    const id = Number(
      task.run(list, `Task ${list}-${i}`, t.done, t.position, t.priority, t.due_date, t.assignee_id, null, t.created_at, t.created_at).lastInsertRowid,
    );
    for (const tagId of tags) if (rnd() < 0.25) tag.run(id, tagId);
    if (rnd() < 0.35) for (let c = Math.floor(rnd() * 3); c >= 0; c--) comment.run(id, null, `Comment ${c}`);
  }
}

interface Fixture {
  template: string;
  alice: number;
  bob: number;
  lists: Record<string, number>;
}

const templates: Record<string, Fixture> = {};

/** 1,200 tasks full of ties ("ties"), a small second list of alice ("other"), and a list of bob ("bobs"). */
function tiesFixture(): Fixture {
  if (templates.ties) return templates.ties;
  const lists: Record<string, number> = {};
  let alice = 0;
  let bob = 0;
  const template = shippedDb((db) => {
    alice = insertUser(db, "alice");
    bob = insertUser(db, "bob");
    const rnd = random(7);
    const draw = (): Partial<Row> => ({
      done: rnd() < 0.4 ? 1 : 0,
      position: Math.floor(rnd() * 170),
      priority: Math.floor(rnd() * 4),
      due_date: rnd() < 0.25 ? null : `2026-0${1 + Math.floor(rnd() * 6)}-15`,
      assignee_id: rnd() < 0.5 ? alice : null,
      created_at: `2026-01-${String(1 + Math.floor(rnd() * 15)).padStart(2, "0")}T09:00:00.000Z`,
    });
    lists.ties = insertList(db, alice, "Ties");
    insertTasks(db, lists.ties, 1200, ["a", "b", "c", "d"], rnd, draw);
    lists.other = insertList(db, alice, "Other");
    insertTasks(db, lists.other, 40, ["a"], rnd, draw);
    lists.bobs = insertList(db, bob, "Bob's");
    insertTasks(db, lists.bobs, 30, ["a"], rnd, () => ({ ...draw(), assignee_id: null }));
  });
  return (templates.ties = { template, alice, bob, lists });
}

/** 10,000 tasks in alice's "Warehouse". */
function bigFixture(): Fixture {
  if (templates.big) return templates.big;
  const lists: Record<string, number> = {};
  let alice = 0;
  const template = shippedDb((db) => {
    alice = insertUser(db, "alice");
    const rnd = random(11);
    lists.big = insertList(db, alice, "Warehouse");
    insertTasks(db, lists.big, 10_000, ["home", "work", "urgent", "later", "errand"], rnd, (i) => ({
      done: rnd() < 0.3 ? 1 : 0,
      position: Math.floor(i / 2),
      priority: Math.floor(rnd() * 4),
      due_date: rnd() < 0.2 ? null : `2026-${String(1 + Math.floor(rnd() * 12)).padStart(2, "0")}-${String(1 + Math.floor(rnd() * 5) * 5).padStart(2, "0")}`,
      assignee_id: rnd() < 0.5 ? alice : null,
      created_at: `2025-${String(1 + (i % 12)).padStart(2, "0")}-${String(1 + (i % 28)).padStart(2, "0")}T10:00:00.000Z`,
    }));
  });
  return (templates.big = { template, alice, bob: 0, lists });
}

async function open(fixture: Fixture): Promise<{ app: App; path: string; alice: Auth }> {
  const path = tempDb();
  copyFileSync(fixture.template, path);
  const app = await start(path);
  return { app, path, alice: await signIn(app, "alice") };
}

const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);

// The orders of the README: ties go by id, ascending, except for `created` where the newest comes first.
const ORDERS: Record<string, (a: Row, b: Row) => number> = {
  position: (a, b) => a.position - b.position || a.id - b.id,
  due_date: (a, b) => Number(a.due_date === null) - Number(b.due_date === null) || cmp(a.due_date ?? "", b.due_date ?? "") || a.id - b.id,
  priority: (a, b) => b.priority - a.priority || a.id - b.id,
  created: (a, b) => cmp(b.created_at, a.created_at) || b.id - a.id,
};

function tagsByTask(path: string): Map<number, string[]> {
  const map = new Map<number, string[]>();
  for (const r of sql(path, "SELECT tt.task_id, g.name FROM task_tags tt JOIN tags g ON g.id = tt.tag_id ORDER BY g.name")) {
    map.set(r.task_id, [...(map.get(r.task_id) ?? []), r.name]);
  }
  return map;
}

/** The ids `GET /api/lists/:id/tasks?<query>` must answer, in order, from the template's data. */
function expected(fixture: Fixture, list: number, query: string): number[] {
  const params = new URLSearchParams(query);
  const tags = tagsByTask(fixture.template);
  let rows = sql(fixture.template, "SELECT * FROM tasks WHERE list_id = ?", list) as Row[];
  const done = params.get("done");
  if (done !== null) rows = rows.filter((r) => r.done === (done === "true" ? 1 : 0));
  const tag = params.get("tag");
  if (tag !== null) rows = rows.filter((r) => (tags.get(r.id) ?? []).includes(tag));
  const assignee = params.get("assignee");
  if (assignee !== null) rows = rows.filter((r) => (assignee === "none" ? r.assignee_id === null : r.assignee_id === Number(assignee)));
  return rows.sort(ORDERS[params.get("sort") ?? "position"]).map((r) => r.id);
}

/** Every page of `?<query>`, `limit` at a time: all the items, in the order answered. */
async function walk(app: App, auth: Auth, list: number, query: string, limit: number): Promise<any[]> {
  const items: any[] = [];
  let cursor: string | null = null;
  for (let pages = 1; ; pages++) {
    const after: string = cursor === null ? "" : `&cursor=${encodeURIComponent(cursor)}`;
    const res = await app.call("GET", `/api/lists/${list}/tasks?${query}&limit=${limit}${after}`, undefined, auth);
    assert.equal(res.status, 200, `page ${pages} of ?${query}: ${res.status}`);
    assert.ok(Array.isArray(res.body?.items), `page ${pages} of ?${query} has no items array`);
    assert.ok(res.body.items.length <= limit, `page ${pages} of ?${query} is longer than the limit`);
    assert.ok(res.body.next_cursor === null || typeof res.body.next_cursor === "string", "next_cursor is neither null nor a string");
    items.push(...res.body.items);
    cursor = res.body.next_cursor;
    if (cursor === null) return items;
    assert.ok(pages < 2000, `the pages of ?${query} never end`);
  }
}

function sameIds(actual: number[], wanted: number[], what: string): void {
  assert.equal(new Set(actual).size, actual.length, `${what}: a task comes twice`);
  assert.equal(actual.length, wanted.length, `${what}: ${actual.length} tasks instead of ${wanted.length}`);
  const at = actual.findIndex((id, i) => id !== wanted[i]);
  assert.equal(at, -1, `${what}: the order differs from position ${at}`);
}

async function checkSort(sort: string): Promise<void> {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const items = await walk(app, alice, fixture.lists.ties, `sort=${sort}`, 37);
  await app.close();
  sameIds(
    items.map((t) => t.id),
    expected(fixture, fixture.lists.ties, `sort=${sort}`),
    `sort=${sort}`,
  );
}

test("a page is { items, next_cursor }: 50 tasks by default, up to 200, next_cursor null on the last one", async () => {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const first = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks`, undefined, alice);
  const big = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=200`, undefined, alice);
  const one = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=1`, undefined, alice);
  const all = await walk(app, alice, fixture.lists.ties, "", 200);
  const small = await app.call("GET", `/api/lists/${fixture.lists.other}/tasks?limit=200`, undefined, alice);
  await app.close();
  assert.equal(first.status, 200);
  assert.ok(Array.isArray(first.body?.items), "no items array");
  assert.equal(first.body.items.length, 50);
  assert.equal(typeof first.body.next_cursor, "string");
  assert.equal(big.body.items.length, 200);
  assert.equal(one.body.items.length, 1);
  assert.equal(all.length, 1200);
  assert.equal(small.body.items.length, 40);
  assert.equal(small.body.next_cursor, null);
});

test("walking the pages answers every task once, in the order of sort=position (the default)", async () => {
  await checkSort("position");
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const items = await walk(app, alice, fixture.lists.ties, "", 64);
  await app.close();
  sameIds(
    items.map((t) => t.id),
    expected(fixture, fixture.lists.ties, "sort=position"),
    "no sort",
  );
});

test("walking the pages answers every task once, in the order of sort=due_date", () => checkSort("due_date"));

test("walking the pages answers every task once, in the order of sort=priority", () => checkSort("priority"));

test("walking the pages answers every task once, in the order of sort=created", () => checkSort("created"));

test("paging keeps the done, tag and assignee filters", async () => {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const list = fixture.lists.ties;
  for (const query of [
    "done=false&sort=due_date",
    "tag=b&sort=priority",
    `assignee=${fixture.alice}&sort=created`,
    "assignee=none&done=true&sort=position",
    "tag=a&done=false&sort=created",
    "tag=c&assignee=none&sort=due_date",
  ]) {
    const items = await walk(app, alice, list, query, 23);
    sameIds(
      items.map((t) => t.id),
      expected(fixture, list, query),
      query,
    );
  }
  await app.close();
});

test("the tasks of a page keep their fields: tags, assignee, comment_count and the rest", async () => {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const items = await walk(app, alice, fixture.lists.ties, "sort=due_date", 50);
  await app.close();
  const rows = new Map((sql(fixture.template, "SELECT * FROM tasks") as Row[]).map((r) => [r.id, r]));
  const tags = tagsByTask(fixture.template);
  const comments = new Map(sql(fixture.template, "SELECT task_id, COUNT(*) AS n FROM comments GROUP BY task_id").map((r) => [r.task_id, r.n]));
  assert.equal(items.length, 1200);
  for (const item of items) {
    const row = rows.get(item.id)!;
    const want = {
      list_id: row.list_id,
      title: row.title,
      done: row.done === 1,
      position: row.position,
      priority: row.priority,
      due_date: row.due_date,
      assignee: row.assignee_id === null ? null : { id: fixture.alice, username: "alice" },
      tags: tags.get(row.id) ?? [],
      comment_count: comments.get(row.id) ?? 0,
    };
    for (const [field, value] of Object.entries(want)) {
      const got = field === "tags" && Array.isArray(item.tags) ? [...item.tags].sort() : item[field];
      assert.deepEqual(got, value, `task ${item.id}: ${field}`);
    }
  }
});

test("a cursor from another sort or another list, or a made-up one, is a 400", async () => {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const tasks = (list: number, query: string) => app.call("GET", `/api/lists/${list}/tasks?${query}`, undefined, alice);
  const page = await tasks(fixture.lists.ties, "sort=priority&limit=10");
  const cursor: string = page.body?.next_cursor;
  assert.equal(typeof cursor, "string", "no cursor");
  const enc = encodeURIComponent;
  const ok = await tasks(fixture.lists.ties, `sort=priority&limit=10&cursor=${enc(cursor)}`);
  assert.equal(ok.status, 200, "the cursor does not work where it came from");
  const refused: [string, number, string][] = [
    ["sort=position", fixture.lists.ties, cursor],
    ["sort=due_date", fixture.lists.ties, cursor],
    ["sort=created", fixture.lists.ties, cursor],
    ["", fixture.lists.ties, cursor],
    ["sort=priority", fixture.lists.other, cursor],
    ["sort=priority", fixture.lists.ties, "abc"],
    ["sort=priority", fixture.lists.ties, "!!!"],
    ["sort=priority", fixture.lists.ties, Buffer.from("{}").toString("base64url")],
    ["sort=priority", fixture.lists.ties, Buffer.from("[1,2]").toString("base64url")],
    ["sort=priority", fixture.lists.ties, "x".repeat(3000)],
    ["sort=priority", fixture.lists.ties, "\u0000"],
  ];
  for (const [query, list, bad] of refused) {
    const res = await tasks(list, `${query}&limit=10&cursor=${enc(bad)}`);
    assert.equal(res.status, 400, `cursor ${bad.slice(0, 20)} on list ${list} with ?${query}: ${res.status}`);
  }
  await app.close();
});

test("limit stays between 1 and 200", async () => {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  for (const limit of ["0", "-1", "201", "1000", "abc", "1.5", "1e3"]) {
    const res = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=${limit}`, undefined, alice);
    assert.ok(res.status === 400 || res.status === 200, `limit=${limit}: ${res.status}`);
    if (res.status === 200) {
      assert.ok(Array.isArray(res.body?.items), `limit=${limit}: no items array`);
      assert.ok(res.body.items.length <= 200, `limit=${limit}: ${res.body.items.length} items`);
    }
  }
  await app.close();
});

test("tasks added or deleted between two pages make no task come twice nor go missing", async () => {
  const fixture = tiesFixture();
  const list = fixture.lists.ties;
  for (const [sort, fields] of [
    ["created", {}],
    ["position", { position: 0 }],
    ["priority", { priority: 3 }],
  ] as const) {
    const { app, alice } = await open(fixture);
    const wanted = expected(fixture, list, `sort=${sort}`);
    const first = await app.call("GET", `/api/lists/${list}/tasks?sort=${sort}&limit=25`, undefined, alice);
    assert.ok(Array.isArray(first.body?.items), "no items array");
    const seen: number[] = first.body.items.map((t: any) => t.id);
    const added = await app.call("POST", `/api/lists/${list}/tasks`, { title: `Added during the ${sort} walk`, ...fields }, alice);
    assert.equal(added.status, 201);
    const deletedSeen = seen[3];
    const deletedAhead = wanted.slice(100).find((id) => !seen.includes(id))!;
    for (const id of [deletedSeen, deletedAhead]) assert.equal((await app.call("DELETE", `/api/tasks/${id}`, undefined, alice)).status, 204);

    let cursor: string | null = first.body.next_cursor;
    const ids = [...seen];
    while (cursor !== null) {
      const res = await app.call("GET", `/api/lists/${list}/tasks?sort=${sort}&limit=25&cursor=${encodeURIComponent(cursor)}`, undefined, alice);
      assert.equal(res.status, 200, `sort=${sort}: ${res.status}`);
      ids.push(...res.body.items.map((t: any) => t.id));
      cursor = res.body.next_cursor;
    }
    await app.close();
    assert.equal(new Set(ids).size, ids.length, `sort=${sort}: a task comes twice`);
    assert.ok(!ids.includes(deletedAhead), `sort=${sort}: a deleted task is answered`);
    const missing = wanted.filter((id) => id !== deletedAhead && !ids.includes(id));
    assert.equal(missing.length, 0, `sort=${sort}: ${missing.length} tasks missing`);
  }
});

test("the hot paths of the task list are indexed", async () => {
  const fixture = tiesFixture();
  const { app, path } = await open(fixture);
  await app.close();
  assert.ok(indexedFirstColumns(path, "tasks").includes("list_id"), "no index on tasks starting with list_id");
  assert.ok(indexedFirstColumns(path, "task_tags").includes("task_id"), "no index on task_tags starting with task_id");
  assert.ok(indexedFirstColumns(path, "comments").includes("task_id"), "no index on comments starting with task_id");
});

test("paging leaks nothing: someone else's list stays a 404, with or without a cursor", async () => {
  const fixture = tiesFixture();
  const { app, alice } = await open(fixture);
  const bob = await signIn(app, "bob");
  const cursor = (await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=5`, undefined, alice)).body?.next_cursor;
  assert.equal(typeof cursor, "string", "no cursor");
  const plain = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=5`, undefined, bob);
  const withCursor = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=5&cursor=${encodeURIComponent(cursor)}`, undefined, bob);
  const anonymous = await app.call("GET", `/api/lists/${fixture.lists.ties}/tasks?limit=5`);
  const own = await walk(app, bob, fixture.lists.bobs, "sort=due_date", 7);
  const foreign = await app.call("GET", `/api/lists/${fixture.lists.bobs}/tasks?limit=5&cursor=${encodeURIComponent(cursor)}`, undefined, bob);
  await app.close();
  assert.equal(plain.status, 404);
  assert.equal(withCursor.status, 404);
  assert.equal(anonymous.status, 401);
  assert.equal(own.length, 30);
  assert.equal(foreign.status, 400);
});

test("the page reads the pages and asks for the next one", () => {
  const page = publicText();
  assert.ok(page.includes("next_cursor"), "the page never reads next_cursor");
  assert.ok(/cursor=/.test(page) || /["'`]cursor["'`]/.test(page), "the page never sends a cursor");
});

// Last on purpose: `--test-timeout` bounds the whole file, and a slow implementation must not hide the other results.
test("each page of a 10,000-task list answers fast, for every sort and filter", async () => {
  const fixture = bigFixture();
  const { app, alice } = await open(fixture);
  const list = fixture.lists.big;
  const warmUp = await app.call("GET", `/api/lists/${list}/tasks`, undefined, alice);
  assert.ok(Array.isArray(warmUp.body?.items), "the response is not a page");
  for (const query of [
    "sort=position",
    "sort=due_date",
    "sort=priority&done=false",
    "sort=created&tag=work",
    `sort=due_date&assignee=${fixture.alice}`,
    "sort=priority&tag=urgent&done=true&assignee=none",
  ]) {
    let cursor: string | null = null;
    for (let page = 1; page <= 5; page++) {
      const after: string = cursor === null ? "" : `&cursor=${encodeURIComponent(cursor)}`;
      // The fastest of up to three tries: a busy machine adds pauses of tens of ms, a slow query is slow every time.
      let fastest = Infinity;
      let res: any;
      for (let attempt = 0; attempt === 0 || (attempt < 3 && fastest >= PAGE_BOUND_MS / 3 && fastest < PAGE_BOUND_MS * 4); attempt++) {
        const began = performance.now();
        res = await app.call("GET", `/api/lists/${list}/tasks?${query}&limit=200${after}`, undefined, alice);
        fastest = Math.min(fastest, performance.now() - began);
        assert.equal(res.status, 200, `?${query} page ${page}: ${res.status}`);
      }
      assert.ok(fastest < PAGE_BOUND_MS, `${Math.round(fastest)} ms for ?${query} page ${page}`);
      cursor = res.body.next_cursor;
      if (cursor === null) break;
    }
  }
  await app.close();
});
