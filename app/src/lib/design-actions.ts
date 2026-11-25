import type { Action } from "$lib/actions.svelte";
import { design } from "$lib/design.svelte";
import { t } from "$lib/i18n";
import { app } from "$lib/state.svelte";

const reposOf = (project: string) => app.workspace?.projects.find((p) => p.id === project)?.repos ?? [];

const scan: Action = {
  id: "design.scan",
  description: "Runs the Design system view's analysis of a repository of the project again: its tokens, components, fonts and findings show there.",
  fields: { repo: { type: "string", description: "The repository's name in the project, by default the one the view shows." } },
  label: "actDesignScan",
  describe: (a, project) => {
    const repo = reposOf(project).find((r) => (a.repo ? r.name === a.repo || r.path === a.repo : r.path === design.repo));
    return t("actDesignScanText", { repo: repo?.name ?? String(a.repo ?? "") });
  },
  async run(a, _secrets, project) {
    if (typeof a.repo === "string" && a.repo.trim()) {
      const repo = reposOf(project).find((r) => r.name === a.repo || r.path === a.repo);
      if (!repo) throw new Error(`No repository ${a.repo} in this project.`);
      design.repo = repo.path;
    }
    design.scans++;
    return { scanning: design.repo, note: "The findings show in the Design system view." };
  },
};

export const designActions: Action[] = [scan];
