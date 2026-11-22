import { invoke } from "@tauri-apps/api/core";
import { focusSession } from "$lib/commands";
import { app, type Session } from "$lib/state.svelte";
import { claudeView, type Folder, type Overview } from "$lib/claude.svelte";

export type FileText = { path: string; text: string; masked: boolean; readOnly: boolean; hash: string | null };
export type Diff = { path: string; diff: string };
export type SavedSession = { id: string; title: string | null; prompt: string | null; modified: number; size: number };
export type SessionProject = { folder: string; cwd: string | null; sessions: SavedSession[] };
/** One entry of `claude agents --json --all`: its fields depend on the session (code.claude.com/docs/en/agent-view). */
export type BackgroundSession = {
  id?: string;
  name?: string;
  kind?: string;
  state?: string;
  status?: string;
  waitingFor?: string;
  cwd?: string;
  startedAt?: number;
  sessionId?: string;
};
export type Background = { sessions: BackgroundSession[]; error: string | null };
export type LimitWindow = { usedPercentage: number; resetsAt: number | null; usedUsd: number | null; limitUsd: number | null };
export type Limits = { fiveHour: LimitWindow | null; sevenDay: LimitWindow | null; spendLimit: LimitWindow | null };
/** A window as last seen: when, and from a tile (`session`) or a headless conversation (`conversation`). */
export type SeenWindow = LimitWindow & { at: number; session: number | null; conversation: number | null };
export type Usage = {
  limits: { fiveHour: SeenWindow | null; sevenDay: SeenWindow | null; spendLimit: SeenWindow | null };
  sessions: Record<string, { cost: number | null; model: string | null; at: number }>;
};
export type ExportPreview = { dest: string; exists: boolean; diff: string };

export const repoArg = () => claudeView.repo || null;

/** Skills and the like, and memory, read again: the overview's slow commands are not run for that. It never
 * throws: it runs after a write that succeeded, whose success must not read as a failure. */
export async function refreshFiles() {
  const o: Overview | null = claudeView.overview;
  if (!o) return;
  try {
    const files = await invoke<{ extensions: Folder[]; memory: Overview["memory"] }>("claude_files", { repo: repoArg() });
    // Another repository chosen meanwhile has its own overview.
    if (claudeView.overview !== o) return;
    o.extensions = files.extensions;
    o.memory = files.memory;
  } catch (e) {
    claudeView.error = String(e);
  }
}

/** A tile a command opened: shown in the terminals, focused. */
export async function openTile(session: Session) {
  if (!app.sessions.some((s) => s.id === session.id)) app.sessions.push(session);
  await focusSession(session.id);
}

export async function attempt<T>(run: () => Promise<T>): Promise<T | undefined> {
  try {
    return await run();
  } catch (e) {
    claudeView.error = String(e);
    return undefined;
  }
}

export const diffLines = (diff: string) => diff.replace(/\n$/, "").split("\n");
export const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;
