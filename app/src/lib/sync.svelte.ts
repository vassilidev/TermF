import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ago, t } from "$lib/i18n";
import { loadKeys } from "$lib/keymap.svelte";
import { library } from "$lib/library.svelte";
import { refresh as refreshSchedule, schedule } from "$lib/schedule.svelte";
import { showSettings } from "$lib/settings.svelte";
import { app, callLayouts, load, type ContextSettings, type KeepAwakeState } from "$lib/state.svelte";

export type Scope = "config" | "projects" | "conventions" | "vault" | "sshKeys";
/** S3: access key id and secret key; WebDAV: user and password. Sent when setting up only. */
export type Credentials = { user: string; secret: string };
export type Remote =
  | { kind: "git"; url: string }
  | { kind: "folder"; path: string }
  | { kind: "s3"; endpoint: string; bucket: string; region: string; prefix: string; credentials?: Credentials }
  | { kind: "webdav"; url: string; credentials?: Credentials };
/** `local` / `remote` null: deleted on that side. */
export type Conflict = { path: string; keys: string[]; local: string | null; remote: string | null };
export type MissingRepo = { project: string; path: string; remote: string | null };
export type Report = {
  pulled: string[];
  pushed: string[];
  conflicts: Conflict[];
  missingRepos: MissingRepo[];
  skipped: { path: string; error: string }[];
};
export type SyncStatus = {
  enabled: boolean;
  remote: Remote | null;
  scopes: Scope[];
  deviceId: string | null;
  deviceName: string | null;
  unlocked: boolean;
  remembered: boolean;
  lastSync: number | null;
  lastError: string | null;
  conflicts: Conflict[];
  missingRepos: MissingRepo[];
  /** Another machine forgot this one: nothing syncs here until set up again. */
  forgotten: boolean;
};
export type Device = { id: string; name: string; os: string; lastSync: number; current: boolean };
/** `sync-pulled`: what a pull brought that does not apply at once. */
export type Pulled = { paths: string[]; restart: "confirmCli"[]; sessions: boolean };

export const scopes: [Scope, Parameters<typeof t>[0], Parameters<typeof t>[0]][] = [
  ["config", "syncScopeConfig", "syncScopeConfigHelp"],
  ["projects", "syncScopeProjects", "syncScopeProjectsHelp"],
  ["conventions", "syncScopeConventions", "syncScopeConventionsHelp"],
  ["vault", "vault3SyncScope", "vault3SyncScopeHelp"],
  ["sshKeys", "sync2ScopeSsh", "sync2ScopeSshHelp"],
];

export const sync = $state({
  /** Shown once, kept only in memory until the owner says it is stored. */
  recoveryKey: null as { key: string; reason: "setup" | "changed" | "recovered" | "forgot" } | null,
  /** The last pull that brought something that applies only later, until dismissed. */
  pulled: null as Pulled | null,
  /** git's last progress line, by `project + "\n" + path` of the repository being cloned. */
  cloning: {} as Record<string, string>,
  /** The last manual sync, with the `lastSync` and `lastError` it left: any later sync, even a failed one, makes it stale. */
  report: null as (Report & { at: number | null; error: string | null }) | null,
  /** The command in progress, or "". */
  busy: "",
  error: "",
});

export function showSync() {
  showSettings("sync");
  sync.error = "";
}

/** The repositories being cloned (`project + "\n" + path`), while `sync_clone` runs. */
export const clones = new Set<string>();

/** Reloads what a pull may have changed (projects, shortcuts, layouts, library, settings) and follows clones. */
export function watchSync(): () => void {
  const stopPulled = listen<Pulled>("sync-pulled", ({ payload }) => {
    load();
    callLayouts("layouts");
    library.version++;
    if (schedule.loaded) refreshSchedule();
    invoke<ContextSettings>("context_settings").then((s) => (app.contextSettings = s), () => {});
    invoke<KeepAwakeState>("keep_awake_state").then((s) => (app.keepAwake = s), () => {});
    if (payload.restart.length || payload.sessions) sync.pulled = payload;
  });
  const stopClone = listen<{ project: string; path: string; line: string }>("sync-clone", ({ payload }) => {
    const key = payload.project + "\n" + payload.path;
    // A line that arrives after the clone ended would bring its spinner back and hide its error.
    if (clones.has(key)) sync.cloning[key] = payload.line;
  });
  return () => {
    stopPulled.then((stop) => stop());
    stopClone.then((stop) => stop());
  };
}

export async function refreshSync() {
  app.sync = await invoke<SyncStatus>("sync_status");
}

/** Runs a sync command, then reloads the status. Returns undefined when it failed (`sync.error` says why). */
export async function syncCall<T>(command: string, args: Record<string, unknown> = {}): Promise<T | undefined> {
  sync.busy = command;
  sync.error = "";
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    sync.error = String(e);
  } finally {
    // A failed refresh must not lose the result, which may be a recovery key shown only once.
    await refreshSync().catch(() => {});
    sync.busy = "";
  }
}

export async function syncNow(publish = true) {
  if (sync.busy) return;
  const report = await syncCall<Report>(publish ? "sync_push" : "sync_pull");
  if (report) keepReport(report);
}

export function keepReport(report: Report) {
  if (report.pulled.length) loadKeys();
  sync.report = { ...report, at: app.sync?.lastSync ?? null, error: app.sync?.lastError ?? null };
}

export function currentReport(): Report | null {
  const r = sync.report;
  return r && r.at === app.sync?.lastSync && r.error === app.sync?.lastError ? r : null;
}

/** One line for the status, worst first; `tone` other than "ok" puts a dot on the Settings entry of the navigation. */
export function summary(status: SyncStatus | null): { tone: "ok" | "off" | "attention" | "error"; text: string } {
  if (sync.recoveryKey) return { tone: "attention", text: t("syncKeyPending") };
  if (!status?.enabled) return { tone: "off", text: t("syncOff") };
  if (status.forgotten) return { tone: "error", text: t("sync2ForgottenStatus") };
  if (!status.unlocked) return { tone: "attention", text: t("syncLocked") };
  if (status.conflicts.length) return { tone: "attention", text: t("syncConflicts", { count: status.conflicts.length }) };
  // The last manual sync says when the error is only files left aside.
  const skipped = currentReport()?.skipped.length;
  if (status.lastError && skipped) return { tone: "attention", text: t("syncSkipped", { count: skipped }) };
  if (status.lastError) return { tone: "error", text: t("syncFailed") };
  if (status.lastSync === null) return { tone: "ok", text: t("syncNever") };
  return { tone: "ok", text: t("syncDone", { ago: ago(status.lastSync) }) };
}

export function remoteText(remote: Remote): string {
  switch (remote.kind) {
    case "git":
    case "webdav":
      return remote.url;
    case "folder":
      return remote.path;
    case "s3":
      return remote.endpoint && remote.bucket
        ? `${remote.endpoint.replace(/\/+$/, "")}/${remote.bucket}/${remote.prefix.replace(/^\/+/, "")}`
        : "";
  }
}
