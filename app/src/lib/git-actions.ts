import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { git, proposeCleanup } from "$lib/git.svelte";
import { t } from "$lib/i18n";
import { toast } from "$lib/ui/toast.svelte";

const list = (v: unknown) => (Array.isArray(v) ? v.map(String).filter((s) => s.trim()) : []);
/** The repository the Git view shows, by its folder's name. */
const shown = () => git.repo.split(/[\\/]/).pop() ?? git.repo;

function repoShown(): string {
  if (!git.repo) throw new Error("The Git view shows no repository: ask the user to open one.");
  return git.repo;
}

const createBranch: Action = {
  id: "git.create_branch",
  description: "Creates a branch in the repository the Git view shows, from a branch, tag or commit, and checks it out unless `checkout` is false.",
  fields: {
    name: { type: "string", description: "The new branch's name." },
    from: { type: "string", description: "A branch, tag or commit, HEAD by default." },
    checkout: { type: "boolean", description: "Check it out, true by default." },
  },
  required: ["name"],
  label: "actGitBranch",
  describe: (a) =>
    t(a.checkout === false ? "actGitBranchText" : "actGitBranchSwitchText", { name: String(a.name), from: String(a.from ?? "HEAD"), repo: shown() }),
  async run(a) {
    const name = String(a.name).trim();
    const from = String(a.from ?? "HEAD").trim();
    if (!name || name.startsWith("-") || from.startsWith("-")) throw new Error("name and from: a branch, tag or commit");
    await invoke("git_branch_create", { repo: repoShown(), name, from, checkout: a.checkout !== false });
    git.reads++;
    toast(t("gitCreated", { name }));
    return { created: name, from, checkedOut: a.checkout !== false };
  },
};

const cleanup: Action = {
  id: "git.cleanup",
  description:
    "Cleans up branches and worktrees of the repository the Git view shows, as its guided cleanup does: it picks them and applies at once what can come back; what loses work (commits, uncommitted changes) or closes a run waits for the user's confirmation there. Name them as the cleanup lists them.",
  fields: {
    branches: { type: "array", description: "Branch names." },
    worktrees: { type: "array", description: "Worktree paths." },
  },
  label: "actGitCleanup",
  describe: (a) => {
    const items = [...list(a.branches), ...list(a.worktrees)];
    return `${t("actGitCleanupText", { repo: shown(), count: items.length })}\n${items.join("\n")}`;
  },
  loses: () => t("actGitCleanupLoses"),
  async run(a) {
    const keys = [...list(a.branches).map((b) => `b:${b}`), ...list(a.worktrees).map((w) => `w:${w}`)];
    if (!keys.length) throw new Error("Name at least one branch or worktree.");
    proposeCleanup(repoShown(), keys, true);
    return { picked: keys.length, note: "Applied in the Git view's cleanup; what loses work waits for the user's confirmation there, and the view says what was done." };
  },
};

export const gitActions: Action[] = [createBranch, cleanup];
