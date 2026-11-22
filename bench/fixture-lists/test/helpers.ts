import { createServer } from "../src/app.ts";

export type App = Awaited<ReturnType<typeof start>>;
export type Auth = Record<string, string>;

/** A server on a random port with an in-memory database, and a JSON client for it. */
export async function start() {
  const server: any = createServer();
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  const call = async (method: string, path: string, body?: unknown, headers: Auth = {}) => {
    const res = await fetch(base + path, {
      method,
      headers: body === undefined ? headers : { "content-type": "application/json", ...headers },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    return { status: res.status, body: res.status === 204 ? null : await res.json() };
  };
  const close = () =>
    new Promise<void>((resolve) => {
      server.closeAllConnections();
      server.close(() => resolve());
    });
  return { base, call, close };
}

export const PASSWORD = "correct horse battery";

/** Creates an account and signs in: the user, and the headers that authenticate as them. */
export async function signUp(app: App, username: string, email?: string) {
  const created = await app.call("POST", "/api/users", { username, password: PASSWORD, email });
  const session = await app.call("POST", "/api/sessions", { username, password: PASSWORD });
  return { user: created.body, auth: { Authorization: `Bearer ${session.body.token}` } as Auth };
}

export async function makeList(app: App, auth: Auth, name = "Groceries") {
  return (await app.call("POST", "/api/lists", { name }, auth)).body;
}

export async function makeTask(app: App, auth: Auth, listId: number, fields: Record<string, unknown> = {}) {
  const res = await app.call("POST", `/api/lists/${listId}/tasks`, { title: "A task", ...fields }, auth);
  if (res.status !== 201) throw new Error(`task not created: ${JSON.stringify(res.body)}`);
  return res.body;
}
