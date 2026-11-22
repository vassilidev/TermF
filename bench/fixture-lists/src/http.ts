/** An error the client caused: answered with its status and `{ "error": message }`. */
export class HttpError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export const badRequest = (message: string) => new HttpError(400, message);
export const notFound = (message: string) => new HttpError(404, message);

export interface Reply {
  status: number;
  body?: unknown;
}

export const ok = (body: unknown): Reply => ({ status: 200, body });
export const created = (body: unknown): Reply => ({ status: 201, body });
export const noContent = (): Reply => ({ status: 204 });

export interface Request {
  req: any;
  params: Record<string, string>;
  query: URLSearchParams;
}

export type Handler = (request: Request) => Reply | Promise<Reply>;

interface Route {
  method: string;
  pattern: RegExp;
  keys: string[];
  handler: Handler;
}

/** Routes like `/api/lists/:id/tasks`; a `:name` segment matches a positive integer. */
export class Router {
  #routes: Route[] = [];

  add(method: string, path: string, handler: Handler): this {
    const keys: string[] = [];
    const source = path.replace(/:(\w+)/g, (_, key: string) => {
      keys.push(key);
      return "([1-9][0-9]{0,15})";
    });
    this.#routes.push({ method, pattern: new RegExp(`^${source}$`), keys, handler });
    return this;
  }

  /** The handler for this request, `"method"` if the path exists for other methods only, or undefined. */
  match(method: string, pathname: string): { handler: Handler; params: Record<string, string> } | "method" | undefined {
    let pathExists = false;
    for (const route of this.#routes) {
      const found = route.pattern.exec(pathname);
      if (!found) continue;
      pathExists = true;
      if (route.method !== method) continue;
      const params: Record<string, string> = {};
      route.keys.forEach((key, i) => (params[key] = found[i + 1]));
      return { handler: route.handler, params };
    }
    return pathExists ? "method" : undefined;
  }
}

const MAX_BODY = 1024 * 1024;

export async function readJson(req: any): Promise<any> {
  let text = "";
  for await (const chunk of req) {
    text += chunk;
    if (text.length > MAX_BODY) throw new HttpError(413, "request body too large");
  }
  if (!text) return {};
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    throw badRequest("invalid JSON");
  }
  if (typeof body !== "object" || body === null || Array.isArray(body)) throw badRequest("the body must be a JSON object");
  return body;
}

export function send(res: any, status: number, body?: unknown): void {
  if (res.headersSent) return;
  if (body === undefined) {
    res.writeHead(status);
    res.end();
    return;
  }
  res.writeHead(status, { "content-type": "application/json; charset=utf-8" });
  res.end(JSON.stringify(body));
}
