import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { agents, ctoOf, expertOf, upsert, type AgentSession } from "$lib/agents.svelte";
import { chipsOf, compose, Owners, removedOf, reroute, roleFor, target, threadKey, type Chip } from "$lib/ask-logic";
import { ensure } from "$lib/home.svelte";
import { t } from "$lib/i18n";
import type { Item as LibraryItem } from "$lib/library.svelte";
import { app, ui, type ViewId } from "$lib/state.svelte";
import { views } from "$lib/views";

export type { Chip } from "$lib/ask-logic";

/** What a screen gives the panel: the role that answers there, the object open and the selection, its repository, its run. */
export type Given = { role?: string; chips: Chip[]; repo?: string; run?: number };

/** A conversation the panel started; `step`: the session of a run's step, whose follow-ups are routed again. */
type Thread = { id: number; step: boolean };

const THREADS = "termf.ask.threads";
const ORIGINS = "termf.ask.origins";

function stored<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "") ?? fallback;
  } catch {
    return fallback;
  }
}

function keep(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage off: kept until the app closes.
  }
}

/**
 * What the user picked on a screen: the role chosen by hand or by `@role` (null: the screen's), the chips added with
 * `@` or from a selection, the ids of those taken off. Another screen or project starts afresh.
 */
type Here = { view: string; project: string | null; role: string | null; added: Chip[]; removed: string[] };
const NONE: Here = { view: "", project: null, role: null, added: [], removed: [] };

export const ask = $state({
  open: false,
  picked: NONE as Here,
  /** What each screen gives, by view. */
  given: {} as Record<string, Given>,
  /** A question to put in the field, for the user to read and send. */
  draft: "",
  /** Bumped to put the cursor in the panel's field. */
  focus: 0,
  busy: false,
  error: "",
  /** The project's roles that can answer, read once per project. */
  roles: [] as string[],
  rolesOf: null as string | null,
  /** A role's conversation, by `threadKey` (project, role, run or repository), kept across launches. */
  threads: stored<Record<string, Thread>>(THREADS, {}),
  /** The screen each conversation started from (session → view), for Conversations. */
  origins: stored<Record<number, string>>(ORIGINS, {}),
});

const screens: Partial<Record<ViewId, Parameters<typeof t>[0]>> = {
  run: "aigScreenRun",
  review: "aigScreenReview",
  onboarding: "obConfigure",
  newProject: "newProject",
};

/** A screen by its name, as the chips and Conversations say it. */
export function screenName(view: string): string {
  const key = views.find((v) => v.id === view)?.label ?? screens[view as ViewId];
  return key ? t(key) : view;
}

/** What the user picked on the screen shown. */
export function here(): Here {
  return ask.picked.view === ui.view && ask.picked.project === app.project ? ask.picked : NONE;
}

/** The picks of the screen shown, to change them. */
export function pick(): Here {
  if (here() === NONE) ask.picked = { view: ui.view, project: app.project, role: null, added: [], removed: [] };
  return ask.picked;
}

/** The project asked: the current one, or under every project the one of the repository the screen shows. */
export function askedProject(): string | null {
  const repo = ask.given[ui.view]?.repo;
  return app.project ?? app.workspace?.projects.find((p) => p.repos.some((r) => r.path === repo))?.id ?? null;
}

export const screenRole = () => roleFor(ui.view, ask.given[ui.view]?.role);
export const currentRole = () => here().role ?? screenRole();

/** The screen's chips: the view itself, then what the screen gives. */
function screenChips(): Chip[] {
  const project = app.workspace?.projects.find((p) => p.id === askedProject());
  const view: Chip = { id: "view", kind: "view", label: [screenName(ui.view), project?.name].filter(Boolean).join(" · ") };
  return [view, ...(ask.given[ui.view]?.chips ?? [])];
}

/** The chips sent with a question: the screen, then what it gives, then what the user added. */
export const chips = (): Chip[] => chipsOf(screenChips(), here().added, here().removed);

/** The chips taken off on the screen shown, to put back. */
export const removedChips = (): Chip[] => removedOf(screenChips(), here().added, here().removed);

// `ask.given` holds proxies: a cleanup knows its own giving by a token, not by comparing what it gave.
const owners = new Owners();

/** A screen gives its context while it shows; the cleanup takes it back. Call it from an `$effect`. */
export function provide(view: ViewId, given: Given): () => void {
  const token = owners.take(view);
  ask.given[view] = given;
  return () => {
    if (owners.release(view, token)) delete ask.given[view];
  };
}

/** Opens the panel, the cursor in its field; `role`, `chips` and `draft` prefill it (the "Why?" buttons of a screen). */
export async function askAI(options: { role?: string; chips?: Chip[]; draft?: string } = {}) {
  ask.open = true;
  ask.error = "";
  if (options.role) pick().role = options.role === screenRole() ? null : options.role;
  if (options.chips) {
    const ids = options.chips.map((c) => c.id);
    const p = pick();
    p.added = [...p.added.filter((c) => !ids.includes(c.id)), ...options.chips];
    p.removed = p.removed.filter((id) => !ids.includes(id));
  }
  if (options.draft !== undefined) ask.draft = options.draft;
  await tick();
  ask.focus++;
}

/** ⌘I: opens the panel, or closes it from inside. */
export function toggleAsk() {
  const inside = !!document.activeElement?.closest("[data-ask]");
  if (ask.open && inside) ask.open = false;
  else void askAI({ chips: selection() });
}

/** The text selected on the screen shown, as a chip: what ⌘I asks about. */
function selection(): Chip[] | undefined {
  const s = window.getSelection();
  const text = s?.toString().trim();
  const node = s?.anchorNode;
  const el = node instanceof Element ? node : node?.parentElement;
  if (!text || !el?.closest(`[data-panel="${ui.view}"]`)) return undefined;
  const first = text.split("\n")[0];
  return [{ id: "selection", kind: "selection", label: t("aigSelection", { text: first.length > 40 ? `${first.slice(0, 40)}…` : first }), text }];
}

/** The project's roles, read once per project: `@role` writes to one of them. */
export function loadRoles(project: string | null) {
  if (ask.rolesOf === project) return;
  ask.rolesOf = project;
  invoke<LibraryItem[]>("library_list", { kind: "role", project }).then(
    (items) => void (ask.roles = items.filter((i) => i.active || i.layer === "shipped").map((i) => i.name)),
    () => void (ask.roles = []),
  );
}

/** Where the screen shown keeps a role's conversation. */
const keyOf = (project: string, role: string) => threadKey(project, role, ask.given[ui.view] ?? {});

function thread(project: string | null, role: string): Thread | undefined {
  const kept = project === null ? undefined : ask.threads[keyOf(project, role)];
  return typeof kept?.id === "number" ? kept : undefined;
}

/**
 * The conversation a role keeps: the project's CTO's and expert's, or the one the panel started with this role about
 * the screen's run or repository.
 */
export function threadOf(project: string | null, role: string): AgentSession | undefined {
  if (project === null) return undefined;
  if (role === "cto") return ctoOf(project);
  if (role === "expert") return expertOf(project);
  const id = thread(project, role)?.id;
  return agents.sessions.find((a) => a.id === id);
}

function started(session: AgentSession, view: string) {
  if (ask.origins[session.id]) return;
  ask.origins[session.id] = view;
  keep(ORIGINS, ask.origins);
}

/**
 * Sends a question from the panel: the screen's context first, to the role chosen or written as `@role`. The CTO and
 * the expert get it in their conversation; any other role in its own, started the first time (the CTO hears of it).
 * From the Run page, a role at work in the run gets it at its step. False when it did not go.
 */
export async function send(text: string): Promise<boolean> {
  const project = askedProject();
  if (project === null || !text.trim()) return false;
  // What the screen was when the question went: the user may be elsewhere once it is answered.
  const view = ui.view;
  const mine = pick();
  const screen = screenRole();
  const to = target(text, currentRole(), ask.roles);
  const body = compose(to.text, chips(), screenName(view));
  const key = keyOf(project, to.role);
  ask.busy = true;
  ask.error = "";
  try {
    if (to.role === "cto" || to.role === "expert") {
      const session = await ensure(project, to.role);
      if (!session) return false;
      await invoke("agent_send", { id: session.id, text: body });
      started(session, view);
    } else {
      const reached = await invoke<{ session: AgentSession; via: string }>("cto_address", {
        project,
        role: to.role,
        text: body,
        repo: ask.given[view]?.repo ?? null,
        direct: view !== "run",
      });
      upsert(reached.session);
      ask.threads[key] = { id: reached.session.id, step: reached.via === "step" };
      keep(THREADS, ask.threads);
      started(reached.session, view);
    }
    mine.role = to.role === screen ? null : to.role;
    return true;
  } catch (e) {
    ask.error = String(e);
    return false;
  } finally {
    ask.busy = false;
  }
}

/**
 * What a conversation shown in the panel sends: the question with the screen's context; or null when it went through
 * `send` instead, written to another role with `@role`, or following up a run's step, which is routed again (the step
 * may be over: the role's own conversation then takes it).
 */
export async function outgoing(text: string): Promise<string | null> {
  const role = currentRole();
  const to = target(text, role, ask.roles);
  if (!reroute(to.role, role, !!thread(askedProject(), role)?.step)) return compose(to.text, chips(), screenName(ui.view));
  if (!(await send(text))) throw new Error(ask.error);
  return null;
}
