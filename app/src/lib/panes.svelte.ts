import { tick, untrack } from "svelte";
import * as logic from "$lib/panes-logic";
import { app, holds, panes, ui, type ViewId } from "$lib/state.svelte";
import { currentTab } from "$lib/tabs.svelte";
import { views } from "$lib/views";
// Each module reads the other only inside functions, so the import cycle is safe.
import { focusSession } from "$lib/commands";

const stored = "termf.panes";
const known = () => views.map((v) => v.id);
/** While the handle is dragged, the width is kept only once it is let go. */
const dragging = $state({ on: false });

/** The keyboard follows, so that what is typed goes where the view-wide shortcuts now point. */
export async function openBeside(id: ViewId) {
  const view = views.find((v) => v.id === id);
  if (!view || id === ui.view) return;
  if (id !== panes.other) {
    const { last } = panes;
    const p = logic.beside({ ...panes, view: ui.view }, id);
    if (holds(p, () => openBeside(id))) return;
    Object.assign(panes, { other: p.other, active: p.active });
    view.show();
    panes.last = last;
  } else view.show();
  await focusPanel();
}

/** The second panel reopens on the view closed. */
export function closePanel(id: ViewId) {
  const p = logic.close({ ...panes, view: ui.view }, id);
  if (panes.other === null || p.other !== null || holds(p, () => closePanel(id))) return;
  Object.assign(panes, { other: null, active: 0 });
  ui.view = p.view;
  panes.last = id;
}

/** With two panels, the one the keyboard is not in closes: the one being worked in stays. */
export async function toggleSplit() {
  if (panes.other) return closePanel(panes.other);
  const last = panes.last;
  await openBeside(last && last !== ui.view && known().includes(last) ? last : ui.view === "terminals" ? "tickets" : "terminals");
}

export async function otherPanel() {
  if (!panes.other) return;
  ui.view = panes.other;
  await focusPanel();
}

async function focusPanel() {
  const tab = ui.view === "terminals" ? currentTab() : undefined;
  if (tab) return focusSession(tab.active);
  await tick();
  document.querySelector<HTMLElement>(`[data-panel="${ui.view}"]`)?.focus({ preventScroll: true });
}

export function resizePanes(size: number) {
  panes.size = logic.clamp(size);
}

export function dragPanes(on: boolean) {
  dragging.on = on;
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
      if (!ready || dragging.on) return;
      try {
        if (text) localStorage.setItem(stored, text);
        // The onboarding assistant beside a view is not kept, and does not forget the panels kept before.
        else if (single) localStorage.removeItem(stored);
      } catch {
        // Storage off: the panels hold until the app closes.
      }
    });
  });
}
