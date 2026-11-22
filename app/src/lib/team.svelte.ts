import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import type { Item } from "$lib/library.svelte";
import { t } from "$lib/i18n";
import { app, ui } from "$lib/state.svelte";
import { asks, notices, upsert, type Notice, type TeamRun } from "$lib/team-run-logic";

export const team = $state({
  /** Loaded once first shown, then kept mounted. */
  loaded: false,
  /** The current project's roles, or the global layer's under every project. */
  roles: [] as Item[],
  tab: "roles" as "roles" | "runs",
  /** Every team run, most recent first. */
  runs: [] as TeamRun[],
  /** The launch form shown in place of the list. */
  launching: false,
});

export function showTeam() {
  ui.view = "team";
  team.loaded = true;
}

/** A team run shows on its ticket run's page. */
export { showRun as showTeamRun } from "$lib/commands";

/** The current project's team runs, most recent first. */
export function scopedRuns(): TeamRun[] {
  return team.runs.filter((r) => app.project === null || r.target.project === app.project);
}

/** A gate or a blocked step waits for the user. */
export function waitingRuns(): TeamRun[] {
  return team.runs.filter((r) => asks(r).length);
}

/** False when the list already holds a copy updated later. */
export function upsertRun(run: TeamRun): boolean {
  const runs = upsert(team.runs, run);
  if (runs === team.runs) return false;
  team.runs = runs;
  return true;
}

const notes: Record<Notice["kind"], Parameters<typeof t>[0]> = {
  gate: "teamRunsAskGate",
  blocked: "teamRunsAskBlocked",
  done: "teamRunsNotifyDone",
  budget: "teamRunsNotifyBudget",
};

/** Keeps the team runs current whatever view is shown, and notifies what waits while the window is in the background. */
export function watchTeam(): () => void {
  // An update heard while the list loads is newer than the list's copy, unless the list says otherwise.
  invoke<TeamRun[]>("team_runs").then(
    (list) => {
      const heard = new Map(team.runs.map((r) => [r.run, r]));
      const read = list.map((r) => {
        const known = heard.get(r.run);
        return known && known.updated >= r.updated ? known : r;
      });
      team.runs = [...team.runs.filter((r) => !list.some((l) => l.run === r.run)), ...read];
    },
    (e) => (app.error = String(e)),
  );
  const stop = listen<TeamRun>("team-updated", async ({ payload }) => {
    const before = team.runs.find((r) => r.run === payload.run);
    if (!upsertRun(payload)) return;
    const list = notices(before, payload);
    if (!list.length || document.hasFocus()) return;
    const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
    const body = list.map((n) => t(notes[n.kind], { step: n.step ?? "" })).join("\n");
    if (granted) sendNotification({ title: `${payload.target.ticket} · ${payload.target.title}`, body });
  });
  return () => void stop.then((f) => f());
}
