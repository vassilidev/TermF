import { tick, untrack } from "svelte";
import * as logic from "$lib/panes-logic";
import { app, panes, ui, type ViewId } from "$lib/state.svelte";
import { currentTab } from "$lib/tabs.svelte";
import { views } from "$lib/views";
// Each module reads the other only inside functions, so the import cycle is safe.
import { focusSession } from "$lib/commands";

/** Remembered on this machine, like the current project. */
const stored = "termf.panes";
const known = () => views.map((v) => v.id);

/** Shows `id` in the other panel, opened on the right if there was one panel, and moves there. */
export function openBeside(id: ViewId) {
  const view = views.find((v) => v.id === id);
  if (!view || id === ui.view) return;
  if (id !== panes.other) {
    const { last } = panes;
    const p = logic.beside({ ...panes, view: ui.view }, id);
    Object.assign(panes, { other: p.other, active: p.active });
    view.show();
    panes.last = last;
  } else view.show();
}

/** Closes the panel showing `id`; the other one takes the whole area, and the second panel reopens on `id`. */
export function closePanel(id: ViewId) {
  const p = logic.close({ ...panes, view: ui.view }, id);
  if (panes.other === null || p.other !== null) return;
  Object.assign(panes, { other: null, active: 0 });
  ui.view = p.view;
  panes.last = id;
}

/** Opens the second panel on the view shown before, or closes the one the keyboard is not in. */
export async function toggleSplit() {
  if (panes.other) return closePanel(panes.other);
  const last = panes.last;
  const next = last && last !== ui.view && known().includes(last) ? last : ui.view === "terminals" ? "tickets" : "terminals";
  openBeside(next);
  await focusPanel();
}

/** The keyboard to the other panel. */
export async function otherPanel() {
  if (!panes.other) return;
  ui.view = panes.other;
  await focusPanel();
}

/** Gives the keyboard to the active panel: its active terminal, else the panel itself. */
async function focusPanel() {
  const tab = ui.view === "terminals" ? currentTab() : undefined;
  if (tab) return focusSession(tab.active);
  await tick();
  document.querySelector<HTMLElement>(`[data-panel="${ui.view}"]`)?.focus({ preventScroll: true });
}

export function resizePanes(size: number) {
  panes.size = logic.clamp(size);
}

function restore() {
  let saved: ReturnType<typeof logic.decode<ViewId>> = null;
  try {
    saved = logic.decode(localStorage.getItem(stored), known());
  } catch {
    return;
  }
  if (!saved) return;
  for (const id of [saved.right, saved.left]) views.find((v) => v.id === id)?.show();
  Object.assign(panes, { other: saved.right, active: 0, size: saved.size });
}

/** Reopens the panels of the last launch once the workspace is read, then keeps them. Returns a cleanup. */
export function watchPanes(): () => void {
  return $effect.root(() => {
    let ready = false;
    $effect(() => {
      if (ready || !app.workspace) return;
      ready = true;
      untrack(restore);
    });
    $effect(() => {
      const text = logic.encode({ ...panes, view: ui.view }, known());
      const single = panes.other === null;
      if (!ready) return;
      try {
        if (text) localStorage.setItem(stored, text);
        // A project's page beside a view is not kept, and does not forget the panels kept before.
        else if (single) localStorage.removeItem(stored);
      } catch {
        // Storage off: the panels hold until the app closes.
      }
    });
  });
}
