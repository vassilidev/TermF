// No imports, so `pnpm test` runs it in plain Node.

export type Step = "analysis" | "questions" | "proposals" | "trust" | "done";
export type Theme = "quality" | "tests" | "workflow" | "agents" | "other";
export type Kind = "choice" | "multi" | "bool" | "text";
export type Question = { id: string; theme: Theme; text: string; kind: Kind; options: string[]; default: string[] };
export type RepoTrust = { name: string; path: string; root: string; trusted: boolean; allow: number; mcp: number };

export const steps: Step[] = ["analysis", "questions", "proposals", "trust", "done"];
const themes: Theme[] = ["quality", "tests", "workflow", "agents", "other"];

/** The questions by theme, in a fixed order, empty themes left out. */
export function byTheme(questions: Question[]): [Theme, Question[]][] {
  return themes.map((theme) => [theme, questions.filter((q) => q.theme === theme)] as [Theme, Question[]]).filter(
    ([, list]) => list.length,
  );
}

/** The answer once `option` is clicked: one option for a choice or yes/no, ticked or not for a multiple choice. */
export function pick(q: Pick<Question, "kind" | "options">, value: string[], option: string): string[] {
  if (q.kind !== "multi") return [option];
  const next = value.includes(option) ? value.filter((v) => v !== option) : [...value, option];
  return q.options.filter((o) => next.includes(o));
}

/** Whether the owner may go to `step`: back to any step reached, forward never past where the assistant is. */
export function reachable(step: Step, current: Step, analysed: boolean): boolean {
  if (step === "analysis") return true;
  return analysed && steps.indexOf(step) <= steps.indexOf(current);
}

/** How much of the project Claude Code trusts: every repository, some, or none. */
export function trustState(repos: RepoTrust[]): "on" | "partial" | "off" {
  const trusted = repos.filter((r) => r.trusted).length;
  return trusted && trusted === repos.length ? "on" : trusted ? "partial" : "off";
}

type GithubTracker = {
  repo: string | null;
  comment: boolean;
  issues: boolean;
  project: string | null;
  columns: string[];
  mine: boolean;
  transition: string | null;
};
export type Trackers = {
  github: GithubTracker | null;
  jira: unknown;
  mcp: Record<string, unknown>;
  implicit: boolean;
};

/**
 * The ticket proposal: `saved` when the project already lists its sources, `github` to write GitHub Issues
 * explicitly for the project's GitHub repositories, `none` when nothing can be guessed.
 */
export function ticketProposal(trackers: Trackers, githubRepos: string[]): "saved" | "github" | "none" {
  if (!trackers.implicit) return "saved";
  return githubRepos.length ? "github" : "none";
}

/** GitHub Issues of the project's repositories, everything else at its default. */
export function githubIssues(trackers: Trackers): Trackers {
  return {
    ...trackers,
    github: { repo: null, comment: false, issues: true, project: null, columns: [], mine: true, transition: null },
    implicit: false,
  };
}

type Dep = { dir: string; clone: boolean; install: string[] };
export type RepoRecipe = {
  stack: string[];
  include: string[];
  deps: Dep[];
  database?: string;
  compose: boolean;
  ports: string[];
  commands: string[];
};

export type TeamSettings = { writes?: string[]; model?: string; effort?: string };
export type TeamSquad = {
  id: string;
  lead: string;
  roles: Record<string, number>;
  repos: string[];
  paths: string[];
  domains: string[];
};
export type TeamEdit = {
  team: { squads: TeamSquad[]; staff: string[] } | null;
  workflow: string | null;
  roles: Record<string, TeamSettings>;
};
export type TeamStatus = "new" | "same" | "changed" | "edited";
export type TeamDecision = "accepted" | "rejected";
/** The owner's changes to a part: the workflow, or a role's settings. */
export type TeamChanges = { workflow?: string; roles: Record<string, TeamSettings> };

/** The parts of the team reviewed one by one: the squads, the workflow, then each role's settings. */
export function teamKeys(proposal: TeamEdit): string[] {
  return [
    ...(proposal.team ? ["team"] : []),
    ...(proposal.workflow ? ["workflow"] : []),
    ...Object.keys(proposal.roles).map((r) => `roles.${r}`),
  ];
}

/** What to write: only the accepted parts, as the owner changed them. */
export function teamEdit(
  proposal: TeamEdit,
  decisions: Record<string, TeamDecision>,
  changes: TeamChanges,
): TeamEdit {
  const ok = (key: string) => decisions[key] === "accepted";
  const roles: Record<string, TeamSettings> = {};
  for (const [role, settings] of Object.entries(proposal.roles)) {
    if (ok(`roles.${role}`)) roles[role] = changes.roles[role] ?? settings;
  }
  return {
    team: ok("team") ? proposal.team : null,
    workflow: ok("workflow") ? (changes.workflow ?? proposal.workflow) : null,
    roles,
  };
}

/** The parts to review: those not already in place in the project's file, and those decided anyway. */
export function toReview(
  keys: string[],
  status: Record<string, TeamStatus>,
  decisions: Record<string, TeamDecision>,
): string[] {
  return keys.filter((key) => status[key] !== "same" || decisions[key]);
}

/** Whether writing `edit` touches a part the file holds as written by hand, even one already in place. */
export function replacesHand(edit: TeamEdit, hand: string[]): boolean {
  return teamKeys(edit).some((key) => hand.includes(key));
}

/** Globs typed one per line or separated by commas. */
export function globList(text: string): string[] {
  return [...new Set(text.split(/[\n,]/).map((g) => g.trim()).filter(Boolean))];
}

/** A squad's roles in a few words: `front ×2`, `qa`. */
export function members(roles: Record<string, number>): string[] {
  return Object.entries(roles).map(([role, n]) => (n > 1 ? `${role} ×${n}` : role));
}

/** A recipe in a few words per repository: what it installs and copies, and what each worktree gets of its own. */
export function recipeLines(repos: Record<string, RepoRecipe>): { repo: string; parts: string[] }[] {
  return Object.entries(repos).map(([repo, r]) => ({
    repo,
    parts: [
      ...r.deps.map((d) => d.install[d.install.length - 1] ?? d.dir),
      ...r.include,
      ...(r.database ? [r.database] : []),
      ...r.ports.map((p) => `TERMF_PORT_${p.toUpperCase()}`),
      ...r.commands,
    ],
  }));
}
