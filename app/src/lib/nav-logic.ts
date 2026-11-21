// No imports but types, so `pnpm test` runs it in plain Node.
import type { Component } from "svelte";

type Repo = { path: string; modified: number | null };
type Project = { id: string; repos: Repo[] };
type Session = { cwd: string; running: boolean; status: string | null; exitCode: number | null };
type Run = { project: string; worktree: string | null };

export type Tally = { total: number; working: number; waiting: number; failed: number };

function inside(path: string, root: string): boolean {
  if (path === root) return true;
  const sep = root.endsWith("/") || root.endsWith("\\") ? "" : root.includes("\\") ? "\\" : "/";
  return path.startsWith(root + sep);
}

/** The projects a folder belongs to: through a repository of theirs, or a ticket run's worktree. */
export function projectsOf(cwd: string, projects: Project[], runs: Run[]): string[] {
  const ids = projects.filter((p) => p.repos.some((r) => inside(cwd, r.path))).map((p) => p.id);
  for (const run of runs) {
    if (run.worktree && inside(cwd, run.worktree) && !ids.includes(run.project)) ids.push(run.project);
  }
  return ids;
}

/** Agent states per project id, "" for the sessions outside every project, "*" for all of them. */
export function tally(sessions: Session[], projects: Project[], runs: Run[]): Record<string, Tally> {
  const out: Record<string, Tally> = {};
  const add = (key: string, s: Session) => {
    const t = (out[key] ??= { total: 0, working: 0, waiting: 0, failed: 0 });
    t.total++;
    if (s.running && s.status === "working") t.working++;
    if (s.running && s.status === "waiting") t.waiting++;
    if (!s.running && s.exitCode !== null && s.exitCode !== 0) t.failed++;
  };
  for (const s of sessions) {
    const ids = projectsOf(s.cwd, projects, runs);
    for (const id of ids.length ? ids : [""]) add(id, s);
    add("*", s);
  }
  return out;
}

/** A stored current project, kept only while it still exists; null is every project. */
export function validProject(id: unknown, projects: { id: string }[]): string | null {
  return typeof id === "string" && projects.some((p) => p.id === id) ? id : null;
}

export function mostRecent<R extends Repo>(repos: R[]): R | null {
  return repos.reduce<R | null>((best, r) => (!best || (r.modified ?? 0) > (best.modified ?? 0) ? r : best), null);
}

type Keys = { key: string; code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean };
export type Shortcut =
  | { action: "palette" | "next" | "ticket" | "maximize" | "switcher" | "launcher" }
  | { action: "tile" | "project"; n: number };

/**
 * The app-wide shortcut a key press asks for, null to leave the key alone. `inTerminal`: the focus is in a terminal,
 * where Linux and Windows leave Ctrl+K, Ctrl+J, Ctrl+O and Alt+digit to the shell unless Shift is held too. `count`:
 * the tiles shown and the projects, so that a digit with nothing behind it goes on to the terminal.
 */
export function shortcut(
  e: Keys,
  mac: boolean,
  inTerminal: boolean,
  count: { tiles: number; projects: number },
): Shortcut | null {
  // The code, not the key: AZERTY layouts type digits with Shift.
  const digit = /^Digit[1-9]$/.test(e.code) ? Number(e.code.slice(5)) : 0;
  const free = e.shiftKey || !(inTerminal && !mac);
  // shortcut: projects on ⌃1…9 (macOS) and Alt+1…9 elsewhere, until the T4 shortcut registry makes them configurable.
  const projectMod = mac ? e.ctrlKey && !e.metaKey && !e.altKey : e.altKey && !e.ctrlKey && !e.metaKey;
  if (digit && projectMod) return free && digit <= count.projects ? { action: "project", n: digit } : null;
  if (e.altKey || !(mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey)) return null;
  const letter = e.key.toLowerCase();
  if (letter === "k" && free) return { action: "palette" };
  if (letter === "j" && free) return { action: "next" };
  if (letter === "o" && free) return { action: "switcher" };
  if (letter === "l" && e.shiftKey) return { action: "ticket" };
  if (letter === "t" && (mac ? !e.shiftKey : e.shiftKey)) return { action: "launcher" };
  if (e.key === "Enter" && !e.shiftKey) return { action: "maximize" };
  if (digit && digit <= count.tiles) return { action: "tile", n: digit };
  return null;
}

export type Group = "projects" | "sessions" | "repos" | "others" | "views" | "actions";
export type Command = {
  id: string;
  label: string;
  hint?: string;
  keys?: string;
  icon: Component;
  checked?: boolean;
  group: Group;
  /** Listed only when the query matches the label: the repositories of the other projects. */
  searchOnly?: boolean;
  run: () => void;
};

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

/** Score of `text` for one query word: a substring beats scattered letters, a word start beats both. -1 when absent. */
function wordScore(text: string, word: string): number {
  const at = text.indexOf(word);
  if (at >= 0) return 20 + word.length * 2 + (at === 0 || /[\s·/«“(._-]/.test(text[at - 1]) ? 10 : 0);
  let from = 0;
  let score = 0;
  for (const ch of word) {
    const found = text.indexOf(ch, from);
    if (found < 0) return -1;
    score += found === from ? 2 : 1;
    from = found + 1;
  }
  return score;
}

/**
 * Commands matching every word of `query`, grouped: the groups come in the order of their best match, each one best
 * first; ties keep the default order. A search-only command must contain each word as is in its label.
 */
export function rank(list: Command[], query: string): Command[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (!words.length) return list.filter((c) => !c.searchOnly);
  const ranked = list
    .map((command, i) => {
      const label = fold(command.label);
      const hint = command.searchOnly ? "" : fold(command.hint ?? "");
      let score = 0;
      for (const word of words) {
        if (command.searchOnly && !label.includes(word)) return null;
        const s = Math.max(wordScore(label, word), wordScore(hint, word) - 4);
        if (s < 0) return null;
        score += s;
      }
      return { command, score, i };
    })
    .filter((r) => r !== null)
    .sort((a, b) => b.score - a.score || a.i - b.i)
    .map((r) => r.command);
  const order = [...new Set(ranked.map((c) => c.group))];
  return order.flatMap((group) => ranked.filter((c) => c.group === group));
}
