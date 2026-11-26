import { invoke } from "@tauri-apps/api/core";
import { noVault, type Args } from "$lib/actions-logic";
import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import {
  ENVS,
  SSLMODES,
  asksPassword,
  clean,
  missingSecret,
  passwordName,
  target,
  type Check,
  type Connection,
  type Kind,
  type Query,
} from "$lib/ops-logic";
import { loadOps, ops, runSql } from "$lib/ops.svelte";
import { toast } from "$lib/ui/toast.svelte";
import type { VaultStatus } from "$lib/vault.svelte";

/** The kinds an agent adds: a database TermF reads, an SSH server, or Redis. */
const KINDS: Kind[] = ["ssh", "postgres", "mysql", "mariadb", "clickhouse", "sqlite", "redis"];

/** What the agent proposes for a secret the vault lacks: the user types it in TermF's card. */
const typeIt = (name: string, env: string, then: string) =>
  `The vault has no ${name} (${env}) for this project: propose vault.set_secret {name: "${name}", scope: "${env}"}, whose card asks the user for its value (you never see it), then ${then}.`;

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
    path: text(a.path),
    ...(asksPassword(a) ? { secrets: { password: passwordName(name) } } : {}),
  });
}

/** The fields an edit changes, as the card lists them. */
const changes = (a: Args, skip: string[]) =>
  Object.entries(a)
    .filter(([k, v]) => !skip.includes(k) && v !== null && v !== undefined)
    .map(([k, v]) => `${k} → ${String(v)}`)
    .join("\n");

/** Database connections, for what needs one. */
async function hasDatabase(project: string): Promise<string | null> {
  await loaded(project);
  return ops.connections.some((c) => c.kind !== "ssh")
    ? null
    : "This project has no database connection yet: propose ops.add_connection first (ops.test_connection after), then this action again.";
}

function queryNamed(name: unknown): Query {
  const found = ops.queries.find((q) => q.name === name);
  if (!found) throw new Error(`No saved query ${String(name)}: ${ops.queries.map((q) => q.name).join(", ") || "none yet"}.`);
  return found;
}

function connectionNamed(name: unknown): Connection {
  const found = ops.connections.find((c) => c.name === name);
  if (!found) throw new Error(`No connection ${String(name)} in this project: ${ops.connections.map((c) => c.name).join(", ") || "none yet"}.`);
  return found;
}

const addConnection: Action = {
  id: "ops.add_connection",
  description:
    "Adds a connection to the project's Ops: a database TermF reads (postgres, mysql, mariadb, clickhouse, sqlite), Redis, or an SSH server. The password of a database server or of Redis is asked to the user in TermF's card and kept in the vault, never given by you: say in `hints.password` where they find it (REDIS_PASSWORD in the server's .env); pass `no_password` only when it surely takes none. A database or Redis behind SSH names the project's SSH connection in `via` (add that one first), its `host` then as that server sees it (127.0.0.1, or a managed database's host).",
  fields: {
    name: { type: "string", description: "Letters, digits, _ . -, starting with a letter: acme-prod-db." },
    kind: { type: "string", description: "What it reaches.", enum: KINDS },
    env: { type: "string", description: "The environment it belongs to.", enum: ENVS },
    host: { type: "string", description: "SSH: an alias of ~/.ssh/config or a host name. A database server or Redis: its host (as the `via` server sees it)." },
    port: { type: "integer", description: "When not the default one." },
    user: { type: "string", description: "SSH: the login. A database: its user. Redis: its ACL user, if it has one." },
    jump: { type: "string", description: "SSH only: a bastion (ProxyJump)." },
    via: { type: "string", description: "A database server or Redis: the project's SSH connection to tunnel through." },
    database: { type: "string", description: "A database's name; SQLite: the absolute path of its file; Redis: its number (0 by default)." },
    path: { type: "string", description: "SSH only: a Laravel site's folder on the server (/home/forge/acme.com), for its recipes." },
    sslmode: { type: "string", description: "A database server's or Redis' TLS, in Postgres' words.", enum: [...SSLMODES] },
    description: { type: "string", description: "One line on what it is." },
    ai_access: { type: "string", description: "What agents may see of its output: none (default), redacted, full.", enum: ["none", "redacted", "full"] },
    no_password: {
      type: "boolean",
      description: "A database server or Redis that surely takes no password (a local Redis without requirepass): none is asked, no vault needed.",
    },
    password: { type: "string", secret: true, description: "The password of a database server, or of Redis (its requirepass, or its ACL user's)." },
  },
  required: ["name", "kind", "env"],
  asks: (a) => (asksPassword(a) ? ["password"] : []),
  needs: async (a) => (asksPassword(a) ? noVault(await invoke<VaultStatus>("vault_status")) : null),
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
    const secret = connection.secrets?.password;
    return {
      added: connection,
      password: secret ? (password ? "kept in the vault" : `not given in the card: if the vault lacks ${secret}, the test says how the user types it`) : undefined,
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
    const lacks = missingSecret(checks);
    return { connection: connection.name, ok, checks, ...(lacks ? { next: typeIt(lacks, connection.env, "this test again") } : {}) };
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
  needs: (_a, project) => hasDatabase(project),
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
  needs: (_a, project) => hasDatabase(project),
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

const EDITABLE = ["env", "host", "port", "user", "jump", "via", "database", "sslmode", "description", "ai_access"] as const;

const editConnection: Action = {
  id: "ops.edit_connection",
  description:
    "Changes one of the project's connections, as the Ops view's form does: the fields given replace its own, an empty text clears one, `rename` gives it another name. Its kind stays; its password stays in the vault (vault.set_secret replaces it).",
  fields: {
    name: { type: "string", description: "The connection's name now." },
    rename: { type: "string", description: "Its new name." },
    env: addConnection.fields.env,
    host: addConnection.fields.host,
    port: addConnection.fields.port,
    user: addConnection.fields.user,
    jump: addConnection.fields.jump,
    via: addConnection.fields.via,
    database: addConnection.fields.database,
    sslmode: addConnection.fields.sslmode,
    description: addConnection.fields.description,
    ai_access: addConnection.fields.ai_access,
  },
  required: ["name"],
  label: "actOpsEdit",
  describe: (a) => `${t("actOpsEditText", { name: String(a.name) })}\n${changes(a, ["name"])}`,
  async run(a, _secrets, project) {
    await loaded(project);
    const found = connectionNamed(a.name);
    const given = Object.fromEntries(EDITABLE.filter((k) => a[k] !== undefined && a[k] !== null).map((k) => [k, a[k]]));
    const connection = clean({ ...found, ...given, name: text(a.rename)?.trim() || found.name });
    ops.connections = await invoke<Connection[]>("ops_connection_save", { project, connection, previous: found.name, password: null, found: null, replace: false });
    toast(t("actOpsEdited", { name: connection.name }));
    return { saved: connection, next: "Test it with ops.test_connection." };
  },
};

const deleteConnection: Action = {
  id: "ops.delete_connection",
  description: "Deletes one of the project's connections, as the Ops view does. Its password stays in the vault (vault.delete_secret removes it).",
  fields: { name: { type: "string", description: "The connection's name." } },
  required: ["name"],
  label: "actOpsDelete",
  describe: (a) => {
    const c = ops.connections.find((x) => x.name === a.name);
    return t("actOpsDeleteText", { name: String(a.name), target: c ? target(c, ops.connections) : "" });
  },
  loses: (a) => t("actOpsDeleteLoses", { name: String(a.name) }),
  async run(a, _secrets, project) {
    await loaded(project);
    const name = connectionNamed(a.name).name;
    ops.connections = await invoke<Connection[]>("ops_connection_delete", { project, name });
    if (ops.connection === name) ops.connection = null;
    toast(t("actOpsDeleted", { name }));
    return { deleted: name };
  },
};

const editQuery: Action = {
  id: "ops.edit_query",
  description: "Changes a saved query of the database studio: the fields given replace its own, `rename` gives it another name.",
  fields: {
    name: { type: "string", description: "The saved query's name now." },
    rename: { type: "string", description: "Its new name." },
    sql: { type: "string", description: "The query: one that reads." },
    connection: { type: "string", description: "The database it runs on." },
    description: { type: "string", description: "One line on what it answers." },
    folder: { type: "string", description: "A folder of the saved queries." },
  },
  required: ["name"],
  label: "actOpsEditQuery",
  describe: (a) => `${t("actOpsEditQueryText", { name: String(a.name) })}\n${changes(a, ["name"])}`,
  async run(a, _secrets, project) {
    await loaded(project);
    const found = queryNamed(a.name);
    const query: Query = { ...found, name: text(a.rename)?.trim() || found.name };
    if (text(a.sql)?.trim()) query.sql = text(a.sql)!;
    for (const key of ["connection", "description", "folder"] as const) {
      const v = text(a[key]);
      if (v === undefined) continue;
      if (v.trim()) query[key] = v.trim();
      else delete query[key];
    }
    ops.queries = await invoke<Query[]>("ops_query_save", { project, query, previous: found.name });
    toast(t("opsdbSaved", { name: query.name }));
    return { saved: query.name };
  },
};

const deleteQuery: Action = {
  id: "ops.delete_query",
  description: "Deletes a saved query of the database studio.",
  fields: { name: { type: "string", description: "The saved query's name." } },
  required: ["name"],
  label: "actOpsDeleteQuery",
  describe: (a) => {
    const q = ops.queries.find((x) => x.name === a.name);
    return `${t("actOpsDeleteQueryText", { name: String(a.name) })}${q ? `\n${q.sql}` : ""}`;
  },
  loses: (a) => t("actOpsDeleteQueryLoses", { name: String(a.name) }),
  async run(a, _secrets, project) {
    await loaded(project);
    const name = queryNamed(a.name).name;
    ops.queries = await invoke<Query[]>("ops_query_delete", { project, name });
    toast(t("opsdbDeleted", { name }));
    return { deleted: name };
  },
};

const makeRecipe: Action = {
  id: "ops.make_recipe",
  description:
    "Makes a saved query a recipe of the project, as the studio's menu does: agents may then ask for it with ops_run, each run through the user's approval.",
  fields: { name: { type: "string", description: "The saved query's name." } },
  required: ["name"],
  label: "actOpsRecipe",
  describe: (a) => t("actOpsRecipeText", { name: String(a.name) }),
  async run(a, _secrets, project) {
    await loaded(project);
    const recipe = await invoke<string>("ops_query_recipe", { project, name: queryNamed(a.name).name });
    await loadOps(project);
    toast(t("opsdbRecipeDone", { name: recipe }));
    return { recipe, next: "Run it with ops_run." };
  },
};

/** The project's connections and saved queries, by name. */
export async function opsHere(project: string) {
  await loaded(project);
  return {
    connections: ops.connections.map((c) => ({ name: c.name, kind: c.kind, env: c.env })),
    queries: ops.queries.map((q) => ({ name: q.name, connection: q.connection ?? null })),
    recipes: ops.recipes.map((r) => r.name),
  };
}

export const opsActions: Action[] = [
  addConnection,
  editConnection,
  deleteConnection,
  testConnection,
  saveQuery,
  editQuery,
  deleteQuery,
  runQuery,
  makeRecipe,
];
