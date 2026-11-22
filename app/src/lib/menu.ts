import { listen } from "@tauri-apps/api/event";
import { showSettings } from "$lib/settings.svelte";

/** Settings… (⌘,) of the native macOS menu. */
export function watchMenu(): () => void {
  const off = listen("menu-settings", () => showSettings());
  return () => void off.then((f) => f());
}
