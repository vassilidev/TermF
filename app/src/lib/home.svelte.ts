import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { agentProjects, agents, ctoOf, expertOf, showConversation, upsert, waitingAgents, type AgentSession } from "$lib/agents.svelte";
import { pending, type StoredEvent } from "$lib/conversation-logic";
import { askKey, waits, type Wait } from "$lib/home-logic";
import { sessionProjects } from "$lib/project.svelte";
import { awaitsDelivery } from "$lib/run-logic";
import { app, onScreen, setProject, ui, waitingSessions } from "$lib/state.svelte";
import { ops } from "$lib/ops.svelte";
import { agentEnv } from "$lib/agentenv.svelte";
import { asks, type Ask, type TeamRun } from "$lib/team-run-logic";
import { openRun, team } from "$lib/team.svelte";
import { tickets } from "$lib/tickets.svelte";
// Each module reads the other only inside functions, so the import cycle is safe.
import { focusSession } from "$lib/commands";
import { askAI } from "$lib/ask.svelte";

/** A headless session's permission request, as the rail tells it. */
export type Asked = { tool: string; description: string | null; input: unknown };

export const home = $state({
  /** The wait a click or ⌘J went to last: the next ⌘J goes on from it. */
  shown: null as string | null,
  /** What each waiting headless session asks, by id. */
  asked: {} as Record<number, Asked | null>,
});

export function showHome() {
  ui.view = "home";
}

/** A step of a team run waiting for the user, as its card decides it; `ask` null: the delivery, once the team is done. */
export type Decision = { key: string; run: TeamRun; step: string; ask: Ask | null };

/** The decisions of a project's team runs, null for every project's. */
export function decisions(project: string | null): Decision[] {
  return team.runs
    .filter((r) => project === null || r.target.project === project)
    .filter(openRun)
    .flatMap((run) => {
      const list: Decision[] = asks(run).map((ask) => ({ key: askKey(run.run, ask.step.key), run, step: ask.step.key, ask }));
      const ticket = tickets.runs.find((r) => r.id === run.run);
      const connector = run.steps.find((s) => s.kind === "connector")?.key;
      if (ticket && connector && awaitsDelivery(ticket, run)) list.push({ key: askKey(run.run, connector), run, step: connector, ask: null });
      return list;
    });
}

/** What waits for the user in a project, null for every project. */
export function projectWaits(project: string | null): Wait[] {
  const mine = (ids: string[]) => project === null || ids.includes(project);
  return waits(
    waitingSessions().filter((s) => mine(sessionProjects(s))),
    waitingAgents()
      .filter((a) => mine(agentProjects(a)))
      .map((a) => ({ id: a.id, updated: a.updated, cto: !!a.mcp && a.role === "cto" })),
    decisions(project).map((d) => ({ run: d.run.run, step: d.step, kind: d.ask?.kind ?? "deliver", updated: d.run.updated })),
    ops.requests.filter((r) => mine([r.project])),
    agentEnv.requests.filter((r) => mine([r.project])),
  );
}

/** The project a wait belongs to, for a wait listed under every project. */
export function waitProject(w: Wait): string | null {
  if ("run" in w) return team.runs.find((r) => r.run === w.run)?.target.project ?? null;
  if (w.kind === "ops" || w.kind === "secret") return w.project;
  if (w.kind === "tile") {
    const session = app.sessions.find((s) => s.id === w.id);
    return session ? (sessionProjects(session)[0] ?? null) : null;
  }
  const agent = agents.sessions.find((a) => a.id === w.id);
  return agent ? (agentProjects(agent)[0] ?? null) : null;
}

const read = new Map<number, number>();
const reading = new Set<string>();

/** Reads once per change what a waiting session asks: the request it has not had an answer to; a failed read is tried again. */
export async function learn(session: AgentSession) {
  const at = `${session.id}:${session.updated}`;
  if (read.get(session.id) === session.updated || reading.has(at)) return;
  reading.add(at);
  try {
    const events = await invoke<StoredEvent[]>("agent_events", { id: session.id });
    const request = pending(events);
    const e = events.findLast((e) => e.kind === "permission" && e.request === request);
    home.asked[session.id] = e?.kind === "permission" ? { tool: e.tool, description: e.description, input: e.input } : null;
    read.set(session.id, session.updated);
  } catch {
    home.asked[session.id] = null;
  } finally {
    reading.delete(at);
  }
}

const opening = new Map<string, Promise<AgentSession | undefined>>();

/** The project's CTO or expert conversation, created without a process the first time, as from the palette. */
export function ensure(project: string, who: "cto" | "expert"): Promise<AgentSession | undefined> {
  const known = who === "cto" ? ctoOf(project) : expertOf(project);
  if (known) return Promise.resolve(known);
  const key = `${who}:${project}`;
  let asking = opening.get(key);
  if (!asking) {
    asking = invoke<AgentSession>(who === "cto" ? "cto_open" : "expert_open", { project })
      .then(
        (session) => (upsert(session), session),
        (e) => void (app.error = String(e)),
      )
      .finally(() => opening.delete(key));
    opening.set(key, asking);
  }
  return asking;
}

/** The element once the conversation has drawn it: its events load after it mounts. */
async function reach(selector: string): Promise<HTMLElement | null> {
  for (let i = 0; i < 20; i++) {
    await tick();
    const el = document.querySelector<HTMLElement>(selector);
    if (el) return el;
    await new Promise((r) => setTimeout(r, 50));
  }
  return null;
}

/** ⌘⇧H: the CTO in the ⌘I panel, the cursor in its field; `draft` is written there for the user to read and send. */
export async function talkCto(draft?: string) {
  if (app.project === null) return;
  await askAI({ role: "cto", draft });
}

/** Goes where a wait is decided: its card on the Home, the CTO's request in the panel, its tile, or its conversation. */
export async function goToWait(w: Wait) {
  home.shown = w.key;
  if (w.kind === "tile") return focusSession(w.id);
  if (w.kind === "agent") return showConversation(w.id);
  const project = waitProject(w);
  if (project !== null && project !== app.project) setProject(project);
  if (w.kind === "cto") await askAI({ role: "cto" });
  else {
    showHome();
    if (!onScreen("home")) return;
  }
  // The CTO's own request is its permission card, the last thing it said.
  const card = await reach(w.kind === "cto" ? '[data-ask] [role="log"] .ask[role="group"]' : `[data-panel="home"] [data-wait="${w.key}"]`);
  card?.scrollIntoView({ block: "center" });
  if (card) {
    if (!card.hasAttribute("tabindex")) card.tabIndex = -1;
    card.focus({ preventScroll: true });
  }
}
