import * as split from "$lib/split-logic";
import type { Pane, Side } from "$lib/split-logic";
import { shown } from "$lib/project.svelte";
import { neighbour, tileName } from "$lib/tile-logic";
import { pushRecent } from "$lib/workspace-logic";
// Each module reads the other only inside functions, so the import cycle is safe.
import { app, closeSession, openRepos, openSession, type Layout, type Session } from "$lib/state.svelte";

export type Tab = {
  id: number;
  /** Given by the user; "" shows the active session's name. */
  name: string;
  root: Pane;
  /** The split that has, or last had, the keyboard. */
  active: number;
  /** Only the active split is shown. */
  zoom: boolean;
};
export type Arrangement = "tabs" | "row" | "two" | "grid" | "focus";

/** Terminals: tabs of splits, each session in exactly one. */
export const terminals = $state({
  tabs: [] as Tab[],
  current: null as number | null,
  /** The running session whose close waits for a confirmation. */
  asking: null as number | null,
  /** The tabs and splits closed by the user, newest first: each a one-tab layout to open again (⌘⇧T). */
  closed: [] as Layout[],
});

let nextId = 1;
/** Sessions opened for a caller that places them itself: `reconcile` leaves them alone meanwhile. */
const held = new Set<number>();

const sessionOf = (id: number): Session | undefined => app.sessions.find((s) => s.id === id);
const isShown = (id: number) => {
  const s = sessionOf(id);
  return !!s && shown(s);
};

export function tabOf(id: number): Tab | undefined {
  return terminals.tabs.find((tab) => split.leaves(tab.root).includes(id));
}

/** The tabs under the current project: those with a session it shows. */
export function visibleTabs(): Tab[] {
  return terminals.tabs.filter((tab) => split.leaves(tab.root).some(isShown));
}

/** The split a tab opens on, so that going to it never changes the current project. */
export const entryOf = (tab: Tab) => split.entry(tab.root, tab.active, isShown);

export function currentTab(): Tab | undefined {
  const list = visibleTabs();
  return list.find((tab) => tab.id === terminals.current) ?? list[0];
}

/** The session zoomed in the current tab. */
export function zoomed(): Session | undefined {
  const tab = currentTab();
  return tab?.zoom ? sessionOf(tab.active) : undefined;
}

export function tabName(tab: Tab): string {
  const s = sessionOf(tab.active);
  return tab.name || (s ? tileName(s.title, s.agent ?? "shell") : "");
}

/** A new tab right after `after`, else at the end. */
function newTab(root: Pane, active: number, after?: Tab): Tab {
  const at = after ? terminals.tabs.indexOf(after) + 1 : terminals.tabs.length;
  terminals.tabs.splice(at, 0, { id: nextId++, name: "", root, active, zoom: false });
  return terminals.tabs[at];
}

function adopt(id: number): Tab {
  return tabOf(id) ?? newTab(id, id);
}

/** Where `openSession` puts a session nobody holds: in a new tab brought to the front. */
export function place(id: number) {
  terminals.current = adopt(id).id;
}

/** Keeps the tabs in step with the sessions: a closed one leaves its split, one opened elsewhere gets a tab. */
export function reconcile() {
  const ids = new Set(app.sessions.map((s) => s.id));
  terminals.tabs.flatMap((tab) => split.leaves(tab.root)).filter((id) => !ids.has(id)).forEach(drop);
  for (const id of ids) if (!held.has(id)) adopt(id);
}

/** Takes `id` out of its tab; the next split (else the previous) becomes active, and an empty tab goes. */
function drop(id: number) {
  const tab = tabOf(id);
  if (terminals.asking === id) terminals.asking = null;
  if (!tab) return;
  const next = neighbour(split.leaves(tab.root), id);
  const root = split.remove(tab.root, id);
  if (root === null || next === null) {
    terminals.tabs.splice(terminals.tabs.indexOf(tab), 1);
    return;
  }
  tab.root = root;
  if (tab.active === id) tab.active = next;
  if (typeof root === "number") tab.zoom = false;
}

/**
 * Runs `open`, which passes `hold` to `openSession` so that what it opens stays out of the tabs, then hands those still
 * open to `put` before anything else runs. Each call holds only its own sessions, and one `put` leaves out gets a tab.
 */
async function together(open: (hold: (id: number) => void) => Promise<unknown>, put: (ids: number[]) => void) {
  const ids: number[] = [];
  try {
    await open((id) => {
      held.add(id);
      ids.push(id);
    });
  } finally {
    ids.forEach((id) => held.delete(id));
    const alive = ids.filter((id) => sessionOf(id));
    if (alive.length) put(alive);
    alive.forEach(adopt);
  }
}

/**
 * From the launcher: in a new tab (a grid when several), or beside the current tab's active split. `remote`: the
 * launcher's Remote Control switch; others leave it to each project's policy.
 */
export async function openFrom(
  paths: string[],
  agent: string | null,
  side: Side | null,
  remote?: boolean,
): Promise<number | null> {
  const target = side ? currentTab() : undefined;
  let last: number | null = null;
  await together(
    (hold) => openRepos(paths, agent, hold, remote),
    (ids) => {
      last = ids[ids.length - 1];
      const tab = target && tabOf(target.active);
      if (tab && side) {
        let at = tab.active;
        for (const id of ids) {
          tab.root = split.insert(tab.root, at, id, side);
          at = id;
        }
        if (ids.length > 1) tab.root = split.equalize(tab.root);
        tab.zoom = false;
        tab.active = last;
        terminals.current = tab.id;
      } else {
        terminals.current = newTab(split.grid(ids) ?? last, last).id;
      }
    },
  );
  return last;
}

/**
 * Each saved tab in a tab of its own; a layout saved before tabs, or unreadable, as one tab in a grid. Returns the
 * session the first tab opens on.
 */
export async function openLayout(layout: Layout): Promise<number | null> {
  const count = layout.sessions.length;
  const saved = (layout.tabs ?? []).map((tab) => ({ name: tab.name ?? "", root: split.decode(tab.splits, count) }));
  const used = saved.flatMap((tab) => (tab.root === null ? [] : split.leaves(tab.root)));
  const valid = saved.length > 0 && saved.every((tab) => tab.root !== null) && new Set(used).size === used.length;
  const plan = valid ? saved : [{ name: "", root: split.grid(layout.sessions.map((_, i) => i)) }];
  let first: number | null = null;
  for (const { name, root } of plan) {
    if (root === null) continue;
    const opened = new Map<number, number>();
    await together(
      async (hold) => {
        for (const i of split.leaves(root)) {
          const id = await openSession(layout.sessions[i].path, layout.sessions[i].agent, hold);
          if (id !== null) opened.set(i, id);
        }
      },
      (ids) => {
        const tree = split.relabel(root, (i) => {
          const id = opened.get(i);
          return id !== undefined && ids.includes(id) ? id : null;
        });
        const tab = newTab(tree ?? ids[0], ids[0]);
        tab.name = name;
        if (first === null) {
          terminals.current = tab.id;
          first = tab.active;
        }
      },
    );
  }
  return first;
}

/** The tabs on screen as a layout: their sessions, and each tab's splits over them. */
export function layoutOf(name: string): Layout {
  const tabs = visibleTabs();
  const ids = tabs.flatMap((tab) => split.leaves(tab.root));
  return {
    name,
    sessions: ids.map((id) => ({ path: sessionOf(id)?.cwd ?? "", agent: sessionOf(id)?.agent ?? null })),
    tabs: tabs.map((tab) => ({ name: tab.name, splits: split.encode(tab.root, ids) })),
  };
}

/** Kept to open again, a tab whole or one split: `name` for the palette, `given` the one the user gave the tab. */
function remember(ids: number[], root: Pane, name: string, given = "") {
  const sessions = ids.map((id) => sessionOf(id)).filter((s): s is Session => !!s);
  if (sessions.length !== ids.length) return;
  const layout = {
    name,
    sessions: sessions.map((s) => ({ path: s.cwd, agent: s.agent })),
    tabs: [{ name: given, splits: split.encode(root, ids) }],
  };
  terminals.closed = pushRecent(terminals.closed, layout);
}

/** The `i`-th tab closed last, opened again in a new tab; returns the session that takes the keyboard. */
export async function reopenTab(i = 0): Promise<number | null> {
  const [layout] = terminals.closed.splice(i, 1);
  return layout ? openLayout(layout) : null;
}

/** A split beside `id` running the same tool in the same folder: one gesture, no launcher. */
export async function splitSame(id: number, side: Side): Promise<number | null> {
  const s = sessionOf(id);
  if (!s) return null;
  showSession(id);
  return openFrom([s.cwd], s.agent, side);
}

/** Every split of `from` dropped beside `at`, on `side`: the two tabs become one. */
export function mergeTab(from: Tab, at: number, side: Side) {
  const to = tabOf(at);
  if (!to || to === from) return;
  to.root = split.attach(to.root, at, from.root, side);
  to.active = from.active;
  to.zoom = false;
  terminals.tabs.splice(terminals.tabs.indexOf(from), 1);
  terminals.current = to.id;
}

/** Brings `id`'s tab to the front with `id` as its active split. */
export function showSession(id: number) {
  if (!sessionOf(id)) return;
  const tab = adopt(id);
  tab.active = id;
  terminals.current = tab.id;
}

export function activate(id: number) {
  const tab = tabOf(id);
  if (tab) tab.active = id;
}

export function toggleZoom(id: number) {
  const tab = tabOf(id);
  if (!tab) return;
  tab.zoom = !(tab.zoom && tab.active === id) && typeof tab.root !== "number";
  tab.active = id;
}

/**
 * Closes `id`'s split, first asking while its session runs (asking twice is a yes). Returns the session that takes
 * the keyboard: the next split of the tab, else the previous one, else the next tab's (else the previous tab's).
 */
export async function close(id: number, sure = false): Promise<number | null> {
  if (sessionOf(id)?.running && !sure && terminals.asking !== id) {
    terminals.asking = id;
    return null;
  }
  const tab = tabOf(id);
  if (tab) {
    const alone = split.leaves(tab.root).length === 1;
    const s = sessionOf(id);
    remember([id], id, alone ? tabName(tab) : s ? tileName(s.title, s.agent ?? "shell") : "", alone ? tab.name : "");
  }
  const tabs = visibleTabs();
  const after = tab ? (tabs[tabs.indexOf(tab) + 1] ?? tabs[tabs.indexOf(tab) - 1]) : undefined;
  const order = tab ? split.leaves(tab.root).filter((l) => l === id || isShown(l)) : [];
  const next = neighbour(order, id) ?? (after ? entryOf(after) : null);
  drop(id);
  await closeSession(id);
  return next;
}

/** Closes every session of a tab; returns the session that takes the keyboard. */
export async function closeTab(tab: Tab): Promise<number | null> {
  const tabs = visibleTabs();
  const after = tabs[tabs.indexOf(tab) + 1] ?? tabs[tabs.indexOf(tab) - 1];
  const ids = split.leaves(tab.root);
  remember(ids, tab.root, tabName(tab), tab.name);
  terminals.tabs.splice(terminals.tabs.indexOf(tab), 1);
  await Promise.all(ids.map(closeSession));
  return after ? entryOf(after) : null;
}

/** The tab `step` away from the current one, among those shown, round the ends. */
export function stepTab(step: number): Tab | undefined {
  const list = visibleTabs();
  const at = list.indexOf(currentTab()!);
  return list[(at + step + list.length) % list.length];
}

export function moveTab(from: Tab, to: Tab) {
  terminals.tabs = split.move(terminals.tabs, terminals.tabs.indexOf(from), terminals.tabs.indexOf(to));
}

export function neighbourOf(id: number, side: Side): number | null {
  const tab = tabOf(id);
  return tab ? split.neighbour(tab.root, id, side) : null;
}

export function resizeSplit(id: number, side: Side) {
  const tab = tabOf(id);
  if (tab) tab.root = split.resize(tab.root, id, side, 0.05);
}

export function resizeAt(tab: Tab, path: number[], i: number, delta: number) {
  tab.root = split.resizeAt(tab.root, path, i, delta);
}

export function equalize(tab = currentTab()) {
  if (tab) tab.root = split.equalize(tab.root);
}

export function equalizeAt(tab: Tab, path: number[]) {
  tab.root = split.equalizeAt(tab.root, path);
}

/** Lays the current tab out again: one tab per session, side by side, two columns, an even grid, or focus. */
export function arrange(how: Arrangement) {
  const tab = currentTab();
  if (!tab) return;
  const ids = split.leaves(tab.root);
  tab.zoom = false;
  if (how === "tabs") {
    tab.root = tab.active;
    let after = tab;
    for (const id of ids) if (id !== tab.active) after = newTab(id, id, after);
    return;
  }
  const columns = { row: ids.length, two: 2, grid: undefined, focus: undefined }[how];
  tab.root = (how === "focus" ? split.focus(ids, tab.active) : split.grid(ids, columns)) ?? tab.root;
}
