<script lang="ts">
  import { ago, lang, t } from "$lib/i18n";
  import { showClaude } from "$lib/claude.svelte";
  import { NEARLY_USED, fullest, pressure, windows as live } from "$lib/usage-logic";
  import { billingText, isSubscribed, resetClock, usage, windowName } from "$lib/usage.svelte";

  let open = $state(false);
  let box = $state<HTMLElement>();

  const limits = $derived(usage.data?.limits ?? null);
  const top = $derived(fullest(limits));
  const warn = $derived(pressure(limits));
  const windows = $derived(live(limits));
  const percent = new Intl.NumberFormat(lang, { maximumFractionDigits: 0 });
  const level = (p: number) => (p >= 100 ? "over" : p >= NEARLY_USED ? "warn" : "");
  const used = $derived(top ? Math.min(100, top.window.usedPercentage) : 0);
  const label = $derived(
    top ? `${t("usage2Gauge")} · ${windowName(top.name)} ${t("usage2GaugeShort", { percent: percent.format(top.window.usedPercentage) })}` : t("usage2Gauge"),
  );

  function outside(e: PointerEvent) {
    if (open && box && !box.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === "Escape" && open && (open = false)} />

{#if isSubscribed() || top}
  <div class="usage" bind:this={box}>
    <button class="icon-btn gauge {top ? level(top.window.usedPercentage) : ''}" aria-label={label} title={label} aria-expanded={open} onclick={() => (open = !open)}>
      <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <circle class="track" cx="8" cy="8" r="6" />
        {#if top}<circle class="fill" cx="8" cy="8" r="6" pathLength="100" stroke-dasharray="{used} 100" />{/if}
      </svg>
    </button>
    {#if open}
      <div class="card" role="dialog" aria-label={t("usage2Gauge")}>
        <p class="mode">{billingText(usage.billing)}</p>
        {#if windows.length}
          {#each windows as w (w.name)}
            <div class="row {level(w.window.usedPercentage)}">
              <span>{windowName(w.name)}</span>
              <b>{t("usage2GaugeShort", { percent: percent.format(w.window.usedPercentage) })}</b>
            </div>
            <div class="bar {level(w.window.usedPercentage)}"><span style="width: {Math.min(100, w.window.usedPercentage)}%"></span></div>
            {#if w.window.resetsAt}
              <p class="muted small">{t("usage2ResetsAt", { time: resetClock(w.window.resetsAt), when: ago(w.window.resetsAt) })}</p>
            {/if}
          {/each}
          {#if warn}
            <p class="alert" role="status">
              {warn.window.usedPercentage >= 100
                ? warn.window.resetsAt
                  ? t("usage2Reached", { window: windowName(warn.name), time: resetClock(warn.window.resetsAt) })
                  : t("usage2ReachedNoTime", { window: windowName(warn.name) })
                : t("usage2NearlyUsed", {
                    window: windowName(warn.name),
                    percent: percent.format(warn.window.usedPercentage),
                    time: warn.window.resetsAt ? resetClock(warn.window.resetsAt) : "—",
                  })}
            </p>
          {/if}
        {:else}
          <p class="muted small">{t("usage2NoReading")}</p>
        {/if}
        {#if isSubscribed()}<p class="muted small">{t("usage2Help")}</p>{/if}
        <button class="btn sm" onclick={() => ((open = false), showClaude("usage"))}>{t("usage2ShowUsage")}</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .usage {
    position: relative;
    display: flex;
  }

  .gauge {
    width: 24px;
  }

  svg {
    transform: rotate(-90deg);
  }

  circle {
    fill: none;
    stroke-width: 2.5;
  }

  .track {
    stroke: var(--border);
  }

  .fill {
    stroke: var(--done);
    stroke-linecap: round;
  }

  .gauge.warn .fill {
    stroke: var(--waiting);
  }

  .gauge.over .fill {
    stroke: var(--danger);
  }

  .card {
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    z-index: 30;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 260px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    box-shadow: var(--shadow-3);
    font-size: var(--text-sm);
  }

  .mode {
    margin: 0 0 2px;
    font-weight: 600;
  }

  .row {
    display: flex;
    justify-content: space-between;
    color: var(--muted);
  }

  .row b {
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .bar {
    height: 5px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--hover);
  }

  .bar span {
    display: block;
    height: 100%;
    background: var(--done);
  }

  .bar.warn span {
    background: var(--waiting);
  }

  .bar.over span {
    background: var(--danger);
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .alert {
    margin: 2px 0 0;
    color: var(--waiting-ring);
    font-size: var(--text-xs);
    font-weight: 500;
  }

  .card .btn {
    align-self: flex-start;
    margin-top: 4px;
  }
</style>
