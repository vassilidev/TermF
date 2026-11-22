import type { User } from "./auth.ts";
import { now, transaction, type Db } from "./db.ts";
import { badRequest, notFound } from "./http.ts";
import { loadList, type List } from "./lists.ts";
import * as activity from "./activity.ts";
import * as jobs from "./jobs.ts";

export interface Task {
  id: number;
  list_id: number;
  title: string;
  notes: string;
  done: number;
  position: number;
  priority: number;
  due_date: string | null;
  assignee_id: number | null;
  created_by: number | null;
  created_at: string;
  updated_at: string;
}

export interface TaskView {
  id: number;
  list_id: number;
  title: string;
  notes: string;
  done: boolean;
  position: number;
  priority: number;
  due_date: string | null;
  assignee: { id: number; username: string } | null;
  tags: string[];
  comment_count: number;
  created_by: number | null;
  created_at: string;
  updated_at: string;
}

interface TaskInput {
  title?: string;
  notes?: string;
  done?: number;
  priority?: number;
  due_date?: string | null;
  assignee_id?: number | null;
  position?: number;
  tags?: string[];
}

const MAX_TITLE = 200;
const MAX_NOTES = 5000;
const MAX_TAGS = 10;
const TAG = /^[\p{L}\p{N}_-]{1,30}$/u;

// Ties go by id: positions are not renumbered when a task moves, so two tasks can share one.
const SORTS: Record<string, string> = {
  position: "t.position, t.id",
  due_date: "t.due_date IS NULL, t.due_date, t.id",
  priority: "t.priority DESC, t.id",
  created: "t.created_at DESC, t.id DESC",
};

function cleanTitle(value: unknown): string {
  if (typeof value !== "string" || value.trim() === "") throw badRequest("title is required");
  const title = value.trim();
  if (title.length > MAX_TITLE) throw badRequest(`title is too long (max ${MAX_TITLE} characters)`);
  return title;
}

function checkDate(value: unknown): string | null {
  if (value === null) return null;
  if (typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value)) {
    const date = new Date(`${value}T00:00:00.000Z`);
    if (!Number.isNaN(date.getTime()) && date.toISOString().startsWith(value)) return value;
  }
  throw badRequest("due_date must be a date as YYYY-MM-DD, or null");
}

function checkAssignee(list: List, value: unknown): number | null {
  if (value === null) return null;
  if (value !== list.owner_id) throw badRequest("the assignee must have access to the list");
  return value;
}

function cleanTags(value: unknown): string[] {
  if (!Array.isArray(value) || value.length > MAX_TAGS) throw badRequest(`tags must be a list of at most ${MAX_TAGS} names`);
  const names = value.map((tag) => {
    if (typeof tag !== "string" || !TAG.test(tag.trim())) throw badRequest("a tag is 1 to 30 letters, digits, - or _");
    return tag.trim().toLowerCase();
  });
  return [...new Set(names)].sort();
}

/** The task fields in `body`, checked. For a PATCH (`partial`) each one is optional. */
function readInput(list: List, body: any, partial: boolean): TaskInput {
  const input: TaskInput = {};
  if (!partial || body.title !== undefined) input.title = cleanTitle(body.title);
  if (body.notes !== undefined) {
    if (typeof body.notes !== "string" || body.notes.length > MAX_NOTES) throw badRequest(`notes must be a text of at most ${MAX_NOTES} characters`);
    input.notes = body.notes;
  }
  if (body.done !== undefined) {
    if (typeof body.done !== "boolean") throw badRequest("done must be true or false");
    input.done = body.done ? 1 : 0;
  }
  if (body.priority !== undefined) {
    if (![0, 1, 2, 3].includes(body.priority)) throw badRequest("priority must be 0, 1, 2 or 3");
    input.priority = body.priority;
  }
  if (body.due_date !== undefined) input.due_date = checkDate(body.due_date);
  if (body.assignee_id !== undefined) input.assignee_id = checkAssignee(list, body.assignee_id);
  if (body.position !== undefined) {
    if (!Number.isInteger(body.position) || body.position < 0) throw badRequest("position must be a whole number, 0 or more");
    input.position = body.position;
  }
  if (body.tags !== undefined) input.tags = cleanTags(body.tags);
  return input;
}

function tagsOf(db: Db, taskId: number): string[] {
  const rows = db
    .prepare("SELECT g.name FROM task_tags tt JOIN tags g ON g.id = tt.tag_id WHERE tt.task_id = ? ORDER BY g.name")
    .all(taskId) as { name: string }[];
  return rows.map((row) => row.name);
}

function setTags(db: Db, listId: number, taskId: number, names: string[]): void {
  db.prepare("DELETE FROM task_tags WHERE task_id = ?").run(taskId);
  for (const name of names) {
    db.prepare("INSERT INTO tags (list_id, name) VALUES (?, ?) ON CONFLICT (list_id, name) DO NOTHING").run(listId, name);
    const tag = db.prepare("SELECT id FROM tags WHERE list_id = ? AND name = ?").get(listId, name) as { id: number };
    db.prepare("INSERT INTO task_tags (task_id, tag_id) VALUES (?, ?)").run(taskId, tag.id);
  }
}

function toView(db: Db, task: Task): TaskView {
  const assignee =
    task.assignee_id === null
      ? null
      : ((db.prepare("SELECT id, username FROM users WHERE id = ?").get(task.assignee_id) as TaskView["assignee"]) ?? null);
  const { count } = db.prepare("SELECT COUNT(*) AS count FROM comments WHERE task_id = ?").get(task.id) as { count: number };
  return {
    id: task.id,
    list_id: task.list_id,
    title: task.title,
    notes: task.notes,
    done: task.done === 1,
    position: task.position,
    priority: task.priority,
    due_date: task.due_date,
    assignee: assignee && { id: assignee.id, username: assignee.username },
    tags: tagsOf(db, task.id),
    comment_count: count,
    created_by: task.created_by,
    created_at: task.created_at,
    updated_at: task.updated_at,
  };
}

function viewOf(db: Db, id: number): TaskView {
  const task = db.prepare("SELECT * FROM tasks WHERE id = ?").get(id) as Task | undefined;
  if (!task) throw notFound("task not found");
  return toView(db, task);
}

/** The task if `user` owns its list. Someone else's task is answered as if it did not exist. */
export function loadTask(db: Db, user: User, id: number): Task {
  const task = db
    .prepare("SELECT t.* FROM tasks t JOIN lists l ON l.id = t.list_id WHERE t.id = ? AND l.owner_id = ?")
    .get(id, user.id) as Task | undefined;
  if (!task) throw notFound("task not found");
  return task;
}

export function listTasks(db: Db, user: User, listId: number, query: URLSearchParams): TaskView[] {
  loadList(db, user, listId);
  const where = ["t.list_id = ?"];
  const args: (string | number)[] = [listId];

  const done = query.get("done");
  if (done !== null) {
    if (done !== "true" && done !== "false") throw badRequest("done must be true or false");
    where.push("t.done = ?");
    args.push(done === "true" ? 1 : 0);
  }
  const tag = query.get("tag");
  if (tag !== null) {
    where.push("EXISTS (SELECT 1 FROM task_tags tt JOIN tags g ON g.id = tt.tag_id WHERE tt.task_id = t.id AND g.name = ?)");
    args.push(tag.trim().toLowerCase());
  }
  const assignee = query.get("assignee");
  if (assignee === "none") {
    where.push("t.assignee_id IS NULL");
  } else if (assignee !== null) {
    if (!/^[1-9][0-9]*$/.test(assignee)) throw badRequest("assignee must be a user id, or none");
    where.push("t.assignee_id = ?");
    args.push(Number(assignee));
  }
  const sort = query.get("sort") ?? "position";
  if (!Object.hasOwn(SORTS, sort)) throw badRequest(`sort must be one of ${Object.keys(SORTS).join(", ")}`);

  const rows = db.prepare(`SELECT t.* FROM tasks t WHERE ${where.join(" AND ")} ORDER BY ${SORTS[sort]}`).all(...args) as Task[];
  return rows.map((row) => toView(db, row));
}

export function getTask(db: Db, user: User, id: number): TaskView {
  return toView(db, loadTask(db, user, id));
}

export async function createTask(db: Db, user: User, listId: number, body: any): Promise<TaskView> {
  const list = loadList(db, user, listId);
  const input = readInput(list, body, false);
  const stamp = now();
  const task = transaction(db, () => {
    const { next } = db.prepare("SELECT COALESCE(MAX(position) + 1, 0) AS next FROM tasks WHERE list_id = ?").get(listId) as { next: number };
    const { lastInsertRowid } = db
      .prepare(
        `INSERT INTO tasks (list_id, title, notes, done, priority, due_date, assignee_id, position, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .run(listId, input.title!, input.notes ?? "", input.done ?? 0, input.priority ?? 0, input.due_date ?? null, input.assignee_id ?? null, input.position ?? next, user.id, stamp, stamp);
    const id = Number(lastInsertRowid);
    if (input.tags) setTags(db, listId, id, input.tags);
    return db.prepare("SELECT * FROM tasks WHERE id = ?").get(id) as Task;
  });
  jobs.scheduleReminder(db, task);
  await activity.record(db, { list_id: listId, task_id: task.id, user_id: user.id, action: "task.created", detail: { title: task.title } });
  return viewOf(db, task.id);
}

export async function updateTask(db: Db, user: User, id: number, body: any): Promise<TaskView> {
  const task = loadTask(db, user, id);
  const input = readInput(loadList(db, user, task.list_id), body, true);
  const current: Record<string, unknown> = { ...task, tags: input.tags && tagsOf(db, id) };
  const changes = Object.entries(input)
    .filter(([key, value]) => JSON.stringify(current[key]) !== JSON.stringify(value))
    .map(([key]) => key);
  if (changes.length === 0) return toView(db, task);

  const next = { ...task, ...input, updated_at: now() };
  await activity.record(db, { list_id: task.list_id, task_id: id, user_id: user.id, action: "task.updated", detail: { changes } });
  transaction(db, () => {
    db.prepare(
      `UPDATE tasks SET title = ?, notes = ?, done = ?, priority = ?, due_date = ?, assignee_id = ?, position = ?, updated_at = ?
       WHERE id = ?`,
    ).run(next.title, next.notes, next.done, next.priority, next.due_date, next.assignee_id, next.position, next.updated_at, id);
    if (input.tags) setTags(db, task.list_id, id, input.tags);
  });
  if (changes.some((key) => key === "due_date" || key === "assignee_id" || key === "done")) jobs.scheduleReminder(db, next);
  return viewOf(db, id);
}

export async function deleteTask(db: Db, user: User, id: number): Promise<void> {
  const task = loadTask(db, user, id);
  db.prepare("DELETE FROM tasks WHERE id = ?").run(id);
  jobs.cancelReminders(db, id);
  await activity.record(db, { list_id: task.list_id, task_id: id, user_id: user.id, action: "task.deleted", detail: { title: task.title } });
}

/** The tags used in a list, with how many tasks carry each. */
export function listTags(db: Db, user: User, listId: number): { name: string; count: number }[] {
  loadList(db, user, listId);
  return db
    .prepare(
      `SELECT g.name, COUNT(tt.task_id) AS count FROM tags g LEFT JOIN task_tags tt ON tt.tag_id = g.id
       WHERE g.list_id = ? GROUP BY g.id ORDER BY g.name`,
    )
    .all(listId) as { name: string; count: number }[];
}
