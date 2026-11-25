import type { Action } from "$lib/actions.svelte";
import { t } from "$lib/i18n";
import { creating, draft } from "$lib/newproject.svelte";
import { app } from "$lib/state.svelte";

const applySetup: Action = {
  id: "setup.apply",
  description:
    "Applies the project's setup as the setup assistant's Ready step shows it (project, tickets, worktrees, CI, team, skills, Claude Code's trust), once the user went through its questions. Only on that step.",
  fields: {},
  label: "actSetupApply",
  describe: () =>
    t("actSetupApplyText", { project: app.workspace?.projects.find((p) => p.id === creating.project)?.name ?? draft.name ?? "" }),
  async run() {
    if (!creating.apply) throw new Error("The setup assistant is not on its Ready step, or that step is not ready: ask the user to go through it first.");
    await creating.apply();
    return { applied: true, note: "The assistant says in TermF what was done, and what failed if anything did." };
  },
};

export const setupActions: Action[] = [applySetup];
