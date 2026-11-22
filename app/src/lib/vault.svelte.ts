import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "$lib/i18n";
import { CLEAR_AFTER, digest } from "$lib/ssh.svelte";
import { app, ui } from "$lib/state.svelte";
import type { ItemInfo, SecretInfo } from "$lib/vault-logic";

export type VaultStatus = {
  initialized: boolean;
  unlocked: boolean;
  remembered: boolean;
  autoLockMinutes: number;
  prodUntil: number | null;
};
/** "vault": the current project's items and the global ones; "vaultAll": every one, under "This machine". */
export type VaultView = "vault" | "vaultAll";

/** Names and descriptions only: a value reaches the webview when the user reveals or copies it, and is dropped after. */
export const vault = $state({
  /** Each view, mounted once first shown. */
  loaded: false,
  loadedAll: false,
  status: null as VaultStatus | null,
  items: [] as ItemInfo[],
  secrets: [] as SecretInfo[],
  error: "",
  /** What a copy did, said for a few seconds. */
  notice: "",
  /** The entry each view shows: an item's id or `secretKey`; the palette sets it too. */
  selected: { vault: null, vaultAll: null } as Record<VaultView, string | null>,
});

let watching = false;

function watch() {
  if (watching) return;
  watching = true;
  // Locked by itself (idle, sleep) or by `termf vault lock`.
  listen<VaultStatus>("vault-status", (e) => {
    const opened = e.payload.unlocked && !vault.status?.unlocked;
    setStatus(e.payload);
    if (opened) refreshVault();
  });
}

export function setStatus(status: VaultStatus) {
  vault.status = status;
  if (!status.unlocked) {
    vault.items = [];
    vault.secrets = [];
  }
}

export async function refreshVault() {
  try {
    setStatus(await invoke<VaultStatus>("vault_status"));
    if (!vault.status?.unlocked) return;
    const [items, secrets] = await Promise.all([
      invoke<ItemInfo[]>("vault_items"),
      invoke<SecretInfo[]>("vault_list"),
    ]);
    vault.items = items;
    vault.secrets = secrets;
  } catch (e) {
    failed(e);
  }
}

/** An error to show, unless the vault locked meanwhile, or prod asks for the password: their screens say so. */
export function failed(e: unknown): string {
  const text = String(e);
  if (/needs the master password again/.test(text)) return "";
  if (/the vault is locked/.test(text)) {
    refreshVault();
    return "";
  }
  if (/wrong master password/.test(text)) return t("vaultWrongPassword");
  return text;
}

export function showVault() {
  ui.view = "vault";
  vault.loaded = true;
  watch();
  refreshVault();
}

export function showVaultAll() {
  ui.view = "vaultAll";
  vault.loadedAll = true;
  watch();
  refreshVault();
}

/** From the palette: in the project's vault when the project sees it, otherwise in the whole vault. */
export function openEntry(key: string, project: string | null) {
  const here = app.project === null || project === null || project === app.project;
  vault.selected[here ? "vault" : "vaultAll"] = key;
  if (here) showVault();
  else showVaultAll();
}

let quiet: ReturnType<typeof setTimeout> | undefined;

function say(text: string) {
  vault.notice = text;
  clearTimeout(quiet);
  quiet = setTimeout(() => (vault.notice = ""), 5000);
}

/** Copies a value; a secret one leaves the clipboard after a while if it still holds it. */
export async function copy(text: string, secret = true) {
  try {
    await navigator.clipboard.writeText(text);
  } catch (e) {
    vault.error = String(e);
    return;
  }
  if (!secret) return say(t("vaultCopied"));
  say(t("vaultCopiedSecret", { seconds: CLEAR_AFTER }));
  const kept = (e: unknown) => (vault.error = t("vaultClipboardKept", { error: String(e) }));
  let sum = "";
  try {
    sum = await digest(text);
  } catch (e) {
    return kept(e);
  }
  setTimeout(async () => {
    try {
      if (await invoke<boolean>("keys_clipboard_clear", { digest: sum })) say(t("keysClipboardCleared"));
    } catch (e) {
      kept(e);
    }
  }, CLEAR_AFTER * 1000);
}
