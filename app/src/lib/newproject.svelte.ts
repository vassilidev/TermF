import { invoke } from "@tauri-apps/api/core";
import { withAnalysis, type Card } from "$lib/setup-logic";
import type { Detected } from "$lib/RepoDetection.svelte";
import type { Analysis, TeamChoices } from "$lib/onboarding-logic";
import { app, onScreen, panes, ui, type ViewId } from "$lib/state.svelte";
// Each module reads the other only inside functions, so the import cycles are safe.
import { follow, setup, usable } from "$lib/onboarding.svelte";
import { closePanel } from "$lib/panes.svelte";

export type Mode = "repos" | "scratch";
/** What the last step sets up, each kept unless switched off. */
export type Row = "team" | "tickets" | "worktree" | "checks" | "ci" | "mcp" | "databases" | "visual";
export type Notify = "mine" | "all" | "off";

/** Everything decided in the setup, kept between visits and launches until the project is created. */
export type Draft = {
  mode: Mode;
  step: number;
  /** The furthest step reached: the steps up to it can be opened again. */
  reached: number;
  chosen: string[];
  /** The name typed by the owner; null follows the proposed one. */
  name: string | null;
  idea: string;
  /** The folder of a project from scratch; null follows the one beside the known repositories. */
  parent: string | null;
  github: "private" | "public" | null;
  analysis: Analysis | null;
  /** The repositories the analysis is of, one per line. */
  analyzed: string;
  /** Why Claude Code could not read them. */
  failed: string;
  answers: Record<string, string[]>;
  /** The question shown, among Claude Code's. */
  question: number;
  choices: TeamChoices;
  off: Row[];
  /** Check commands typed by the owner, by repository path. */
  checks: Record<string, string>;
  notify: Notify | null;
  /** Skills the stack switches on that the owner switched off. */
  skillsOff: string[];
  /** MCP servers of the stack switched off, by name; on, those that run the project's code. */
  mcpOff: string[];
  mcpOn: string[];
  /** Declared databases left out, by where they are declared. */
  dbOff: string[];
  /** Dev servers typed by the owner, by repository path, and the pages the visual review captures. */
  dev: Record<string, string>;
  routes: string | null;
  /** Replace the project's convention though it was edited in the Library. */
  replace: boolean;
  /** The selection trusted in Claude Code, one path per line: another selection is trusted only once seen. */
  trusted: string;
};

const fresh = (): Draft => ({
  mode: "repos",
  step: 0,
  reached: 0,
  chosen: [],
  name: null,
  idea: "",
  parent: null,
  github: null,
  analysis: null,
  analyzed: "",
  failed: "",
  answers: {},
  question: 0,
  choices: { keep: {} },
  off: [],
  checks: {},
  notify: null,
  skillsOff: [],
  mcpOff: [],
  mcpOn: [],
  dbOff: [],
  dev: {},
  routes: null,
  replace: false,
  trusted: "",
});

/** The setup: open or not, `project` when it is one that exists, `round` to start its page over. */
export const creating = $state({
  open: false,
  round: 0,
  project: null as string | null,
  /** Applies the setup as its Ready step shows it, while that step is ready (an agent's action takes it). */
  apply: null as null | (() => Promise<void>),
});
export const draft = $state<Draft>(fresh());

/** Each repository's card by path, once its git state and stack are read. */
export const cards = $state<Record<string, Card>>({});

/** What TermF finds by itself in each chosen repository, by path: read once, the error when it fails. */
export const found = $state<Record<string, Detected | string>>({});
const detecting = new Set<string>();

export function detect(paths: string[]) {
  for (const path of paths) {
    if (detecting.has(path)) continue;
    detecting.add(path);
    invoke<Detected>("setup_detect", { path }).then(
      (d) => {
        found[path] = d;
      },
      (e) => ((found[path] = String(e)), detecting.delete(path)),
    );
  }
}

let back: ViewId = "terminals";
/** Paths whose card was asked for since a list of repositories was shown: each is read once per showing. */
const asked = new Set<string>();

const storeKey = (project: string | null) => `termf.setup.${project ?? ""}`;

function stored(project: string | null): Draft {
  try {
    const text = localStorage.getItem(storeKey(project));
    if (text) return { ...fresh(), ...JSON.parse(text) };
  } catch {
    // Storage off or a draft unreadable: the setup starts over.
  }
  return fresh();
}

/** Keeps the draft for the next visit, even after the app is closed. */
export function keep(project = creating.project, value: Draft = draft) {
  try {
    localStorage.setItem(storeKey(project), JSON.stringify(value));
  } catch {
    // Storage off: the draft holds until the app closes.
  }
}

/** The draft is gone: the next setup starts over. */
export function forget() {
  try {
    localStorage.removeItem(storeKey(creating.project));
  } catch {
    // Nothing kept to remove.
  }
  Object.assign(draft, fresh());
}

/** True once the owner has decided anything: worth offering to start over. */
export const started = () => draft.step > 0 || draft.chosen.length > 0 || !!draft.idea.trim();

function show(project: string | null) {
  if (ui.view !== "newProject") back = ui.view;
  ui.switcher = false;
  if (!creating.open || creating.project !== project) {
    if (creating.open) keep();
    creating.project = project;
    Object.assign(draft, stored(project));
    creating.round++;
  }
  setup.project = project;
  creating.open = true;
  ui.view = "newProject";
}

/** A new project, where its last visit left it. */
export function newProject() {
  show(null);
}

/** The setup of project `id` again: its repositories are given, the setup opens on what TermF understood. */
export function openProjectSetup(id: string) {
  const project = app.workspace?.projects.find((p) => p.id === id);
  if (!project) return;
  show(id);
  const paths = project.repos.map((r) => r.path);
  if (paths.join("\n") !== draft.chosen.join("\n")) Object.assign(draft, { chosen: paths, step: 1, reached: 1 });
}

/** The setup waits: its draft is kept, to go on later. */
export function closeNewProject() {
  keep();
  creating.open = false;
  setup.project = null;
  if (panes.other !== null && onScreen("newProject")) closePanel("newProject");
  else if (ui.view === "newProject") ui.view = usable(back);
}

/** The setup is over: nothing of it is kept. */
export function endNewProject() {
  forget();
  creating.open = false;
  creating.round++;
  setup.project = null;
}

/** Reads the cards of `paths` not asked for yet; a card read before is shown until the new one comes. */
export async function readCards(paths: string[]) {
  const todo = paths.filter((p) => !asked.has(p));
  if (!todo.length) return;
  for (const p of todo) asked.add(p);
  try {
    const list = await invoke<Card[]>("setup_repos", { paths: todo });
    for (const card of list) cards[card.path] = card;
  } catch (e) {
    for (const p of todo) asked.delete(p);
    throw e;
  }
}

/** A list of repositories is shown: its cards are read again, the stack's survey being kept by the backend. */
export function forgetCards() {
  asked.clear();
}

type Found = { analysis: Analysis; error: string | null };
const analyses = new Map<string, Promise<Found>>();

/** The analysis of `repos` running, if any. */
export const runningFor = (project: string | null, repos: string[]) => analyses.has([project ?? "", ...repos].join("\n"));

/**
 * What TermF and Claude Code understand of `repos`, nothing written: the one running for them, else a new one. Its
 * result lands in the draft it is for, even once the setup is left.
 */
export function analyze(project: string | null, repos: string[], lang: string): Promise<Found> {
  const key = [project ?? "", ...repos].join("\n");
  let run = analyses.get(key);
  if (run) return run;
  run = follow(project ?? "", () => invoke<Found>("setup_analyze", { repos, lang, project })).finally(() => analyses.delete(key));
  analyses.set(key, run);
  // Kept as soon as it lands, open or not: reopening the setup never pays for it again.
  const land = (result: Parameters<typeof withAnalysis>[2]) => {
    const live = creating.open && creating.project === project ? withAnalysis($state.snapshot(draft) as Draft, repos, result) : null;
    if (live) Object.assign(draft, live);
    const next = live ?? withAnalysis(stored(project), repos, result);
    if (next) keep(project, next);
  };
  run.then(
    (found) => land(found),
    (e) => land({ failure: String(e) }),
  );
  return run;
}

/** What goes on in the background once a project is set up, by project: the Home follows it. */
export type Job = { id: "analysis" | "conventions" | "memory"; state: "running" | "done" | "failed"; detail?: string };
export const jobs = $state<Record<string, Job[]>>({});

function mark(project: string, job: Job) {
  const list = jobs[project] ?? [];
  jobs[project] = list.some((j) => j.id === job.id) ? list.map((j) => (j.id === job.id ? job : j)) : [...list, job];
}

/** Runs `work` for `project`, its state shown on the Home as it goes; a failure is said there, never thrown. */
export async function track<T>(project: string, id: Job["id"], work: () => Promise<T>): Promise<T | undefined> {
  mark(project, { id, state: "running" });
  try {
    const done = await work();
    mark(project, { id, state: "done" });
    return done;
  } catch (e) {
    mark(project, { id, state: "failed", detail: String(e) });
  }
}
