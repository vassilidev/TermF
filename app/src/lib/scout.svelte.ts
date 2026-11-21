import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { library, showLibrary, type Kind, type Layer } from "$lib/library.svelte";

export type Flaw = "vague" | "contradiction" | "redundancy" | "outdated" | "missing";
export type Review = "pending" | "accepted" | "rejected";
export type Finding = { flaw: Flaw; text: string; practice: string; source: string };
export type Entry = {
  kind: Kind;
  name: string;
  /** Absent for a new item. */
  layer?: Layer;
  critique: Finding[];
  summary: string;
  sources: string[];
  content: string;
  script: string;
  scriptFile: string;
  before?: string;
  beforeScript?: string;
  executable: boolean;
  review: Review;
};
export type Draft = { model: string; budget: number; cost?: number; at: number; dropped: string[]; entries: Entry[] };
export type Logged = {
  at: number;
  project: string | null;
  action: "applied" | "noted" | "rejected";
  kind: Kind;
  name: string;
  path: string | null;
  summary: string;
  sources: string[];
};
export type ScoutState = { model: string; budget: number; draft: Draft | null; journal: Logged[] };
export type Progress = { kind: "search" | "fetch"; text: string };
export type ItemRef = { kind: Kind; name: string };

/** Kept outside the panel, so a run goes on, and shows, when the panel is closed and opened again. */
export const scout = $state({
  data: null as ScoutState | null,
  /** The scope `data` is for: a project id, or "" for the global library. */
  scope: "",
  /** The slow call in progress: "run", "revise:<kind>/<name>", or "". */
  running: "",
  started: 0,
  progress: [] as Progress[],
  error: "",
});

export function showScout() {
  showLibrary();
  library.panel = { kind: "scout" };
}

let listening: Promise<unknown> | null = null;

async function call(command: string, args: Record<string, unknown>, what = "") {
  const scope = library.scope;
  scout.error = "";
  if (what) {
    listening ??= listen<Progress>("scout-progress", (e) => {
      scout.progress = [...scout.progress, e.payload].slice(-40);
    });
    await listening;
    Object.assign(scout, { running: what, started: Date.now(), progress: [] });
  }
  try {
    const data = await invoke<ScoutState>(command, { project: scope || null, ...args });
    if (scope === library.scope) Object.assign(scout, { data, scope });
    return true;
  } catch (e) {
    scout.error = String(e);
    return false;
  } finally {
    if (what) scout.running = "";
  }
}

export const loadScout = () => call("scout_state", {});
export const runScout = (items: ItemRef[]) => call("scout_run", { items }, "run");
export const reviseEntry = (item: ItemRef, comment: string) =>
  call("scout_revise", { item, comment }, `revise:${item.kind}/${item.name}`);
export const decide = (item: ItemRef, review: Review, confirm = false) =>
  call("scout_review", { item, review, confirm });
export const discardScout = () => call("scout_discard", {});

export async function applyScout() {
  if (await call("scout_apply", {})) library.version++;
}
