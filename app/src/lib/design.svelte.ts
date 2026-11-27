import { chooseRepo, files, openFile, refreshStatus, shownRepo } from "$lib/files.svelte";
import type { Section } from "$lib/design-logic";
import { ui } from "$lib/state.svelte";

export const design = $state({
  /** Loaded once first shown, then kept mounted, like the CI/CD view. */
  loaded: false,
  /** The views' common choice: a repository's path, "" for all of the project's. */
  get repo() {
    return shownRepo();
  },
  set repo(path: string) {
    chooseRepo(path);
  },
  section: "color" as Section,
  /** Bumped to read the repository again (an agent's action). */
  scans: 0,
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
