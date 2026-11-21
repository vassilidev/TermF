import { invoke } from "@tauri-apps/api/core";
import { t } from "$lib/i18n";
import { app, type Repo } from "$lib/state.svelte";

export type Entry = { name: string; path: string; dir: boolean };
export type Content =
  | { kind: "text"; text: string }
  | { kind: "binary" }
  | { kind: "tooLarge"; size: number };
export type Tab = {
  repo: string;
  /** Relative to `repo`, `/`-separated. */
  path: string;
  name: string;
  content: Content;
  mtime: number;
  dirty: boolean;
  diff: boolean;
  /** Line to reveal once the tab is shown. */
  line: number | null;
};

export const files = $state({
  view: "terminals" as "terminals" | "files",
  /** The Files view is only loaded once first shown, then kept mounted. */
  loaded: false,
  repo: "",
  status: {} as Record<string, string>,
  tabs: [] as Tab[],
  active: null as Tab | null,
  panel: "tree" as "tree" | "search",
  finder: false,
  /** Bumped to reload the expanded folders. */
  version: 0,
  error: "",
  question: null as { message: string; answer: (yes: boolean) => void } | null,
});

export const isMac = navigator.platform.startsWith("Mac");

export function tabKey(tab: { repo: string; path: string }): string {
  return `${tab.repo}\n${tab.path}`;
}

export function allRepos(): Repo[] {
  const repos = [
    ...(app.workspace?.repos ?? []),
    ...(app.workspace?.projects.flatMap((p) => p.repos) ?? []),
  ];
  return repos.filter((r, i) => repos.findIndex((o) => o.path === r.path) === i);
}

/** Shows the Files view on the focused session's repository, else keeps the current one or takes the most recent. */
export function showFiles() {
  const repos = allRepos();
  const focused = app.sessions.find((s) => s.id === app.focused)?.cwd;
  if (focused && repos.some((r) => r.path === focused)) files.repo = focused;
  else if (!repos.some((r) => r.path === files.repo)) {
    files.repo = repos.reduce<Repo | null>(
      (best, r) => (!best || (r.modified ?? 0) > (best.modified ?? 0) ? r : best),
      null,
    )?.path ?? "";
  }
  files.view = "files";
  files.loaded = true;
  refreshStatus();
}

export async function refreshStatus() {
  const repo = files.repo;
  if (!repo) return;
  const status = await invoke<Record<string, string>>("files_status", { repo }).catch(() => ({}));
  if (repo === files.repo) files.status = status;
}

export async function openFile(path: string, line: number | null = null) {
  const repo = files.repo;
  const open = files.tabs.find((tab) => tab.repo === repo && tab.path === path);
  if (open) {
    open.line = line;
    if (line !== null) open.diff = false;
    files.active = open;
    return;
  }
  try {
    const { mtime, ...content } = await invoke<Content & { mtime: number }>("files_read", {
      repo,
      path,
    });
    // A double click opens the file once.
    const opened = files.tabs.find((tab) => tab.repo === repo && tab.path === path);
    if (opened) {
      files.active = opened;
      return;
    }
    files.tabs.push({
      repo,
      path,
      name: path.slice(path.lastIndexOf("/") + 1),
      content,
      mtime,
      dirty: false,
      diff: false,
      line,
    });
    files.active = files.tabs[files.tabs.length - 1];
    files.error = "";
  } catch (e) {
    files.error = String(e);
  }
}

export async function closeTab(tab: Tab) {
  if (tab.dirty && !(await ask(t("fileUnsaved", { name: tab.name })))) return;
  const index = files.tabs.indexOf(tab);
  files.tabs.splice(index, 1);
  if (files.active === tab) files.active = files.tabs[Math.min(index, files.tabs.length - 1)] ?? null;
}

export async function openExternal(tab: Tab) {
  try {
    await invoke("files_open_external", { repo: tab.repo, path: tab.path });
  } catch (e) {
    files.error = String(e);
  }
}

/** In-app confirmation: WKWebView ignores `window.confirm`. */
export function ask(message: string): Promise<boolean> {
  return new Promise((answer) => {
    files.question = { message, answer };
  });
}
