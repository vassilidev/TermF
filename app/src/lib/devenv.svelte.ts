import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { fresh, missing, type Entry, type Found, type Key } from "$lib/devenv-logic";
import { app, ui, type Session } from "$lib/state.svelte";

export type Manifest = { entries: Entry[]; scripts: string[]; file: string };
/** One source of a detection, once it ended. */
export type Scanned = { source: string; found: Found[]; error: string | null };

export const devenv = $state({
  loaded: false,
  manifest: null as Manifest | null,
  /** Each source of the detection running or last run: null while it runs. */
  sources: {} as Record<string, Scanned | null>,
  found: [] as Found[],
  scanning: false,
  /** A detection ran to its end in this session: what it did not find is missing. */
  scanned: false,
  tab: "manifest" as "manifest" | "found" | "script",
  error: "",
});

export function showEnv(tab?: typeof devenv.tab) {
  ui.view = "env";
  devenv.loaded = true;
  if (tab) devenv.tab = tab;
  void loadEnv();
}

/** The manifest; the first time it lists something, a detection follows, to tell what is missing here. */
export async function loadEnv(): Promise<Manifest | null> {
  try {
    devenv.manifest = await invoke<Manifest>("env_manifest");
  } catch (e) {
    devenv.error = String(e);
    return null;
  }
  // Its error shows on the screen (`devenv.error`).
  if (devenv.manifest.entries.length && !devenv.scanned && !devenv.scanning) scan().catch(() => {});
  return devenv.manifest;
}

/** Reads what this machine has, each source shown as it ends. */
export async function scan(): Promise<Found[]> {
  if (devenv.scanning) throw new Error("A detection is already running.");
  devenv.scanning = true;
  devenv.error = "";
  devenv.sources = {};
  devenv.found = [];
  const offs = await Promise.all([
    listen<string[]>("env-scan-started", ({ payload }) => {
      devenv.sources = Object.fromEntries(payload.map((s) => [s, null]));
    }),
    listen<Scanned>("env-scanned", ({ payload }) => {
      devenv.sources[payload.source] = payload;
      devenv.found = [...devenv.found, ...payload.found];
    }),
  ]);
  try {
    devenv.found = await invoke<Found[]>("env_scan");
    devenv.scanned = true;
    return devenv.found;
  } catch (e) {
    devenv.error = String(e);
    throw e;
  } finally {
    offs.forEach((off) => off());
    devenv.scanning = false;
  }
}

export const newFound = () => fresh(devenv.found, devenv.manifest?.entries ?? []);
export const missingHere = () => missing(devenv.manifest?.entries ?? [], devenv.found, devenv.scanned);

export async function addEntries(entries: Entry[]) {
  devenv.manifest = await invoke<Manifest>("env_add", { entries });
}

export async function updateEntry(previous: Key, entry: Entry) {
  devenv.manifest = await invoke<Manifest>("env_update", { previous: { manager: previous.manager, name: previous.name }, entry });
}

export async function removeEntries(keys: Key[]) {
  devenv.manifest = await invoke<Manifest>("env_remove", { keys: keys.map((k) => ({ manager: k.manager, name: k.name })) });
}

export async function saveScript(name: string, text: string) {
  devenv.manifest = await invoke<Manifest>("env_script_save", { name, text });
}

export async function deleteScript(name: string) {
  devenv.manifest = await invoke<Manifest>("env_script_delete", { name });
}

/** The script written from the manifest (`name` null), or a named one, in a terminal tile shown at once. */
export async function install(name: string | null): Promise<Session> {
  const session = await invoke<Session>("env_install", { name });
  if (!app.sessions.some((s) => s.id === session.id)) app.sessions.push(session);
  // Not imported up top: commands imports the views, which import this module.
  await (await import("$lib/commands")).focusSession(session.id);
  return session;
}
