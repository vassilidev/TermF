<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ServerCog from "@lucide/svelte/icons/server-cog";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import { t } from "$lib/i18n";
  import { secondsLeft, type Request } from "$lib/ops-logic";
  import { dropRequest, openProposal } from "$lib/ops.svelte";
  import type { VaultStatus } from "$lib/vault.svelte";

  /**
   * An agent's request, as the Home and the Ops view show it: the exact command, its target, environment and mode,
   * then the user's answer. Approving prod asks for the master password when it was not typed within 5 minutes.
   */
  let { request: r, primary = true }: { request: Request; primary?: boolean } = $props();

  const who = $derived(r.agent.role === "cto" ? t("ux3t1Cto") : (r.agent.role ?? r.agent.title));
  let now = $state(Date.now() / 1000);
  const tick = setInterval(() => (now = Date.now() / 1000), 1000);
  onDestroy(() => clearInterval(tick));
  const left = $derived(secondsLeft(r.until, now));
  const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  let busy = $state(false);
  let error = $state("");
  let password = $state("");
  let asking = $state(false);

  // Prod asks for the password up front when the vault would refuse the approval without it.
  $effect(() => {
    if (r.kind !== "approve" || r.plan.env !== "prod") return;
    invoke<VaultStatus>("vault_status").then(
      (s) => (asking = s.initialized && !s.prodUntil),
      () => {},
    );
  });

  async function answer(allow: boolean) {
    busy = true;
    error = "";
    try {
      await invoke("ops_answer", { id: r.id, allow, password: allow && asking ? password : null });
      dropRequest(r.id);
    } catch (e) {
      const text = String(e);
      if (/needs the master password again/.test(text)) asking = true;
      else error = /wrong master password/.test(text) ? t("vaultWrongPassword") : text;
    } finally {
      busy = false;
      password = "";
    }
  }
</script>

<article class="request" class:prod={r.plan.env === "prod"} data-wait={`ops:${r.id}`} tabindex="-1">
  <header>
    {#if r.kind === "approve"}<ShieldAlert size={15} aria-hidden="true" />{:else}<ServerCog size={15} aria-hidden="true" />{/if}
    <b>{t(r.kind === "approve" ? "opsAskApprove" : "opsAskPropose", { who, recipe: r.plan.recipe, connection: r.plan.connection })}</b>
  </header>
  <p class="facts">
    <span class="badge env" class:prod={r.plan.env === "prod"}>{r.plan.env}</span>
    <span class="badge" class:write={r.plan.mode === "write"}>{t(r.plan.mode === "write" ? "opsWrite" : "opsRead")}</span>
    <span class="mono target">{r.plan.target}</span>
  </p>
  <div class="command">
    <span class="label">{r.plan.remote ? t("opsRemote", { target: r.plan.target }) : t("opsCommand")}</span>
    <pre>{r.plan.remote ?? r.plan.command}</pre>
    {#if r.plan.remote || r.plan.tunnel}
      <details>
        <summary>{t("opsCommand")}</summary>
        {#if r.plan.tunnel}<pre>{r.plan.tunnel}</pre>{/if}
        <pre>{r.plan.command}</pre>
      </details>
    {/if}
    {#if r.plan.secrets.length}<p class="muted small">{t("opsVaultSecrets", { names: r.plan.secrets.join(", ") })}</p>{/if}
  </div>
  {#if r.kind === "propose"}<p class="muted small">{t("opsAskProposeHelp")}</p>{/if}
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  {#if r.kind === "approve" && asking}
    <label class="labelled pw">
      <span>{t("opsPasswordReauth")}</span>
      <input class="field" type="password" autocomplete="current-password" placeholder={t("opsPassword")} bind:value={password} />
    </label>
  {/if}
  <footer>
    {#if r.kind === "approve" && left !== null}<span class="muted small" role="timer">{t("opsExpiresIn", { time: clock(left) })}</span>{/if}
    <span class="acts">
      {#if r.kind === "approve"}
        <button class="btn sm" disabled={busy} onclick={() => answer(false)}>{t("opsRefuse")}</button>
        <button class="btn sm" class:primary disabled={busy || (asking && !password)} onclick={() => answer(true)}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("opsApprove")}
        </button>
      {:else}
        <button class="btn sm" disabled={busy} onclick={() => answer(false)}>{t("opsIgnore")}</button>
        <button class="btn sm" class:primary onclick={() => openProposal(r)}>{t("opsOpen")}</button>
      {/if}
    </span>
  </footer>
</article>

<style>
  .request {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--waiting-ring);
    border-radius: var(--radius-lg);
    background: var(--waiting-soft);
    color: var(--text);
    font-size: var(--text-sm);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--waiting-text);
  }

  header b {
    font-size: var(--text-base);
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0;
  }

  .badge {
    background: var(--panel);
  }

  .badge.prod {
    border-color: var(--danger);
    background: var(--danger);
    color: var(--danger-text);
  }

  .badge.write {
    border-color: var(--danger);
    color: var(--danger);
  }

  .target {
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .command {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  pre {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--panel);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  details pre {
    margin-top: 4px;
  }

  summary {
    color: var(--muted);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .pw input {
    max-width: 320px;
  }

  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .acts {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
</style>
