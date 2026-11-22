import { test } from "node:test";
import assert from "node:assert/strict";
import { openDb, type Db } from "../src/db.ts";
import { signUp } from "../src/auth.ts";
import { createList } from "../src/lists.ts";
import { createTask, deleteTask, updateTask } from "../src/tasks.ts";
import { enqueue, runDueJobs } from "../src/jobs.ts";

async function setUp(email: string | null = "alice@example.com") {
  const db = openDb();
  const user = await signUp(db, { username: "alice", password: "correct horse battery", email });
  const list = await createList(db, user, { name: "Home" });
  return { db, user, list };
}

const outbox = (db: Db) => db.prepare("SELECT to_address, subject, body FROM outbox ORDER BY id").all() as any[];
const at = (iso: string) => new Date(iso);

test("a reminder goes to the assignee on the morning of the due date, not before", async () => {
  const { db, user, list } = await setUp();
  await createTask(db, user, list.id, { title: "Pay rent", due_date: "2026-04-01", assignee_id: user.id });
  runDueJobs(db, at("2026-03-31T06:00:00.000Z"));
  assert.equal(outbox(db).length, 0);
  runDueJobs(db, at("2026-04-01T08:00:00.000Z"));
  const mails = outbox(db).filter((m) => m.subject.startsWith("Due today"));
  assert.equal(mails.length, 1);
  assert.equal(mails[0].to_address, "alice@example.com");
  assert.equal(mails[0].subject, "Due today: Pay rent");
  runDueJobs(db, at("2026-04-01T09:00:00.000Z"));
  assert.equal(outbox(db).filter((m) => m.subject.startsWith("Due today")).length, 1);
  db.close();
});

test("no reminder for a task done, unassigned, or whose assignee has no email", async () => {
  const { db, user, list } = await setUp();
  const done = await createTask(db, user, list.id, { title: "Done", due_date: "2026-04-01", assignee_id: user.id });
  await updateTask(db, user, done.id, { done: true });
  await createTask(db, user, list.id, { title: "Nobody's", due_date: "2026-04-01" });
  runDueJobs(db, at("2026-04-01T06:30:00.000Z"));
  runDueJobs(db, at("2026-04-01T08:00:00.000Z"));
  assert.deepEqual(outbox(db).filter((m) => m.subject.startsWith("Due today")), []);
  db.close();

  const silent = await setUp(null);
  await createTask(silent.db, silent.user, silent.list.id, { title: "Quiet", due_date: "2026-04-01", assignee_id: silent.user.id });
  runDueJobs(silent.db, at("2026-04-01T08:00:00.000Z"));
  assert.deepEqual(outbox(silent.db), []);
  silent.db.close();
});

test("moving the due date moves the reminder; deleting the task cancels it", async () => {
  const { db, user, list } = await setUp();
  const moved = await createTask(db, user, list.id, { title: "Moved", due_date: "2026-04-01", assignee_id: user.id });
  await updateTask(db, user, moved.id, { due_date: "2026-04-03" });
  const deleted = await createTask(db, user, list.id, { title: "Deleted", due_date: "2026-04-01", assignee_id: user.id });
  await deleteTask(db, user, deleted.id);
  runDueJobs(db, at("2026-04-01T06:00:00.000Z"));
  runDueJobs(db, at("2026-04-01T08:00:00.000Z"));
  assert.deepEqual(outbox(db), []);
  runDueJobs(db, at("2026-04-03T08:00:00.000Z"));
  assert.deepEqual(
    outbox(db)
      .map((m) => m.subject)
      .filter((s) => s.startsWith("Due today")),
    ["Due today: Moved"],
  );
  db.close();
});

test("one digest a day lists the open tasks due or overdue", async () => {
  const { db, user, list } = await setUp();
  await createTask(db, user, list.id, { title: "Overdue", due_date: "2026-03-30" });
  await createTask(db, user, list.id, { title: "Today", due_date: "2026-04-01" });
  await createTask(db, user, list.id, { title: "Later", due_date: "2026-04-09" });
  runDueJobs(db, at("2026-04-01T06:59:00.000Z"));
  assert.deepEqual(outbox(db), []);
  runDueJobs(db, at("2026-04-01T07:00:00.000Z"));
  runDueJobs(db, at("2026-04-01T18:00:00.000Z"));
  const digests = outbox(db);
  assert.equal(digests.length, 1);
  assert.equal(digests[0].subject, "2 tasks due");
  assert.match(digests[0].body, /Overdue \(Home, overdue since 2026-03-30\)/);
  assert.match(digests[0].body, /Today \(Home, due today\)/);
  assert.doesNotMatch(digests[0].body, /Later/);
  runDueJobs(db, at("2026-04-02T07:00:00.000Z"));
  assert.equal(outbox(db).length, 2);
  db.close();
});

test("a failing job is retried later, then given up", async () => {
  const { db } = await setUp(null);
  enqueue(db, "unknown", {}, "2026-04-01T00:00:00.000Z");
  let clock = at("2026-04-01T00:00:00.000Z").getTime();
  for (let i = 0; i < 10; i++) {
    runDueJobs(db, new Date(clock));
    clock += 60 * 60_000;
  }
  const job = db.prepare("SELECT attempts, last_error, done_at FROM jobs WHERE kind = 'unknown'").get() as any;
  assert.equal(job.attempts, 5);
  assert.equal(job.last_error, "unknown job kind: unknown");
  assert.notEqual(job.done_at, null);
  db.close();
});
