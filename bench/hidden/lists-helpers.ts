// Copied next to the project under test (`<project>/.bench-hidden/`) only once the agent is done. For the "Lists"
// fixture (`bench/fixture-lists/`) and its tickets r1, r2, r3.
//
// What the hidden tests assume of the project, beyond the API of its README and of the ticket:
// - `createServer(dbPath)` of `src/app.ts`, not listening, closing its database when the server closes.
// - The database as the fixture shipped it (`PRAGMA user_version` = 6): `shippedDb()` rebuilds it from a copy of the
//   migrations of `src/db.ts`; tests seed it directly, then the agent's server opens the file and runs its own
//   migrations on top. Seeding uses only: users (id, username, password_hash, email), lists (id, owner_id, name),
//   tasks (id, list_id, title, done, position, priority, due_date, assignee_id, created_by, created_at,
//   updated_at), tags (id, list_id, name), task_tags (task_id, tag_id), comments (task_id, author_id, body).
// - Passwords stored as `<salt hex>:<scrypt hex>` with a 64-byte key, as `src/auth.ts` does (`passwordHash()`).
// - r1 only: an invitation's expiry is stored in a column whose name contains "expir" (`expireInvitations()`).
// - r2 only: indexes are looked up with `PRAGMA index_list` / `index_info` (`indexedFirstColumns()`).
import { mkdtempSync, readdirSync, readFileSync, statSync } from "node:fs";
import { randomBytes, scryptSync } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { createServer } from "../src/app.ts";

export interface Reply {
  status: number;
  body: any;
  text: string;
}

export type App = Awaited<ReturnType<typeof start>>;
export type Auth = Record<string, string>;

export async function start(dbPath?: string) {
  const server: any = dbPath === undefined ? createServer() : createServer(dbPath);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  // A failed assertion skips `close()`: the server must not keep the test process alive.
  server.unref();
  const base = `http://127.0.0.1:${server.address().port}`;
  /** `body` as a string is sent as it is, anything else as JSON. */
  const call = async (method: string, path: string, body?: unknown, headers: Auth = {}): Promise<Reply> => {
    const raw = body === undefined ? undefined : typeof body === "string" ? body : JSON.stringify(body);
    const res = await fetch(base + path, {
      method,
      headers: raw === undefined ? headers : { "content-type": "application/json", ...headers },
      body: raw,
    });
    const text = await res.text();
    let parsed: any = null;
    try {
      parsed = text ? JSON.parse(text) : null;
    } catch {
      parsed = undefined;
    }
    return { status: res.status, body: parsed, text };
  };
  const close = () =>
    new Promise<void>((resolve) => {
      server.closeAllConnections?.();
      server.close(() => resolve());
    });
  return { base, call, close };
}

export const PASSWORD = "correct horse battery";

/** Creates an account and signs in: the user's id and the headers that authenticate as them. */
export async function signUp(app: App, username: string): Promise<{ id: number; auth: Auth }> {
  const created = await app.call("POST", "/api/users", { username, password: PASSWORD });
  if (created.status !== 201) throw new Error(`sign up ${username}: ${created.status}`);
  return { id: created.body.id, auth: await signIn(app, username) };
}

export async function signIn(app: App, username: string, password = PASSWORD): Promise<Auth> {
  const session = await app.call("POST", "/api/sessions", { username, password });
  if (session.status !== 201) throw new Error(`sign in ${username}: ${session.status}`);
  return { Authorization: `Bearer ${session.body.token}` };
}

export function tempDb(): string {
  return join(mkdtempSync(join(tmpdir(), "bench-lists-")), "lists.db");
}

export function sql(path: string, query: string, ...args: unknown[]): any[] {
  const db = new DatabaseSync(path);
  try {
    return db.prepare(query).all(...args);
  } finally {
    db.close();
  }
}

/** Every text value stored in every table of the database. */
export function storedText(path: string): string {
  const tables = sql(path, "SELECT name FROM sqlite_master WHERE type = 'table'").map((t) => t.name);
  return tables.map((t) => JSON.stringify(sql(path, `SELECT * FROM "${t}"`))).join("\n");
}

/** Everything under `public/`, concatenated. */
export function publicText(): string {
  const root = fileURLToPath(new URL("../public/", import.meta.url));
  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((name: string) => {
      const path = join(dir, name);
      return statSync(path).isDirectory() ? walk(path) : [readFileSync(path, "utf8")];
    });
  try {
    return walk(root).join("\n");
  } catch {
    return "";
  }
}

export function passwordHash(password = PASSWORD): string {
  const salt = randomBytes(16);
  return `${salt.toString("hex")}:${scryptSync(password, salt, 64).toString("hex")}`;
}

const NOW = "(strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))";

// The migrations of the fixture's `src/db.ts`, as shipped.
const SHIPPED: string[] = [
  `CREATE TABLE users (id INTEGER PRIMARY KEY AUTOINCREMENT, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL,
     created_at TEXT NOT NULL DEFAULT ${NOW});
   CREATE TABLE sessions (token_hash TEXT PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
     created_at TEXT NOT NULL DEFAULT ${NOW});`,
  `CREATE TABLE lists (id INTEGER PRIMARY KEY AUTOINCREMENT, owner_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
     name TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT ${NOW});
   CREATE INDEX lists_owner ON lists(owner_id);
   CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, list_id INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
     title TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0, position INTEGER NOT NULL DEFAULT 0,
     created_by INTEGER REFERENCES users(id) ON DELETE SET NULL, created_at TEXT NOT NULL DEFAULT ${NOW});`,
  `ALTER TABLE users ADD COLUMN email TEXT;
   ALTER TABLE tasks ADD COLUMN notes TEXT NOT NULL DEFAULT '';
   ALTER TABLE tasks ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
   ALTER TABLE tasks ADD COLUMN due_date TEXT;
   ALTER TABLE tasks ADD COLUMN assignee_id INTEGER REFERENCES users(id) ON DELETE SET NULL;
   ALTER TABLE tasks ADD COLUMN updated_at TEXT;
   UPDATE tasks SET updated_at = created_at;`,
  `CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, list_id INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
     name TEXT NOT NULL, UNIQUE (list_id, name));
   CREATE TABLE task_tags (task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
     tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE);
   CREATE TABLE comments (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
     author_id INTEGER REFERENCES users(id) ON DELETE SET NULL, body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT ${NOW});`,
  `CREATE TABLE activity (id INTEGER PRIMARY KEY AUTOINCREMENT, list_id INTEGER NOT NULL, task_id INTEGER, user_id INTEGER,
     action TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '{}', created_at TEXT NOT NULL DEFAULT ${NOW});
   CREATE INDEX activity_list ON activity(list_id, id);`,
  `CREATE TABLE jobs (id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, payload TEXT NOT NULL DEFAULT '{}',
     run_at TEXT NOT NULL, dedupe_key TEXT UNIQUE, attempts INTEGER NOT NULL DEFAULT 0, last_error TEXT, done_at TEXT);
   CREATE INDEX jobs_due ON jobs(done_at, run_at);
   CREATE TABLE outbox (id INTEGER PRIMARY KEY AUTOINCREMENT, to_address TEXT NOT NULL, subject TEXT NOT NULL,
     body TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT ${NOW}, sent_at TEXT);`,
];

/** A database file with the schema the fixture shipped, filled by `seed` in one transaction. */
export function shippedDb(seed: (db: InstanceType<typeof DatabaseSync>) => void, path = tempDb()): string {
  const db = new DatabaseSync(path);
  db.exec("PRAGMA foreign_keys = ON");
  for (const migration of SHIPPED) db.exec(migration);
  db.exec(`PRAGMA user_version = ${SHIPPED.length}`);
  db.exec("BEGIN");
  seed(db);
  db.exec("COMMIT");
  db.close();
  return path;
}

/** Inserts a user who can sign in with PASSWORD; answers their id. */
export function insertUser(db: InstanceType<typeof DatabaseSync>, username: string): number {
  return Number(db.prepare("INSERT INTO users (username, password_hash) VALUES (?, ?)").run(username, passwordHash()).lastInsertRowid);
}

/** Moves every stored invitation expiry to the past, whatever the column's name holds (ISO text or a number). */
export function expireInvitations(path: string): number {
  const db = new DatabaseSync(path);
  let changed = 0;
  try {
    const tables = db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all() as { name: string }[];
    for (const { name } of tables) {
      const columns = db.prepare(`PRAGMA table_info("${name}")`).all() as { name: string }[];
      for (const column of columns.filter((c) => /expir/i.test(c.name))) {
        changed += Number(
          db
            .prepare(
              `UPDATE "${name}" SET "${column.name}" = CASE typeof("${column.name}")
                 WHEN 'text' THEN '2000-01-01T00:00:00.000Z' WHEN 'null' THEN NULL ELSE 1 END`,
            )
            .run().changes,
        );
      }
    }
  } finally {
    db.close();
  }
  return changed;
}

/** For each index of `table`, its first column (implicit indexes of UNIQUE and PRIMARY KEY constraints included). */
export function indexedFirstColumns(path: string, table: string): string[] {
  const db = new DatabaseSync(path);
  try {
    const indexes = db.prepare(`PRAGMA index_list("${table}")`).all() as { name: string }[];
    const firsts = indexes.map((index) => (db.prepare(`PRAGMA index_info("${index.name}")`).all() as { seqno: number; name: string }[]).find((c) => c.seqno === 0)?.name ?? "");
    const pk = (db.prepare(`PRAGMA table_info("${table}")`).all() as { name: string; pk: number }[]).find((c) => c.pk === 1);
    const withoutRowid = /WITHOUT\s+ROWID/i.test((db.prepare("SELECT sql FROM sqlite_master WHERE name = ?").get(table) as any)?.sql ?? "");
    if (pk && withoutRowid) firsts.push(pk.name);
    return firsts;
  } finally {
    db.close();
  }
}

/** A small seeded random generator, so that the data is the same on every run. */
export function random(seed: number): () => number {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
