import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { shownRepo } from "$lib/files.svelte";
import { stack } from "$lib/stack.svelte";
import { app } from "$lib/state.svelte";
import { ending, matches, proposal, rows, type Check, type Filter, type StackMap } from "$lib/stack-logic";

const FILTERS: Filter[] = ["all", "updates", "vulnerable", "eol"];

/** The project's repositories by path; `repo`, a folder's name or path, keeps that one. */
export function projectRepos(project: string, repo?: unknown): string[] {
  const all = app.workspace?.projects.find((p) => p.id === project)?.repos ?? [];
  const wanted = typeof repo === "string" ? repo.trim() : "";
  if (!wanted) return all.map((r) => r.path);
  const one = all.find((r) => r.path === wanted || r.name === wanted);
  if (!one) throw new Error(`No repository ${wanted} in this project; its repositories: ${all.map((r) => r.name).join(", ")}.`);
  return [one.path];
}

const stackRead: Action = {
  id: "stack.read",
  description:
    "Reads the project's stack, as the Stack view shows it: each repository's languages, frameworks and versions, Compose services, databases, ports, expected variables (names only) and CI workflows, the links between repositories, and each dependency with the version used, the latest, the update to make (`major` breaks things), its known vulnerabilities (severity, the version that fixes them) and the end of support of runtimes. `filter` keeps the dependencies to update, the vulnerable ones or those out of support. Reads only; the first check of the registries can take a few seconds.",
  fields: {
    repo: { type: "string", description: "One repository, by its folder's name; all the project's by default." },
    filter: { type: "string", description: "Which dependencies, all by default.", enum: FILTERS },
  },
  label: "actStackRead",
  describe: () => "",
  async run(a, _secrets, project) {
    const filter = FILTERS.includes(a.filter as Filter) ? (a.filter as Filter) : "all";
    const today = new Date().toISOString().slice(0, 10);
    const map = await invoke<StackMap>("stack_map", { repos: projectRepos(project, a.repo) });
    const checks = await Promise.all(
      map.repos.map((r) => invoke<Check>("stack_check", { repo: r.path, refresh: false }).catch((e) => `unread: ${String(e)}`)),
    );
    const repos = map.repos.map((r, i) => {
      const check = typeof checks[i] === "string" ? null : (checks[i] as Check);
      const deps = rows(r, check, today)
        .filter((row) => matches(row, filter, "", today))
        .map((row) => ({
          name: row.name,
          used: row.version,
          latest: row.check?.latest ?? row.support?.latest ?? null,
          update: proposal(row),
          ...(row.dep ? { manifest: row.dep.manifest, manager: row.dep.manager, dev: row.dep.dev } : { runtime: true }),
          ...(row.check?.vulns.length ? { vulns: row.check.vulns.map((v) => ({ id: v.id, severity: v.severity, fixed: v.fixed, summary: v.summary })) } : {}),
          ...(row.support
            ? { support: { cycle: row.support.cycle, eol: row.support.eol, ending: ending(row.support, today), currentLts: row.support.currentLts } }
            : {}),
        }));
      return {
        name: r.name,
        path: r.path,
        tags: r.tags.map((t) => (t.version ? `${t.name} ${t.version}` : t.name)),
        services: r.services,
        databases: r.databases.map((d) => ({ engine: d.engine, name: d.name, host: d.host, port: d.port })),
        ports: r.ports,
        env: r.env,
        workflows: r.workflows.map((w) => w.name),
        deps,
        ...(typeof checks[i] === "string" ? { check: checks[i] } : check?.offline ? { check: "offline: latest versions and vulnerabilities unknown" } : {}),
      };
    });
    return { repos, links: map.links };
  },
};

export const stackActions: Action[] = [stackRead];

/** What the Stack view shows: its repository, its filter and search. */
export async function stackHere() {
  const repo = shownRepo();
  return { repo: repo ? (repo.split(/[\\/]/).pop() ?? repo) : null, filter: stack.filter, search: stack.query };
}
