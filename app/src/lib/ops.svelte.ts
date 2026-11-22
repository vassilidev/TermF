import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";
import { historyRow, type Connection, type Ran, type Recipe, type Request, type Run, type Streamed } from "$lib/ops-logic";
import { ui } from "$lib/state.svelte";

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
      if (payload.id !== ops.live.id) return;
      if (payload.kind === "end") {
        ops.live.ended = { exit: payload.exit, errors: payload.errors };
        ops.live.id = null;
      } else if (ops.live.paused) ops.live.held = last([...ops.live.held, ...payload.lines]);
      else ops.live.lines = last([...ops.live.lines, ...payload.lines]);
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
