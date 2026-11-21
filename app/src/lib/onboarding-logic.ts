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
export function pick(q: Question, value: string[], option: string): string[] {
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
