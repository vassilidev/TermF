import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";
import {
  historyRow,
  isSql,
  runsOn,
  shippedTexts,
  streamed,
  type Connection,
  type Plan,
  type Query,
  type Ran,
  type Recipe,
  type Request,
  type SchemaTable,
  type Run,
  type Streamed,
} from "$lib/ops-logic";
import { ui } from "$lib/state.svelte";

/** A shipped recipe's English text in the user's language; a recipe's own words show as written. */
export const said = (text: string) =>
  Object.hasOwn(shippedTexts, text) ? t(shippedTexts[text] as Parameters<typeof t>[0]) : text;

export type OpsTab = "connections" | "recipes" | "sql" | "live" | "history";

/** Names and outputs only, already masked; a value of the vault reaches the webview by `ops_reveal`, on demand. */
export const ops = $state({
  loaded: false,
  /** The project the lists are of. */
  project: null as string | null,
  connections: [] as Connection[],
  recipes: [] as Recipe[],
  /** Recipe files that do not read. */
  errors: [] as string[],
  history: [] as Run[],
  /** What the agents wait for, every project. */
  requests: [] as Request[],
  tab: "connections" as OpsTab,
  /** The connection shown, or `""` for a new one being added. */
  connection: null as string | null,
  editing: null as string | null,
  recipe: null as string | null,
  /** The run form's connection and parameters, kept while the view is left. */
  form: { connection: "", params: {} as Record<string, string> },
  result: null as Ran | null,
  /** The proposal the run form answers. */
  proposal: null as number | null,
  /** The SQL console: its database, the query typed, its last result. */
  sql: { connection: "", query: "", result: null as (Ran & { query?: string }) | null },
  /** The project's saved queries. */
  queries: [] as Query[],
  /** Saved queries that do not read, left out. */
  queryErrors: [] as string[],
  /** The database studio: each database's schema once read, the side panel shown, and the DBA's conversation per
   * database (its headless session). */
  studio: {
    schemas: {} as Record<string, SchemaTable[] | "loading" | { error: string }>,
    aside: "saved" as "dba" | "saved" | "recent",
    dba: {} as Record<string, number>,
    /** The saved query being written: a new one (`previous` null) or one being changed. */
    editing: null as { query: Query; previous: string | null } | null,
  },
  /** The live logs: what is followed, and the lines received (masked), the last ones kept. */
  live: {
    recipe: "",
    connection: "",
    params: {} as Record<string, string>,
    id: null as number | null,
    lines: [] as string[],
    /** Lines received while paused, shown on resume. */
    paused: false,
    held: [] as string[],
    ended: null as { exit: number | null; errors: string } | null,
  },
  error: "",
});

/** The live lines kept: the oldest go first. */
const LIVE_LINES = 5000;
const last = (lines: string[]) => (lines.length > LIVE_LINES ? lines.slice(-LIVE_LINES) : lines);
let nextFollow = Date.now() * 1000;

/** Follows a recipe: its id is chosen here, before the start, so that no event of it comes unknown. */
export async function startFollow(project: string, recipe: string, connection: string, params: Record<string, string>) {
  const id = ++nextFollow;
  Object.assign(ops.live, { id, lines: [], held: [], paused: false, ended: null });
  try {
    return (await invoke<{ id: number; plan: Plan }>("ops_follow_start", { id, project, recipe, connection, params })).plan;
  } catch (e) {
    if (ops.live.id === id) ops.live.id = null;
    throw e;
  }
}

/** Stops the followed stream, its client with it. */
export async function stopFollow() {
  const id = ops.live.id;
  ops.live.id = null;
  if (id !== null) await invoke("ops_follow_stop", { id }).catch(() => {});
}

export function resumeFollow() {
  ops.live.lines = last([...ops.live.lines, ...ops.live.held]);
  ops.live.held = [];
  ops.live.paused = false;
}

export function showOps() {
  ui.view = "ops";
  ops.loaded = true;
}

export async function loadOps(project: string) {
  try {
    const overview = await invoke<{ connections: Connection[]; recipes: Recipe[]; errors: string[]; queries: Query[]; queryErrors: string[] }>(
      "ops_overview",
      { project },
    );
    ops.queries = overview.queries ?? [];
    ops.queryErrors = overview.queryErrors ?? [];
    ops.project = project;
    ops.connections = overview.connections;
    ops.recipes = overview.recipes;
    ops.errors = overview.errors;
    ops.error = "";
  } catch (e) {
    ops.project = project;
    ops.connections = [];
    ops.error = String(e);
  }
}

/** A database's schema, read once per session unless `again`; the vault's errors go to `ask`. */
export async function loadSchema(project: string, connection: string, again = false) {
  const key = `${project}/${connection}`;
  const known = ops.studio.schemas[key];
  if (!again && known && !(typeof known === "object" && "error" in known)) return;
  ops.studio.schemas[key] = "loading";
  try {
    ops.studio.schemas[key] = await invoke<SchemaTable[]>("ops_schema", { project, connection });
  } catch (e) {
    ops.studio.schemas[key] = { error: String(e) };
  }
}

/** The SQL console's recipe for a database: one that reads, the query free. */
export const sqlRecipe = (c: Connection | undefined) =>
  c ? ops.recipes.find((r) => isSql(r) && runsOn(r, c) && !r.params.some((p) => p.type === "sql" && p.values?.length)) : undefined;

/** Runs a query in the SQL console, read-only: its result shows there. */
export async function runSql(project: string, conn: Connection, query: string): Promise<Ran> {
  const recipe = sqlRecipe(conn);
  if (!recipe) throw new Error(t("actNoSqlRecipe", { name: conn.name }));
  const param = recipe.params.find((p) => p.type === "sql")?.name ?? "query";
  const ran = await invoke<Ran>("ops_run", {
    project,
    recipe: recipe.name,
    connection: conn.name,
    params: { [param]: query },
    confirm: null,
    proposal: null,
  });
  // The query as launched, for a full export: the result's own carries the console's LIMIT.
  ops.sql.result = { ...ran, query };
  loadHistory(project);
  return ran;
}

export async function loadHistory(project: string) {
  try {
    const entries = await invoke<Parameters<typeof historyRow>[0][]>("ops_history", { project });
    ops.history = entries.map(historyRow);
  } catch (e) {
    ops.error = String(e);
  }
}

/** The recipe of a proposal in the run form, its connection and parameters filled in. */
export function openProposal(r: Request) {
  ops.project = r.project;
  ops.tab = "recipes";
  ops.recipe = r.plan.recipe;
  ops.form = { connection: r.plan.connection, params: { ...r.plan.params } };
  ops.proposal = r.id;
  ops.result = null;
  showOps();
}

const what = (r: Request) =>
  t(r.kind === "propose" ? "opsNotifyPropose" : "opsNotifyApprove", {
    who: r.agent.role ?? r.agent.title,
    recipe: r.plan.recipe,
    connection: r.plan.connection,
  });

/** The agents' requests: listened to from the start, so the Home lists them whatever view is open. */
export function watchOps(): () => void {
  const finished = new Set<number>();
  const stops = Promise.all([
    listen<Request>("ops-approval", async ({ payload }) => {
      if (finished.has(payload.id)) return;
      ops.requests = [...ops.requests.filter((r) => r.id !== payload.id), payload];
      if (document.hasFocus()) return;
      const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
      if (granted) sendNotification({ title: t("opsNotify"), body: what(payload) });
    }),
    listen<Request>("ops-approval-done", ({ payload }) => {
      finished.add(payload.id);
      ops.requests = ops.requests.filter((r) => r.id !== payload.id);
      if (ops.project === payload.project) loadHistory(payload.project);
    }),
    listen<Streamed>("ops-follow", ({ payload }) => {
      const change = streamed(ops.live, payload, LIVE_LINES);
      if (change) Object.assign(ops.live, change);
    }),
  ]);
  stops
    .then(() => invoke<Request[]>("ops_requests"))
    .then(
      (list) => {
        const fresh = list.filter((r) => !finished.has(r.id) && !ops.requests.some((p) => p.id === r.id));
        ops.requests = [...ops.requests, ...fresh];
      },
      () => {},
    );
  return () => void stops.then((list) => list.forEach((stop) => stop()));
}

/** Answered here: gone from the cards at once, whatever the event says later. */
export function dropRequest(id: number) {
  ops.requests = ops.requests.filter((r) => r.id !== id);
}
