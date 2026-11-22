import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";
import {
  historyRow,
  shippedTexts,
  streamed,
  type Connection,
  type Plan,
  type Ran,
  type Recipe,
  type Request,
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
  sql: { connection: "", query: "", result: null as Ran | null },
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
    const overview = await invoke<{ connections: Connection[]; recipes: Recipe[]; errors: string[] }>("ops_overview", {
      project,
    });
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
