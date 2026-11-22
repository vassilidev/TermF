import { invoke } from "@tauri-apps/api/core";
import { t } from "$lib/i18n";
import { mostRecent } from "$lib/nav-logic";
import { app, ui, type Project, type Repo } from "$lib/state.svelte";
import { zoomed } from "$lib/tabs.svelte";

export type Entry = { name: string; path: string; dir: boolean };
export type Content =
  /** `eol` is the file's line separator, `null` when it mixes several (read-only then). */
  | { kind: "text"; text: string; eol: string | null }
  | { kind: "binary" }
  | { kind: "tooLarge"; size: number };
export type Stamp = { mtime: number; size: number };
export type Tab = {
  repo: string;
  /** Relative to `repo`, `/`-separated. */
  path: string;
  name: string;
  content: Content;
  /** The file on disk when `content` was read or last saved. */
  stamp: Stamp;
  dirty: boolean;
  /** Changed on disk while it had unsaved edits. */
  stale: boolean;
  diff: boolean;
  /** Line to reveal once the tab is shown. */
  line: number | null;
};

export const files = $state({
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

export function currentProject(): Project | undefined {
  return app.workspace?.projects.find((p) => p.id === app.project);
}

/** The current project's repositories, or every one. */
export function scopeRepos(): Repo[] {
  return app.project === null ? allRepos() : (currentProject()?.repos ?? []);
}

/** Keeps the Files view's repository when the current project has it, else takes the project's most recent one. */
export function scopeRepo() {
  const repos = scopeRepos();
  if (!repos.some((r) => r.path === files.repo)) files.repo = mostRecent(repos)?.path ?? "";
}

/** Shows the Files view on the zoomed session's repository, else keeps the current one or takes the most recent. */
export function showFiles() {
  const focused = zoomed()?.cwd;
  if (focused && scopeRepos().some((r) => r.path === focused)) files.repo = focused;
  else scopeRepo();
  ui.view = "files";
  files.loaded = true;
  refreshStatus();
  if (files.active) sync(files.active);
}

export async function refreshStatus() {
  const repo = files.repo;
  if (!repo) return;
  const status = await invoke<Record<string, string>>("files_status", { repo }).catch(() => ({}));
  if (repo === files.repo) files.status = status;
}

function read(repo: string, path: string) {
  return invoke<Content & { stamp: Stamp }>("files_read", { repo, path });
}

/** Reloads a tab whose file changed on disk; one with unsaved edits only gets a notice. */
export async function sync(tab: Tab) {
  try {
    const stamp = await invoke<Stamp>("files_stat", { repo: tab.repo, path: tab.path });
    if (stamp.mtime === tab.stamp.mtime && stamp.size === tab.stamp.size) return;
    if (!tab.dirty) {
      const { stamp, ...content } = await read(tab.repo, tab.path);
      // Typing during the read wins over the reload.
      if (!tab.dirty) {
        Object.assign(tab, { content, stamp, stale: false });
        return;
      }
    }
    tab.stale = true;
  } catch (e) {
    files.error = String(e);
  }
}

export function activate(tab: Tab) {
  files.active = tab;
  sync(tab);
}

export async function openFile(path: string, line: number | null = null) {
  const repo = files.repo;
  const open = files.tabs.find((tab) => tab.repo === repo && tab.path === path);
  if (open) {
    // Before revealing the line: a reload would lose it.
    await sync(open);
    open.line = line;
    if (line !== null) open.diff = false;
    files.active = open;
    return;
  }
  try {
    const { stamp, ...content } = await read(repo, path);
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
      stamp,
      dirty: false,
      stale: false,
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
  if (files.active !== tab) return;
  const next = files.tabs[Math.min(index, files.tabs.length - 1)];
  if (next) activate(next);
  else files.active = null;
}

export async function openExternal(tab: Tab) {
  try {
    await invoke("files_open_external", { repo: tab.repo, path: tab.path });
  } catch (e) {
    files.error = String(e);
  }
}

/** In-app confirmation: WKWebView ignores `window.confirm`. A newer question cancels a pending one. */
export function ask(message: string): Promise<boolean> {
  files.question?.answer(false);
  return new Promise((answer) => {
    files.question = { message, answer };
  });
}
