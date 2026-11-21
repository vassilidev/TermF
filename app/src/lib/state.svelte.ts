import { SvelteSet } from "svelte/reactivity";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";
import type { SyncStatus } from "$lib/sync.svelte";

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
  /** Why the local API (termf CLI, agent status) is off. */
  apiError: string | null;
};
export type Status = "starting" | "working" | "waiting" | "idle" | "done";
export type Layout = { name: string; sessions: { path: string; agent: string | null }[] };
export type KeepAwakeMode = "auto" | "always" | "off";
export type KeepAwakeState = {
  mode: KeepAwakeMode;
  displayOn: boolean;
  active: boolean;
  error: string | null;
};
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
  /** Context gauge and reset, for Claude Code sessions that report to TermF. */
  context: Gauge | null;
};
export type ResetPhase = "idle" | "queued" | "handoff" | "clearing" | "resuming";
export type ResetFailure =
  | "typed"
  | "busy"
  | "interrupted"
  | "empty_handoff"
  | "save_failed"
  | "write_failed"
  | "timeout";
export type Gauge = {
  /** Null until the conversation's first answer. */
  percent: number | null;
  tokens: number | null;
  window: number | null;
  phase: ResetPhase;
  failure: ResetFailure | null;
};
export type ContextSettings = { on: boolean; resetAt: number; warningAt: number };

export const app = $state({
  workspace: null as Workspace | null,
  error: "",
  sessions: [] as Session[],
  focused: null as number | null,
  layouts: [] as Layout[],
  keepAwake: null as KeepAwakeState | null,
  sync: null as SyncStatus | null,
  contextSettings: null as ContextSettings | null,
});

/** Overlays opened from the top bar, the shortcuts or the palette. */
export const ui = $state({
  palette: false,
  settings: false,
  /** The project whose editor is open in the sidebar. */
  editing: null as string | null,
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
  load().then(() => callLayouts("layouts"));
  invoke<Session[]>("pty_list").then((list) => {
    app.sessions = list;
  });
  invoke<KeepAwakeState>("keep_awake_state").then((s) => {
    app.keepAwake = s;
  });
  const unlistenKeepAwake = listen<KeepAwakeState>("keep-awake", ({ payload }) => {
    app.keepAwake = payload;
  });
  invoke<SyncStatus>("sync_status")
    .then((s) => {
      app.sync ??= s;
    })
    .catch((e) => {
      app.error = String(e);
    });
  const unlistenSync = listen<SyncStatus>("sync-status", ({ payload }) => {
    app.sync = payload;
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
  invoke<ContextSettings>("context_settings").then((s) => (app.contextSettings = s), () => {});
  const unlistenContext = listen<{ id: number; context: Gauge }>("session-context", ({ payload }) => {
    const session = app.sessions.find((s) => s.id === payload.id);
    if (session) session.context = payload.context;
  });
  return () => {
    unlisten.then((stop) => stop());
    unlistenStatus.then((stop) => stop());
    unlistenOpened.then((stop) => stop());
    unlistenKeepAwake.then((stop) => stop());
    unlistenSync.then((stop) => stop());
    unlistenContext.then((stop) => stop());
  };
}

/** Starts the handoff, clear and resume sequence by hand: at once if the agent is idle, else at the end of its turn. */
export function resetContext(id: number) {
  invoke("context_reset", { id }).catch((e) => (app.error = String(e)));
}

export async function setContextSettings(settings: ContextSettings): Promise<string> {
  try {
    app.contextSettings = await invoke<ContextSettings>("context_settings_set", settings);
    return "";
  } catch (e) {
    return String(e);
  }
}

/** Sessions waiting for the user; an exited session never is, whatever its last status. */
export function waitingSessions(): Session[] {
  return app.sessions.filter((s) => s.running && s.status === "waiting");
}

export function repoName(path: string): string {
  const all = [
    ...(app.workspace?.repos ?? []),
    ...(app.workspace?.projects.flatMap((p) => p.repos) ?? []),
  ];
  return all.find((r) => r.path === path)?.name ?? path;
}

/** `~/code/acme/api` for a path under the home folder. */
export function shortPath(path: string): string {
  const home = app.workspace?.home ?? "";
  return home && path.startsWith(home) ? "~" + path.slice(home.length) : path;
}

export function toggleProject(project: Project) {
  const all = project.repos.every((r) => selected.has(r.path));
  for (const r of project.repos) {
    if (all) selected.delete(r.path);
    else selected.add(r.path);
  }
}

/** Runs a layout command, which answers with the saved layouts. */
export async function callLayouts(command: string, args: Record<string, unknown> = {}): Promise<boolean> {
  try {
    app.layouts = await invoke<Layout[]>(command, args);
    return true;
  } catch (e) {
    app.error = String(e);
    return false;
  }
}

export async function openLayout(layout: Layout) {
  for (const s of layout.sessions) await openSession(s.path, s.agent);
}

export async function setKeepAwake(mode: KeepAwakeMode, displayOn: boolean): Promise<string> {
  try {
    app.keepAwake = await invoke<KeepAwakeState>("keep_awake_set", { mode, displayOn });
    return "";
  } catch (e) {
    return String(e);
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

/** Options of the Open button. `agent` is null for a shell, undefined for the first agent. Remote Control only applies to Claude Code agents. */
export const launch = $state({ remoteControl: false, agent: undefined as string | null | undefined });

export function launchAgent(): string | null {
  const agents = app.workspace?.agents ?? [];
  if (launch.agent === null) return null;
  return launch.agent && agents.includes(launch.agent) ? launch.agent : (agents[0] ?? null);
}

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
