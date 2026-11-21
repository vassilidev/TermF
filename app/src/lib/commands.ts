import { tick, type Component } from "svelte";
import ArrowRight from "@lucide/svelte/icons/arrow-right";
import BellRing from "@lucide/svelte/icons/bell-ring";
import Coffee from "@lucide/svelte/icons/coffee";
import FileSearch from "@lucide/svelte/icons/file-search";
import FolderTree from "@lucide/svelte/icons/folder-tree";
import LayoutGrid from "@lucide/svelte/icons/layout-grid";
import Maximize2 from "@lucide/svelte/icons/maximize-2";
import Play from "@lucide/svelte/icons/play";
import RefreshCw from "@lucide/svelte/icons/refresh-cw";
import SquareTerminal from "@lucide/svelte/icons/square-terminal";
import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
import { fontReady } from "$lib/Terminal.svelte";
import { t } from "$lib/i18n";
import { files, isMac, showFiles } from "$lib/files.svelte";
import {
  app,
  load,
  openLayout,
  openSelected,
  openSession,
  selected,
  setKeepAwake,
  shortPath,
  ui,
  waitingSessions,
  type KeepAwakeMode,
} from "$lib/state.svelte";

const mod = isMac ? "⌘" : "Ctrl+";

export const keys = {
  palette: `${mod}K`,
  quick: isMac ? "⌃`" : "Ctrl+`",
  next: `${mod}J`,
  maximize: isMac ? "⌘↵" : "Ctrl+Enter",
  file: `${mod}P`,
  tile: (n: number) => `${mod}${n}`,
};

/** The session whose tile holds the keyboard focus, else the maximized one. */
function activeSession(): number | null {
  const tile = document.activeElement?.closest<HTMLElement>("[data-session]");
  return tile ? Number(tile.dataset.session) : app.focused;
}

/** Shows a session's tile (switching the maximized one if needed) and gives its terminal the keyboard. */
export async function focusSession(id: number) {
  files.view = "terminals";
  if (app.focused !== null) app.focused = id;
  await tick();
  await fontReady;
  document
    .querySelector<HTMLElement>(`[data-session="${id}"] .xterm-helper-textarea`)
    ?.focus();
}

export function nextWaiting() {
  const waiting = waitingSessions();
  if (!waiting.length) return;
  const from = app.sessions.findIndex((s) => s.id === activeSession());
  const next = waiting.find((s) => app.sessions.indexOf(s) > from) ?? waiting[0];
  focusSession(next.id);
}

export function toggleMaximize(id = activeSession()) {
  if (id === null) return;
  app.focused = app.focused === id ? null : id;
  focusSession(id);
}

/** App-wide shortcuts, in the capture phase so a focused terminal does not also get them. */
export function onShortcut(e: KeyboardEvent) {
  if (e.altKey || !(isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey)) return;
  // Ctrl+K and Ctrl+J are readline keys: inside a terminal, Linux and Windows need Shift as well.
  const needsShift = !isMac && !!(e.target as Element | null)?.closest?.(".xterm");
  const letter = e.key.toLowerCase();
  let action: (() => void) | null = null;
  if (letter === "k" && (e.shiftKey || !needsShift)) action = () => (ui.palette = !ui.palette);
  else if (ui.palette) return;
  else if (letter === "j" && (e.shiftKey || !needsShift)) action = nextWaiting;
  else if (e.key === "Enter" && !e.shiftKey && files.view === "terminals") action = toggleMaximize;
  else if (/^Digit[1-9]$/.test(e.code)) {
    // The code, not the key: AZERTY layouts type digits with Shift.
    const session = app.sessions[Number(e.code.slice(5)) - 1];
    if (session) action = () => focusSession(session.id);
  }
  if (!action) return;
  e.preventDefault();
  e.stopPropagation();
  action();
}

export type Command = {
  id: string;
  label: string;
  hint?: string;
  keys?: string;
  icon: Component;
  checked?: boolean;
  run: () => void;
};

const statusLabel = {
  starting: "statusStarting",
  working: "statusWorking",
  waiting: "statusWaiting",
  idle: "statusIdle",
  done: "statusDone",
} as const;

/** Every action the palette offers, in the order shown before typing. */
export function commands(): Command[] {
  const list: Command[] = [];
  const agents = app.workspace?.agents ?? [];
  const waiting = waitingSessions().length;
  if (waiting) {
    list.push({ id: "next", label: t("nextWaiting"), keys: keys.next, icon: BellRing, run: nextWaiting });
  }
  app.sessions.forEach((s, i) =>
    list.push({
      id: `session-${s.id}`,
      label: s.title,
      hint: !s.running ? t("exitedLabel") : s.status ? t(statusLabel[s.status]) : undefined,
      keys: i < 9 ? keys.tile(i + 1) : undefined,
      icon: ArrowRight,
      run: () => focusSession(s.id),
    }),
  );
  if (selected.size) {
    for (const agent of agents) {
      list.push({
        id: `open-${agent}`,
        label: t("open", { agent, count: selected.size }),
        icon: Play,
        run: () => openSelected(agent),
      });
    }
    list.push({
      id: "open-shell",
      label: t("openShell", { count: selected.size }),
      icon: SquareTerminal,
      run: () => openSelected(null),
    });
  }
  if (app.sessions.length) {
    list.push({ id: "maximize", label: t("maximizeActive"), keys: keys.maximize, icon: Maximize2, run: () => toggleMaximize() });
  }
  list.push({ id: "quick", label: t("quickTerminal"), keys: keys.quick, icon: SquareTerminal, run: toggleQuickTerminal });
  list.push(
    files.view === "files"
      ? { id: "view", label: t("showTerminals"), icon: LayoutGrid, run: () => (files.view = "terminals") }
      : { id: "view", label: t("showFiles"), icon: FolderTree, run: showFiles },
  );
  list.push({
    id: "file",
    label: t("filesFind"),
    keys: keys.file,
    icon: FileSearch,
    run: () => {
      showFiles();
      if (files.repo) files.finder = true;
    },
  });
  for (const layout of app.layouts) {
    list.push({
      id: `layout-${layout.name}`,
      label: t("openLayout", { name: layout.name }),
      icon: LayoutGrid,
      run: () => openLayout(layout),
    });
  }
  const repos = [...(app.workspace?.repos ?? [])].sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0));
  for (const repo of repos) {
    for (const agent of agents) {
      list.push({
        id: `repo-${agent}-${repo.path}`,
        label: t("openIn", { agent, repo: repo.name }),
        hint: shortPath(repo.path),
        icon: Play,
        run: () => openSession(repo.path, agent),
      });
    }
    list.push({
      id: `repo-shell-${repo.path}`,
      label: t("openShellIn", { repo: repo.name }),
      hint: shortPath(repo.path),
      icon: SquareTerminal,
      run: () => openSession(repo.path, null),
    });
  }
  const awake = app.keepAwake;
  if (awake) {
    const modes: [KeepAwakeMode, "keepAwakeAuto" | "keepAwakeAlways" | "keepAwakeOff"][] = [
      ["auto", "keepAwakeAuto"],
      ["always", "keepAwakeAlways"],
      ["off", "keepAwakeOff"],
    ];
    for (const [mode, label] of modes) {
      list.push({
        id: `awake-${mode}`,
        label: t("keepAwakeSet", { mode: t(label) }),
        icon: Coffee,
        checked: awake.mode === mode,
        run: () => setKeepAwake(mode, awake.displayOn).then((error) => error && (app.error = error)),
      });
    }
  }
  list.push({ id: "reload", label: t("reload"), icon: RefreshCw, run: load });
  return list;
}

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

/** Commands matching every word of `query`, best first; ties keep the default order. */
export function rank(list: Command[], query: string): Command[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (!words.length) return list;
  return list
    .map((command, i) => {
      const label = fold(command.label);
      const hint = fold(command.hint ?? "");
      let score = 0;
      for (const word of words) {
        const s = Math.max(wordScore(label, word), wordScore(hint, word) - 4);
        if (s < 0) return null;
        score += s;
      }
      return { command, score, i };
    })
    .filter((r) => r !== null)
    .sort((a, b) => b.score - a.score || a.i - b.i)
    .map((r) => r.command);
}
