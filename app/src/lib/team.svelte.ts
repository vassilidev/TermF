import type { Item } from "$lib/library.svelte";
import { ui } from "$lib/state.svelte";

export const team = $state({
  /** Loaded once first shown, then kept mounted. */
  loaded: false,
  /** The current project's roles, or the global layer's under every project. */
  roles: [] as Item[],
});

export function showTeam() {
  ui.view = "team";
  team.loaded = true;
}
