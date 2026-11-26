<script lang="ts">
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { t } from "$lib/i18n";
  import { saveRecoveryKey } from "$lib/recovery-key";
  import { shortPath } from "$lib/state.svelte";
  import { copy, vault } from "$lib/vault.svelte";

  const lead = { first: "vault2KeyFirst", renewed: "vault2KeyRenewed", recovered: "vault2KeyRecovered" } as const;
  let stored = $state(false);
  let saved = $state("");

  async function download(key: string) {
    try {
      saved = (await saveRecoveryKey(key, "vault")) ?? saved;
    } catch (e) {
      vault.error = String(e);
    }
  }
</script>

{#if vault.recoveryKey}
  {@const { key, reason } = vault.recoveryKey}
  <section class="recovery" aria-labelledby="vault-recovery-title">
    <h2 id="vault-recovery-title">{t("vault2KeyTitle")}</h2>
    <p class="lead"><CircleCheck size={16} aria-hidden="true" class="ok" />{t(lead[reason])}</p>
    <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("vault2KeyOnce")}</p>
    <div class="key">
      <code aria-label={t("vault2Recovery")}>{key}</code>
      <button class="btn sm" onclick={() => copy(key)}><Copy size={14} aria-hidden="true" />{t("vault2KeyCopy")}</button>
      <button class="btn sm" onclick={() => download(key)}><Download size={14} aria-hidden="true" />{t("syncVisibleKeyDownload")}</button>
    </div>
    {#if saved}<p class="muted fine" role="status">{t("syncVisibleKeySaved", { path: shortPath(saved) })}</p>{/if}
    <p class="muted fine">{t("vault2RecoveryLost")}</p>
    <label class="check">
      <input type="checkbox" bind:checked={stored} />
      {t("vault2KeyStored")}
    </label>
    <div>
      <button class="btn primary" disabled={!stored} onclick={() => (vault.recoveryKey = null)}>{t("vault2Continue")}</button>
    </div>
  </section>
{/if}

<style>
  .recovery {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    max-width: 620px;
    margin: 40px auto;
    padding: 0 24px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
  }

  .lead {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }

  .lead :global(.ok) {
    flex-shrink: 0;
    color: var(--done);
  }

  .key {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--bg);
  }

  .key code {
    font: 600 var(--text-md) / 1.4 var(--font-mono);
    letter-spacing: 0.04em;
    user-select: all;
  }

  .fine {
    margin: 0;
    font-size: var(--text-sm);
  }

  .check {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-weight: 500;
    cursor: pointer;
  }

  .check input {
    margin: 0;
    transform: translateY(2px);
  }
</style>
