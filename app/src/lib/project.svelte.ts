import type { Component } from "svelte";
import BrainCircuit from "@lucide/svelte/icons/brain-circuit";
import Folder from "@lucide/svelte/icons/folder";
import GitBranch from "@lucide/svelte/icons/git-branch";
import LibraryBig from "@lucide/svelte/icons/library-big";
import ShieldCheck from "@lucide/svelte/icons/shield-check";
import Ticket from "@lucide/svelte/icons/ticket";
import Trash2 from "@lucide/svelte/icons/trash-2";
import Users from "@lucide/svelte/icons/users";
import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
import { allRepos, currentProject, scopeRepos } from "$lib/files.svelte";
import type { t } from "$lib/i18n";
import { projectsOf, tally, type Tally } from "$lib/nav-logic";
import { app, grid, held, launch, onScreen, panes, selected, ui, type Repo, type Session, type ViewId } from "$lib/state.svelte";
import { usable } from "$lib/onboarding.svelte";
// Each module reads the other only inside functions, so the import cycle is safe.
import { closePanel } from "$lib/panes.svelte";
import { tickets } from "$lib/tickets.svelte";

const projects = () => app.workspace?.projects ?? [];

export function sessionProjects(session: Session): string[] {
  return projectsOf(session.cwd, projects(), tickets.runs);
}

/** Whether the session's tile shows under the current project, or under every project's tiles. */
export function shown(session: Session): boolean {
  return grid.all || app.project === null || sessionProjects(session).includes(app.project);
}

/** Agent states per project id, "" outside every project, "*" for all. */
export function tallies(): Record<string, Tally> {
  return tally(app.sessions, projects(), tickets.runs);
}

/** The repositories outside the current project, for the launcher's folded list. */
export function otherRepos(): Repo[] {
  const own = currentProject()?.repos ?? [];
  return app.project === null ? [] : allRepos().filter((r) => !own.some((o) => o.path === r.path));
}

/** The ticked repositories the launcher shows: those of another project stay ticked, out of the count. */
export function launchable(): string[] {
  const repos = [...scopeRepos(), ...(launch.others ? otherRepos() : [])];
  return repos.filter((r) => selected.has(r.path)).map((r) => r.path);
}

type Key = Parameters<typeof t>[0];
export type ConfigSection = "project" | "setup" | "tickets" | "team" | "library" | "worktrees" | "memory" | "trust" | "delete";

/** Each section of a project's Configuration, with the texts its search looks into. */
export const configSections: { id: ConfigSection; label: Key; icon: Component; words: Key[] }[] = [
  { id: "project", label: "runMetaProject", icon: Folder, words: ["projectName", "repos", "t5AddRepos", "stackOffersTitle"] },
  { id: "setup", label: "ux3t3Setup", icon: WandSparkles, words: ["obConfigure", "obConfigureHint", "obJournal"] },
  {
    id: "tickets",
    label: "viewTickets",
    icon: Ticket,
    words: ["ticketsHelp", "ticketsGithubIssues", "ticketsGithubProject", "ticketsJira", "ticketsMcp"],
  },
  { id: "team", label: "t5Team", icon: Users, words: ["t6bHelpTeam", "metaRoles", "t5EditRoles", "rsSquads", "rsStaff"] },
  { id: "library", label: "viewLibrary", icon: LibraryBig, words: ["t6bHelpLibrary"] },
  { id: "worktrees", label: "wtTitle", icon: GitBranch, words: ["wtIntro", "t5RecipeEdit"] },
  {
    id: "memory",
    label: "expertMemoryTitle",
    icon: BrainCircuit,
    words: ["expertMemoryIntro", "expertMemorySources", "expertSourceClaude", "expertSourceGit", "expertSourceDocs"],
  },
  { id: "trust", label: "obTrustTitle", icon: ShieldCheck, words: ["obTrustIntro"] },
  { id: "delete", label: "t5DeleteProject", icon: Trash2, words: [] },
];

export const config = $state({ section: "project" as ConfigSection });

let back: ViewId = "terminals";

/** Opens a project's Configuration at a section, "" for a new project; Escape goes back to the view it was opened from. */
export function editProject(id: string, section: ConfigSection = "project") {
  if (ui.editing !== null && id !== ui.editing && held.page?.hold(() => editProject(id, section))) return;
  if (ui.view !== "project") back = ui.view;
  ui.switcher = false;
  ui.editing = id;
  // A new project's page keeps its own section.
  if (id) config.section = section;
  ui.view = "project";
}

/** The current project's Configuration where it was left; under every project, a new project and the global library. */
export function showConfig() {
  editProject(app.project ?? "", config.section);
}

export function closeProject() {
  if (held.page?.hold(closeProject)) return;
  ui.editing = null;
  // Beside another view, its panel closes.
  if (panes.other !== null && onScreen("project")) closePanel("project");
  else if (ui.view === "project") ui.view = usable(back);
}
