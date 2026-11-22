import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Preview } from "$lib/library.svelte";
import { load, ui } from "$lib/state.svelte";

export type Section = "awake" | "context" | "keys" | "ci" | "claude" | "memory" | "config" | "sync";
export type Transfer =
  | { kind: "exported"; files: number; skippedLinks: string[] }
  | { kind: "import"; path: string; preview: Preview; replace: string[]; done: boolean };

export const settings = $state({
  /** Loaded once first shown, then kept mounted, like the other views. */
  loaded: false,
  section: "awake" as Section,
  /** What an export wrote, or an import's preview, shown in Configuration. */
  transfer: null as Transfer | null,
  busy: false,
  error: "",
});

export function showSettings(section?: Section) {
  ui.view = "settings";
  settings.loaded = true;
  if (section) settings.section = section;
}

async function busy(run: () => Promise<void>) {
  settings.busy = true;
  settings.error = "";
  try {
    await run();
  } catch (e) {
    settings.error = String(e);
  } finally {
    settings.busy = false;
  }
}

const archiveFilter = [{ name: "TermF", extensions: ["gz", "tgz"] }];

export async function exportConfig() {
  showSettings("config");
  const path = await save({ defaultPath: "termf-config.tar.gz", filters: archiveFilter });
  if (!path) return;
  await busy(async () => {
    const result = await invoke<{ files: number; skippedLinks: string[] }>("config_export", { path });
    settings.transfer = { kind: "exported", ...result };
  });
}

export async function importConfig() {
  showSettings("config");
  const path = await open({ multiple: false, directory: false, filters: archiveFilter });
  if (!path) return;
  await busy(async () => {
    const preview = await invoke<Preview>("config_import_preview", { path });
    settings.transfer = { kind: "import", path, preview, replace: [], done: false };
  });
}

export async function applyImport() {
  const transfer = settings.transfer;
  if (transfer?.kind !== "import") return;
  await busy(async () => {
    transfer.preview = await invoke<Preview>("config_import_apply", { path: transfer.path, replace: transfer.replace });
    transfer.done = true;
    await load();
  });
}
