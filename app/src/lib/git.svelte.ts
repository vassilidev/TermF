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
  /** A merge, rebase, cherry-pick or revert stopped in it. */
  operation: Operation | null;
};
export type GitState = {
  current: string | null;
  head: string | null;
  defaultBranch: string | null;
  remotes: string[];
  branches: Branch[];
  worktrees: Worktree[];
  /** Newest first. */
  tags: GitTag[];
  /** `stash@{0}` first. */
  stashes: Stash[];
};
export type Ref = { name: string; kind: "local" | "remote" | "tag" };
export type Commit = { oid: string; parents: string[]; author: string; date: number; subject: string; refs: Ref[] };
export type Graph = { commits: Commit[]; more: boolean };
export type Pr = { number: number; title: string; state: "OPEN" | "MERGED" | "CLOSED"; draft: boolean; url: string; branch: string };
export type Unique = { commits: Commit[]; total: number };
export type Pull = { outcome: "updated" | "upToDate" | "diverged" | "noUpstream" | "gone"; ahead: number; behind: number };
export type Push = { remote: string; upstreamSet: boolean };
export type Operation = {
  kind: "merge" | "rebase" | "cherryPick" | "revert";
  /** The commit merged, picked, reverted, or replayed by the rebase. */
  theirs: string | null;
  /** A rebase: the branch rebased and the commit it goes onto, then the step of how many. */
  branch: string | null;
  onto: string | null;
  step: number;
  total: number;
};
/** `oid`: the tag ref's own value; `target`: its commit. `message` is empty for a lightweight tag. */
export type GitTag = { name: string; oid: string; target: string; annotated: boolean; message: string; tagger: string; date: number };
export type Stash = { index: number; oid: string; message: string; date: number };
export type DiffLine = { kind: "+" | "-" | " "; text: string; old: number | null; new: number | null; noEol: boolean };
export type Hunk = { oldStart: number; oldLines: number; newStart: number; newLines: number; section: string; lines: DiffLine[] };
export type FileDiff = {
  path: string;
  from: string | null;
  status: "A" | "D" | "R" | "C" | "M";
  binary: boolean;
  /** Its hunks and lines can be staged one by one. */
  partial: boolean;
  added: number;
  removed: number;
  hunks: Hunk[];
  /** Past the size cap: counted, not listed. */
  cut: boolean;
  /** Untracked: the file, or the files of the folder. */
  untracked: string[];
  /** What it was when read: an action on it is refused once it changed. */
  id: string;
};
/** Lines of a hunk; all of them when `lines` is null. */
export type Pick = { hunk: number; lines: number[] | null };
export type Outcome = { ok: boolean; pending: boolean; output: string };
export type CommitDetail = { commit: Commit; email: string; message: string; files: FileDiff[]; remote: string | null };
export type FileCommit = Commit & { path: string };
export type FileHistory = { commits: FileCommit[]; more: boolean };
export type Blame = { lines: { oid: string; text: string }[]; commits: Record<string, Tip>; cut: boolean };
export type RebasePlan = { commits: Commit[]; total: number; pushed: number };
export type ResetPlan = { branch: string; leaving: number; lost: Unique; changes: Change[] };
export type ForcePlan = { remote: string; theirs: string; lost: Unique };

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
