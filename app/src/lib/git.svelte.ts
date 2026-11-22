import { ui } from "$lib/state.svelte";

export type Tip = { oid: string; author: string; date: number; subject: string };
export type Branch = {
  /** `main`, or `origin/main` for a remote-tracking branch. */
  name: string;
  remote: string | null;
  current: boolean;
  upstream: string | null;
  /** The upstream is set but no longer exists. */
  gone: boolean;
  ahead: number;
  behind: number;
  tip: Tip;
  /** In the default branch; null without one. */
  merged: boolean | null;
  /** The worktree it is checked out in, other than the repository's folder. */
  worktree: string | null;
};
/** git's two status letters: `index` against HEAD, `tree` against the index; `.` unchanged, `?` untracked. */
export type Change = { path: string; from: string | null; index: string; tree: string; conflict: boolean };
export type Worktree = {
  path: string;
  branch: string | null;
  head: string | null;
  main: boolean;
  bare: boolean;
  locked: boolean;
  prunable: boolean;
  changes: Change[];
  more: number;
  error: string | null;
};
export type GitState = {
  current: string | null;
  head: string | null;
  defaultBranch: string | null;
  remotes: string[];
  branches: Branch[];
  worktrees: Worktree[];
};
export type Ref = { name: string; kind: "local" | "remote" | "tag" };
export type Commit = { oid: string; parents: string[]; author: string; date: number; subject: string; refs: Ref[] };
export type Graph = { commits: Commit[]; more: boolean };
export type Pr = { number: number; title: string; state: "OPEN" | "MERGED" | "CLOSED"; draft: boolean; url: string; branch: string };
export type Unique = { commits: Commit[]; total: number };
export type Pull = { outcome: "updated" | "upToDate" | "diverged" | "noUpstream" | "gone"; ahead: number; behind: number };
export type Push = { remote: string; upstreamSet: boolean };

export const git = $state({
  /** Loaded once first shown, then kept mounted. */
  loaded: false,
  /** The repository's path, one of the current project's. */
  repo: "",
});

export function showGit() {
  ui.view = "git";
  git.loaded = true;
}
