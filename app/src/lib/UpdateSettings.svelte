<script lang="ts">
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Banner from "$lib/ui/Banner.svelte";
  import { ago, t } from "$lib/i18n";
  import { agentsAtWork, checkUpdates, installUpdate, setUpdates, updates, type Channel } from "$lib/release.svelte";

  const s = $derived(updates.state);
  const available = $derived(s?.available ?? null);
  const busy = $derived(agentsAtWork());
</script>

<h2>{t("releaseUpdates")}</h2>
{#if s}
  <dl class="facts">
    <dt>{t("releaseVersion")}</dt>
    <dd>{s.version}</dd>
  </dl>

  <label class="labelled">
    <span>{t("releaseChannel")}</span>
    <select
      class="field"
      value={s.channel}
      onchange={(e) => setUpdates(e.currentTarget.value as Channel, s.check)}
    >
      <option value="stable">{t("releaseStable")}</option>
      <option value="beta">{t("releaseBeta")}</option>
    </select>
  </label>
  <p class="help">{t("releaseChannelHelp")}</p>

  <label class="check">
    <input
      type="checkbox"
      role="switch"
      class="switch"
      checked={s.check}
      onchange={(e) => setUpdates(s.channel, e.currentTarget.checked)}
    />
    {t("releaseAuto")}
  </label>

  <div class="row">
    <button class="btn sm" onclick={checkUpdates} disabled={updates.checking || updates.installing}>
      {#if updates.checking}
        <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("releaseChecking")}
      {:else}
        <RefreshCw size={14} aria-hidden="true" />{t("releaseCheckNow")}
      {/if}
    </button>
    <span class="muted" role="status">
      {#if !available && s.checked && !updates.checking}{t("releaseUpToDate")}{/if}
      {#if s.checked}{t("releaseCheckedAt", { when: ago(s.checked) })}{/if}
    </span>
  </div>

  {#if updates.error || s.error}
    <Banner kind="error" ondismiss={() => (updates.error = "")}>{updates.error || s.error}</Banner>
  {/if}

  {#if available}
    <section class="update" aria-labelledby="update-title">
      <header>
        <CircleArrowUp size={16} aria-hidden="true" />
        <h3 id="update-title">{t("releaseAvailable", { version: available.version })}</h3>
        {#if available.date}<span class="muted">{t("releasePublished", { when: ago(available.date) })}</span>{/if}
      </header>
      <div class="notes">{available.notes ?? t("releaseNoNotes")}</div>
      {#if updates.installing}
        <p class="muted" role="status">
          <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("releaseDownloading", { percent: updates.progress })}
        </p>
      {:else if updates.waiting}
        <Banner>
          {t("releaseWaiting")}
          {#snippet action()}
            <button class="btn sm" onclick={() => (updates.waiting = false)}>{t("releaseCancel")}</button>
          {/snippet}
        </Banner>
      {:else if busy}
        <Banner kind="warn">{t(busy === 1 ? "releaseAgentsOne" : "releaseAgentsMany", { count: busy })}</Banner>
        <div class="row">
          <button class="btn sm primary" onclick={() => (updates.waiting = true)}>{t("releaseAfterAgents")}</button>
          <button class="btn sm" onclick={installUpdate}>{t("releaseNow")}</button>
        </div>
      {:else}
        <div class="row">
          <button class="btn sm primary" onclick={installUpdate}>{t("releaseInstall")}</button>
        </div>
      {/if}
      <p class="help">{t("releaseRestartHelp")}</p>
    </section>
  {/if}
{/if}

<style>
  h2 {
    margin: 0 0 12px;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 16px;
    margin: 0 0 16px;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    font-family: var(--font-mono);
  }

  .labelled {
    max-width: 320px;
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    cursor: pointer;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    margin: 16px 0 12px;
    font-size: var(--text-sm);
  }

  .update {
    margin-top: 20px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .update header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    color: var(--accent-ink);
  }

  h3 {
    margin: 0;
    color: var(--text);
    font-size: var(--text-base);
    font-weight: 600;
  }

  .notes {
    max-height: 240px;
    margin: 10px 0 0;
    overflow-y: auto;
    font-size: var(--text-sm);
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .update .row {
    margin-bottom: 0;
  }

  .update :global(.banner) {
    margin-top: 12px;
  }

  p[role="status"] {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 12px 0 0;
    font-size: var(--text-sm);
  }
</style>
