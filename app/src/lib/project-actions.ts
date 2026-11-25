import { invoke } from "@tauri-apps/api/core";
import type { Args } from "$lib/actions-logic";
import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import { app, type Project, type Repo } from "$lib/state.svelte";
import { toast } from "$lib/ui/toast.svelte";

const list = (v: unknown) => (Array.isArray(v) ? v.map(String).filter((s) => s.trim()) : []);
const projectOf = (id: string) => app.workspace?.projects.find((p) => p.id === id);
const nameOf = (r: Repo) => r.name;

/** A repository TermF found, or one of the project's, by its folder's name or its path. */
function repoNamed(project: Project, given: string): Repo {
  const found = [...project.repos, ...(app.workspace?.repos ?? [])].find((r) => r.name === given || r.path === given);
  if (!found) throw new Error(`No repository ${given} found by TermF: name one of its repositories' folders, or its path.`);
  return found;
}

function edited(project: Project, a: Args) {
  const removed = list(a.remove_repos).map((r) => repoNamed(project, r).path);
  const added = list(a.add_repos).map((r) => repoNamed(project, r));
  const repos = [...project.repos.map((r) => r.path).filter((p) => !removed.includes(p)), ...added.map((r) => r.path)];
  const checks: Record<string, string> = { ...project.checks };
  for (const [repo, command] of Object.entries((a.checks ?? {}) as Record<string, string>)) checks[repoNamed(project, repo).path] = command.trim();
  const kept = [...new Set(repos)];
  // As the form saves it: every repository's command, an empty one for none.
  return { name: typeof a.name === "string" && a.name.trim() ? a.name.trim() : project.name, repos: kept, checks: Object.fromEntries(kept.map((p) => [p, checks[p] ?? ""])) };
}

const edit: Action = {
  id: "project.edit",
  description:
    "Changes the project's settings, as Configuration > Project does: its name, its repositories (added or removed by their folder's name or path, among those TermF found), each repository's check command before delivery (`checks`: repository → command, an empty one removes it), and whether its sessions get the language servers of its stack.",
  fields: {
    name: { type: "string", description: "The project's new name." },
    add_repos: { type: "array", description: "Repositories to add." },
    remove_repos: { type: "array", description: "Repositories to remove from the project (nothing is deleted on disk)." },
    checks: { type: "object", description: "Repository → the command that checks it before a delivery (npm test)." },
    language_servers: { type: "boolean", description: "Whether its sessions get the language servers of its stack." },
  },
  label: "actProjectEdit",
  describe: (a, id) => {
    const project = projectOf(id);
    const lines = [t("actProjectEditText", { project: project?.name ?? id })];
    if (typeof a.name === "string" && a.name.trim()) lines.push(t("actProjectName", { name: a.name.trim() }));
    if (list(a.add_repos).length) lines.push(t("actProjectAdd", { repos: list(a.add_repos).join(", ") }));
    if (list(a.remove_repos).length) lines.push(t("actProjectRemove", { repos: list(a.remove_repos).join(", ") }));
    for (const [repo, command] of Object.entries((a.checks ?? {}) as Record<string, string>))
      lines.push(command.trim() ? t("actProjectCheck", { repo, command: command.trim() }) : t("actProjectCheckOff", { repo }));
    if (typeof a.language_servers === "boolean") lines.push(t("actProjectLsp", { state: t(a.language_servers ? "actSettingOn" : "actSettingOff") }));
    return lines.join("\n");
  },
  async run(a, _secrets, id) {
    const project = projectOf(id);
    if (!project) throw new Error("This project is not in TermF any more.");
    const changed = ["name", "add_repos", "remove_repos", "checks"].some((k) => a[k] !== undefined && a[k] !== null);
    if (changed) {
      const next = edited(project, a);
      if (!next.repos.length) throw new Error("A project keeps one repository at least.");
      app.workspace!.projects = await invoke<Project[]>("project_save", { id, ...next });
    }
    if (typeof a.language_servers === "boolean") await invoke<boolean>("library_lsp", { project: id, enabled: a.language_servers });
    const now = projectOf(id);
    toast(t("actProjectEdited", { project: now?.name ?? id }));
    return { name: now?.name, repos: now?.repos.map(nameOf), checks: now?.checks ?? {} };
  },
};

export const projectActions: Action[] = [edit];
