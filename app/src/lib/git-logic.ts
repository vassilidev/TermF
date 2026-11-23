// No imports but types, so `pnpm test` runs it in plain Node.
import type { Branch, BranchCleanup, Change, Cleanup, Pick as Parts, Pr, Reason, WorktreeCleanup } from "./git.svelte";

/**
 * One stroke of a graph row. `through`: straight down lane `from`; `in`: from the top of lane `from` to the commit,
 * in lane `to`; `out`: from the commit, in lane `from`, to the bottom of lane `to`.
 */
export type Line = { kind: "through" | "in" | "out"; from: number; to: number; color: number };
/** A commit's row: its lane, its color, the strokes around it and how many lanes the row spans. */
export type Row = { lane: number; color: number; lines: Line[]; width: number };

/**
 * Lays commits out in lanes, `colors` colors in turn. Commits come children first (git's `--date-order`): each
 * lane waits for one commit, the first parent takes over its child's lane, a merge's other parents get a lane of
 * their own, and lanes waiting for the same commit join it there. A parent never read keeps its lane to the end.
 */
export function lanes(commits: { oid: string; parents: string[] }[], colors = 8): Row[] {
  const waiting: (string | null)[] = [];
  const hue: number[] = [];
  let next = 0;
  const open = (oid: string) => {
    let at = waiting.indexOf(null);
    if (at < 0) at = waiting.length;
    waiting[at] = oid;
    hue[at] = next++ % colors;
    return at;
  };
  return commits.map((commit) => {
    const lines: Line[] = [];
    let lane = waiting.indexOf(commit.oid);
    const tip = lane < 0;
    if (tip) lane = open(commit.oid);
    const above = waiting.length;
    for (let i = 0; i < above; i++) {
      if (waiting[i] === null || (tip && i === lane)) continue;
      if (waiting[i] !== commit.oid) {
        lines.push({ kind: "through", from: i, to: i, color: hue[i] });
        continue;
      }
      lines.push({ kind: "in", from: i, to: lane, color: hue[i] });
      if (i !== lane) waiting[i] = null;
    }
    const color = hue[lane];
    const [first, ...others] = commit.parents;
    waiting[lane] = first ?? null;
    if (first !== undefined) lines.push({ kind: "out", from: lane, to: lane, color });
    for (const parent of others) {
      let to = waiting.indexOf(parent);
      if (to < 0) to = open(parent);
      lines.push({ kind: "out", from: lane, to, color: hue[to] });
    }
    const width = Math.max(above, waiting.length, lane + 1);
    while (waiting.length && waiting[waiting.length - 1] === null) waiting.pop();
    return { lane, color, lines, width };
  });
}

/** The SVG path of a stroke, lanes `step` apart, in a row `height` tall. */
export function path(line: Line, step: number, height: number): string {
  const x = (lane: number) => step / 2 + lane * step;
  const mid = height / 2;
  const [x1, x2] = [x(line.from), x(line.to)];
  if (line.kind === "through") return `M${x1} 0V${height}`;
  const [y1, y2] = line.kind === "in" ? [0, mid] : [mid, height];
  if (x1 === x2) return `M${x1} ${y1}V${y2}`;
  return `M${x1} ${y1}C${x1} ${y2} ${x2} ${y1} ${x2} ${y2}`;
}

/** The branch name on its remote: `origin/feature/x` → `feature/x`. */
export function onRemote(branch: Pick<Branch, "name" | "remote">): string {
  return branch.remote ? branch.name.slice(branch.remote.length + 1) : branch.name;
}

/** A branch's pull request: its upstream's (else its own name's) newest open one, else the newest. */
export function prFor(branch: Branch, prs: Pr[], remotes: string[]): Pr | undefined {
  let name = onRemote(branch);
  if (!branch.remote && branch.upstream) {
    const remote = remotes.filter((r) => branch.upstream!.startsWith(`${r}/`)).sort((a, b) => b.length - a.length)[0];
    if (remote) name = branch.upstream.slice(remote.length + 1);
  }
  const mine = prs.filter((pr) => pr.branch === name);
  return mine.find((pr) => pr.state === "OPEN") ?? mine[0];
}

/** The local branch that tracks a remote one, to switch to it rather than make another. */
export function localFor(remote: Branch, branches: Branch[]): Branch | undefined {
  return branches.find((b) => !b.remote && b.upstream === remote.name);
}

/** Local branches, the current one first then the most recent; remote ones the same, without the current. */
export function sorted(branches: Branch[]): { local: Branch[]; remote: Branch[] } {
  const order = (a: Branch, b: Branch) => Number(b.current) - Number(a.current) || b.tip.date - a.tip.date;
  return {
    local: branches.filter((b) => !b.remote).sort(order),
    remote: branches.filter((b) => b.remote).sort(order),
  };
}

export type ChangeKind = "conflict" | "untracked" | "added" | "deleted" | "renamed" | "modified";

/** What happened to a file, by git's two letters (index, work tree). */
export function changeKind(change: Change): ChangeKind {
  if (change.conflict) return "conflict";
  if (change.index === "?") return "untracked";
  const letters = change.index + change.tree;
  if (letters.includes("R") || letters.includes("C")) return "renamed";
  if (letters.includes("A")) return "added";
  if (letters.includes("D")) return "deleted";
  return "modified";
}

/** Changed and conflicted files, for a short summary. */
export function tally(changes: Change[], more = 0): { changed: number; conflicts: number } {
  const conflicts = changes.filter((c) => c.conflict).length;
  return { changed: changes.length + more - conflicts, conflicts };
}

/** Same folder, whatever the slashes and a trailing one: git lists `C:/a/b`, TermF may keep `C:\a\b\`. */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
  return norm(a) === norm(b);
}

/** A worktree's changes by where they stand: conflicts, then staged (index), then the work tree's and untracked. */
export function sides(changes: Change[]): { conflicts: Change[]; staged: Change[]; unstaged: Change[] } {
  const rest = changes.filter((c) => !c.conflict);
  return {
    conflicts: changes.filter((c) => c.conflict),
    staged: rest.filter((c) => c.index !== "." && c.index !== "?"),
    unstaged: rest.filter((c) => c.tree !== "."),
  };
}

/** Picked lines (`hunk:line` keys) as git picks, one per hunk, in order. */
export function picks(keys: Iterable<string>): Parts[] {
  const by = new Map<number, number[]>();
  for (const key of keys) {
    const [hunk, line] = key.split(":").map(Number);
    by.set(hunk, [...(by.get(hunk) ?? []), line]);
  }
  return [...by].sort(([a], [b]) => a - b).map(([hunk, lines]) => ({ hunk, lines: lines.sort((a, b) => a - b) }));
}

/** The changed lines from `from` to `to` in a hunk, both included, as `hunk:line` keys (a shift-click range). */
export function span(hunk: number, lines: { kind: string }[], from: number, to: number): string[] {
  const [a, b] = from < to ? [from, to] : [to, from];
  const keys: string[] = [];
  for (let i = a; i <= b; i++) if (lines[i]?.kind !== " ") keys.push(`${hunk}:${i}`);
  return keys;
}

/** Consecutive lines of the same commit, for a blame: where each run starts and how long it is. */
export function runs(lines: { oid: string }[]): { oid: string; start: number; count: number }[] {
  const out: { oid: string; start: number; count: number }[] = [];
  lines.forEach((line, i) => {
    const last = out[out.length - 1];
    if (last?.oid === line.oid) last.count++;
    else out.push({ oid: line.oid, start: i, count: 1 });
  });
  return out;
}

/** A commit's message as a subject and a body. */
export function splitMessage(message: string): { subject: string; body: string } {
  const [subject, ...rest] = message.split("\n");
  return { subject: subject.trim(), body: rest.join("\n").replace(/^\s*\n/, "").trimEnd() };
}

/** The message git gets: the subject, a blank line, the body. */
export function joinMessage(subject: string, body: string): string {
  return body.trim() ? `${subject.trim()}\n\n${body.trimEnd()}` : subject.trim();
}

/** Not committed yet, in a blame. */
export function uncommitted(oid: string): boolean {
  return /^0+$/.test(oid);
}

/** A worktree's folder relative to the repository's, `/`-separated, when it is inside it; null otherwise. */
export function within(repo: string, dir: string): string | null {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");
  const [r, d] = [norm(repo), norm(dir)];
  if (r === d) return "";
  return d.startsWith(`${r}/`) ? d.slice(r.length + 1) : null;
}

/** A cleanup item's key: `b:<branch>` or `w:<worktree path>`. */
export const keyOf = (item: BranchCleanup | WorktreeCleanup): string => ("path" in item ? `w:${item.path}` : `b:${item.name}`);

/**
 * What a selection removes and loses, worktrees first (a branch checked out in one goes after it). `runs`: the runs it
 * closes; `final`: something cannot come back (commits, changes, a closed run), so it asks first.
 */
export function chosen(cleanup: Cleanup, keys: Set<string>) {
  const worktrees = cleanup.worktrees.filter((w) => !w.blocked && keys.has(keyOf(w)));
  const branches = cleanup.branches.filter((b) => !b.blocked && keys.has(keyOf(b)));
  const lost = [...worktrees, ...branches].reduce((n, i) => n + i.lost, 0);
  const changes = worktrees.reduce((n, w) => n + w.changes, 0);
  const runs = worktrees.filter((w) => w.closeRun).length;
  return { worktrees, branches, lost, changes, runs, final: lost + changes + runs > 0, count: worktrees.length + branches.length };
}

/** One step of an undo, in the order to play it. */
export type UndoStep = { kind: "branch"; name: string; at: string } | { kind: "worktree"; path: string; branch: string | null; head: string };

/**
 * What puts a cleaned worktree back, from what was done: its branch first when it went, then the folder on the branch
 * it had checked out (detached only when it had none). A closed run, a folder already gone or one whose changes were
 * lost does not come back; its branch does.
 */
export function undoFor(w: WorktreeCleanup, done: { removed: boolean; branchDeleted: boolean }): UndoStep[] {
  const steps: UndoStep[] = [];
  if (w.closeRun || !w.head) return steps;
  if (done.branchDeleted && w.branch) steps.push({ kind: "branch", name: w.branch, at: w.head });
  if (done.removed && !w.reasons.includes("prunable") && !w.changes)
    steps.push({ kind: "worktree", path: w.path, branch: w.checkedOut, head: w.head });
  return steps;
}

/** The project's repository a plan names, or null: a plan without one, or naming another, is never applied. */
export function planTarget(repo: string | null, repos: string[]): string | null {
  return (repo && repos.find((r) => samePath(r, repo))) ?? null;
}

/** The plan waiting for `repo`, taken once: the holder forgets it, so reopening the cleanup never plays it again. */
export function takePlan<T extends { repo: string }>(holder: { plan: T | null }, repo: string): T | null {
  const plan = holder.plan;
  if (!plan || !samePath(plan.repo, repo)) return null;
  holder.plan = null;
  return plan;
}

/** The keys of what can be picked, and of what is suggested. */
export function pickable(cleanup: Cleanup): { all: string[]; suggested: string[] } {
  const items = [...cleanup.worktrees, ...cleanup.branches].filter((i) => !i.blocked);
  return { all: items.map(keyOf), suggested: items.filter((i) => i.suggested).map(keyOf) };
}

/**
 * A cleanup an agent proposed in a `termf-cleanup` block: `branch <name>` and `worktree <path>` lines, an optional
 * `repo <path>`; anything else is left out.
 */
export function planOf(text: string): { repo: string | null; keys: string[] } {
  let repo: string | null = null;
  const keys: string[] = [];
  for (const raw of text.split("\n")) {
    const m = /^\s*[-*]?\s*(branch|worktree|repo)\s+`?([^`]+?)`?\s*$/.exec(raw);
    if (!m) continue;
    if (m[1] === "repo") repo = m[2];
    else keys.push(`${m[1] === "branch" ? "b" : "w"}:${m[2]}`);
  }
  return { repo, keys: [...new Set(keys)] };
}

const folder = (key: string) => (key.startsWith("w:") ? `w:${key.slice(2).replace(/\\/g, "/").replace(/\/+$/, "")}` : key);

/** The keys of a plan as the cleanup knows them (a worktree by the same folder), and how many it does not know. */
export function matchPlan(cleanup: Cleanup, keys: string[]): { keys: string[]; unknown: number } {
  const known = new Map(pickable(cleanup).all.map((k) => [folder(k), k]));
  const found = keys.map((k) => known.get(folder(k)));
  return { keys: found.filter((k): k is string => !!k), unknown: found.filter((k) => !k).length };
}

/** The one next step of the Git view, the most pressing first; null when there is none (or a conflict says it). */
export function nextStep(f: {
  operation: boolean;
  behind: number;
  changed: number;
  ahead: number;
  suggested: number;
}): "pull" | "commit" | "push" | "cleanup" | null {
  if (f.operation) return null;
  if (f.behind) return "pull";
  if (f.changed) return "commit";
  if (f.ahead) return "push";
  return f.suggested ? "cleanup" : null;
}

const day = (seconds: number) => new Date(seconds * 1000).toISOString().slice(0, 10);
const words: Record<Reason, string> = {
  merged: "merged",
  prMerged: "pull request merged",
  gone: "deleted on the remote",
  stale: "no commit for 3 months",
  run: "TermF run over",
  detached: "on no branch",
  prunable: "folder gone",
};

/**
 * What the git expert reads with a question from the Git view: the branches and worktrees that could go, why and
 * what each would lose, the recent history, and how to propose a cleanup the user can apply.
 */
export function cleanupContext(
  repo: string,
  state: { current: string | null; defaultBranch: string | null },
  cleanup: Cleanup | null,
  commits: { oid: string; author: string; date: number; subject: string }[],
): string {
  const lines = [`Repository ${repo}, on ${state.current ?? "a detached HEAD"}; default branch ${state.defaultBranch ?? "unknown"}.`];
  if (cleanup) {
    const why = (r: Reason[]) => (r.length ? r.map((x) => words[x]).join(", ") : "in progress");
    const blocked = (b: string | null) => (b ? ` · cannot go: ${b === "locked" ? "locked" : "its run is still open"}` : "");
    lines.push("Local branches checked out nowhere (the default one left out):");
    for (const b of cleanup.branches)
      lines.push(
        `- ${b.name}: ${why(b.reasons)} · ${b.lost} commit(s) only here · last commit ${day(b.tip.date)} "${b.tip.subject}"${b.run ? ` · run #${b.run}` : ""}${blocked(b.blocked)}`,
      );
    if (!cleanup.branches.length) lines.push("- none");
    lines.push("Worktrees other than the repository's folder (removing one deletes its folder):");
    for (const w of cleanup.worktrees)
      lines.push(
        `- ${w.path}: ${w.branch ? `branch ${w.branch} goes with it` : "its branch stays"} · ${why(w.reasons)} · ${w.changes} uncommitted change(s) · ${w.lost} commit(s) lost${w.run ? ` · run #${w.run}` : ""}${blocked(w.blocked)}`,
      );
    if (!cleanup.worktrees.length) lines.push("- none");
  }
  if (commits.length) {
    lines.push("Recent history of every branch, newest first:");
    for (const c of commits.slice(0, 30)) lines.push(`- ${c.oid.slice(0, 7)} ${c.subject} (${c.author}, ${day(c.date)})`);
  }
  lines.push(
    `To propose a cleanup, end your answer with a fenced code block whose language is termf-cleanup: first \`repo ${repo}\`, then one line per item of these lists, \`branch <name>\` or \`worktree <path>\`. TermF shows it with Apply, Modify and Ignore; nothing is removed until the user applies it, and whatever would lose work is confirmed first.`,
  );
  return lines.join("\n");
}
