import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "$lib/i18n";
import { app, shortPath, ui, type ViewId } from "$lib/state.svelte";
// Each module reads the other only inside functions, so the import cycle is safe.
import { creating, openProjectSetup } from "$lib/newproject.svelte";

/** The project whose setup is open, if it exists: the Home reads its state again once it closes. */
export const setup = $state({ project: null as string | null });

/** What runs, by key: a project's analysis (its id, or "" for a new one), or an advice (`advice:<question>`). */
export const analyzing = $state<Record<string, boolean>>({});

/** `view`, or Terminals when it no longer has anything to show (its page was closed meanwhile). */
export function usable(view: ViewId): ViewId {
  const gone = (view === "project" && ui.editing === null) || (view === "newProject" && !creating.open);
  return gone ? "terminals" : view;
}

/** The setup of an existing project, from what TermF understood of it. */
export function openSetup(id: string) {
  openProjectSetup(id);
}

/** A file Claude Code reads, or a search it makes, during an analysis. */
export type Read = { tool: string; target: string };
export const readText = (r: Read) =>
  r.tool === "Read" ? t("liveRead", { target: shortPath(r.target) }) : t(r.tool === "Grep" ? "liveGrep" : "liveGlob", { target: r.target });
/** Each analysis or advice as it goes, by key: how many reads so far, and the last ones. */
export const reads = $state<Record<string, { count: number; last: Read[] }>>({});
let listening: Promise<unknown> | null = null;

/** Runs `work`, its reads shown under `key` as they come. */
export function follow<T>(key: string, work: () => Promise<T>): Promise<T> {
  analyzing[key] = true;
  reads[key] = { count: 0, last: [] };
  listening ??= listen<Read & { project: string }>("onboarding-progress", ({ payload: { project: id, ...read } }) => {
    const seen = reads[id] ?? { count: 0, last: [] };
    reads[id] = { count: seen.count + 1, last: [...seen.last, read].slice(-6) };
  }).catch((e) => {
    // Not kept: the next one tries to listen again.
    listening = null;
    throw e;
  });
  // Listening first: the first reads come within a second.
  return listening.then(work).finally(() => delete analyzing[key]);
}

/**
 * The setup follows the current project, and opens by itself once for a project created outside it (the backend
 * remembers it was shown).
 */
export function watchSetup() {
  return $effect.root(() => {
    $effect(() => {
      const id = app.project;
      if (id === null) return;
      invoke<{ autoOpen: boolean }>("onboarding_state", { project: id }).then(async (state) => {
        if (!state.autoOpen || app.project !== id) return;
        await invoke("onboarding_seen", { project: id });
        openSetup(id);
      }, () => {});
    });
  });
}
