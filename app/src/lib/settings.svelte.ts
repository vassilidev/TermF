import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { t } from "$lib/i18n";
import type { Preview } from "$lib/library.svelte";
import { load, ui } from "$lib/state.svelte";
import { failed, mergeText, refreshVault, say } from "$lib/vault.svelte";
import type { Merged } from "$lib/vault-logic";

export type Section = "awake" | "context" | "keys" | "ci" | "claude" | "config" | "sync";
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
  /** The vault in the next config export: sealed by its own password, after the master password. */
  vaultExport: { on: false, project: null as string | null, password: "", again: "", master: "" },
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
  const part = settings.vaultExport;
  if (part.on && part.password !== part.again) {
    settings.error = t("vaultPasswordsDiffer");
    return;
  }
  const path = await save({ defaultPath: "termf-config.tar.gz", filters: archiveFilter });
  if (!path) return;
  await busy(async () => {
    const vault = part.on ? { password: part.password, project: part.project, master: part.master } : null;
    try {
      const result = await invoke<{ files: number; skippedLinks: string[] }>("config_export", { path, vault });
      settings.transfer = { kind: "exported", ...result };
    } catch (e) {
      throw /the vault is locked/.test(String(e)) ? t("vault3ConfigLocked") : failed(e);
    } finally {
      part.password = part.again = part.master = "";
    }
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

/** `vault`: the export password of the vault the archive holds, once its items were shown. */
export async function applyImport(vault: string | null = null) {
  const transfer = settings.transfer;
  if (transfer?.kind !== "import") return;
  await busy(async () => {
    transfer.preview = await invoke<Preview>("config_import_apply", { path: transfer.path, replace: transfer.replace });
    if (vault) {
      const rows = await invoke<Merged[]>("vault_import_apply", { path: transfer.path, password: vault });
      say(t("vault3Imported", { summary: mergeText(rows) }));
      await refreshVault();
    }
    transfer.done = true;
    await load();
  });
}
