import { allRepos, currentProject, scopeRepos } from "$lib/files.svelte";
import { projectsOf, tally, type Tally } from "$lib/nav-logic";
import { app, grid, launch, selected, type Repo, type Session } from "$lib/state.svelte";
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
