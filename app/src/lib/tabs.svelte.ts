import * as split from "$lib/split-logic";
import type { Pane, Side } from "$lib/split-logic";
import { shown } from "$lib/project.svelte";
import { neighbour, tileName } from "$lib/tile-logic";
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
});

let nextId = 1;
/** The sessions opening together, placed once they have all opened. */
let batch: number[] | null = null;

const sessionOf = (id: number): Session | undefined => app.sessions.find((s) => s.id === id);

export function tabOf(id: number): Tab | undefined {
  return terminals.tabs.find((tab) => split.leaves(tab.root).includes(id));
}

/** The tabs under the current project: those with a session it shows. */
export function visibleTabs(): Tab[] {
  return terminals.tabs.filter((tab) =>
    split.leaves(tab.root).some((id) => {
      const s = sessionOf(id);
      return !!s && shown(s);
    }),
  );
}

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

/** Where `openSession` puts a session: with those opening together, else in a new tab brought to the front. */
export function place(id: number) {
  if (batch) batch.push(id);
  else terminals.current = adopt(id).id;
}

/** Keeps the tabs in step with the sessions: a closed one leaves its split, one opened elsewhere gets a tab. */
export function reconcile() {
  const ids = new Set(app.sessions.map((s) => s.id));
  terminals.tabs.flatMap((tab) => split.leaves(tab.root)).filter((id) => !ids.has(id)).forEach(drop);
  for (const id of ids) if (!batch?.includes(id)) adopt(id);
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

/** Runs `open` with the sessions it opens kept out of the tabs, then hands them to `put` before anything else runs. */
async function together(open: () => Promise<unknown>, put: (ids: number[]) => void) {
  const ids: number[] = [];
  batch = ids;
  try {
    await open();
  } finally {
    batch = null;
    if (ids.length) put(ids);
  }
}

/** From the launcher: in a new tab (a grid when several), or beside the current tab's active split. */
export async function openFrom(paths: string[], agent: string | null, side: Side | null): Promise<number | null> {
  const target = side ? currentTab() : undefined;
  let last: number | null = null;
  await together(
    () => openRepos(paths, agent),
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

/** Each saved tab in a tab of its own; a layout saved before tabs, or unreadable, as one tab in a grid. */
export async function openLayout(layout: Layout) {
  const count = layout.sessions.length;
  const saved = (layout.tabs ?? []).map((tab) => ({ name: tab.name ?? "", root: split.decode(tab.splits, count) }));
  const used = saved.flatMap((tab) => (tab.root === null ? [] : split.leaves(tab.root)));
  const valid = saved.length > 0 && saved.every((tab) => tab.root !== null) && new Set(used).size === used.length;
  const plan = valid ? saved : [{ name: "", root: split.grid(layout.sessions.map((_, i) => i)) }];
  for (const { name, root } of plan) {
    if (root === null) continue;
    const opened = new Map<number, number>();
    await together(
      async () => {
        for (const i of split.leaves(root)) {
          const id = await openSession(layout.sessions[i].path, layout.sessions[i].agent);
          if (id !== null) opened.set(i, id);
        }
      },
      (ids) => {
        const tab = newTab(split.relabel(root, (i) => opened.get(i) ?? null) ?? ids[0], ids[0]);
        tab.name = name;
        terminals.current = tab.id;
      },
    );
  }
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

/** Brings `id`'s tab to the front with `id` as its active split. */
export function showSession(id: number) {
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
  const tabs = visibleTabs();
  const after = tab ? (tabs[tabs.indexOf(tab) + 1] ?? tabs[tabs.indexOf(tab) - 1]) : undefined;
  const next = tab ? (neighbour(split.leaves(tab.root), id) ?? after?.active ?? null) : null;
  drop(id);
  await closeSession(id);
  return next;
}

/** Closes every session of a tab; returns the session that takes the keyboard. */
export async function closeTab(tab: Tab): Promise<number | null> {
  const tabs = visibleTabs();
  const after = tabs[tabs.indexOf(tab) + 1] ?? tabs[tabs.indexOf(tab) - 1];
  const ids = split.leaves(tab.root);
  terminals.tabs.splice(terminals.tabs.indexOf(tab), 1);
  await Promise.all(ids.map(closeSession));
  return after?.active ?? null;
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
