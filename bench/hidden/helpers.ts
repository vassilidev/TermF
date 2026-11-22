// Copied next to the project under test (`<project>/.bench-hidden/`) only once the agent is done.
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { createServer } from "../src/app.ts";

export interface Reply {
  status: number;
  body: any;
  text: string;
}

export async function start(dbPath?: string) {
  const server: any = dbPath === undefined ? createServer() : createServer(dbPath);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  // A failed assertion skips `close()`: the server must not keep the test process alive.
  server.unref();
  const base = `http://127.0.0.1:${server.address().port}`;
  /** `body` as a string is sent as it is, anything else as JSON. */
  const call = async (method: string, path: string, body?: unknown, headers: Record<string, string> = {}): Promise<Reply> => {
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

export function tempDb(): string {
  return join(mkdtempSync(join(tmpdir(), "bench-db-")), "tasks.db");
}

/** A database file as the project's first release left it (schema version 1), with these tasks. */
export function legacyDb(tasks: { title: string; done: boolean; created_at: string }[]): string {
  const path = tempDb();
  const db = new DatabaseSync(path);
  db.exec(`CREATE TABLE tasks (
     id INTEGER PRIMARY KEY AUTOINCREMENT,
     title TEXT NOT NULL,
     done INTEGER NOT NULL DEFAULT 0,
     created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
   )`);
  db.exec("PRAGMA user_version = 1");
  const insert = db.prepare("INSERT INTO tasks (title, done, created_at) VALUES (?, ?, ?)");
  for (const t of tasks) insert.run(t.title, t.done ? 1 : 0, t.created_at);
  db.close();
  return path;
}

export function sql(path: string, query: string): any[] {
  const db = new DatabaseSync(path);
  try {
    return db.prepare(query).all();
  } finally {
    db.close();
  }
}

/** Every text value stored in every table of the database. */
export function storedText(path: string): string {
  const tables = sql(path, "SELECT name FROM sqlite_master WHERE type = 'table'").map((t) => t.name);
  return tables.map((t) => JSON.stringify(sql(path, `SELECT * FROM "${t}"`))).join("\n");
}

export function page(): string {
  const read = (f: string) => {
    try {
      return readFileSync(new URL(`../public/${f}`, import.meta.url), "utf8");
    } catch {
      return "";
    }
  };
  return read("index.html") + "\n" + read("app.js");
}
