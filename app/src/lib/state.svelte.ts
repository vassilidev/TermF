import { SvelteSet } from "svelte/reactivity";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";

export type Repo = { name: string; path: string; modified: number | null };
export type Project = { id: string; name: string; repos: Repo[] };
export type Workspace = {
  configDir: string;
  home: string;
  agents: string[];
  projects: Project[];
  repos: Repo[];
  scanTruncated: boolean;
  quickTerminalError: string | null;
};
export type Status = "starting" | "working" | "waiting" | "idle" | "done";
export type Session = {
  id: number;
  title: string;
  cwd: string;
  agent: string | null;
  running: boolean;
  exitCode: number | null;
  /** Only agents that report their state (Claude Code) have one. */
  status: Status | null;
  remoteControl: boolean;
};

export const app = $state({
  workspace: null as Workspace | null,
  error: "",
  sessions: [] as Session[],
  focused: null as number | null,
});

/** Paths of the repositories ticked in the sidebar. */
export const selected = new SvelteSet<string>();

/** Ids of the sessions ticked for broadcast. */
export const ticked = new SvelteSet<number>();

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
  invoke<Session[]>("pty_list").then((list) => {
    app.sessions = list;
  });
  const unlisten = listen<{ id: number; exitCode: number | null }>("pty-exit", ({ payload }) => {
    const session = app.sessions.find((s) => s.id === payload.id);
    if (session) {
      session.running = false;
      session.exitCode = payload.exitCode;
    }
  });
  const unlistenStatus = listen<{ id: number; status: Status }>("session-status", ({ payload }) =>
    updateStatus(payload.id, payload.status),
  );
  const unlistenOpened = listen<Session>("session-opened", ({ payload }) => {
    if (!app.sessions.some((s) => s.id === payload.id)) app.sessions.push(payload);
  });
  return () => {
    unlisten.then((stop) => stop());
    unlistenStatus.then((stop) => stop());
    unlistenOpened.then((stop) => stop());
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
  for (const path of selected) await openSession(path, agent);
}

export async function openSession(path: string, agent: string | null) {
  try {
    app.sessions.push(
      await invoke<Session>("pty_spawn", {
        agent,
        cwd: path,
        title: `${repoName(path)} · ${agent ?? "shell"}`,
        cols: 120,
        rows: 32,
        remoteControl: launch.remoteControl,
      }),
    );
  } catch (e) {
    app.error = String(e);
  }
}

export async function closeSession(id: number) {
  await invoke("pty_kill", { id }).catch(() => {});
  app.sessions = app.sessions.filter((s) => s.id !== id);
  if (app.focused === id) app.focused = null;
}

/** Options of the Open buttons. Remote Control only applies to Claude Code agents. */
export const launch = $state({ remoteControl: false });

/** Notifies, while the window is in the background, when a session needs the user or has finished. */
async function updateStatus(id: number, status: Status) {
  const session = app.sessions.find((s) => s.id === id);
  if (!session) return;
  const before = session.status;
  session.status = status;
  const message =
    status === "waiting"
      ? t("notifyWaiting")
      : status === "done" || (status === "idle" && before === "working")
        ? t("notifyFinished")
        : null;
  if (!message || document.hasFocus()) return;
  const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
  if (granted) sendNotification({ title: session.title, body: message });
}
