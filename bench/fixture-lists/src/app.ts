import { createServer as createHttpServer } from "node:http";
import { readFile } from "node:fs/promises";
import { openDb, type Db } from "./db.ts";
import { HttpError, Router, created, noContent, ok, readJson, send } from "./http.ts";
import { startJobs } from "./jobs.ts";
import * as activity from "./activity.ts";
import * as auth from "./auth.ts";
import * as comments from "./comments.ts";
import * as lists from "./lists.ts";
import * as tasks from "./tasks.ts";

const PUBLIC = new URL("../public/", import.meta.url);
const TYPES: Record<string, string> = {
  html: "text/html; charset=utf-8",
  js: "text/javascript; charset=utf-8",
  css: "text/css; charset=utf-8",
};

export interface ServerOptions {
  /** Runs the reminder and digest jobs every minute (`npm start` does; the tests call `runDueJobs` themselves). */
  jobs?: boolean;
}

/** The HTTP server of the app, not listening yet. `dbPath` is a SQLite file, or `:memory:`. */
export function createServer(dbPath = ":memory:", options: ServerOptions = {}) {
  const db = openDb(dbPath);
  const router = routes(db);
  const stopJobs = options.jobs ? startJobs(db) : () => {};
  const server = createHttpServer((req: any, res: any) => {
    handle(router, req, res).catch((err: unknown) => fail(res, err));
  });
  server.on("close", () => {
    stopJobs();
    db.close();
  });
  return server;
}

function routes(db: Db): Router {
  const me = (req: any) => auth.requireUser(db, req);
  const limitOf = (query: URLSearchParams) => {
    const asked = Number(query.get("limit") ?? 50);
    return Number.isInteger(asked) && asked > 0 ? Math.min(asked, 200) : 50;
  };

  return new Router()
    .add("POST", "/api/users", async ({ req }) => created(await auth.signUp(db, await readJson(req))))
    .add("POST", "/api/sessions", async ({ req }) => created(await auth.signIn(db, await readJson(req))))
    .add("DELETE", "/api/sessions", ({ req }) => {
      auth.signOut(db, req);
      return noContent();
    })
    .add("GET", "/api/me", ({ req }) => ok(me(req)))

    .add("GET", "/api/lists", ({ req }) => ok(lists.listLists(db, me(req))))
    .add("POST", "/api/lists", async ({ req }) => {
      const user = me(req);
      return created(await lists.createList(db, user, await readJson(req)));
    })
    .add("GET", "/api/lists/:id", ({ req, params }) => ok(lists.getList(db, me(req), Number(params.id))))
    .add("PATCH", "/api/lists/:id", async ({ req, params }) => {
      const user = me(req);
      return ok(await lists.renameList(db, user, Number(params.id), await readJson(req)));
    })
    .add("DELETE", "/api/lists/:id", ({ req, params }) => {
      lists.deleteList(db, me(req), Number(params.id));
      return noContent();
    })
    .add("GET", "/api/lists/:id/activity", ({ req, params, query }) => {
      const list = lists.loadList(db, me(req), Number(params.id));
      return ok(activity.forList(db, list.id, limitOf(query)));
    })
    .add("GET", "/api/lists/:id/tags", ({ req, params }) => ok(tasks.listTags(db, me(req), Number(params.id))))

    .add("GET", "/api/lists/:id/tasks", ({ req, params, query }) => ok(tasks.listTasks(db, me(req), Number(params.id), query)))
    .add("POST", "/api/lists/:id/tasks", async ({ req, params }) => {
      const user = me(req);
      return created(await tasks.createTask(db, user, Number(params.id), await readJson(req)));
    })
    .add("GET", "/api/tasks/:id", ({ req, params }) => ok(tasks.getTask(db, me(req), Number(params.id))))
    .add("PATCH", "/api/tasks/:id", async ({ req, params }) => {
      const user = me(req);
      return ok(await tasks.updateTask(db, user, Number(params.id), await readJson(req)));
    })
    .add("DELETE", "/api/tasks/:id", async ({ req, params }) => {
      await tasks.deleteTask(db, me(req), Number(params.id));
      return noContent();
    })

    .add("GET", "/api/tasks/:id/comments", ({ req, params }) => ok(comments.listComments(db, me(req), Number(params.id))))
    .add("POST", "/api/tasks/:id/comments", async ({ req, params }) => {
      const user = me(req);
      return created(await comments.addComment(db, user, Number(params.id), await readJson(req)));
    });
}

async function handle(router: Router, req: any, res: any): Promise<void> {
  const url = new URL(req.url ?? "/", "http://localhost");
  const method: string = req.method ?? "GET";
  if (method === "GET" && !url.pathname.startsWith("/api/")) return serveStatic(url.pathname, res);

  const route = router.match(method, url.pathname);
  if (route === undefined) return send(res, 404, { error: "not found" });
  if (route === "method") return send(res, 405, { error: "method not allowed" });
  const reply = await route.handler({ req, params: route.params, query: url.searchParams });
  send(res, reply.status, reply.body);
}

async function serveStatic(pathname: string, res: any): Promise<void> {
  const name = pathname === "/" ? "index.html" : pathname.slice(1);
  const type = /^[a-z-]+\.(html|js|css)$/.test(name) ? TYPES[name.slice(name.lastIndexOf(".") + 1)] : undefined;
  let body;
  try {
    body = type && (await readFile(new URL(name, PUBLIC)));
  } catch {
    body = undefined;
  }
  if (!body) return send(res, 404, { error: "not found" });
  res.writeHead(200, { "content-type": type });
  res.end(body);
}

function fail(res: any, err: unknown): void {
  if (err instanceof HttpError) return send(res, err.status, { error: err.message });
  console.error(err);
  send(res, 500, { error: "internal error" });
}
