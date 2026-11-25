import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { Approved, forAgent, refused, type Args, type Spec } from "$lib/actions-logic";
import { ask } from "$lib/ask.svelte";
import { ciActions } from "$lib/ci-actions";
import { designActions } from "$lib/design-actions";
import { gitActions } from "$lib/git-actions";
import { t } from "$lib/i18n";
import { libraryActions } from "$lib/library-actions";
import { opsActions } from "$lib/ops-actions";
import { schedulerActions } from "$lib/scheduler-actions";
import { settingsActions } from "$lib/settings-actions";
import { setupActions } from "$lib/setup-actions";
import { ticketActions } from "$lib/tickets-actions";
import { toast } from "$lib/ui/toast.svelte";

type Key = Parameters<typeof t>[0];

/**
 * An action of a screen that an agent may ask for (`do_action` of `termf mcp`): what the agent reads (`Spec`), what
 * TermF's card says, and its run with the app's own commands.
 */
export type Action = Spec & {
  /** The card's title. */
  label: Key;
  /** The card's label of each secret field. */
  labels?: Record<string, Key>;
  /** Exactly what it does, as the card says it. */
  describe: (args: Args, project: string) => string;
  /** What it makes lose, for one that destroys or replaces. */
  loses?: (args: Args, project: string) => string;
  /** Runs it, updates the screen and says so in a toast; what the agent reads back, never a secret. */
  run: (args: Args, secrets: Record<string, string>, project: string) => Promise<unknown>;
};

/** Each screen's actions, by view, and those of anywhere. */
const SCREENS: Record<string, Action[]> = {
  ops: opsActions,
  tickets: ticketActions,
  ci: ciActions,
  library: libraryActions,
  git: gitActions,
  settings: settingsActions,
  scheduler: schedulerActions,
  newProject: setupActions,
  design: designActions,
};
const GLOBAL: Action[] = [];

/** A call of `termf mcp`, as the app gives it; `action` null: the list. */
export type Call = {
  id: number;
  session: number;
  project: string;
  role: string | null;
  title: string;
  action: string | null;
  args: Args;
  hints: Record<string, string>;
};

/** How long a card waits for the user. */
const WAIT = 300_000;

export const agentActions = $state({
  /** The calls whose card waits for the user, every conversation. */
  waiting: [] as Call[],
});

/** The screen a conversation's user writes from: the last one a question went from, else the one it started from. */
export function screenOf(session: number): string | null {
  return ask.screens[session] ?? ask.origins[session] ?? null;
}

/** What a conversation may do: its screen's actions, then those of anywhere. */
export function available(session: number): Action[] {
  const screen = screenOf(session);
  return [...((screen && SCREENS[screen]) || []), ...GLOBAL];
}

export const actionOf = (session: number, id: unknown) => available(session).find((a) => a.id === id);

const approved = new Approved<Record<string, string>>();

/** The permission card allowed a `do_action`, with what the user typed in it: its call runs at once, no second card. */
export function approve(session: number, input: unknown, secrets: Record<string, string>) {
  const i = (input ?? {}) as { action?: unknown; args?: unknown };
  if (typeof i.action === "string") approved.keep(session, i.action, i.args, { ...secrets });
}

async function reply(id: number, result: unknown, error: string | null) {
  await invoke("actions_reply", { id, result: error === null ? (result ?? null) : null, error }).catch(() => {});
}

function drop(id: number) {
  agentActions.waiting = agentActions.waiting.filter((c) => c.id !== id);
}

/** Runs an action the user approved: the screen updated and a toast; its result, or its error, to the agent. */
export async function perform(call: Call, secrets: Record<string, string>) {
  drop(call.id);
  const action = actionOf(call.session, call.action);
  if (!action) return reply(call.id, null, `No action ${call.action} here any more: list them again.`);
  try {
    const result = await action.run(call.args, secrets, call.project);
    await reply(call.id, result ?? { done: true }, null);
  } catch (e) {
    toast(String(e), { kind: "error" });
    await reply(call.id, null, String(e));
  }
}

export function refuse(call: Call, why: string) {
  drop(call.id);
  const said = why.trim() ? ` They said: ${why.trim()}` : "";
  return reply(call.id, null, `The user refused this in TermF: it did not run.${said} Do not ask again unless they tell you to.`);
}

/** The card answered with the user's question: the call did not run, and nothing is decided yet. */
export function askFirst(call: Call, message: string) {
  drop(call.id);
  return reply(call.id, null, message);
}

async function answer(call: Call) {
  const here = available(call.session);
  if (call.action === null) {
    return reply(call.id, { screen: screenOf(call.session), actions: here.map(forAgent) }, null);
  }
  const action = here.find((a) => a.id === call.action);
  if (!action) return reply(call.id, null, `No action ${call.action} here: actions lists those you may do from the user's screen.`);
  const why = refused(action, call.args, call.hints);
  if (why) return reply(call.id, null, why);
  const secrets = approved.take(call.session, action.id, call.args);
  if (secrets) return perform(call, secrets);
  // Not allowed in a permission card (another agent CLI, an older card): TermF's own card asks.
  agentActions.waiting = [...agentActions.waiting, call];
  if (ask.origins[call.session]) ask.open = true;
  setTimeout(() => {
    if (!agentActions.waiting.some((c) => c.id === call.id)) return;
    drop(call.id);
    void reply(call.id, null, "Nobody approved this in TermF within 5 minutes, so it did not run.");
  }, WAIT);
  if (document.hasFocus()) return;
  const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
  if (granted) sendNotification({ title: t("actNotify"), body: t("actAsks", { who: call.role ?? call.title, what: t(action.label) }) });
}

/** Listened to from the start: an agent may act from any conversation. */
export function watchActions(): () => void {
  const stops = Promise.all([
    listen<Call>("agent-action", ({ payload }) => void answer(payload)),
    listen<Call>("agent-action-done", ({ payload }) => drop(payload.id)),
  ]);
  return () => void stops.then((list) => list.forEach((stop) => stop()));
}
