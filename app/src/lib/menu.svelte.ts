import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { closeSplit, openLauncher, reopenTab } from "$lib/commands";
import { isMac } from "$lib/files.svelte";
import { current } from "$lib/keymap.svelte";
import { showSettings } from "$lib/settings.svelte";
import { onScreen } from "$lib/state.svelte";
import { foldNav, toggleNav, zoomBy } from "$lib/workspace.svelte";

/**
 * The native macOS menu's items, by id (`window.rs`): a key the page did not take reaches them, as does a click.
 * `ai_panel` (⌘I) is for the AI panel to add here.
 */
export const menuActions: Record<string, () => void> = {
  new_session: () => openLauncher(),
  close_split: () => onScreen("terminals") && closeSplit(),
  reopen_tab: () => reopenTab(),
  toggle_nav: toggleNav,
  hide_nav: () => foldNav("hidden"),
  zoom_in: () => zoomBy(1),
  zoom_out: () => zoomBy(-1),
  zoom_reset: () => zoomBy(0),
};

/** The shortcuts the menu shows, as the keymap has them now. */
const shown = ["new_session", "close_split", "reopen_tab", "toggle_nav", "zoom_in", "zoom_out", "zoom_reset"];

/** Settings… (⌘,) and the File and View menus of the native macOS menu, which follow the shortcuts' changes. */
export function watchMenu(): () => void {
  const off = listen("menu-settings", () => showSettings());
  const offItem = listen<string>("menu", ({ payload }) => menuActions[payload]?.());
  const stop = $effect.root(() => {
    $effect(() => {
      if (!isMac) return;
      const all = current();
      invoke("menu_keys", { keys: Object.fromEntries(shown.map((id) => [id, all[id] ?? ""])) }).catch(() => {});
    });
  });
  return () => {
    stop();
    void off.then((f) => f());
    void offItem.then((f) => f());
  };
}
