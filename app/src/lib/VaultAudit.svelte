<script lang="ts">
  import Download from "@lucide/svelte/icons/download";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import X from "@lucide/svelte/icons/x";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import Banner from "$lib/ui/Banner.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { askAI, type Chip } from "$lib/ask.svelte";
  import { lang, t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";
  import { failed, say } from "$lib/vault.svelte";
  import type { AuditCheck, AuditEntry } from "$lib/vault-logic";

  /** The vault's audit log: who opened, read or changed what, its chain checked, exported as JSON lines. */
  let {
    projectName,
    onclose,
    onread,
  }: { projectName: (id: string) => string; onclose: () => void; onread: (chip: Chip) => void } = $props();

  const LIMIT = 500;
  let entries = $state<AuditEntry[] | null>(null);
  let check = $state<AuditCheck | null>(null);
  let error = $state("");
  let exporting = $state(false);

  const actions: Record<string, Parameters<typeof t>[0]> = {
    init: "restVaultActInit",
    unlock: "restVaultActUnlock",
    unlock_failed: "restVaultActUnlockFailed",
    lock: "restVaultActLock",
    reauth: "restVaultActReauth",
    reauth_failed: "restVaultActReauthFailed",
    write: "restVaultActWrite",
    reference: "restVaultActReference",
    delete: "restVaultActDelete",
    reveal: "restVaultActReveal",
    read: "restVaultActRead",
    resolve: "restVaultActResolve",
    refused: "restVaultActRefused",
    security: "restVaultActSecurity",
    merge: "restVaultActMerge",
    export: "restVaultActExport",
    import: "restVaultActImport",
    restore: "restVaultActRestore",
    ops: "restVaultActOps",
  };
  const failing = new Set(["unlock_failed", "reauth_failed", "refused"]);

  const action = (e: AuditEntry) => (actions[e.action] ? t(actions[e.action]) : e.action);
  const who = (e: AuditEntry) =>
    e.actor.kind === "human"
      ? t("restVaultAuditYou")
      : e.actor.kind === "termf"
        ? t("restVaultAuditTermf")
        : e.actor.run !== null
          ? t("restVaultAuditAgentRun", { run: e.actor.run })
          : t("restVaultAuditAgent");
  const day = (at: number) => new Date(at * 1000).toLocaleDateString(lang, { weekday: "long", day: "numeric", month: "long" });
  const time = (at: number) => new Date(at * 1000).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });

  const days = $derived.by(() => {
    const out: { day: string; entries: AuditEntry[] }[] = [];
    for (const e of entries ?? []) {
      const d = day(e.at);
      if (out.at(-1)?.day !== d) out.push({ day: d, entries: [] });
      out.at(-1)?.entries.push(e);
    }
    return out;
  });

  const chain = $derived(
    !check ? t("restVaultAuditChecking") : check.ok ? t("restVaultAuditIntact", { n: check.entries }) : t("restVaultAuditBroken", { id: check.broken ?? "?" }),
  );

  // The ⌘I panel gets what was done, never a value (the log holds none) nor the details (an Ops call's parameters).
  $effect(() => {
    const lines = (entries ?? []).slice(0, 40).map((e) =>
      [new Date(e.at * 1000).toISOString(), e.action, e.target, e.scope, e.project, e.actor.kind, e.actor.run !== null ? `run ${e.actor.run}` : null]
        .filter(Boolean)
        .join(" "),
    );
    onread({ id: "vault-audit", kind: "object", label: t("restVaultAuditChip", { state: chain }), text: [chain, ...lines].join("\n") });
  });

  async function load() {
    error = "";
    try {
      [entries, check] = await Promise.all([invoke<AuditEntry[]>("vault_audit", { limit: LIMIT }), invoke<AuditCheck>("vault_audit_verify")]);
    } catch (e) {
      error = failed(e) || String(e);
      entries ??= [];
    }
  }

  async function exportLog() {
    exporting = true;
    error = "";
    try {
      const stamp = new Date().toISOString().slice(0, 10);
      const path = await save({ defaultPath: `termf-vault-audit-${stamp}.jsonl`, filters: [{ name: "JSON Lines", extensions: ["jsonl"] }] });
      if (!path) return;
      const n = await invoke<number>("vault_audit_export", { path });
      say(t("restVaultAuditExported", { n, path: shortPath(path) }));
    } catch (e) {
      error = failed(e) || String(e);
    } finally {
      exporting = false;
    }
  }

  function askBroken() {
    void askAI({ role: "secops", draft: t("restVaultAuditBrokenDraft", { id: check?.broken ?? "?" }) });
  }

  load();
</script>

<section class="audit" aria-labelledby="vault-audit-title">
  <header class="head">
    <span class="avatar" aria-hidden="true"><ScrollText size={18} /></span>
    <div class="titles">
      <h2 id="vault-audit-title">{t("restVaultAudit")}</h2>
      <!-- Broken, the banner below says it, with what to do. -->
      {#if !check || check.ok}
        <p class="chain" class:ok={check?.ok} role="status">
          {#if check}<ShieldCheck size={13} aria-hidden="true" />{:else}<LoaderCircle size={13} class="spin" aria-hidden="true" />{/if}
          {chain}
        </p>
      {/if}
    </div>
    <button class="btn sm" data-testid="vault-audit-export" disabled={exporting || !entries?.length} onclick={exportLog}>
      {#if exporting}<LoaderCircle size={14} class="spin" aria-hidden="true" />{:else}<Download size={14} aria-hidden="true" />{/if}{t("restVaultAuditExport")}
    </button>
    <button class="icon-btn" aria-label={t("close")} title={t("close")} onclick={onclose}><X size={16} aria-hidden="true" /></button>
  </header>

  <p class="intro muted">{t("restVaultAuditIntro")}</p>

  {#if check && !check.ok}
    <Banner kind="error" icon={ShieldX}>
      {t("restVaultAuditBroken", { id: check.broken ?? "?" })}
      {#snippet action()}
        <button class="btn sm" onclick={askBroken}>{t("aigAskRole", { role: "secops" })}</button>
      {/snippet}
    </Banner>
  {/if}
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  {#if entries === null}
    <Skeleton shape="table" rows={8} label={t("restVaultAuditLoading")} />
  {:else if !entries.length}
    <p class="muted empty">{t("restVaultAuditEmpty")}</p>
  {:else}
    <div class="log">
      {#each days as group (group.day)}
        <h3 class="day">{group.day}</h3>
        <ul>
          {#each group.entries as e (e.id)}
            <li title={e.detail ?? undefined} data-action={e.action}>
              <span class="time num muted">{time(e.at)}</span>
              <span class="what" class:fail={failing.has(e.action)}>{action(e)}</span>
              <span class="target">
                {#if e.target}<span class="mono name">{e.target}</span>{/if}
                {#if e.scope}<span class="badge" class:danger={e.scope === "prod"}>{e.scope}</span>{/if}
                {#if e.target || e.scope}<span class="muted">{e.project ? projectName(e.project) : t("vaultGlobal")}</span>{/if}
              </span>
              <span class="who muted">{who(e)}</span>
            </li>
          {/each}
        </ul>
      {/each}
      {#if entries.length >= LIMIT}<p class="muted fine">{t("restVaultAuditLatest", { n: LIMIT })}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .audit {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .avatar {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .chain {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .chain.ok {
    color: var(--done);
  }

  .intro {
    max-width: 64ch;
    margin: 0;
  }

  .empty {
    margin: 8px 0;
  }

  .log {
    container-type: inline-size;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .day {
    position: sticky;
    top: 0;
    margin: 0;
    padding: 10px 14px 6px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .day::first-letter {
    text-transform: uppercase;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: grid;
    grid-template-columns: 44px minmax(120px, 180px) minmax(0, 1fr) minmax(0, 150px);
    align-items: center;
    gap: 12px;
    min-height: var(--row);
    padding: 4px 14px;
    font-size: var(--text-sm);
  }

  li + li {
    border-top: 1px solid var(--border);
  }

  .what {
    font-weight: 500;
  }

  .what.fail {
    color: var(--danger);
  }

  .target {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  .target .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Narrow (the ⌘I panel open): what was touched goes under the action. */
  @container (max-width: 600px) {
    li {
      grid-template-columns: 44px minmax(0, 1fr) auto;
      row-gap: 2px;
      padding-block: 6px;
    }

    .target {
      grid-area: 2 / 2 / 3 / 4;
    }

    .target:empty {
      display: none;
    }
  }

  .who {
    overflow: hidden;
    text-align: right;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fine {
    margin: 0;
    padding: 10px 14px;
    border-top: 1px solid var(--border);
    font-size: var(--text-sm);
  }
</style>
