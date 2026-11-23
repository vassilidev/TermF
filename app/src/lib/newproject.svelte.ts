import { invoke } from "@tauri-apps/api/core";
import type { Card } from "$lib/setup-logic";
import type { Detected } from "$lib/RepoDetection.svelte";
import type { Analysis, TeamChoices } from "$lib/onboarding-logic";
import { app, onScreen, panes, ui, type ViewId } from "$lib/state.svelte";
// Each module reads the other only inside functions, so the import cycles are safe.
import { follow, setup, usable } from "$lib/onboarding.svelte";
import { closePanel } from "$lib/panes.svelte";

export type Mode = "repos" | "scratch";
/** What the last step sets up, each kept unless switched off. */
export type Row = "team" | "tickets" | "worktree" | "checks" | "ci";
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
  trusted: "",
});

/** The setup: open or not, `project` when it is one that exists, `round` to start its page over. */
export const creating = $state({ open: false, round: 0, project: null as string | null });
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
      (d) => (found[path] = d),
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
  const land = (patch: Partial<Draft>) => {
    const analyzed = repos.join("\n");
    if (creating.project === project && draft.chosen.join("\n") === analyzed) return Object.assign(draft, patch);
    const kept = stored(project);
    if (kept.chosen.join("\n") === analyzed) keep(project, { ...kept, ...patch });
  };
  run.then(
    ({ analysis, error }) =>
      land({
        analysis,
        analyzed: repos.join("\n"),
        failed: error ?? "",
        answers: Object.fromEntries(analysis.questions.map((q) => [q.id, q.default])),
        question: 0,
      }),
    (e) => land({ failed: String(e), analyzed: repos.join("\n") }),
  );
  return run;
}
