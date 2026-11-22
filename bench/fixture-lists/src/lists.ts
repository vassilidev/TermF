import type { User } from "./auth.ts";
import { transaction, type Db } from "./db.ts";
import { badRequest, notFound } from "./http.ts";
import * as activity from "./activity.ts";

export interface List {
  id: number;
  owner_id: number;
  name: string;
  created_at: string;
}

export interface ListView extends List {
  task_count: number;
  open_count: number;
}

const MAX_NAME = 100;

function cleanName(value: unknown): string {
  if (typeof value !== "string" || value.trim() === "") throw badRequest("name is required");
  const name = value.trim();
  if (name.length > MAX_NAME) throw badRequest(`name is too long (max ${MAX_NAME} characters)`);
  return name;
}

/** The list if `user` owns it. Someone else's list is answered as if it did not exist. */
export function loadList(db: Db, user: User, id: number): List {
  const list = db.prepare("SELECT * FROM lists WHERE id = ? AND owner_id = ?").get(id, user.id) as List | undefined;
  if (!list) throw notFound("list not found");
  return list;
}

const VIEW = `SELECT l.*,
  (SELECT COUNT(*) FROM tasks t WHERE t.list_id = l.id) AS task_count,
  (SELECT COUNT(*) FROM tasks t WHERE t.list_id = l.id AND t.done = 0) AS open_count
  FROM lists l`;

export function listLists(db: Db, user: User): ListView[] {
  return db.prepare(`${VIEW} WHERE l.owner_id = ? ORDER BY l.name COLLATE NOCASE, l.id`).all(user.id) as ListView[];
}

export function getList(db: Db, user: User, id: number): ListView {
  loadList(db, user, id);
  return db.prepare(`${VIEW} WHERE l.id = ?`).get(id) as ListView;
}

export async function createList(db: Db, user: User, body: any): Promise<ListView> {
  const name = cleanName(body.name);
  const { lastInsertRowid } = db.prepare("INSERT INTO lists (owner_id, name) VALUES (?, ?)").run(user.id, name);
  const id = Number(lastInsertRowid);
  await activity.record(db, { list_id: id, user_id: user.id, action: "list.created", detail: { name } });
  return getList(db, user, id);
}

export async function renameList(db: Db, user: User, id: number, body: any): Promise<ListView> {
  const list = loadList(db, user, id);
  const name = cleanName(body.name);
  if (name !== list.name) {
    db.prepare("UPDATE lists SET name = ? WHERE id = ?").run(name, id);
    await activity.record(db, { list_id: id, user_id: user.id, action: "list.renamed", detail: { from: list.name, to: name } });
  }
  return getList(db, user, id);
}

export function deleteList(db: Db, user: User, id: number): void {
  loadList(db, user, id);
  transaction(db, () => {
    db.prepare("DELETE FROM activity WHERE list_id = ?").run(id);
    db.prepare("DELETE FROM lists WHERE id = ?").run(id);
  });
}
