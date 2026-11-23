import { untrack } from "svelte";
import { creating } from "$lib/newproject.svelte";
import { app, ui } from "$lib/state.svelte";
import { introResume } from "$lib/workspace-logic";

const seen = "termf.tour";

/**
 * The first launch's introduction (`FirstLaunch`): its step, null when closed. `paused`: put away while the project
 * assistant it opened is on screen, back once that one closes.
 */
export const tour = $state({ step: null as number | null, paused: false });

export function startTour() {
  ui.help = false;
  tour.paused = false;
  tour.step = 0;
}

/** Finished or skipped: remembered either way. */
export function endTour() {
  tour.step = null;
  tour.paused = false;
  try {
    localStorage.setItem(seen, "1");
  } catch {
    // Storage off: it opens again at the next launch.
  }
}

let projectsBefore = 0;

/** From the step "your first project": the assistant takes the screen, the introduction comes back after it. */
export function pauseTour() {
  projectsBefore = app.workspace?.projects.length ?? 0;
  tour.paused = true;
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
        // Unreadable storage: better no introduction than one at every launch.
        done = true;
      }
      if (!done) untrack(startTour);
    });
    // A project created: the next step is the CTO's; the assistant left without one, the same step again.
    $effect(() => {
      if (!tour.paused || creating.open) return;
      untrack(() => {
        tour.paused = false;
        if (tour.step !== null) tour.step = introResume(tour.step, projectsBefore, app.workspace?.projects.length ?? 0);
      });
    });
  });
}
