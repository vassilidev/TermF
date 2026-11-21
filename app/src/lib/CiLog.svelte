<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import ChevronUp from "@lucide/svelte/icons/chevron-up";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import { t } from "$lib/i18n";
  import type { Job, JobLog } from "$lib/ci.svelte";
  import { cycle } from "$lib/ci-logic";

  let { repo, job }: { repo: string; job: Job } = $props();

  let log = $state<JobLog | null>(null);
  let error = $state("");
  let query = $state("");
  /** Index in `matches` of the match shown. */
  let at = $state(-1);
  /** Index in `log.errors` of the error shown. */
  let errorAt = $state(-1);
  /** The line brought into view. */
  let current = $state<number | null>(null);
  let box = $state<HTMLElement>();

  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!log || !q) return [];
    const found: number[] = [];
    log.lines.forEach((line, i) => line.toLowerCase().includes(q) && found.push(i));
    return found;
  });
  const matched = $derived(new Set(matches));
  const errors = $derived(new Set(log?.errors ?? []));

  async function load() {
    error = "";
    log = null;
    try {
      log = await invoke<JobLog>("ci_log", { repo, job: job.id });
      // The first error is usually the cause; the last one only says the step failed.
      if (log.errors.length) nextError();
    } catch (e) {
      error = String(e);
    }
  }

  onMount(load);

  async function show(line: number) {
    current = line;
    await tick();
    // Only the log scrolls: scrollIntoView would also move the window's other scroll areas.
    const el = box?.querySelector<HTMLElement>(`[data-line="${line}"]`);
    if (box && el) box.scrollTop = el.offsetTop - box.clientHeight / 2;
  }

  function nextMatch(direction: 1 | -1) {
    if (!matches.length) return;
    at = cycle(at, direction, matches.length);
    show(matches[at]);
  }

  function nextError() {
    if (!log?.errors.length) return;
    errorAt = cycle(errorAt, 1, log.errors.length);
    show(log.errors[errorAt]);
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    e.preventDefault();
    nextMatch(e.shiftKey ? -1 : 1);
  }
</script>

<div class="log" role="region" aria-label={t("ciLogOf", { job: job.name })}>
  <div class="bar">
    <label class="search">
      <Search size={13} aria-hidden="true" />
      <input
        class="field"
        placeholder={t("ciLogSearch")}
        aria-label={t("ciLogSearch")}
        spellcheck="false"
        bind:value={query}
        oninput={() => (at = -1)}
        onkeydown={onSearchKey}
        disabled={!log}
      />
    </label>
    {#if query.trim() && log}
      <span class="count" aria-live="polite">
        {matches.length ? t("ciLogMatch", { at: Math.max(at, 0) + 1, count: matches.length }) : t("ciLogNoMatch")}
      </span>
      <button class="icon-btn sm" onclick={() => nextMatch(-1)} disabled={!matches.length} aria-label={t("ciLogPrevious")} title={t("ciLogPrevious")}>
        <ChevronUp size={14} aria-hidden="true" />
      </button>
      <button class="icon-btn sm" onclick={() => nextMatch(1)} disabled={!matches.length} aria-label={t("ciLogNext")} title={t("ciLogNext")}>
        <ChevronDown size={14} aria-hidden="true" />
      </button>
    {/if}
    {#if log?.errors.length}
      <button class="btn sm error-btn" onclick={nextError}>
        <CircleX size={13} aria-hidden="true" />{t("ciLogNextError", { count: log.errors.length })}
      </button>
    {/if}
  </div>
  {#if error}
    <div class="state">
      <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{error}</p>
      <button class="btn sm" onclick={load}><RefreshCw size={13} aria-hidden="true" />{t("ciRetry")}</button>
    </div>
  {:else if !log}
    <p class="state muted" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("ciLogLoading")}</p>
  {:else if !log.lines.length}
    <p class="state muted">{t("ciLogEmpty")}</p>
  {:else}
    {#if log.skipped}<p class="skipped muted">{t("ciLogSkipped", { count: log.skipped })}</p>{/if}
    <div class="lines" bind:this={box}>
      {#each log.lines as line, i (i)}
        <div
          class="line"
          class:error={errors.has(i)}
          class:match={matched.has(i)}
          class:current={current === i}
          data-line={i}
        >
          <span class="n">{log.skipped + i + 1}</span><span class="text">{line}</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .log {
    margin: 4px 0 8px 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .search {
    position: relative;
    display: flex;
    flex: 0 1 280px;
    color: var(--muted);
  }

  .search > :global(svg) {
    position: absolute;
    top: 7px;
    left: 8px;
    pointer-events: none;
  }

  .search .field {
    height: 26px;
    padding-left: 26px;
  }

  .count {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .error-btn {
    margin-left: auto;
    color: var(--danger);
  }

  .state {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding: 12px;
    font-size: var(--text-sm);
  }

  .state .notice {
    flex: 1;
    margin: 0;
  }

  .skipped {
    margin: 0;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
  }

  .lines {
    position: relative;
    max-height: 440px;
    padding: 4px 0;
    overflow: auto;
    background: var(--panel);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.6;
  }

  .line {
    display: flex;
    min-width: max-content;
    padding-right: 12px;
  }

  .n {
    flex-shrink: 0;
    width: 56px;
    padding-right: 10px;
    color: var(--muted);
    text-align: right;
    user-select: none;
    font-variant-numeric: tabular-nums;
  }

  .text {
    white-space: pre;
  }

  .line.match {
    background: var(--waiting-soft);
  }

  .line.error {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .line.current {
    box-shadow: inset 3px 0 0 var(--accent);
  }
</style>
