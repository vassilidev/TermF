import { DatabaseSync } from "node:sqlite";

export type Db = InstanceType<typeof DatabaseSync>;

const NOW = "(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))";

// Applied in order; `PRAGMA user_version` holds how many already ran. Never edit one that shipped: add a new one.
const migrations: string[] = [
  `CREATE TABLE users (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     username TEXT NOT NULL UNIQUE,
     password_hash TEXT NOT NULL,
     created_at TEXT NOT NULL DEFAULT ${NOW}
   );
   CREATE TABLE sessions (
     token_hash TEXT PRIMARY KEY,
     user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
     created_at TEXT NOT NULL DEFAULT ${NOW}
   );`,

  `CREATE TABLE lists (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
     name TEXT NOT NULL,
     created_at TEXT NOT NULL DEFAULT ${NOW}
   );
   CREATE INDEX lists_owner ON lists(owner_id);
   CREATE TABLE tasks (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     list_id INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
     title TEXT NOT NULL,
     done INTEGER NOT NULL DEFAULT 0,
     position INTEGER NOT NULL DEFAULT 0,
     created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
     created_at TEXT NOT NULL DEFAULT ${NOW}
   );`,

  `ALTER TABLE users ADD COLUMN email TEXT;
   ALTER TABLE tasks ADD COLUMN notes TEXT NOT NULL DEFAULT '';
   ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
   ALTER TABLE tasks ADD COLUMN due_date TEXT;
   ALTER TABLE tasks ADD COLUMN assignee_id INTEGER REFERENCES users(id) ON DELETE SET NULL;
   ALTER TABLE tasks ADD COLUMN updated_at TEXT;
   UPDATE tasks SET updated_at = created_at;`,

  `CREATE TABLE tags (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     list_id INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
     name TEXT NOT NULL,
     UNIQUE (list_id, name)
   );
   CREATE TABLE task_tags (
     task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
     tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE
   );
   CREATE TABLE comments (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
     author_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
     body TEXT NOT NULL,
     created_at TEXT NOT NULL DEFAULT ${NOW}
   );`,

  `CREATE TABLE activity (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     list_id INTEGER NOT NULL,
     task_id INTEGER,
     user_id INTEGER,
     action TEXT NOT NULL,
     detail TEXT NOT NULL DEFAULT '{}',
     created_at TEXT NOT NULL DEFAULT ${NOW}
   );
   CREATE INDEX activity_list ON activity(list_id, id);`,

  `CREATE TABLE jobs (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     kind TEXT NOT NULL,
     payload TEXT NOT NULL DEFAULT '{}',
     run_at TEXT NOT NULL,
     dedupe_key TEXT UNIQUE,
     attempts INTEGER NOT NULL DEFAULT 0,
     last_error TEXT,
     done_at TEXT
   );
   CREATE INDEX jobs_due ON jobs(done_at, run_at);
   CREATE TABLE outbox (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     to_address TEXT NOT NULL,
     subject TEXT NOT NULL,
     body TEXT NOT NULL,
     created_at TEXT NOT NULL DEFAULT ${NOW},
     sent_at TEXT
   );`,
];

export function openDb(path = ":memory:"): Db {
  const db = new DatabaseSync(path);
  db.exec("PRAGMA foreign_keys = ON");
  migrate(db);
  return db;
}

export function migrate(db: Db): void {
  const { user_version: version } = db.prepare("PRAGMA user_version").get() as { user_version: number };
  for (let i = version; i < migrations.length; i++) {
    db.exec("BEGIN");
    try {
      db.exec(migrations[i]);
      db.exec(`PRAGMA user_version = ${i + 1}`);
      db.exec("COMMIT");
    } catch (err) {
      db.exec("ROLLBACK");
      throw err;
    }
  }
}

/** Runs `fn` in a transaction: all of its writes, or none. */
export function transaction<T>(db: Db, fn: () => T): T {
  db.exec("BEGIN");
  try {
    const result = fn();
    db.exec("COMMIT");
    return result;
  } catch (err) {
    db.exec("ROLLBACK");
    throw err;
  }
}

export const now = (): string => new Date().toISOString();
