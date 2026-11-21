<script lang="ts">
  import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { t } from "$lib/i18n";
  import { app, ui } from "$lib/state.svelte";
  import { remoteText, showSync, summary, sync, syncNow } from "$lib/sync.svelte";

  const info = $derived(summary(app.sync));
  const status = $derived(app.sync);
  const ready = $derived(!!status?.enabled && status.unlocked && !sync.recoveryKey);
  const openLabel = $derived(
    t(
      !status?.enabled
        ? "syncOpenEnable"
        : !status.unlocked
          ? "syncOpenUnlock"
          : status.conflicts.length
            ? "syncOpenResolve"
            : "syncOpenManage",
    ),
  );
</script>

{#if status}
  <section class="sync">
    <h3>{t("syncTitle")}</h3>
    <p class="state tone-{info.tone}" role="status">{info.text}</p>
    {#if status.remote}
      <code title={remoteText(status.remote)}>{remoteText(status.remote)}</code>
    {/if}
    {#if sync.error && !sync.busy}
      <p class="notice error" role="alert">{sync.error}</p>
    {/if}
    <div class="buttons">
      <button class="btn sm" onclick={() => ((ui.settings = false), showSync())}>{openLabel}</button>
      {#if ready}
        <button class="btn sm" onclick={() => syncNow()} disabled={!!sync.busy}>
          {#if sync.busy === "sync_push"}
            <LoaderCircle size={14} aria-hidden="true" class="spin" />
          {:else}
            <ArrowDownUp size={14} aria-hidden="true" />
          {/if}
          {t("syncNow")}
        </button>
      {/if}
    </div>
  </section>
{/if}

<style>
  .sync {
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .state {
    margin: 0 0 4px;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .tone-off {
    color: var(--muted);
    font-weight: 400;
  }

  .tone-ok {
    color: var(--done);
  }

  .tone-attention {
    color: var(--warning);
  }

  .tone-error {
    color: var(--danger);
  }

  code {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font: var(--text-sm) var(--font-mono);
  }

  .notice {
    margin-top: 8px;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 10px;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .buttons :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .buttons :global(.spin) {
      animation: none;
    }
  }
</style>
