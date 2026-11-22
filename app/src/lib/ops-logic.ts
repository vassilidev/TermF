// No imports, so `pnpm test` runs it in plain Node.

export type Env = "dev" | "staging" | "prod";
export type AiAccess = "none" | "redacted" | "full";
export type Kind = "ssh" | "postgres" | "mysql" | "kubernetes" | "cloudwatch" | "gcp" | "loki" | "datadog";
export type Field =
  | "host"
  | "port"
  | "user"
  | "jump"
  | "via"
  | "database"
  | "context"
  | "namespace"
  | "log_group"
  | "region"
  | "profile"
  | "gcp_project"
  | "url"
  | "site";

/** As `[[ops.connections]]` writes it. */
export type Connection = {
  name: string;
  kind: Kind;
  env: Env;
  ai_access?: AiAccess;
  description?: string;
  port?: number;
  secrets?: Record<string, string>;
} & Partial<Record<Exclude<Field, "port">, string>>;

export type Param = {
  name: string;
  type: "text" | "int" | "duration" | "choice" | "command" | "path" | "name" | "sql";
  description: string;
  required: boolean;
  default?: string;
  values?: string[];
  min?: number;
  max?: number;
  max_len?: number;
};

export type Recipe = {
  name: string;
  description: string;
  connection?: string;
  mode: "read" | "write";
  params: Param[];
  limits: { lines: number; bytes: number; timeout_s: number; rows?: number };
  ai_access: AiAccess;
  approval: "always" | "write" | "never";
  redact: string[];
  /** Its output is a `.env`; it runs until stopped. */
  env?: boolean;
  follow?: boolean;
  run: { kind: Kind }[];
  source: "shipped" | "project";
};

/** A recipe of the SQL console: it takes a query. */
export const isSql = (r: Recipe) => r.params.some((p) => p.type === "sql");
/** A recipe of the Recipes tab: neither a query nor a stream. */
export const bounded = (r: Recipe) => !isSql(r) && !r.follow;

export type Plan = {
  recipe: string;
  connection: string;
  kind: Kind;
  env: Env;
  mode: "read" | "write";
  target: string;
  command: string;
  remote: string | null;
  tunnel: string | null;
  params: Record<string, string>;
  secrets: string[];
  limits: { lines: number; bytes: number; timeout_s: number };
  approval: string;
  redact: string[];
  aiAccess: AiAccess;
};

export type Part = { text: string } | { hidden: number };
export type Table = { columns: string[]; rows: string[][] };
export type Output = {
  text: string;
  parts: Part[];
  errors: string;
  exit: number | null;
  stopped: string | null;
  truncated: boolean;
  bytes: number;
  masks: number;
  millis: number;
  /** A query's result, masked. */
  table?: Table;
  /** A `.env`'s keys: their values stay in TermF, revealed one by one. */
  keys?: string[];
};
export type EnvDiff = { key: string; state: "same" | "different" | "prod" | "local" };
export type Streamed =
  | { id: number; kind: "lines"; lines: string[]; masks: number }
  | { id: number; kind: "end"; exit: number | null; errors: string };
export type Ran = { id: number; plan: Plan; output: Output };
export type Check = {
  code: "tool" | "secret" | "tunnel" | "connect" | "readonly" | "journal";
  ok: boolean;
  warn: boolean;
  subject: string;
  detail: string;
};
export type Request = {
  id: number;
  project: string;
  kind: "approve" | "propose";
  agent: { session: number; run: number | null; title: string; role: string | null };
  plan: Plan;
  at: number;
  until: number | null;
};

export const KINDS: Kind[] = ["ssh", "postgres", "mysql", "kubernetes", "cloudwatch", "gcp", "loki", "datadog"];
export const ENVS: Env[] = ["dev", "staging", "prod"];
export const ACCESS: AiAccess[] = ["none", "redacted", "full"];

/** What a kind's form shows, what it needs, and the secrets of the vault it may name. */
export const FORM: Record<Kind, { fields: Field[]; required: Field[]; secrets: string[] }> = {
  ssh: { fields: ["host", "user", "port", "jump"], required: ["host"], secrets: [] },
  postgres: { fields: ["database", "user", "host", "port", "via"], required: ["database", "user"], secrets: ["password"] },
  mysql: { fields: ["database", "user", "host", "port", "via"], required: ["database", "user"], secrets: ["password"] },
  kubernetes: { fields: ["context", "namespace"], required: ["context"], secrets: [] },
  cloudwatch: { fields: ["log_group", "region", "profile"], required: ["log_group"], secrets: [] },
  gcp: { fields: ["gcp_project"], required: [], secrets: [] },
  loki: { fields: ["url"], required: ["url"], secrets: ["token"] },
  datadog: { fields: ["site"], required: [], secrets: ["api_key", "app_key"] },
};

/** Whether a recipe runs on a connection: named by it, or of a kind it handles. */
export function runsOn(recipe: Recipe, c: Connection): boolean {
  return recipe.connection ? recipe.connection === c.name : recipe.run.some((v) => v.kind === c.kind);
}

/** Where a connection reaches, as TermF's cards say it (`Connection::target`). */
export function target(c: Connection, all: Connection[]): string {
  const at = (user: string | undefined, host: string) => (user ? `${user}@${host}` : host);
  switch (c.kind) {
    case "ssh":
      return c.jump ? `${at(c.user, c.host ?? "")} (via ${c.jump})` : at(c.user, c.host ?? "");
    case "postgres":
    case "mysql": {
      const db = `${at(c.user, c.host ?? "127.0.0.1")}/${c.database ?? ""}`;
      const server = c.via ? all.find((x) => x.name === c.via) : undefined;
      return server ? `${db} (tunnel through ${target(server, all)})` : db;
    }
    case "kubernetes":
      return c.namespace ? `${c.context ?? ""} / ${c.namespace}` : (c.context ?? "");
    case "cloudwatch":
      return c.log_group ?? "";
    case "gcp":
      return c.gcp_project ?? "gcloud";
    case "loki":
      return c.url ?? "";
    case "datadog":
      return `api.${c.site ?? "datadoghq.com"}`;
  }
}

/** Only what the kind takes, without empty values: what gets saved. */
export function clean(c: Connection): Connection {
  const form = FORM[c.kind];
  const out: Connection = { name: c.name.trim(), kind: c.kind, env: c.env, ai_access: c.ai_access ?? "none" };
  if (c.description?.trim()) out.description = c.description.trim();
  for (const f of form.fields) {
    const v = f === "port" ? c.port : c[f];
    if (v === undefined || v === null || String(v).trim() === "") continue;
    if (f === "port") out.port = Number(v);
    else out[f] = String(v).trim();
  }
  const secrets = Object.entries(c.secrets ?? {}).filter(([role, name]) => form.secrets.includes(role) && name.trim());
  if (secrets.length) out.secrets = Object.fromEntries(secrets.map(([r, n]) => [r, n.trim()]));
  return out;
}

/** The form's missing fields, before saving. */
export function missing(c: Connection): Field[] {
  const form = FORM[c.kind];
  const lacks = form.required.filter((f) => !String(c[f as Exclude<Field, "port">] ?? "").trim());
  if ((c.kind === "postgres" || c.kind === "mysql") && !c.via?.trim() && !c.host?.trim()) lacks.push("host");
  return lacks;
}

export type Run = {
  id: number;
  at: number;
  by: "human" | "agent" | "termf";
  session: string | null;
  connection: string;
  env: Env | null;
  recipe: string;
  approval: string;
  command: string;
  params: Record<string, string>;
  exit: number | null;
  error: string | null;
  size: number | null;
  masks: number | null;
  /** Not a run but what the user did with one: `follow`, or a `.env`'s `reveal`, `copy`, `write`. */
  event: string | null;
  key: string | null;
  run: number | null;
};

type Entry = {
  id: number;
  at: number;
  actor: { kind: "human" | "agent" | "termf"; session?: string | null };
  target?: string | null;
  scope?: Env | null;
  size?: number | null;
  masks?: number | null;
  detail?: string | null;
};

/** An audit entry of a recipe, as the history lists it. */
export function historyRow(e: Entry): Run {
  let d: Record<string, unknown> = {};
  try {
    d = JSON.parse(e.detail ?? "{}");
  } catch {
    d = {};
  }
  return {
    id: e.id,
    at: e.at,
    by: e.actor.kind,
    session: e.actor.session ?? null,
    connection: e.target ?? "",
    env: e.scope ?? null,
    recipe: String(d.recipe ?? ""),
    approval: String(d.approval ?? ""),
    command: String(d.command ?? ""),
    params: (d.params as Record<string, string>) ?? {},
    exit: typeof d.exit === "number" ? d.exit : null,
    error: typeof d.error === "string" ? d.error : null,
    size: e.size ?? null,
    masks: e.masks ?? null,
    event: typeof d.event === "string" ? d.event : null,
    key: typeof d.key === "string" ? d.key : null,
    run: typeof d.run === "number" ? d.run : null,
  };
}

/** Seconds left before a card expires, never below zero. */
export function secondsLeft(until: number | null, now: number): number | null {
  return until === null ? null : Math.max(0, Math.round(until - now));
}

/** A query's rows sorted on a column: numbers as numbers, NULL last either way. */
export function sortRows(rows: string[][], column: number, desc: boolean): string[][] {
  const number = (v: string) => (v.trim() !== "" && !Number.isNaN(Number(v)) ? Number(v) : null);
  return [...rows].sort((a, b) => {
    const [x, y] = [a[column] ?? "", b[column] ?? ""];
    if (x === y) return 0;
    if (x === "NULL") return 1;
    if (y === "NULL") return -1;
    const [p, q] = [number(x), number(y)];
    const order = p !== null && q !== null ? p - q : x.localeCompare(y, undefined, { numeric: true });
    return desc ? -order : order;
  });
}

/** A table as a spreadsheet pastes it: tab-separated, a cell's own tabs and line breaks as spaces. */
export function tsv(table: Table): string {
  const cell = (c: string) => c.replace(/[\t\r\n]+/g, " ");
  return [table.columns, ...table.rows].map((r) => r.map(cell).join("\t")).join("\n") + "\n";
}

/** The past queries of the SQL console, the most recent first, each once. */
export function pastQueries(history: Run[], recipes: string[]): { query: string; connection: string; at: number }[] {
  const seen = new Set<string>();
  const out: { query: string; connection: string; at: number }[] = [];
  for (const h of history) {
    const query = h.params.query;
    if (!recipes.includes(h.recipe) || h.event || !query || seen.has(query)) continue;
    seen.add(query);
    out.push({ query, connection: h.connection, at: h.at });
  }
  return out;
}

export type Level = "error" | "warn" | "info" | "debug";
const LEVELS: [Level, RegExp][] = [
  ["error", /\b(fatal|panic|emerg(ency)?|alert|crit(ical)?|err(or)?)\b/i],
  ["warn", /\bwarn(ing)?\b/i],
  ["debug", /\b(debug|trace)\b/i],
  ["info", /\b(info|notice)\b/i],
];
const RANK: Record<Level, number> = { debug: 0, info: 1, warn: 2, error: 3 };

/** A log line's level, read from its words; `null` when it says none. */
export function level(line: string): Level | null {
  return LEVELS.find(([, re]) => re.test(line))?.[0] ?? null;
}

/** Whether a line shows with the live filter: its text, and a level at least `min` (a line without one counts as info). */
export function shows(line: string, text: string, min: Level | "all"): boolean {
  if (text && !line.toLowerCase().includes(text.toLowerCase())) return false;
  return min === "all" || RANK[level(line) ?? "info"] >= RANK[min];
}
