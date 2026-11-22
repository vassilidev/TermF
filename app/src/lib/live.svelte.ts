import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StoredEvent } from "$lib/conversation-logic";
import { follow, replay, type Activity, type ToolAct } from "$lib/live-logic";

/** By headless session: what its agent does now and its last calls, whatever view is shown. */
export const activity = $state<Record<number, Activity>>({});

/** Sessions whose stored events were read, or are being read: what is heard meanwhile waits in its list. */
const read = new Map<number, StoredEvent[] | null>();

function apply(session: number, e: StoredEvent) {
  const was = activity[session];
  const next = follow(was, e);
  if (next !== was) activity[session] = next;
}

/** Reads a session's past once, so an agent at work when the window opened shows what it does too. */
export function track(session: number) {
  if (read.has(session)) return;
  read.set(session, []);
  invoke<StoredEvent[]>("agent_events", { id: session }).then(
    (stored) => {
      const now = replay(stored, read.get(session) ?? []);
      read.set(session, null);
      if (now) activity[session] = now;
    },
    () => {
      // Unread, it may be tried again; what was heard meanwhile still counts.
      const heard = read.get(session) ?? [];
      read.delete(session);
      for (const e of heard) apply(session, e);
    },
  );
}

export const calls = (session: number): ToolAct[] => activity[session]?.recent ?? [];

/** Keeps every session's activity current from `agent-event`; a delta changes nothing once the agent writes. */
export function watchLive(): () => void {
  const stop = listen<StoredEvent>("agent-event", ({ payload }) => {
    const waiting = read.get(payload.session);
    if (waiting) waiting.push(payload);
    else apply(payload.session, payload);
  });
  return () => void stop.then((f) => f());
}
