import { untrack } from "svelte";
import { devenv, loadEnv } from "$lib/devenv.svelte";
import { introResume, pending, readSetup, type Done, type IntroStep, type Machine, type SetupStep } from "$lib/intro-logic";
import { creating } from "$lib/newproject.svelte";
import { app, ui } from "$lib/state.svelte";
import { followStatus, vault } from "$lib/vault.svelte";

const seen = "termf.tour";
const kept = "termf.machine";

/** A setup step by the name of the screen it sets up. */
export const SETUP_LABEL = { env: "myenvView", vault: "vaultTitle", sync: "syncTitle" } as const;

/**
 * The first launch's introduction (`FirstLaunch`): its step, null when closed. `paused`: put away while the project
 * assistant it opened is on screen, back once that one closes. `only`: the setup steps put off, resumed on their own.
 */
export const tour = $state({ step: null as IntroStep | null, paused: false, only: null as SetupStep[] | null });

function read() {
  try {
    return readSetup(localStorage.getItem(kept));
  } catch {
    return readSetup(null);
  }
}

/** This machine, the first with TermF or not, and the setup steps put off until done: kept on this machine only. */
export const setup = $state(read());

function save() {
  try {
    localStorage.setItem(kept, JSON.stringify(setup));
  } catch {
    // Storage off: asked again at the next introduction.
  }
}

export function chooseMachine(machine: Machine) {
  setup.machine = machine;
  save();
}

/** The Home's offer to resume, put away: what was put off stays so. */
export function dropSetup() {
  setup.passed = [];
  save();
}

/** What is set up on this machine, null until read (`loadSetup`). */
export function setupDone(): Done | null {
  if (!vault.status || !app.sync || !devenv.manifest) return null;
  return { env: devenv.manifest.entries.length > 0, vault: vault.status.initialized, sync: app.sync.enabled };
}

/** Reads what `setupDone` needs; the environment's list again when `fresh` (the sync may have brought it). */
export async function loadSetup(fresh = false) {
  await Promise.all([followStatus(), fresh || !devenv.manifest ? loadEnv(false) : null]).catch(() => {});
}

/** The setup steps put off and still to do. */
export function pendingSetup(): SetupStep[] {
  const done = setupDone();
  return done ? pending(setup.machine, setup.passed, done) : [];
}

export function startTour() {
  ui.help = false;
  tour.paused = false;
  tour.only = null;
  tour.step = "welcome";
}

/** The setup steps put off, on their own. */
export function resumeSetup() {
  const steps = pendingSetup();
  if (!steps.length) return;
  ui.help = false;
  tour.paused = false;
  tour.only = steps;
  tour.step = steps[0];
}

/** Finished or skipped: remembered either way, with the setup steps it leaves to do (`passed`). */
export function endTour(passed?: SetupStep[]) {
  tour.step = null;
  tour.paused = false;
  tour.only = null;
  if (passed) {
    setup.passed = passed;
    save();
  }
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
