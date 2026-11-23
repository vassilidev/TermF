import type { ChecklistItem } from "$lib/ui/Checklist.svelte";
import { setup, type SetupId } from "$lib/guide-logic";
import { t } from "$lib/i18n";
import { newProject } from "$lib/newproject.svelte";
import { editProject } from "$lib/project.svelte";
import { needs } from "$lib/release.svelte";
import { showSettings } from "$lib/settings.svelte";
import { app } from "$lib/state.svelte";
import { team } from "$lib/team.svelte";
import { launchTicket, showTickets, tickets } from "$lib/tickets.svelte";

const DISMISSED = "termf.setup.dismissed";

function read(): string[] {
  try {
    return JSON.parse(localStorage.getItem(DISMISSED) ?? "[]");
  } catch {
    return [];
  }
}

/** The projects whose setup list was put away. */
export const guide = $state({ dismissed: read() });

export function dismissSetup(project: string) {
  guide.dismissed.push(project);
  try {
    localStorage.setItem(DISMISSED, JSON.stringify(guide.dismissed));
  } catch {
    // Storage off: back at the next launch.
  }
}

const labels: Record<SetupId, [Parameters<typeof t>[0], Parameters<typeof t>[0]]> = {
  claude: ["aigSetupClaude", "aigSetupClaudeAction"],
  project: ["aigSetupProject", "aigSetupProjectAction"],
  tickets: ["aigSetupTickets", "aigSetupTicketsAction"],
  run: ["aigSetupRun", "aigSetupRunAction"],
  deliver: ["aigSetupDeliver", "aigSetupDeliverAction"],
};

/** A project's setup list, as the Home shows it: what is done, and the next item's action. */
export function setupItems(project: string): ChecklistItem[] {
  const runs = tickets.runs.filter((r) => r.project === project);
  const facts = {
    claude: !!needs.tools?.claude.install,
    projects: app.workspace?.projects.length ?? 0,
    tickets: (tickets.inbox ?? []).some((x) => x.project === project),
    runs: runs.length + team.runs.filter((r) => r.target.project === project).length,
    delivered: runs.some((r) => !!r.prUrl),
  };
  const actions: Record<SetupId, () => void> = {
    claude: () => showSettings("tools"),
    project: newProject,
    tickets: () => editProject(project, "tickets"),
    run: launchTicket,
    deliver: () => ((tickets.tab = "runs"), showTickets()),
  };
  return setup(facts).map(({ id, done }) => ({ id, done, label: t(labels[id][0]), action: t(labels[id][1]), run: actions[id] }));
}
