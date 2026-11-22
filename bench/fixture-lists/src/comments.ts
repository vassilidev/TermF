import type { User } from "./auth.ts";
import type { Db } from "./db.ts";
import { badRequest, notFound } from "./http.ts";
import { loadTask } from "./tasks.ts";
import * as activity from "./activity.ts";

export interface CommentView {
  id: number;
  task_id: number;
  author: { id: number; username: string } | null;
  body: string;
  created_at: string;
}

interface Row {
  id: number;
  task_id: number;
  body: string;
  created_at: string;
  author_id: number | null;
  username: string | null;
}

const MAX_LENGTH = 2000;

const SELECT = `SELECT c.id, c.task_id, c.body, c.created_at, u.id AS author_id, u.username
  FROM comments c LEFT JOIN users u ON u.id = c.author_id`;

function toView(row: Row): CommentView {
  return {
    id: row.id,
    task_id: row.task_id,
    author: row.author_id === null ? null : { id: row.author_id, username: row.username! },
    body: row.body,
    created_at: row.created_at,
  };
}

/** The comments of a task, oldest first. */
export function listComments(db: Db, user: User, taskId: number): CommentView[] {
  loadTask(db, user, taskId);
  return (db.prepare(`${SELECT} WHERE c.task_id = ? ORDER BY c.id`).all(taskId) as Row[]).map(toView);
}

export async function addComment(db: Db, user: User, taskId: number, body: any): Promise<CommentView> {
  const task = loadTask(db, user, taskId);
  const text = typeof body.body === "string" ? body.body.trim() : "";
  if (!text) throw badRequest("Comment body can't be empty.");
  if (text.length > MAX_LENGTH) throw badRequest(`Comment is too long, ${MAX_LENGTH} characters at most.`);
  const { lastInsertRowid } = db.prepare("INSERT INTO comments (task_id, author_id, body) VALUES (?, ?, ?)").run(taskId, user.id, text);
  const id = Number(lastInsertRowid);
  await activity.record(db, { list_id: task.list_id, task_id: taskId, user_id: user.id, action: "comment.added", detail: { comment_id: id } });
  const row = db.prepare(`${SELECT} WHERE c.id = ?`).get(id) as Row | undefined;
  if (!row) throw notFound("task not found");
  return toView(row);
}
