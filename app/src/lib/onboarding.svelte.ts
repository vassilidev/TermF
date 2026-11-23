import { untrack } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Step } from "$lib/onboarding-logic";
import { t } from "$lib/i18n";
import { app, onScreen, panes, shortPath, ui, type ViewId } from "$lib/state.svelte";
// Each module reads the other only inside functions, so the import cycle is safe.
import { closePanel } from "$lib/panes.svelte";
import { creating } from "$lib/newproject.svelte";

/** The project whose onboarding assistant is open; `at`: the step it opens on, else where it was left. */
export const setup = $state({ project: null as string | null, at: null as Step | null });

/** Projects whose analysis is running: a remounted assistant waits for it instead of paying for another. */
export const analyzing = $state<Record<string, boolean>>({});
const analyses = new Map<string, Promise<unknown>>();

let back: ViewId = "terminals";

/** `view`, or Terminals when it no longer has anything to show (its page was closed meanwhile). */
export function usable(view: ViewId): ViewId {
  const gone =
    (view === "project" && ui.editing === null) ||
    (view === "onboarding" && setup.project === null) ||
    (view === "newProject" && !creating.open);
  return gone ? "terminals" : view;
}

/** Opens the assistant where the project's last visit left it; leaving goes back to the view it was opened from. */
export function openSetup(id: string, at: Step | null = null) {
  if (ui.view !== "onboarding") back = ui.view;
  ui.switcher = false;
  setup.at = at;
  setup.project = id;
  ui.view = "onboarding";
}

export function closeSetup() {
  setup.project = null;
  // Beside another view, its panel closes.
  if (panes.other !== null && onScreen("onboarding")) closePanel("onboarding");
  else if (ui.view === "onboarding") ui.view = usable(back);
}

/** A file Claude Code reads, or a search it makes, during an analysis. */
export type Read = { tool: string; target: string };
export const readText = (r: Read) =>
  r.tool === "Read" ? t("liveRead", { target: shortPath(r.target) }) : t(r.tool === "Grep" ? "liveGrep" : "liveGlob", { target: r.target });
/** Each project's analysis as it goes: how many reads so far, and the last ones. */
export const reads = $state<Record<string, { count: number; last: Read[] }>>({});
let listening: Promise<unknown> | null = null;

/** The project's analysis: the one running, else a new one. */
export function analyze<T>(project: string, lang: string): Promise<T> {
  let run = analyses.get(project);
  if (!run) {
    analyzing[project] = true;
    reads[project] = { count: 0, last: [] };
    listening ??= listen<Read & { project: string }>("onboarding-progress", ({ payload: { project: id, ...read } }) => {
      const seen = reads[id] ?? { count: 0, last: [] };
      reads[id] = { count: seen.count + 1, last: [...seen.last, read].slice(-6) };
    }).catch((e) => {
      // Not kept: the next analysis tries to listen again.
      listening = null;
      throw e;
    });
    // Listening first: the first reads come within a second.
    run = listening.then(() => invoke<T>("onboarding_analyze", { project, lang })).finally(() => {
      analyses.delete(project);
      delete analyzing[project];
    });
    analyses.set(project, run);
  }
  return run as Promise<T>;
}

/** The analysis of the project running in this window, if any. */
export function running(project: string): Promise<unknown> | undefined {
  return analyses.get(project);
}

/**
 * The assistant follows the current project, and opens by itself once for a project just created (the backend
 * remembers it was shown).
 */
export function watchSetup() {
  return $effect.root(() => {
    $effect(() => {
      const id = app.project;
      untrack(() => {
        if (onScreen("onboarding")) {
          if (id === null) return closeSetup();
          setup.project = id;
        }
      });
      if (id === null) return;
      invoke<{ autoOpen: boolean }>("onboarding_state", { project: id }).then(async (state) => {
        if (!state.autoOpen || app.project !== id) return;
        await invoke("onboarding_seen", { project: id });
        openSetup(id);
      }, () => {});
    });
  });
}
