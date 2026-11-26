import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { lang, t } from "$lib/i18n";

/** Saves a recovery key in a text file that says what it opens, readable by the user only. null: cancelled. */
export async function saveRecoveryKey(key: string, of: "sync" | "vault"): Promise<string | null> {
  const path = await save({ defaultPath: `termf-${of}-recovery-key.txt` });
  if (!path) return null;
  const date = new Date().toLocaleString(lang, { dateStyle: "long", timeStyle: "short" });
  const text = t(of === "sync" ? "syncVisibleKeyFileSync" : "syncVisibleKeyFileVault", { key, date });
  await invoke("recovery_key_save", { path, text });
  return path;
}
