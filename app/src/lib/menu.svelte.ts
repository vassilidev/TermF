import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toggleAsk } from "$lib/ask.svelte";
import { closeSplit, openLauncher, reopenTab } from "$lib/commands";
import { isMac } from "$lib/files.svelte";
import { current } from "$lib/keymap.svelte";
import { showSettings } from "$lib/settings.svelte";
import { ui } from "$lib/state.svelte";
import { menuRuns } from "$lib/workspace-logic";
import { foldNav, toggleNav, zoomBy } from "$lib/workspace.svelte";

/**
 * The native macOS menu's items, by id (`ITEMS` of `window.rs`): a key the page did not take reaches them, as does a
 * click, and `menuRuns` holds them back where the page's own keys would be.
 */
export const menuActions: Record<string, () => void> = {
  new_session: () => openLauncher(),
  close_split: () => closeSplit(),
  reopen_tab: () => reopenTab(),
  toggle_nav: toggleNav,
  hide_nav: () => foldNav("hidden"),
  zoom_in: () => zoomBy(1),
  zoom_out: () => zoomBy(-1),
  zoom_reset: () => zoomBy(0),
  ai_panel: toggleAsk,
};

/** The shortcuts the menu shows, as the keymap has them now. */
const shown = ["new_session", "close_split", "reopen_tab", "toggle_nav", "zoom_in", "zoom_out", "zoom_reset", "ai_panel"];

/** A menu item whose shortcut is the keymap's action of another name. */
const actionOf: Record<string, string> = { ai_panel: "ask_ai" };

/** Settings… (⌘,) and the File and View menus of the native macOS menu, which follow the shortcuts' changes. */
export function watchMenu(): () => void {
  const off = listen("menu-settings", () => showSettings());
  const offItem = listen<string>("menu", ({ payload }) => {
    if (menuRuns(payload, { palette: !!ui.palette, help: ui.help, view: ui.view })) menuActions[payload]?.();
  });
  const stop = $effect.root(() => {
    $effect(() => {
      if (!isMac) return;
      const all = current();
      invoke("menu_keys", { keys: Object.fromEntries(shown.map((id) => [id, all[actionOf[id] ?? id] ?? ""])) }).catch(() => {});
    });
  });
  return () => {
    stop();
    void off.then((f) => f());
    void offItem.then((f) => f());
  };
}
