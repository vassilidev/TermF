import { files, openFile, refreshStatus } from "$lib/files.svelte";
import type { Section } from "$lib/design-logic";
import { ui } from "$lib/state.svelte";

export const design = $state({
  /** Loaded once first shown, then kept mounted, like the CI/CD view. */
  loaded: false,
  /** The repository's path, one of the current project's. */
  repo: "",
  tab: "system" as "system" | "brand",
  section: "issues" as Section,
});

export function showDesign() {
  ui.view = "design";
  design.loaded = true;
}

/** The file in TermF's editor, at the line. */
export function openAt(repo: string, file: string, line: number) {
  files.repo = repo;
  files.loaded = true;
  ui.view = "files";
  refreshStatus();
  openFile(file, line);
}
