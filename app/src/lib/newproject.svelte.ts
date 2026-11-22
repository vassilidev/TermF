import { invoke } from "@tauri-apps/api/core";
import type { Card } from "$lib/setup-logic";
import { onScreen, panes, ui, type ViewId } from "$lib/state.svelte";
// Each module reads the other only inside functions, so the import cycles are safe.
import { usable } from "$lib/onboarding.svelte";
import { closePanel } from "$lib/panes.svelte";

/** The new project's setup: kept while open, so leaving it for another view and coming back loses nothing. */
export const creating = $state({ open: false, round: 0 });

/** Each repository's card by path, once its git state and stack are read. */
export const cards = $state<Record<string, Card>>({});

let back: ViewId = "terminals";
/** Paths whose card was asked for since a list of repositories was shown: each is read once per showing. */
const asked = new Set<string>();

export function newProject() {
  if (ui.view !== "newProject") back = ui.view;
  ui.switcher = false;
  creating.open = true;
  ui.view = "newProject";
}

/** Nothing was written: the setup starts over next time. */
export function closeNewProject() {
  creating.open = false;
  creating.round++;
  if (panes.other !== null && onScreen("newProject")) closePanel("newProject");
  else if (ui.view === "newProject") ui.view = usable(back);
}

/** Reads the cards of `paths` not asked for yet; a card read before is shown until the new one comes. */
export async function readCards(paths: string[]) {
  const todo = paths.filter((p) => !asked.has(p));
  if (!todo.length) return;
  for (const p of todo) asked.add(p);
  try {
    const list = await invoke<Card[]>("setup_repos", { paths: todo });
    for (const card of list) cards[card.path] = card;
  } catch (e) {
    for (const p of todo) asked.delete(p);
    throw e;
  }
}

/** A list of repositories is shown: its cards are read again, the stack's survey being kept by the backend. */
export function forgetCards() {
  asked.clear();
}
