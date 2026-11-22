// No imports, so `pnpm test` runs it in plain Node.

export type RemoteMode = "off" | "ask" | "on";
/** A project's default, and the roles that follow another. */
export type RemotePolicy = { default: RemoteMode; roles: Record<string, RemoteMode> };
/** A tile's Remote Control, as TermF tracks it: `on` while wanted or connected, `url` once connected. */
export type RemoteState = {
  name: string;
  on: boolean;
  url: string | null;
  pending: boolean;
  ask: boolean;
  locked: boolean;
};

export type RemotePhase = "locked" | "live" | "connecting" | "waiting" | "stopping" | "off";

/** What the switch shows. */
export function phase(r: RemoteState): RemotePhase {
  if (r.locked) return "locked";
  if (r.url) return r.pending ? "stopping" : "live";
  if (r.on) return r.pending ? "waiting" : "connecting";
  return r.pending ? "stopping" : "off";
}

type Tile = { id: number; title: string; running: boolean; remoteControl: boolean; remote?: RemoteState | null };

/** The tiles open to remote control, or on their way: what the list and "cut all" are about. */
export function remoteTiles<T extends Tile>(sessions: T[]): T[] {
  return sessions.filter((s) => s.running && (s.remoteControl || (!!s.remote && phase(s.remote) === "stopping")));
}

export type Entry = { key: string; label: string };
export type TrayWords = {
  tooltip: (count: number) => string;
  waits: (count: number) => string;
  nothing: string;
  remote: (count: number) => string;
  cutAll: string;
  quick: string;
  open: string;
  awake: string;
  quit: string;
};

/** What the tray icon and the dock's badge show: the count of what waits, which the badge drops at zero. */
export function trayShown(waits: Entry[], tiles: Tile[], words: TrayWords) {
  const remote = remoteTiles(tiles).map((s) => ({ key: String(s.id), label: s.remote?.name || s.title }));
  return {
    count: waits.length,
    tooltip: waits.length ? words.tooltip(waits.length) : "TermF",
    waitsTitle: words.waits(waits.length),
    waits,
    nothing: words.nothing,
    remoteTitle: words.remote(remote.length),
    remote,
    cutAll: words.cutAll,
    quick: words.quick,
    open: words.open,
    awake: words.awake,
    quit: words.quit,
  };
}
