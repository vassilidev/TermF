import { untrack } from "svelte";
import { app, ui } from "$lib/state.svelte";

const seen = "termf.tour";

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
