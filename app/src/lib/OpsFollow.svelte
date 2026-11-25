<script lang="ts">
  import { tick, untrack } from "svelte";
  import Eraser from "@lucide/svelte/icons/eraser";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pause from "@lucide/svelte/icons/pause";
  import Play from "@lucide/svelte/icons/play";
  import Radio from "@lucide/svelte/icons/radio";
  import Square from "@lucide/svelte/icons/square";
  import OpsParams from "$lib/OpsParams.svelte";
  import { t } from "$lib/i18n";
  import { entries, level, runsOn, shows, type Level, type Plan } from "$lib/ops-logic";
  import { ops, resumeFollow, startFollow, stopFollow } from "$lib/ops.svelte";

  let { project, ask }: { project: string; ask: (e: unknown, retry: () => Promise<void>) => boolean } = $props();

  const recipes = $derived(ops.recipes.filter((r) => r.follow));
  const recipe = $derived(recipes.find((r) => r.name === ops.live.recipe) ?? recipes[0]);
  const fitting = $derived(recipe ? ops.connections.filter((c) => runsOn(recipe, c)) : []);
  $effect(() => {
    if (recipe && ops.live.recipe !== recipe.name) untrack(() => (ops.live.recipe = recipe.name));
    if (fitting.length && !fitting.some((c) => c.name === ops.live.connection)) {
      untrack(() => (ops.live.connection = fitting[0].name));
    }
  });

  let starting = $state(false);
  let error = $state("");
  let plan = $state<Plan | null>(null);
  let filter = $state("");
  let min = $state<Level | "all">("all");
  let pane = $state<HTMLDivElement>();
  const following = $derived(ops.live.id !== null);
  // A stack trace folds under the line it belongs to, and shows with it.
  const visible = $derived(entries(ops.live.lines).filter((e) => shows([e.head, ...e.trace].join("\n"), filter.trim(), min)));

  async function start() {
    if (!recipe) return;
    starting = true;
    error = "";
    try {
      const given = Object.fromEntries(Object.entries(ops.live.params).filter(([, v]) => String(v).trim() !== ""));
      plan = await startFollow(project, recipe.name, ops.live.connection, given);
    } catch (e) {
      if (!ask(e, start)) error = String(e);
    } finally {
      starting = false;
    }
  }

  // The newest line stays in sight, unless paused.
  $effect(() => {
    void visible.length;
    if (ops.live.paused || !pane) return;
    tick().then(() => pane && (pane.scrollTop = pane.scrollHeight));
  });

  const levels: [Level | "all", Parameters<typeof t>[0]][] = [
    ["all", "opssqlLiveLevelAll"],
    ["info", "opssqlLiveLevelInfo"],
    ["warn", "opssqlLiveLevelWarn"],
    ["error", "opssqlLiveLevelError"],
  ];
</script>

{#if !recipes.length || !fitting.length}
  <p class="notice none">{t("opssqlLiveNone")}</p>
{:else}
  <div class="live">
    <form
      class="setup"
      onsubmit={(e) => {
        e.preventDefault();
        if (following) stopFollow();
        else start();
      }}
    >
      <p class="intro muted">{t("opssqlLiveIntro")}</p>
      {#if recipes.length > 1}
        <label class="labelled">
          <span>{t("opsTabRecipes")}</span>
          <select class="field mono" bind:value={ops.live.recipe} disabled={following}>
            {#each recipes as r (r.name)}<option value={r.name}>{r.name}</option>{/each}
          </select>
        </label>
      {/if}
      <label class="labelled">
        <span>{t("opsConnection")}</span>
        <select class="field" bind:value={ops.live.connection} disabled={following}>
          {#each fitting as c (c.name)}<option value={c.name}>{c.name} · {c.env}</option>{/each}
        </select>
      </label>
      {#if recipe}
        <fieldset disabled={following}><OpsParams params={recipe.params} bind:values={ops.live.params} /></fieldset>
      {/if}
      {#if following}
        <button class="btn danger"><Square size={14} aria-hidden="true" />{t("opssqlLiveStop")}</button>
      {:else}
        <button class="btn primary" disabled={starting}>
          {#if starting}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Radio size={14} aria-hidden="true" />{/if}{t(
            "opssqlLiveStart",
          )}
        </button>
      {/if}
      {#if error}<p class="notice error">{error}</p>{/if}
      {#if plan && following}<pre class="command">{plan.remote ?? plan.command}</pre>{/if}
    </form>

    <section class="stream" aria-label={t("opssqlLive")}>
      <div class="toolbar">
        <input class="field filter" type="search" placeholder={t("opssqlLiveFilter")} aria-label={t("opssqlLiveFilter")} bind:value={filter} />
        <select class="field" bind:value={min} aria-label={t("opssqlLiveLevel")}>
          {#each levels as [value, label] (value)}<option {value}>{t(label)}</option>{/each}
        </select>
        {#if ops.live.paused}
          <button class="btn sm" onclick={resumeFollow}>
            <Play size={13} aria-hidden="true" />{t("opssqlLiveResume", { count: ops.live.held.length })}
          </button>
        {:else}
          <button class="btn sm" onclick={() => (ops.live.paused = true)} disabled={!following}>
            <Pause size={13} aria-hidden="true" />{t("opssqlLivePause")}
          </button>
        {/if}
        <button class="btn sm" onclick={() => ((ops.live.lines = []), (ops.live.held = []))}>
          <Eraser size={13} aria-hidden="true" />{t("opssqlLiveClear")}
        </button>
        <span class="end muted count">
          {#if following}<span class="dot" aria-hidden="true"></span>{/if}
          {t("opssqlLiveCount", { shown: visible.length, total: ops.live.lines.length })}
        </span>
      </div>
      <div class="lines" bind:this={pane} role="log" aria-live="off">
        {#each visible as entry, i (i)}
          {@const lv = level(entry.head)}
          {#if entry.trace.length}
            <details class="line {lv ?? ''}"><summary>{entry.head}</summary><div class="trace">{entry.trace.join("\n")}</div></details>
          {:else}
            <div class="line {lv ?? ''}">{entry.head}</div>
          {/if}
        {:else}
          <p class="muted empty">
            {following ? t("opssqlLiveWaiting") : ops.live.ended ? t("opssqlLiveEnded", { code: ops.live.ended.exit ?? "—" }) : t("opssqlLiveIdle")}
          </p>
        {/each}
      </div>
      {#if ops.live.ended?.errors.trim()}<pre class="errors">{ops.live.ended.errors}</pre>{/if}
    </section>
  </div>
{/if}

<style>
  .none {
    margin: 20px 28px;
  }

  .live {
    display: grid;
    flex: 1;
    grid-template-columns: 340px minmax(0, 1fr);
    min-height: 0;
  }

  .setup {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    padding: 16px 18px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .intro {
    margin: 0;
    font-size: var(--text-sm);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  .command {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .stream {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .toolbar .field {
    width: auto;
  }

  .toolbar .filter {
    width: 240px;
  }

  .end {
    margin-left: auto;
  }

  .count {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-variant-numeric: tabular-nums;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--done);
  }

  .lines {
    flex: 1;
    min-height: 0;
    padding: 8px 0;
    overflow-y: auto;
    background: var(--panel);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.6;
  }

  .line {
    padding: 0 16px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .line:hover {
    background: var(--hover);
  }

  details.line summary {
    cursor: pointer;
  }

  .trace {
    padding-left: 16px;
    color: var(--muted);
  }

  .line.error {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .line.warn {
    color: var(--warning);
  }

  .line.debug {
    color: var(--muted);
  }

  .empty {
    margin: 0;
    padding: 20px 16px;
    font-family: var(--font-ui);
    font-size: var(--text-sm);
  }

  .errors {
    max-height: 120px;
    margin: 0;
    padding: 8px 16px;
    overflow-y: auto;
    border-top: 1px solid var(--border);
    color: var(--danger);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
  }
</style>
