<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Info from "@lucide/svelte/icons/info";
  import ContextGauge from "$lib/ContextGauge.svelte";
  import { agents, usd } from "$lib/agents.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { attempt, type LimitWindow, type Usage } from "$lib/claude-native.svelte";

  let usage = $state<Usage | null>(null);

  onMount(() => {
    attempt(() => invoke<Usage>("claude_usage")).then((u) => (usage = u ?? null));
    const stop = listen<Usage>("claude-usage", ({ payload }) => (usage = payload));
    return () => void stop.then((f) => f());
  });

  const limits = $derived(usage?.limits ?? null);
  const windows = $derived(
    limits
      ? ([
          ["claudeDgFiveHour", limits.limits.fiveHour],
          ["claudeDgSevenDay", limits.limits.sevenDay],
          ["claudeDgSpend", limits.limits.spendLimit],
        ] as const).filter((w): w is [typeof w[0], LimitWindow] => w[1] !== null)
      : [],
  );
  const source = $derived(limits ? app.sessions.find((s) => s.id === limits.session)?.title : undefined);

  /** Claude Code tiles TermF started: they report through the status line. */
  const tiles = $derived(app.sessions.filter((s) => s.context));
  const tileCost = $derived(tiles.reduce((sum, s) => sum + (usage?.sessions[s.id]?.cost ?? 0), 0));

  const headless = $derived([...agents.sessions].sort((a, b) => b.updated - a.updated));
  const headlessCost = $derived(headless.reduce((sum, a) => sum + a.cost, 0));
  const dayStart = new Date().setHours(0, 0, 0, 0) / 1000;
  const todayCost = $derived(headless.filter((a) => a.updated >= dayStart).reduce((sum, a) => sum + a.cost, 0));

  const percent = new Intl.NumberFormat(lang, { maximumFractionDigits: 0 });
  const level = (p: number) => (p >= 90 ? "over" : p >= 70 ? "warn" : "");
</script>

<header class="page-head">
  <h2>{t("claudeDgUsage")}</h2>
  <p class="lead">{t("claudeDgUsageHelp")}</p>
</header>

<section class="block" aria-labelledby="claude-limits">
  <h3 id="claude-limits">{t("claudeDgLimits")}</h3>
  {#if windows.length && limits}
    <div class="meters">
      {#each windows as [label, w] (label)}
        <div class="meter {level(w.usedPercentage)}">
          <div class="meter-head">
            <span class="meter-label">{t(label)}</span>
            <span class="meter-value">{percent.format(w.usedPercentage)} %</span>
          </div>
          <div class="bar" role="meter" aria-label={t(label)} aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(w.usedPercentage)}>
            <span style="width: {Math.min(100, w.usedPercentage)}%"></span>
          </div>
          <span class="muted small">
            {#if w.usedUsd !== null && w.limitUsd !== null}{usd(w.usedUsd)} / {usd(w.limitUsd)} · {/if}
            {w.resetsAt ? t("claudeDgResets", { when: ago(w.resetsAt) }) : ""}
          </span>
        </div>
      {/each}
    </div>
    <p class="source muted small">
      <Info size={13} aria-hidden="true" />
      {t("claudeDgLimitsSource", { session: source ?? `#${limits.session}`, when: ago(limits.at) })}
    </p>
  {:else}
    <p class="empty muted">{t("claudeDgNoLimits")}</p>
  {/if}
</section>

<section class="block" aria-labelledby="claude-tiles">
  <h3 id="claude-tiles">{t("claudeDgTiles")} <span class="count">{tiles.length}</span></h3>
  {#if tiles.length}
    <table>
      <thead>
        <tr>
          <th>{t("claudeDgSession")}</th>
          <th>{t("claudeRedesignModel")}</th>
          <th>{t("claudeDgContext")}</th>
          <th class="num">{t("claudeDgCost")}</th>
        </tr>
      </thead>
      <tbody>
        {#each tiles as s (s.id)}
          {@const u = usage?.sessions[s.id]}
          <tr>
            <td class="title">{s.title}</td>
            <td class="muted">{u?.model ?? "—"}</td>
            <td>{#if s.context}<ContextGauge gauge={s.context} />{/if}</td>
            <td class="num mono">{u?.cost != null ? usd(u.cost) : "—"}</td>
          </tr>
        {/each}
      </tbody>
      <tfoot>
        <tr>
          <td colspan="3">{t("claudeDgTotal")}</td>
          <td class="num mono">{usd(tileCost)}</td>
        </tr>
      </tfoot>
    </table>
    <p class="source muted small"><Info size={13} aria-hidden="true" />{t("claudeDgTilesSource")}</p>
  {:else}
    <p class="empty muted">{t("claudeDgNoTiles")}</p>
  {/if}
</section>

<section class="block" aria-labelledby="claude-headless">
  <h3 id="claude-headless">{t("claudeDgHeadless")} <span class="count">{headless.length}</span></h3>
  {#if headless.length}
    <dl class="totals">
      <div><dt>{t("claudeDgToday")}</dt><dd class="mono">{usd(todayCost)}</dd></div>
      <div><dt>{t("claudeDgAllRuns")}</dt><dd class="mono">{usd(headlessCost)}</dd></div>
    </dl>
    <table>
      <thead>
        <tr>
          <th>{t("claudeDgSession")}</th>
          <th>{t("claudeDgState")}</th>
          <th>{t("claudeDgWhen")}</th>
          <th class="num">{t("claudeDgCost")}</th>
        </tr>
      </thead>
      <tbody>
        {#each headless.slice(0, 12) as a (a.id)}
          <tr>
            <td class="title">{a.title}{#if a.role}<span class="badge role">{a.role}</span>{/if}</td>
            <td class="muted">{a.state}</td>
            <td class="muted">{ago(a.updated)}</td>
            <td class="num mono">{usd(a.cost)}{#if a.budget}<span class="muted"> / {usd(a.budget)}</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="source muted small"><Info size={13} aria-hidden="true" />{t("claudeDgHeadlessSource")}</p>
  {:else}
    <p class="empty muted">{t("claudeDgNoHeadless")}</p>
  {/if}
</section>

<style>
  .page-head {
    margin-bottom: 8px;
  }

  h2 {
    margin: 0 0 4px;
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .lead {
    max-width: 72ch;
    margin: 0;
    color: var(--muted);
  }

  .block {
    margin-top: 26px;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 10px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
    line-height: 16px;
    text-align: center;
  }

  .small {
    font-size: var(--text-xs);
  }

  .empty {
    max-width: 72ch;
    margin: 0;
  }

  .meters {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
  }

  .meter {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .meter-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .meter-label {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .meter-value {
    font-size: var(--text-lg);
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  .bar {
    height: 6px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--hover);
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--done);
  }

  .meter.warn .bar span {
    background: var(--waiting);
  }

  .meter.over .bar span {
    background: var(--danger);
  }

  .source {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 8px 0 0;
  }

  table {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    border-collapse: separate;
    border-spacing: 0;
  }

  th {
    height: 32px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
    text-align: left;
  }

  th:first-child {
    border-top-left-radius: var(--radius-lg);
  }

  th:last-child {
    border-top-right-radius: var(--radius-lg);
  }

  td {
    height: 38px;
    padding: 4px 12px;
    border-bottom: 1px solid var(--border);
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tfoot td {
    border-top: 1px solid var(--border);
    border-bottom: 0;
    font-weight: 600;
  }

  .num {
    width: 120px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .title {
    max-width: 320px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.role {
    margin-left: 8px;
  }

  .totals {
    display: flex;
    gap: 32px;
    margin: 0 0 10px;
  }

  .totals div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .totals dt {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .totals dd {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 650;
  }
</style>
