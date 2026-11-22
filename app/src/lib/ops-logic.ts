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
  limits: { lines: number; bytes: number; timeout_s: number };
  ai_access: AiAccess;
  approval: "always" | "write" | "never";
  redact: string[];
  run: { kind: Kind }[];
  source: "shipped" | "project";
};

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
};
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
  if (c.kind === "postgres" || c.kind === "mysql") {
    if (out.via) delete out.port;
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
  };
}

/** Seconds left before a card expires, never below zero. */
export function secondsLeft(until: number | null, now: number): number | null {
  return until === null ? null : Math.max(0, Math.round(until - now));
}
