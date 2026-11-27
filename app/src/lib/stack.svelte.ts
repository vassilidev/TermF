import { ui } from "$lib/state.svelte";
import type { Filter } from "$lib/stack-logic";

export const stack = $state({
  /** Loaded once first shown, then kept mounted, like the Design system view. */
  loaded: false,
  filter: "all" as Filter,
  query: "",
});

export function showStack() {
  ui.view = "stack";
  stack.loaded = true;
}
