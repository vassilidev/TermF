import { createServer as createHttpServer } from "node:http";
import { readFile } from "node:fs/promises";
import { openDb, type Db } from "./db.ts";
import * as tasks from "./tasks.ts";

const PUBLIC = new URL("../public/", import.meta.url);
const STATIC: Record<string, [string, string]> = {
  "/": ["index.html", "text/html; charset=utf-8"],
  "/app.js": ["app.js", "text/javascript; charset=utf-8"],
};

/** The HTTP server of the app, not listening yet. `dbPath` is a SQLite file, or `:memory:`. */
export function createServer(dbPath = ":memory:") {
  const db = openDb(dbPath);
  const server = createHttpServer((req: any, res: any) => {
    handle(db, req, res).catch((err: Error) => send(res, 500, { error: err.message }));
  });
  server.on("close", () => db.close());
  return server;
}

async function handle(db: Db, req: any, res: any): Promise<void> {
  const url = new URL(req.url ?? "/", "http://localhost");
  const method: string = req.method ?? "GET";

  const file = STATIC[url.pathname];
  if (file && method === "GET") {
    const body = await readFile(new URL(file[0], PUBLIC));
    res.writeHead(200, { "content-type": file[1] });
    res.end(body);
    return;
  }

  if (url.pathname === "/api/tasks") {
    if (method === "GET") {
      const done = url.searchParams.get("done") ?? undefined;
      return send(res, 200, tasks.listTasks(db, { done }));
    }
    if (method === "POST") {
      const body = await readJson(req);
      return send(res, 201, tasks.createTask(db, body.title));
    }
    return send(res, 405, { error: "method not allowed" });
  }

  const match = /^\/api\/tasks\/(\d+)$/.exec(url.pathname);
  if (match) {
    const id = Number(match[1]);
    if (method === "GET") {
      const task = tasks.getTask(db, id);
      return task ? send(res, 200, task) : send(res, 404, { error: "task not found" });
    }
    if (method === "PATCH") {
      const body = await readJson(req);
      const task = tasks.updateTask(db, id, { title: body.title, done: body.done });
      return task ? send(res, 200, task) : send(res, 404, { error: "task not found" });
    }
    if (method === "DELETE") {
      return tasks.deleteTask(db, id) ? send(res, 204) : send(res, 404, { error: "task not found" });
    }
    return send(res, 405, { error: "method not allowed" });
  }

  send(res, 404, { error: "not found" });
}

async function readJson(req: any): Promise<any> {
  let text = "";
  for await (const chunk of req) text += chunk;
  return text ? JSON.parse(text) : {};
}

function send(res: any, status: number, body?: unknown): void {
  if (body === undefined) {
    res.writeHead(status);
    res.end();
    return;
  }
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}
