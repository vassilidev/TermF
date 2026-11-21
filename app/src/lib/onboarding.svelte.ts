import { untrack } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { app, ui, type ViewId } from "$lib/state.svelte";

/** The project whose onboarding assistant is open. */
export const setup = $state({ project: null as string | null });

let back: ViewId = "terminals";

/** Opens the assistant where the project's last visit left it; leaving goes back to the view it was opened from. */
export function openSetup(id: string) {
  if (ui.view !== "onboarding") back = ui.view;
  ui.switcher = false;
  setup.project = id;
  ui.view = "onboarding";
}

export function closeSetup() {
  setup.project = null;
  if (ui.view === "onboarding") ui.view = back;
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
        if (ui.view === "onboarding") {
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
