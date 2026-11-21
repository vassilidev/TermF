import { SvelteSet } from "svelte/reactivity";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Repo = { name: string; path: string; modified: number | null };
export type Project = { id: string; name: string; repos: Repo[] };
export type Workspace = {
  configDir: string;
  home: string;
  agents: string[];
  projects: Project[];
  repos: Repo[];
  scanTruncated: boolean;
};
export type Session = {
  id: number;
  title: string;
  cwd: string;
  running: boolean;
  exitCode: number | null;
};

export const app = $state({
  workspace: null as Workspace | null,
  error: "",
  sessions: [] as Session[],
  focused: null as number | null,
});

/** Paths of the repositories ticked in the sidebar. */
export const selected = new SvelteSet<string>();

export async function load() {
  try {
    app.workspace = await invoke<Workspace>("workspace");
    app.error = "";
  } catch (e) {
    app.error = String(e);
  }
}

/** Loads the workspace and re-attaches to sessions that outlived a webview reload. Returns a cleanup. */
export function init(): () => void {
  load();
  invoke<Session[]>("pty_list").then((list) => (app.sessions = list));
  const unlisten = listen<{ id: number; exitCode: number | null }>("pty-exit", ({ payload }) => {
    const session = app.sessions.find((s) => s.id === payload.id);
    if (session) {
      session.running = false;
      session.exitCode = payload.exitCode;
    }
  });
  return () => {
    unlisten.then((stop) => stop());
  };
}

export function repoName(path: string): string {
  const all = [
    ...(app.workspace?.repos ?? []),
    ...(app.workspace?.projects.flatMap((p) => p.repos) ?? []),
  ];
  return all.find((r) => r.path === path)?.name ?? path;
}

export function toggleProject(project: Project) {
  const all = project.repos.every((r) => selected.has(r.path));
  for (const r of project.repos) {
    if (all) selected.delete(r.path);
    else selected.add(r.path);
  }
}

/** Opens `agent` (or a login shell when null) in every ticked repository. */
export async function openSelected(agent: string | null) {
  for (const path of selected) {
    try {
      app.sessions.push(
        await invoke<Session>("pty_spawn", {
          agent,
          cwd: path,
          title: `${repoName(path)} · ${agent ?? "shell"}`,
          cols: 120,
          rows: 32,
        }),
      );
    } catch (e) {
      app.error = String(e);
    }
  }
}

export async function closeSession(id: number) {
  await invoke("pty_kill", { id }).catch(() => {});
  app.sessions = app.sessions.filter((s) => s.id !== id);
  if (app.focused === id) app.focused = null;
}
