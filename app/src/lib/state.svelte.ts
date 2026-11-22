import { SvelteSet } from "svelte/reactivity";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { t } from "$lib/i18n";
import { loadKeys } from "$lib/keymap.svelte";
import { projectsOf } from "$lib/nav-logic";
import { go } from "$lib/panes-logic";
import type { Side } from "$lib/split-logic";
import type { SyncStatus } from "$lib/sync.svelte";
// Each module reads the other only inside functions, so the import cycles are safe.
import { place } from "$lib/tabs.svelte";
import { tickets } from "$lib/tickets.svelte";

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
/** `tabs`: each tab's splits over `sessions` (`split-logic` `encode`); a layout saved before tabs has none. */
export type Layout = {
  name: string;
  sessions: { path: string; agent: string | null }[];
  tabs?: { name?: string; splits: string }[];
};
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
  /** The team role the session works for, once runs launch roles (phase 4). */
  role?: string | null;
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
export type ViewId =
  | "home"
  | "terminals"
  | "conversations"
  | "files"
  | "library"
  | "ssh"
  | "tickets"
  | "ci"
  | "scheduler"
  | "claude"
  | "team"
  | "project"
  | "settings"
  | "onboarding";

const projectKey = "termf.project";

function storedProject(): string | null {
  try {
    return localStorage.getItem(projectKey);
  } catch {
    return null;
  }
}

export const app = $state({
  workspace: null as Workspace | null,
  error: "",
  sessions: [] as Session[],
  layouts: [] as Layout[],
  keepAwake: null as KeepAwakeState | null,
  sync: null as SyncStatus | null,
  contextSettings: null as ContextSettings | null,
  /** The current project's id, read by every view; null is every project. */
  project: storedProject(),
});

/** Remembered on this machine. */
export function setProject(id: string | null) {
  app.project = id;
  try {
    if (id === null) localStorage.removeItem(projectKey);
    else localStorage.setItem(projectKey, id);
  } catch {
    // Storage off: the choice holds until the app closes.
  }
}

/** `last`: what the active panel showed before, where ⌘\ reopens the second panel. */
export const panes = $state({ other: null as ViewId | null, active: 0 as 0 | 1, size: 0.5, last: null as ViewId | null });

/** The view shown, and the overlays opened from the navigation, the shortcuts or the palette. */
class Ui {
  #view = $state<ViewId>("home");
  palette = $state(false);
  switcher = $state(false);
  launcher = $state(false);
  help = $state(false);
  /** The project whose page is open, "" for a new one. */
  editing = $state<string | null>(null);

  /** The active panel's view: setting it shows a view there, or moves to the other panel when that one shows it. */
  get view(): ViewId {
    return this.#view;
  }

  set view(next: ViewId) {
    if (next !== this.#view && next !== panes.other) panes.last = this.#view;
    const p = go({ ...panes, view: this.#view }, next);
    this.#view = p.view;
    panes.other = p.other;
    panes.active = p.active;
  }
}

export const ui = new Ui();

export const onScreen = (view: ViewId) => ui.view === view || panes.other === view;

/** Paths of the repositories ticked in the session launcher. */
export const selected = new SvelteSet<string>();

/** Ids of the sessions ticked for broadcast. */
export const ticked = new SvelteSet<number>();

/**
 * Terminals: `all` shows every project's tiles under the current project; `broadcast` shows the ticks and the bar,
 * whose unsent message is `draft`.
 */
export const grid = $state({ all: false, broadcast: false, draft: "" });

/** A session opened outside the current project would get a hidden tile: show every project's tiles instead. */
function reveal(path: string) {
  const projects = projectsOf(path, app.workspace?.projects ?? [], tickets.runs);
  if (app.project !== null && !projects.includes(app.project)) grid.all = true;
}

/** Reads the workspace, and the shortcuts, which a reload, an import or a sync may have changed in termf.toml. */
export async function load() {
  loadKeys();
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
    if (payload.lastSync !== app.sync?.lastSync) loadKeys();
    app.sync = payload;
  });
  const unlisten = listen<{ id: number; exitCode: number | null }>("pty-exit", ({ payload }) => {
    const session = app.sessions.find((s) => s.id === payload.id);
    if (session) {
      session.running = false;
      session.exitCode = payload.exitCode;
    }
    ticked.delete(payload.id);
  });
  const unlistenStatus = listen<{ id: number; status: Status }>("session-status", ({ payload }) =>
    updateStatus(payload.id, payload.status),
  );
  const unlistenOpened = listen<Session>("session-opened", ({ payload }) => {
    if (!app.sessions.some((s) => s.id === payload.id)) {
      app.sessions.push(payload);
      reveal(payload.cwd);
    }
  });
  invoke<ContextSettings>("context_settings").then((s) => { app.contextSettings = s; }, () => {});
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

export async function setKeepAwake(mode: KeepAwakeMode, displayOn: boolean): Promise<string> {
  try {
    app.keepAwake = await invoke<KeepAwakeState>("keep_awake_set", { mode, displayOn });
    return "";
  } catch (e) {
    return String(e);
  }
}

/** Opens `agent` (or a login shell when null) in each repository. Returns the ids of the sessions opened. */
export async function openRepos(paths: string[], agent: string | null, hold?: (id: number) => void): Promise<number[]> {
  const ids = [];
  for (const path of paths) {
    const id = await openSession(path, agent, hold);
    if (id !== null) ids.push(id);
  }
  return ids;
}

/**
 * Returns the session's id, null when it could not open. It goes in a new tab, unless `hold` takes it for its caller to
 * place.
 */
export async function openSession(
  path: string,
  agent: string | null,
  hold?: (id: number) => void,
): Promise<number | null> {
  try {
    const session = await invoke<Session>("pty_spawn", {
      agent,
      cwd: path,
      title: `${repoName(path)} · ${agent ?? "shell"}`,
      cols: 120,
      rows: 32,
      remoteControl: launch.remoteControl,
    });
    app.sessions.push(session);
    if (hold) hold(session.id);
    else place(session.id);
    reveal(path);
    return session.id;
  } catch (e) {
    app.error = String(e);
    return null;
  }
}

/** Gone from the list at once, so no tab adopts it again while it is being killed. */
export async function closeSession(id: number) {
  app.sessions = app.sessions.filter((s) => s.id !== id);
  await invoke("pty_kill", { id }).catch(() => {});
}

/**
 * Options of the Open button. `agent` is null for a shell, undefined for the first agent. Remote Control only applies
 * to Claude Code agents. `others`: the repositories outside the current project are unfolded in the launcher. `side`:
 * beside the active split, on that side, instead of in a new tab.
 */
export const launch = $state({
  remoteControl: false,
  agent: undefined as string | null | undefined,
  others: false,
  side: null as Side | null,
});

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
