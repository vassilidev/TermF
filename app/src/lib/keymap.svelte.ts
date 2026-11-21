import { invoke } from "@tauri-apps/api/core";
import { isMac } from "$lib/files.svelte";
import * as logic from "$lib/keymap-logic";
import { app, ui } from "$lib/state.svelte";

export { actions } from "$lib/keymap-logic";

/** The shortcuts: the registry's defaults, changed by `[keys]` of termf.toml. */
export const keymap = $state({
  /** Only what differs from the defaults, as saved; "" is no shortcut. */
  overrides: {} as Record<string, string>,
  /** The action whose new combination the editor is waiting for: every shortcut is off meanwhile. */
  recording: null as string | null,
  /** Why the system-wide shortcut is not registered. */
  globalError: null as string | null,
});

type Keys = { keys: Record<string, string>; error: string | null };

export async function loadKeys() {
  try {
    const loaded = await invoke<Keys>("keys");
    keymap.overrides = loaded.keys;
    keymap.globalError = loaded.error;
  } catch {
    // Unreadable termf.toml: the defaults hold, and the workspace error says why.
  }
}

/** Saves `overrides`; on failure nothing changes and the error is thrown. */
export async function saveKeys(overrides: Record<string, string>) {
  const saved = await invoke<Keys>("keys_set", { keys: overrides });
  keymap.overrides = saved.keys;
  keymap.globalError = saved.error;
  if (app.workspace) app.workspace.quickTerminalError = saved.error;
}

export const current = () => logic.bindings(keymap.overrides, isMac);
const combo = (id: string) => logic.parse(current()[id] ?? "");

/** As shown in `<kbd>`; "" when the action has no shortcut. `n`: the digit of a 1-to-9 action. */
export function display(id: string, n?: number): string {
  return logic.display(combo(id), isMac, n === undefined ? "" : String(n));
}

export function aria(id: string, n?: number): string | undefined {
  return logic.aria(combo(id), n === undefined ? "" : String(n));
}

/** The app's action for a key press, null to leave it alone (and while the editor records one). */
export function pressed(e: KeyboardEvent, limits?: Record<string, number>): logic.Hit | null {
  if (keymap.recording) return null;
  const inTerminal = !!(e.target as Element | null)?.closest?.(".xterm");
  return logic.match(e, current(), { mac: isMac, inTerminal, view: ui.view, limits });
}

/** The shortcuts as the views show them; reading one in a template follows its changes. */
export const keys = {
  get palette() {
    return display("palette");
  },
  get quick() {
    return display("quick_terminal");
  },
  get next() {
    return display("next_waiting");
  },
  get maximize() {
    return display("maximize");
  },
  get file() {
    return display("find_file");
  },
  get ticket() {
    return display("launch_ticket");
  },
  get switcher() {
    return display("switch_project");
  },
  get launcher() {
    return display("new_session");
  },
  get broadcast() {
    return display("broadcast");
  },
  tile: (n: number) => display("tile", n),
  project: (n: number) => display("project", n),
};
