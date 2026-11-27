import { invoke } from "@tauri-apps/api/core";
import type { Action } from "$lib/actions.svelte";
import { git, proposeCleanup, type GitState, type Pr } from "$lib/git.svelte";
import { t } from "$lib/i18n";
import { projectRepos } from "$lib/stack-actions";
import { toast } from "$lib/ui/toast.svelte";

const list = (v: unknown) => (Array.isArray(v) ? v.map(String).filter((s) => s.trim()) : []);
/** The repository the Git view shows, by its folder's name. */
const shown = () => git.repo.split(/[\\/]/).pop() ?? git.repo;

function repoShown(): string {
  if (!git.repo) throw new Error("The Git view shows no repository: ask the user to open one.");
  return git.repo;
}

const day = (seconds: number) => new Date(seconds * 1000).toISOString().slice(0, 10);

const gitRead: Action = {
  id: "git.read",
  description:
    "Reads a repository's git state, as the Git view shows it: the branch checked out, the default branch, each local branch with its upstream, how far ahead and behind, whether it is merged and its last commit; the worktrees, the latest tags, the stashes and the open pull requests. Reads only.",
  fields: {
    repo: { type: "string", description: "The repository, by its folder's name: by default the one the Git view shows, else the project's first." },
  },
  label: "actGitRead",
  describe: () => "",
  async run(a, _secrets, project) {
    const named = typeof a.repo === "string" && a.repo.trim();
    const repo = named ? projectRepos(project, a.repo)[0] : git.repo || projectRepos(project)[0];
    if (!repo) throw new Error("This project has no repository.");
    const [state, prs] = await Promise.all([
      invoke<GitState>("git_state", { repo }),
      invoke<Pr[]>("git_prs", { repo }).catch((e) => `unread: ${String(e)}`),
    ]);
    return {
      current: state.current,
      defaultBranch: state.defaultBranch,
      branches: state.branches
        .filter((b) => !b.remote)
        .map((b) => ({
          name: b.name,
          current: b.current,
          upstream: b.upstream,
          gone: b.gone,
          ahead: b.ahead,
          behind: b.behind,
          merged: b.merged,
          last: `${b.tip.subject} (${b.tip.author}, ${day(b.tip.date)})`,
        })),
      worktrees: state.worktrees.map((w) => ({ path: w.path, branch: w.branch })),
      tags: state.tags.slice(0, 5).map((tag) => tag.name),
      stashes: state.stashes.map((s) => s.message),
      pullRequests:
        typeof prs === "string" ? prs : prs.filter((p) => p.state === "OPEN").map((p) => ({ number: p.number, title: p.title, branch: p.branch, draft: p.draft, url: p.url })),
    };
  },
};

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

const ref = (v: unknown, field: string) => {
  const name = String(v ?? "").trim();
  if (!name || name.startsWith("-")) throw new Error(`${field}: a branch name`);
  return name;
};

const switchBranch: Action = {
  id: "git.switch",
  description:
    "Checks out a branch in the repository the Git view shows, as its Switch button does; a remote branch (origin/x) gets its local twin, made when missing.",
  fields: { branch: { type: "string", description: "The branch's name." } },
  required: ["branch"],
  label: "actGitSwitch",
  describe: (a) => t("actGitSwitchText", { branch: String(a.branch), repo: shown() }),
  async run(a) {
    const branch = ref(a.branch, "branch");
    await invoke("git_switch", { repo: repoShown(), branch });
    git.reads++;
    toast(t("gitSwitched", { branch }));
    return { switched: branch };
  },
};

const renameBranch: Action = {
  id: "git.rename_branch",
  description: "Renames a local branch of the repository the Git view shows, as its Rename button does.",
  fields: {
    from: { type: "string", description: "The branch's name now." },
    to: { type: "string", description: "Its new name." },
  },
  required: ["from", "to"],
  label: "actGitRename",
  describe: (a) => t("actGitRenameText", { from: String(a.from), to: String(a.to), repo: shown() }),
  async run(a) {
    const [from, to] = [ref(a.from, "from"), ref(a.to, "to")];
    await invoke("git_branch_rename", { repo: repoShown(), from, to });
    git.reads++;
    toast(t("gitRenamed", { name: to }));
    return { renamed: from, to };
  },
};

export const gitActions: Action[] = [gitRead, createBranch, switchBranch, renameBranch, cleanup];
