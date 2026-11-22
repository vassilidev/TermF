import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "$lib/i18n";
import { CLEAR_AFTER, digest } from "$lib/ssh.svelte";
import { app, ui } from "$lib/state.svelte";
import { counts, type ItemInfo, type Merged, type SecretInfo } from "$lib/vault-logic";

export type VaultStatus = {
  initialized: boolean;
  unlocked: boolean;
  remembered: boolean;
  autoLockMinutes: number;
  prodUntil: number | null;
  keys: number;
  recovery: boolean;
  quickDays: number | null;
  quickUntil: number | null;
  confirmedUntil: number | null;
  /** Another machine's version, brought by the sync, waits beside the vault. */
  synced: "pending" | "secured" | "other" | "newer" | null;
};
export type VaultView = "vault" | "vaultAll";
export type KeyInfo = { id: string; name: string; created: number };
/** What this machine offers besides the password: TermF asks for a key's PIN (not on Windows), Touch ID, Windows Hello. */
export type VaultMethods = { keyPin: boolean; biometry: "touchId" | "windowsHello" | null };
export type RecoveryReason = "first" | "renewed" | "recovered";

/** Names and descriptions only: a value reaches the webview when the user reveals or copies it, and is dropped after. */
export const vault = $state({
  loaded: false,
  loadedAll: false,
  status: null as VaultStatus | null,
  items: [] as ItemInfo[],
  secrets: [] as SecretInfo[],
  error: "",
  notice: "",
  /** Here rather than in the view: the palette sets it too. */
  selected: { vault: null, vaultAll: null } as Record<VaultView, string | null>,
  methods: null as VaultMethods | null,
  /** Shown once, until the user says it is stored; never kept past that. */
  recoveryKey: null as { key: string; reason: RecoveryReason } | null,
});

/** Touch ID or Windows Hello by its name, when this machine has one. */
export const biometryName = () =>
  vault.methods?.biometry === "touchId" ? t("vault2TouchId") : vault.methods?.biometry === "windowsHello" ? t("vault2WindowsHello") : "";

/** Whether Touch ID or Windows Hello can reopen the vault, or confirm prod, right now. */
export const quickReady = () => !!vault.methods?.biometry && (vault.status?.quickUntil ?? 0) > Date.now() / 1000;

export const needsConfirm = (e: unknown) => /needs the master password again/.test(String(e));

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
  // Another machine's version merged while the vault is open.
  listen<Merged[]>("vault-merged", (e) => {
    refreshVault();
    if (counts(e.payload).length) say(t("vault3Merged", { summary: mergeText(e.payload) }));
  });
}

/** "2 ajouté(s), 1 en double version", in the user's language. */
export function mergeText(rows: Merged[]): string {
  const words = { added: "vault3CountAdded", changed: "vault3CountChanged", deleted: "vault3CountDeleted", conflict: "vault3CountConflict" } as const;
  const parts = counts(rows).map(([change, n]) => t(words[change], { n }));
  return parts.length ? parts.join(", ") : t("vault3NothingChanged");
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
    vault.methods ??= await invoke<VaultMethods>("vault_methods");
    setStatus(await invoke<VaultStatus>("vault_status"));
    if (!vault.status?.unlocked) return;
    const [items, secrets] = await Promise.all([
      invoke<ItemInfo[]>("vault_items"),
      invoke<SecretInfo[]>("vault_list"),
    ]);
    vault.items = items;
    vault.secrets = secrets;
  } catch (e) {
    vault.error = failed(e);
  }
}

/** The lock screen and the prod password prompt say it themselves. */
export function failed(e: unknown): string {
  const text = String(e);
  if (/needs the master password again/.test(text)) return "";
  if (/the vault is locked/.test(text)) {
    // The status only, once: listing again could fail the same way, forever (a vault file swapped under it).
    invoke<VaultStatus>("vault_status")
      .then((status) => {
        setStatus(status);
        if (status.unlocked) vault.error = text;
      })
      .catch((e) => (vault.error = String(e)));
    return "";
  }
  if (/wrong master password/.test(text)) return t("vaultWrongPassword");
  if (/wrong recovery key/.test(text)) return t("vault2WrongRecovery");
  if (/master password changed on another machine/.test(text)) return t("vault3PasswordChanged");
  if (/wrong export password/.test(text)) return t("vault3WrongExportPassword");
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

export function openEntry(key: string, project: string | null) {
  const here = app.project === null || project === null || project === app.project;
  vault.selected[here ? "vault" : "vaultAll"] = key;
  if (here) showVault();
  else showVaultAll();
}

let quiet: ReturnType<typeof setTimeout> | undefined;

export function say(text: string) {
  vault.notice = text;
  clearTimeout(quiet);
  quiet = setTimeout(() => (vault.notice = ""), 5000);
}

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
