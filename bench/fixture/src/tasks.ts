import type { Db } from "./db.ts";

export interface Task {
  id: number;
  title: string;
  done: boolean;
  created_at: string;
}

interface Row {
  id: number;
  title: string;
  done: number;
  created_at: string;
}

const toTask = (row: Row): Task => ({ ...row, done: row.done === 1 });

export function listTasks(db: Db, filter: { done?: string } = {}): Task[] {
  if (filter.done === undefined) {
    return (db.prepare("SELECT * FROM tasks ORDER BY id").all() as Row[]).map(toTask);
  }
  const rows = db.prepare("SELECT * FROM tasks WHERE done = ? ORDER BY id").all(filter.done ? 1 : 0) as Row[];
  return rows.map(toTask);
}

export function getTask(db: Db, id: number): Task | undefined {
  const row = db.prepare("SELECT * FROM tasks WHERE id = ?").get(id) as Row | undefined;
  return row && toTask(row);
}

export function createTask(db: Db, title: string): Task {
  const { lastInsertRowid } = db.prepare("INSERT INTO tasks (title) VALUES (?)").run(title);
  return getTask(db, Number(lastInsertRowid))!;
}

export function updateTask(db: Db, id: number, changes: { title?: string; done?: boolean }): Task | undefined {
  const task = getTask(db, id);
  if (!task) return undefined;
  const title = changes.title ?? task.title;
  const done = changes.done ?? task.done;
  db.prepare("UPDATE tasks SET title = ?, done = ? WHERE id = ?").run(title, done ? 1 : 0, id);
  return getTask(db, id);
}

export function deleteTask(db: Db, id: number): boolean {
  return db.prepare("DELETE FROM tasks WHERE id = ?").run(id).changes > 0;
}
