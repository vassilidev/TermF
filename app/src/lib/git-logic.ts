// No imports but types, so `pnpm test` runs it in plain Node.
import type { Branch, Change, Pr } from "./git.svelte";

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
