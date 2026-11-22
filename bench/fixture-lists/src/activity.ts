import { setImmediate } from "node:timers";
import { now, transaction, type Db } from "./db.ts";

export interface Entry {
  list_id: number;
  task_id?: number | null;
  user_id: number | null;
  action: string;
  detail?: Record<string, unknown>;
}

export interface ActivityView {
  id: number;
  action: string;
  task_id: number | null;
  user: { id: number; username: string } | null;
  detail: Record<string, unknown>;
  created_at: string;
}

interface Batch {
  entries: (Entry & { at: string })[];
  written: Promise<void>;
}

const batches = new WeakMap<Db, Batch>();

/**
 * Appends an entry to a list's activity log. Entries are buffered and written in one transaction per turn of the
 * event loop: ticking several tasks on the page sends a burst of requests. Resolves once the entry is stored.
 */
export function record(db: Db, entry: Entry): Promise<void> {
  let batch = batches.get(db);
  if (!batch) {
    const entries: Batch["entries"] = [];
    const written = new Promise<void>((resolve, reject) => {
      setImmediate(() => {
        batches.delete(db);
        try {
          write(db, entries);
          resolve();
        } catch (err) {
          reject(err);
        }
      });
    });
    batch = { entries, written };
    batches.set(db, batch);
  }
  batch.entries.push({ ...entry, at: now() });
  return batch.written;
}

function write(db: Db, entries: Batch["entries"]): void {
  const insert = db.prepare(
    "INSERT INTO activity (list_id, task_id, user_id, action, detail, created_at) VALUES (?, ?, ?, ?, ?, ?)",
  );
  transaction(db, () => {
    for (const e of entries) {
      insert.run(e.list_id, e.task_id ?? null, e.user_id, e.action, JSON.stringify(e.detail ?? {}), e.at);
    }
  });
}

interface Row {
  id: number;
  action: string;
  task_id: number | null;
  user_id: number | null;
  username: string | null;
  detail: string;
  created_at: string;
}

/** The latest entries of a list, newest first. */
export function forList(db: Db, listId: number, limit: number): ActivityView[] {
  const rows = db
    .prepare(
      `SELECT a.id, a.action, a.task_id, a.detail, a.created_at, u.id AS user_id, u.username
       FROM activity a LEFT JOIN users u ON u.id = a.user_id
       WHERE a.list_id = ? ORDER BY a.id DESC LIMIT ?`,
    )
    .all(listId, limit) as Row[];
  return rows.map((row) => ({
    id: row.id,
    action: row.action,
    task_id: row.task_id,
    user: row.user_id === null ? null : { id: row.user_id, username: row.username! },
    detail: JSON.parse(row.detail),
    created_at: row.created_at,
  }));
}
