import { transaction, type Db } from "./db.ts";

// Emails are only written to the `outbox` table; delivering them is the mail relay's job, outside this app.

const MAX_ATTEMPTS = 5;
const REMINDER_TIME = "08:00:00.000Z";
const DIGEST_HOUR_UTC = 7;
const BATCH = 100;

interface Job {
  id: number;
  kind: string;
  payload: string;
  run_at: string;
  attempts: number;
}

type JobHandler = (db: Db, payload: any, at: Date) => void;

const handlers: Record<string, JobHandler> = {
  reminder(db, { task_id, due_date }) {
    const row = db
      .prepare(
        `SELECT t.title, t.done, t.due_date, l.name AS list_name, u.username, u.email
         FROM tasks t JOIN lists l ON l.id = t.list_id JOIN users u ON u.id = t.assignee_id
         WHERE t.id = ?`,
      )
      .get(task_id) as any;
    if (!row || row.done || row.due_date !== due_date || !row.email) return;
    mail(db, row.email, `Due today: ${row.title}`, `Hello ${row.username},\n\n"${row.title}" (${row.list_name}) is due today.\n`);
  },

  digest(db, { user_id, date }) {
    const user = db.prepare("SELECT username, email FROM users WHERE id = ?").get(user_id) as any;
    if (!user?.email) return;
    const tasks = dueTasks(db, user_id, date);
    if (tasks.length === 0) return;
    const lines = tasks.map((t) => `- ${t.title} (${t.list_name}, ${t.due_date === date ? "due today" : `overdue since ${t.due_date}`})`);
    const subject = tasks.length === 1 ? "1 task due" : `${tasks.length} tasks due`;
    mail(db, user.email, subject, `Hello ${user.username},\n\n${lines.join("\n")}\n`);
  },
};

/** Open tasks due on `date` or before that are the user's: assigned to them, or unassigned in their lists. */
function dueTasks(db: Db, userId: number, date: string): { title: string; due_date: string; list_name: string }[] {
  return db
    .prepare(
      `SELECT t.title, t.due_date, l.name AS list_name FROM tasks t JOIN lists l ON l.id = t.list_id
       WHERE t.done = 0 AND t.due_date <= ? AND (t.assignee_id = ? OR (t.assignee_id IS NULL AND l.owner_id = ?))
       ORDER BY t.due_date, t.id`,
    )
    .all(date, userId, userId) as any[];
}

function mail(db: Db, to: string, subject: string, body: string): void {
  db.prepare("INSERT INTO outbox (to_address, subject, body) VALUES (?, ?, ?)").run(to, subject, body);
}

/** Queues a job; a job with a `dedupeKey` already queued (or already run) is not queued again. */
export function enqueue(db: Db, kind: string, payload: object, runAt: string, dedupeKey: string | null = null): void {
  db.prepare("INSERT INTO jobs (kind, payload, run_at, dedupe_key) VALUES (?, ?, ?, ?) ON CONFLICT (dedupe_key) DO NOTHING").run(
    kind,
    JSON.stringify(payload),
    runAt,
    dedupeKey,
  );
}

export function cancelReminders(db: Db, taskId: number): void {
  db.prepare("DELETE FROM jobs WHERE kind = 'reminder' AND done_at IS NULL AND json_extract(payload, '$.task_id') = ?").run(taskId);
}

/** Plans the reminder of a task for the morning of its due date, to its assignee, replacing any planned before. */
export function scheduleReminder(db: Db, task: { id: number; due_date: string | null; assignee_id: number | null; done: number | boolean }): void {
  cancelReminders(db, task.id);
  if (task.due_date === null || task.assignee_id === null || task.done) return;
  const payload = { task_id: task.id, due_date: task.due_date };
  enqueue(db, "reminder", payload, `${task.due_date}T${REMINDER_TIME}`, `reminder:${task.id}:${task.due_date}`);
}

// TODO: queue only the users with something due; today every user gets a job, most of which send nothing.
/** From DIGEST_HOUR_UTC on, one digest a day for each user who gave an email. */
function queueDigests(db: Db, at: Date): void {
  if (at.getUTCHours() < DIGEST_HOUR_UTC) return;
  const date = at.toISOString().slice(0, 10);
  const users = db.prepare("SELECT id FROM users WHERE email IS NOT NULL").all() as { id: number }[];
  for (const { id } of users) enqueue(db, "digest", { user_id: id, date }, at.toISOString(), `digest:${id}:${date}`);
}

/** Runs the jobs due at `at` and answers how many ran. A failed job is retried later, up to MAX_ATTEMPTS times. */
export function runDueJobs(db: Db, at: Date = new Date()): number {
  queueDigests(db, at);
  const stamp = at.toISOString();
  const due = db
    .prepare("SELECT id, kind, payload, run_at, attempts FROM jobs WHERE done_at IS NULL AND run_at <= ? ORDER BY run_at, id LIMIT ?")
    .all(stamp, BATCH) as Job[];
  for (const job of due) {
    try {
      transaction(db, () => {
        const handler = handlers[job.kind];
        if (!handler) throw new Error(`unknown job kind: ${job.kind}`);
        handler(db, JSON.parse(job.payload), at);
        db.prepare("UPDATE jobs SET done_at = ?, attempts = attempts + 1 WHERE id = ?").run(stamp, job.id);
      });
    } catch (err: any) {
      const attempts = job.attempts + 1;
      const retryAt = new Date(at.getTime() + attempts * attempts * 60_000).toISOString();
      db.prepare("UPDATE jobs SET attempts = ?, last_error = ?, run_at = ?, done_at = ? WHERE id = ?").run(
        attempts,
        String(err?.message ?? err),
        retryAt,
        attempts >= MAX_ATTEMPTS ? stamp : null,
        job.id,
      );
    }
  }
  return due.length;
}

/** Runs the due jobs every `everyMs`; answers a function that stops it. */
export function startJobs(db: Db, everyMs = 60_000): () => void {
  const timer: any = setInterval(() => {
    try {
      runDueJobs(db);
    } catch (err) {
      console.error("jobs:", err);
    }
  }, everyMs);
  timer.unref?.();
  return () => clearInterval(timer);
}
