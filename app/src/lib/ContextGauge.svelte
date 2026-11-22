<script lang="ts">
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { lang, t } from "$lib/i18n";
  import { app, type Gauge } from "$lib/state.svelte";

  let { gauge }: { gauge: Gauge } = $props();

  const steps = {
    queued: ["contextQueued", "contextReset"],
    handoff: ["contextHandoff", "contextHandoffHelp"],
    clearing: ["contextClearing", "contextClearingHelp"],
    resuming: ["contextResuming", "contextResumingHelp"],
  } as const;

  const failures = {
    typed: "contextFailTyped",
    busy: "contextFailBusy",
    interrupted: "contextFailInterrupted",
    empty_handoff: "contextFailEmpty",
    save_failed: "contextFailSave",
    write_failed: "contextFailWrite",
    timeout: "contextFailTimeout",
  } as const;

  const percent = new Intl.NumberFormat(lang, { style: "percent" });
  const tokens = new Intl.NumberFormat(lang, { notation: "compact" });

  const settings = $derived(app.contextSettings);
  const level = $derived.by(() => {
    if (gauge.percent === null || !settings) return "";
    if (gauge.percent >= settings.resetAt) return "over";
    return gauge.percent >= settings.warningAt ? "warn" : "";
  });
  const title = $derived.by(() => {
    const used =
      gauge.percent === null
        ? t("contextUnknown")
        : t("contextUsed", {
            tokens: tokens.format(gauge.tokens ?? 0),
            window: tokens.format(gauge.window ?? 0),
            percent: gauge.percent,
          });
    const auto = !settings ? "" : settings.on ? t("contextAutoAt", { percent: settings.resetAt }) : t("contextAutoOff");
    return `${t("uxfT2GaugeHint")}\n${used} ${auto}`.trim();
  });
</script>

{#if gauge.phase !== "idle"}
  {@const [label, help] = steps[gauge.phase]}
  <span class="step" title={t(help)} role="status">
    <LoaderCircle size={13} aria-hidden="true" />{t(label)}
  </span>
{:else}
  <span
    class="gauge {level}"
    role="meter"
    aria-label={t("context")}
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={gauge.percent ?? 0}
    aria-valuetext={gauge.percent === null ? t("contextUnknown") : percent.format(gauge.percent / 100)}
    {title}
  >
    <span class="bar"><span class="fill" style:width="{gauge.percent ?? 0}%"></span></span>
    {gauge.percent === null ? "—" : percent.format(gauge.percent / 100)}
  </span>
  {#if gauge.failure}
    <span class="failed" title={t("contextFailedWhy", { reason: t(failures[gauge.failure]) })} role="status">
      <CircleAlert size={13} aria-hidden="true" />{t("contextFailed")}
    </span>
  {/if}
{/if}

<style>
  .gauge,
  .step,
  .failed {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    gap: 6px;
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .gauge {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .bar {
    width: 32px;
    height: 4px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--selected);
  }

  .fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }

  .warn {
    color: var(--warning);
  }

  .warn .fill {
    background: var(--warning);
  }

  .over {
    color: var(--danger);
  }

  .over .fill {
    background: var(--danger);
  }

  .step,
  .failed {
    gap: 5px;
    height: 22px;
    padding: 0 8px 0 6px;
    border-radius: var(--radius-pill);
  }

  .step {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .step :global(svg) {
    animation: spin 1.2s linear infinite;
  }

  .failed {
    background: var(--danger-soft);
    color: var(--danger);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .step :global(svg) {
      animation: none;
    }
  }
</style>
