import { untrack } from "svelte";
import { app, ui } from "$lib/state.svelte";

/** Seen on this machine: the tour opens by itself only at the first launch. */
const seen = "termf.tour";

/** The guided tour's step, null when closed. */
export const tour = $state({ step: null as number | null });

export function startTour() {
  ui.help = false;
  tour.step = 0;
}

/** Finished or skipped: remembered either way. */
export function endTour() {
  tour.step = null;
  try {
    localStorage.setItem(seen, "1");
  } catch {
    // Storage off: it opens again at the next launch.
  }
}

/** Opens the tour once the workspace is read, at the first launch only. Returns a cleanup. */
export function watchTour(): () => void {
  return $effect.root(() => {
    let checked = false;
    $effect(() => {
      if (checked || !app.workspace) return;
      checked = true;
      let done = false;
      try {
        done = localStorage.getItem(seen) !== null;
      } catch {
        // Unreadable storage: better no tour than one at every launch.
        done = true;
      }
      if (!done) untrack(startTour);
    });
  });
}
