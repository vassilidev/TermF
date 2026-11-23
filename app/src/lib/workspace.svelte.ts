import { untrack } from "svelte";
import { MediaQuery } from "svelte/reactivity";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { t } from "$lib/i18n";
import { display } from "$lib/keymap.svelte";
import { dismiss, toast } from "$lib/ui/toast.svelte";
import * as logic from "$lib/workspace-logic";
import { FONT } from "$lib/workspace-logic";
// Each module reads the other only inside functions, so the import cycle is safe.
import { app, panes, ui } from "$lib/state.svelte";
import { views } from "$lib/views";

const navKey = "termf.nav";
const zoomKey = "termf.zoom";
const viewKey = "termf.view";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Storage off: kept until the app closes.
  }
}

/** The navigation's width and fold, kept on this machine. */
export const nav = $state(logic.readNav(read(navKey)));
const narrowWindow = new MediaQuery("max-width: 1100px");

/** Unfolded by hand while the window is narrow, for as long as the app runs. */
const wide = $state({ forced: false });
const folded = () => narrowWindow.current && !wide.forced;

/** Icons only: folded, or the window too narrow for names. */
export const navRail = () => nav.mode === "rail" || (nav.mode === "full" && folded());
export const navColumn = () => logic.navColumn(nav, folded());

/** `keep`: written to storage, once a drag ends rather than at each move. */
export function setNav(next: logic.Nav, keep = true) {
  Object.assign(nav, next);
  if (next.mode === "full" && narrowWindow.current) wide.forced = true;
  if (keep) write(navKey, JSON.stringify(nav));
}

/** Hidden, it says how to bring it back, and the way back is a click away. */
function hiddenNotice(before: logic.NavMode) {
  if (nav.mode !== "hidden") return;
  toast(t("wsNavHidden", { key: display("toggle_nav") }), { kind: "info", undo: () => setNav({ ...nav, mode: before }) });
}

/** ⌘B and the navigation's button: what is seen folds or unfolds, even when only the narrow window folded it. */
export function toggleNav() {
  if (nav.mode === "full" && narrowWindow.current) {
    wide.forced = !wide.forced;
    return;
  }
  const before = nav.mode;
  setNav(logic.toggleNav(nav));
  hiddenNotice(before);
}

/** What ⌘B folds the navigation to, from now on, folded at once. */
export function foldNav(fold: logic.Nav["fold"]) {
  const before = nav.mode;
  setNav({ ...nav, fold, mode: fold });
  hiddenNotice(before);
}

/** The interface's zoom, kept; each terminal's font size, for as long as its session. */
export const zoom = $state({ ui: Number(read(zoomKey)) || 1, fonts: {} as Record<number, number> });
let zoomNote = 0;

function applyZoom() {
  try {
    getCurrentWebview().setZoom(zoom.ui).catch(() => {});
  } catch {
    // Outside Tauri (a mocked page): nothing to zoom.
  }
}

/** ⌘+ ⌘− ⌘0: the text of the terminal that has the keyboard, else the whole interface. */
export function zoomBy(dir: -1 | 0 | 1) {
  const term = document.activeElement?.closest<HTMLElement>("[data-term]");
  let text: string;
  if (term) {
    const id = Number(term.dataset.term);
    const size = logic.fontStep(zoom.fonts[id] ?? FONT.base, dir);
    zoom.fonts[id] = size;
    text = t("wsZoomTerminal", { pct: Math.round((size / FONT.base) * 100) });
  } else {
    zoom.ui = logic.zoomStep(zoom.ui, dir);
    write(zoomKey, String(zoom.ui));
    applyZoom();
    text = t("wsZoomUi", { pct: Math.round(zoom.ui * 100) });
  }
  dismiss(zoomNote);
  zoomNote = toast(text, { kind: "info", ms: 1500 });
}

/** The terminal a file from the Finder is dragged over. */
export const dropping = $state({ on: null as number | null });

/** Pasted as typed: escaped paths, the keyboard left in that terminal. */
function onDrop(event: { payload: { type: string; paths?: string[]; position?: { x: number; y: number } } }) {
  const { type, paths, position } = event.payload;
  const at = position && logic.dropPoint(position, devicePixelRatio);
  const el = at && document.elementFromPoint(at.x, at.y)?.closest<HTMLElement>("[data-term]");
  dropping.on = type === "enter" || type === "over" ? (el ? Number(el.dataset.term) : null) : null;
  if (type !== "drop" || !el || !paths?.length) return;
  const data = logic.shellPaths(paths, navigator.platform.startsWith("Win"));
  invoke("pty_write", { id: Number(el.dataset.term), data }).catch((e) => (app.error = String(e)));
  el.querySelector<HTMLElement>(".xterm-helper-textarea")?.focus();
}

/** The view shown at the last launch comes back (two panels are `panes`' to restore), and the zoom. */
export function watchWorkspace(): () => void {
  let stopDrop: (() => void) | undefined;
  try {
    getCurrentWebview()
      .onDragDropEvent(onDrop)
      .then((stop) => (stopDrop = stop), () => {});
  } catch {
    // Outside Tauri: no files are dropped.
  }
  if (zoom.ui !== 1) applyZoom();
  const stop = $effect.root(() => {
    let ready = false;
    $effect(() => {
      if (ready || !app.workspace) return;
      ready = true;
      untrack(() => {
        const saved = read(viewKey);
        if (panes.other === null && saved !== ui.view) views.find((v) => v.id === saved)?.show();
      });
    });
    $effect(() => {
      const view = ui.view;
      if (ready && views.some((v) => v.id === view)) write(viewKey, view);
    });
  });
  return () => {
    stop();
    stopDrop?.();
  };
}
