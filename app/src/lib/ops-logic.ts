// No imports, so `pnpm test` runs it in plain Node.

export type Env = "dev" | "staging" | "prod";
export type AiAccess = "none" | "redacted" | "full";
export type Kind =
  | "ssh"
  | "postgres"
  | "mysql"
  | "kubernetes"
  | "cloudwatch"
  | "gcp"
  | "loki"
  | "datadog"
  | "sqlite"
  | "mariadb"
  | "clickhouse";
export type Field =
  | "host"
  | "port"
  | "user"
  | "jump"
  | "via"
  | "database"
  | "sslmode"
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
  /** What went wrong, read from what the client said. */
  hint?: "keyRefused" | "hostKey" | "unknownHost" | "portClosed" | "timeout" | "passwordRefused" | "noDatabase" | "noFile";
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

export const KINDS: Kind[] = [
  "ssh",
  "postgres",
  "mysql",
  "mariadb",
  "sqlite",
  "clickhouse",
  "kubernetes",
  "cloudwatch",
  "gcp",
  "loki",
  "datadog",
];
export const ENVS: Env[] = ["dev", "staging", "prod"];
export const ACCESS: AiAccess[] = ["none", "redacted", "full"];

/** What a kind's form shows, what it needs, and the secrets of the vault it may name. */
export const FORM: Record<Kind, { fields: Field[]; required: Field[]; secrets: string[] }> = {
  ssh: { fields: ["host", "user", "port", "jump"], required: ["host"], secrets: [] },
  postgres: { fields: ["database", "user", "host", "port", "via", "sslmode"], required: ["database", "user"], secrets: ["password"] },
  mysql: { fields: ["database", "user", "host", "port", "via", "sslmode"], required: ["database", "user"], secrets: ["password"] },
  mariadb: { fields: ["database", "user", "host", "port", "via", "sslmode"], required: ["database", "user"], secrets: ["password"] },
  clickhouse: { fields: ["database", "user", "host", "port", "via", "sslmode"], required: ["database", "user"], secrets: ["password"] },
  sqlite: { fields: ["database"], required: ["database"], secrets: [] },
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
    case "sqlite":
      return c.database ?? "";
    case "postgres":
    case "mysql":
    case "mariadb":
    case "clickhouse": {
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
  if (isServer(c.kind) && !c.via?.trim() && !c.host?.trim()) lacks.push("host");
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

/** The shipped recipes' English texts (`library/recipes`), with the i18n key that says each in the user's language. */
export const shippedTexts: Record<string, string> = {
  "The last lines of a service's journal (journalctl) or of a log file, on a server reached by SSH": "uxfT4RecipeLogsTail",
  "Restarts a systemd service on a server reached by SSH (sudo without a password)": "uxfT4RecipeServiceRestart",
  "A command that only reads, from a fixed list (uptime, disk, memory, a service's status, containers…), on a server reached by SSH":
    "uxfT4RecipeSshReadonly",
  "The last log lines of a Kubernetes pod or deployment (deploy/web)": "uxfT4RecipeK8sLogs",
  "The pods of a Kubernetes namespace: state, restarts, age and node": "uxfT4RecipeK8sPods",
  "Recent log events from CloudWatch, Google Cloud Logging, Loki or Datadog": "uxfT4RecipeLogsCloud",
  "How many lines at most": "uxfT4ParamLines",
  "How many events at most": "uxfT4ParamEvents",
  "How far back, such as 15m, 1h or 2d": "uxfT4ParamSince",
  "How far back, such as 15m, 1h or 2d (journal only)": "uxfT4ParamSinceJournal",
  "What to run": "uxfT4ParamCommand",
  "The systemd unit, for systemctl status": "uxfT4ParamUnitStatus",
  "The systemd unit to restart, such as nginx.service": "uxfT4ParamUnitRestart",
  "A systemd unit, such as nginx.service (journal only)": "uxfT4ParamUnit",
  "A log file instead of the journal, such as /var/log/app/error.log": "uxfT4ParamFile",
  "The service's own query instead: a LogQL selector for Loki, a filter for Google Cloud": "uxfT4ParamQuery",
  "The pod, or deploy/<name> for one of a deployment's pods": "uxfT4ParamPod",
  "The container, when the pod has several": "uxfT4ParamContainer",
  "The folder, for du and ls": "uxfT4ParamPath",
  "Only the pods with these labels, such as app=web": "uxfT4ParamSelector",
  "Only the lines that match this pattern": "uxfT4ParamGrepPattern",
  "Only the lines that hold this text": "uxfT4ParamGrepLines",
  "Only the events that hold this text": "uxfT4ParamGrepEvents",
  "Reads a server's .env over SSH and shows it masked: each value revealed on its own; agents get only the key names":
    "uxfT4RecipeEnvFetch",
  "The .env file on the server, such as /srv/app/shared/.env": "uxfT4ParamEnvFile",
  "A read-only SQL query on a Postgres or MySQL database: one SELECT, EXPLAIN or SHOW, in a read-only transaction":
    "uxfT4RecipeSqlReadonly",
  "One SELECT, WITH … SELECT, EXPLAIN or SHOW": "uxfT4ParamSql",
  "Follows logs live until stopped: a journal or a file over SSH, a Kubernetes pod, CloudWatch or Loki": "uxfT4RecipeLogsFollow",
  "SSH: a systemd unit, such as nginx.service": "uxfT4ParamFollowUnit",
  "SSH: a log file instead of the journal, such as /var/log/app/error.log": "uxfT4ParamFollowFile",
  "Kubernetes: the pod, or deploy/<name>": "uxfT4ParamFollowPod",
  "Kubernetes: the container, when the pod has several": "uxfT4ParamFollowContainer",
  'Loki: a LogQL selector, such as {app="web"}': "uxfT4ParamFollowQuery",
  "How many past lines to start with": "uxfT4ParamFollowLines",
};

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
export function pastQueries(
  history: Run[],
  recipes: Pick<Recipe, "name" | "params">[],
): { query: string; connection: string; at: number }[] {
  // Each recipe names its query parameter as it likes.
  const param = new Map(
    recipes.flatMap((r) => {
      const p = r.params.find((x) => x.type === "sql");
      return p ? [[r.name, p.name] as const] : [];
    }),
  );
  const seen = new Set<string>();
  const out: { query: string; connection: string; at: number }[] = [];
  for (const h of history) {
    const name = param.get(h.recipe);
    const query = name === undefined ? undefined : h.params[name];
    if (h.event || !query || seen.has(query)) continue;
    seen.add(query);
    out.push({ query, connection: h.connection, at: h.at });
  }
  return out;
}

/** A query the SQL console refused, as the backend sends it (`{refused: {code, subject}, message}`); `null` for
 * any other error. */
export function refusal(e: unknown): { code: string; subject: string; message: string } | null {
  try {
    const sent = JSON.parse(String(e));
    if (typeof sent?.refused?.code !== "string") return null;
    return { code: sent.refused.code, subject: String(sent.refused.subject ?? ""), message: String(sent.message ?? "") };
  } catch {
    return null;
  }
}

/** The live logs' state that its events change. */
export type Live = {
  id: number | null;
  paused: boolean;
  lines: string[];
  held: string[];
  ended: { exit: number | null; errors: string } | null;
};

/** What an `ops-follow` event changes, or `null` when it is not of the stream followed. The front end picks the
 * stream's id before starting it, so its first lines, which may come before the start answers, are kept. */
export function streamed(live: Live, e: Streamed, max = 5000): Partial<Live> | null {
  if (e.id !== live.id) return null;
  if (e.kind === "end") return { id: null, ended: { exit: e.exit, errors: e.errors } };
  const last = (lines: string[]) => (lines.length > max ? lines.slice(-max) : lines);
  return live.paused ? { held: last([...live.held, ...e.lines]) } : { lines: last([...live.lines, ...e.lines]) };
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

/** A database the SQL console reads. */
export const isDb = (k: Kind) => k === "postgres" || k === "mysql" || k === "mariadb" || k === "sqlite" || k === "clickhouse";
/** A database server, reached over the network (SQLite is a file). */
export const isServer = (k: Kind) => isDb(k) && k !== "sqlite";
export const SSLMODES = ["disable", "prefer", "require", "verify-ca", "verify-full"] as const;

export type SchemaTable = {
  schema: string;
  name: string;
  columns: { name: string; type: string; nullable: boolean; key: "pk" | "fk" | "unique" | "index" | "" }[];
  indexes: { name: string; columns: string; unique: boolean }[];
};

/** A saved query of the SQL console, as `[[ops.queries]]` keeps it. */
export type Query = {
  name: string;
  description?: string;
  folder?: string;
  tags?: string[];
  connection?: string;
  sql: string;
};

/** A database a repository of the project declares (`ops_detect`). */
export type Candidate = {
  id: number;
  source: string;
  connection: Connection | null;
  refused: string | null;
  hasPassword: boolean;
  /** Its engine as its URL names it, to word a refusal. */
  scheme: string;
};

export type ExportFormat = "csv" | "tsv" | "json" | "ndjson" | "markdown" | "sql" | "xlsx";
export const EXPORTS: ExportFormat[] = ["csv", "tsv", "json", "ndjson", "markdown", "sql", "xlsx"];
export const COPIES: ExportFormat[] = ["tsv", "csv", "json", "markdown"];
export const EXTENSIONS: Record<ExportFormat, string> = {
  csv: "csv",
  tsv: "tsv",
  json: "json",
  ndjson: "ndjson",
  markdown: "md",
  sql: "sql",
  xlsx: "xlsx",
};

/** The table a query reads first, for an export's file and its `INSERT`s; `results` when it names none. */
export function firstTable(sql: string): string {
  const m = /\bfrom\s+([`"[]?)([\w.]+)[`"\]]?/i.exec(sql);
  return m ? m[2].split(".").pop()! : "results";
}

/** The last query an answer gives in a ```sql block (or a bare ``` block), as it streams: `null` before one ends. */
export function sqlBlock(text: string): string | null {
  const blocks = [...text.matchAll(/```(?:sql|SQL|[a-z]*)?[ \t]*\n([\s\S]*?)\n?```/g)];
  const last = blocks.at(-1)?.[1].trim();
  return last ? last : null;
}

/** What the user asked, read back from the message TermF sent the DBA (the first one holds the schema too). */
export function asked(text: string): { kind: "write" | "explain" | "optimize"; text: string } {
  const marks = [
    ["write", "Request: "],
    ["explain", "Explain this query in plain words"],
    ["optimize", "This query is slow."],
  ] as const;
  const [kind, mark] = marks.reduce((a, b) => (text.lastIndexOf(b[1]) > text.lastIndexOf(a[1]) ? b : a));
  const at = text.lastIndexOf(mark);
  if (at < 0) return { kind: "write", text };
  return kind === "write" ? { kind, text: text.slice(at + mark.length) } : { kind, text: sqlBlock(text) ?? "" };
}

/** Saved queries matching a search (name, description, folder, tags, the query itself), grouped by folder, those
 * without one last. */
export function savedGroups(queries: Query[], search: string): { folder: string; queries: Query[] }[] {
  const words = search.toLowerCase().split(/\s+/).filter(Boolean);
  const hay = (q: Query) =>
    [q.name, q.description ?? "", q.folder ?? "", ...(q.tags ?? []), q.sql, q.connection ?? ""].join(" ").toLowerCase();
  const found = queries.filter((q) => words.every((w) => hay(q).includes(w)));
  const folders = new Map<string, Query[]>();
  for (const q of found) folders.set(q.folder ?? "", [...(folders.get(q.folder ?? "") ?? []), q]);
  return [...folders.entries()]
    .sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)))
    .map(([folder, list]) => ({ folder, queries: list.sort((a, b) => a.name.localeCompare(b.name)) }));
}

/** A name not taken yet: `Name (copy)`, `Name (copy 2)`… */
export function copyName(name: string, taken: string[], copy: string): string {
  let n = 1;
  let next = `${name} (${copy})`;
  while (taken.includes(next)) next = `${name} (${copy} ${++n})`;
  return next;
}

/** What the editor completes: tables (qualified outside the default schema), then the columns of every table. */
export function completions(schema: SchemaTable[]): { label: string; type: "class" | "property"; detail: string }[] {
  const tables = schema.map((t) => ({
    label: t.schema ? `${t.schema}.${t.name}` : t.name,
    type: "class" as const,
    detail: `${t.columns.length}`,
  }));
  const seen = new Set<string>();
  const columns = schema.flatMap((t) =>
    t.columns.flatMap((c) => {
      const key = `${c.name}:${c.type}`;
      if (seen.has(key)) return [];
      seen.add(key);
      return [{ label: c.name, type: "property" as const, detail: c.type }];
    }),
  );
  return [...tables, ...columns];
}

/** The secret's name a database's password goes under in the vault: `ACME_PROD_PASSWORD`. */
export function passwordName(connection: string): string {
  return `${connection.toUpperCase().replace(/[^A-Z0-9_]/g, "_")}_PASSWORD`;
}

/** Why TermF takes no such database, as the interface says it (`opsdbRefused*`); `null` for a database it takes. */
export function engineRefused(scheme: string): "Sqlserver" | "Mongodb" | "Redis" | "Other" | null {
  const s = scheme.trim().toLowerCase();
  if (s === "sqlserver" || s === "mssql") return "Sqlserver";
  if (s === "mongodb" || s === "mongodb+srv") return "Mongodb";
  if (s === "redis" || s === "rediss") return "Redis";
  const taken = ["postgres", "postgresql", "pgsql", "mysql", "mysql2", "mariadb", "sqlite", "sqlite3", "file", "clickhouse"];
  return taken.includes(s) ? null : "Other";
}

/** A password the backend would not put in the vault unasked: the secret it names, and the connection that has it. */
export function secretClash(e: unknown): { name: string; owner: string | null } | null {
  const text = String(e);
  const owned = /the secret (\S+) already holds the password of (\S+?):/.exec(text);
  if (owned) return { name: owned[1], owner: owned[2] };
  const held = /the vault already has the secret (\S+) \(/.exec(text);
  return held ? { name: held[1], owner: null } : null;
}

/** What a full export runs again: the query as the user launched it, without the console's `LIMIT`. */
export function exportedQuery(result: Ran & { query?: string }, param: string): string {
  return result.query ?? result.plan.params[param] ?? "";
}
