import { invoke } from "@tauri-apps/api/core";
import type { Args } from "$lib/actions-logic";
import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import { ENVS, SSLMODES, clean, isServer, passwordName, target, type Check, type Connection, type Kind, type Query } from "$lib/ops-logic";
import { loadOps, ops, runSql } from "$lib/ops.svelte";
import { toast } from "$lib/ui/toast.svelte";

/** The kinds an agent adds: a database TermF reads, or an SSH server. */
const KINDS: Kind[] = ["ssh", "postgres", "mysql", "mariadb", "clickhouse", "sqlite"];

const kindLabel = (k: Kind) => t(`opsKind${k[0].toUpperCase()}${k.slice(1)}` as Parameters<typeof t>[0]);
const text = (v: unknown) => (typeof v === "string" ? v : undefined);

/** The project's connections and queries, read when the Ops view shows another project. */
async function loaded(project: string) {
  if (ops.project !== project) await loadOps(project);
}

function connectionOf(a: Args): Connection {
  const kind = a.kind as Kind;
  const name = String(a.name ?? "").trim();
  return clean({
    name,
    kind,
    env: a.env as Connection["env"],
    ai_access: (a.ai_access as Connection["ai_access"]) ?? "none",
    description: text(a.description),
    port: typeof a.port === "number" ? a.port : undefined,
    host: text(a.host),
    user: text(a.user),
    jump: text(a.jump),
    via: text(a.via),
    database: text(a.database),
    sslmode: text(a.sslmode),
    ...(isServer(kind) ? { secrets: { password: passwordName(name) } } : {}),
  });
}

function connectionNamed(name: unknown): Connection {
  const found = ops.connections.find((c) => c.name === name);
  if (!found) throw new Error(`No connection ${String(name)} in this project: ${ops.connections.map((c) => c.name).join(", ") || "none yet"}.`);
  return found;
}

const addConnection: Action = {
  id: "ops.add_connection",
  description:
    "Adds a connection to the project's Ops: a database TermF reads (postgres, mysql, mariadb, clickhouse, sqlite) or an SSH server. A database server's password is asked to the user by TermF and kept in the vault; a database behind SSH names the project's SSH connection in `via` (add that one first), its `host` then as that server sees it (127.0.0.1, or a managed database's host).",
  fields: {
    name: { type: "string", description: "Letters, digits, _ . -, starting with a letter: acme-prod-db." },
    kind: { type: "string", description: "What it reaches.", enum: KINDS },
    env: { type: "string", description: "The environment it belongs to.", enum: ENVS },
    host: { type: "string", description: "SSH: an alias of ~/.ssh/config or a host name. A database server: its host (as the `via` server sees it)." },
    port: { type: "integer", description: "When not the default one." },
    user: { type: "string", description: "SSH: the login. A database: its user." },
    jump: { type: "string", description: "SSH only: a bastion (ProxyJump)." },
    via: { type: "string", description: "A database server: the project's SSH connection to tunnel through." },
    database: { type: "string", description: "A database's name; SQLite: the absolute path of its file." },
    sslmode: { type: "string", description: "A database server's TLS, in Postgres' words.", enum: [...SSLMODES] },
    description: { type: "string", description: "One line on what it is." },
    ai_access: { type: "string", description: "What agents may see of its output: none (default), redacted, full.", enum: ["none", "redacted", "full"] },
    password: { type: "string", secret: true, description: "A database server's password." },
  },
  required: ["name", "kind", "env"],
  asks: (a) => (isServer(a.kind as Kind) ? ["password"] : []),
  label: "actOpsAdd",
  labels: { password: "opsSecretPassword" },
  describe: (a) => {
    const c = connectionOf(a);
    const said = t("actOpsAddText", { kind: kindLabel(c.kind), name: c.name, env: c.env, target: target(c, ops.connections) });
    return c.secrets?.password ? `${said} ${t("actOpsPasswordText", { secret: c.secrets.password })}` : said;
  },
  async run(a, secrets, project) {
    await loaded(project);
    const connection = connectionOf(a);
    const password = secrets.password?.trim() || null;
    ops.connections = await invoke<Connection[]>("ops_connection_save", { project, connection, previous: null, password, found: null, replace: false });
    toast(t("actOpsAdded", { name: connection.name }));
    return {
      added: connection,
      password: isServer(connection.kind) ? (password ? "kept in the vault" : "not given: the user adds it in the vault before it connects") : undefined,
      next: "Test it with ops.test_connection.",
    };
  },
};

const testConnection: Action = {
  id: "ops.test_connection",
  description:
    "Tests one of the project's connections as the Ops view does: its client, its secrets in the vault, the tunnel, the connection, and whether a database's user can write.",
  fields: { name: { type: "string", description: "The connection's name." } },
  required: ["name"],
  label: "actOpsTest",
  describe: (a) => {
    const c = ops.connections.find((x) => x.name === a.name);
    return t("actOpsTestText", { name: String(a.name), target: c ? target(c, ops.connections) : "" });
  },
  async run(a, _secrets, project) {
    await loaded(project);
    const connection = connectionNamed(a.name);
    const checks = await invoke<Check[]>("ops_test", { project, connection, previous: connection.name });
    const ok = checks.every((c) => c.ok || c.warn);
    toast(t(ok ? "actOpsTested" : "actOpsTestFailed", { name: connection.name }), { kind: ok ? "ok" : "error" });
    return { connection: connection.name, ok, checks };
  },
};

const saveQuery: Action = {
  id: "ops.save_query",
  description: "Saves a query in the database studio, for the user to run again.",
  fields: {
    name: { type: "string", description: "The query's name." },
    sql: { type: "string", description: "The query: one that reads." },
    connection: { type: "string", description: "The database it runs on, by default the one picked in the studio." },
    description: { type: "string", description: "One line on what it answers." },
    folder: { type: "string", description: "A folder of the saved queries." },
  },
  required: ["name", "sql"],
  label: "actOpsSaveQuery",
  describe: (a) => `${t("actOpsSaveQueryText", { name: String(a.name), connection: String(a.connection ?? ops.sql.connection ?? "") })}\n${String(a.sql)}`,
  async run(a, _secrets, project) {
    await loaded(project);
    const query: Query = { name: String(a.name).trim(), sql: String(a.sql) };
    for (const key of ["connection", "description", "folder"] as const) if (text(a[key])?.trim()) query[key] = text(a[key])!.trim();
    ops.queries = await invoke<Query[]>("ops_query_save", { project, query, previous: null });
    toast(t("opsdbSaved", { name: query.name }));
    return { saved: query.name };
  },
};

const runQuery: Action = {
  id: "ops.run_query",
  description:
    "Runs a saved query in the database studio, read-only, and shows its rows to the user there: you get how many rows and the columns, never the rows.",
  fields: {
    name: { type: "string", description: "The saved query's name." },
    connection: { type: "string", description: "The database, by default the query's own." },
  },
  required: ["name"],
  label: "actOpsRunQuery",
  describe: (a) => {
    const q = ops.queries.find((x) => x.name === a.name);
    return `${t("actOpsRunQueryText", { name: String(a.name), connection: String(a.connection ?? q?.connection ?? "") })}${q ? `\n${q.sql}` : ""}`;
  },
  async run(a, _secrets, project) {
    await loaded(project);
    const query = ops.queries.find((q) => q.name === a.name);
    if (!query) throw new Error(`No saved query ${String(a.name)}: ${ops.queries.map((q) => q.name).join(", ") || "none yet"}.`);
    const connection = connectionNamed(a.connection ?? query.connection ?? ops.sql.connection);
    Object.assign(ops.sql, { connection: connection.name, query: query.sql });
    ops.tab = "sql";
    const ran = await runSql(project, connection, query.sql);
    const table = ran.output.table;
    toast(t("actOpsRan", { name: query.name }));
    return {
      query: query.name,
      connection: connection.name,
      rows: table?.rows.length ?? 0,
      columns: table?.columns ?? [],
      truncated: ran.output.truncated,
      errors: ran.output.errors || undefined,
      note: "The rows show to the user in the database studio, not to you: ask them what you need from it.",
    };
  },
};

export const opsActions: Action[] = [addConnection, testConnection, saveQuery, runQuery];
