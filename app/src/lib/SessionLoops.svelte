<script module lang="ts">
  import { ago, t } from "$lib/i18n";
  import { scheduleLoop, when } from "$lib/schedule.svelte";
  import type { Session } from "$lib/state.svelte";
  import { interval, loopEvery, nextRun, toCron, type Loop } from "$lib/tile-logic";

  const span = (seconds: number) => {
    const { n, unit } = interval(seconds);
    return t(unit === "min" ? "loopsEveryMin" : unit === "h" ? "loopsEveryH" : "loopsEveryD", { n });
  };

  /** What the tile's pill says of a loop: its interval. */
  export function everyLabel(l: Loop): string {
    if (l.kind === "monitor") return t("loopsWatch");
    if (l.kind === "wakeup") return t("loopsAuto");
    const every = loopEvery(l);
    return every ? span(every) : (l.cron ?? t("loopsAuto"));
  }

  export function kindLabel(l: Loop): string {
    if (l.kind === "monitor") return t("loopsKindMonitor");
    if (!l.recurring) return t("loopsKindOnce");
    return l.fromLoop || l.kind === "wakeup" ? t("loopsKindLoop") : t("loopsKindCron");
  }

  /** "every 5 min · next run … · created 3 min ago". */
  export function detail(l: Loop, now: number): string {
    const every = loopEvery(l);
    const pace =
      l.kind === "monitor"
        ? ""
        : l.kind === "wakeup"
          ? l.every
            ? t("loopsIntervalAutoLast", { every: span(l.every) })
            : t("loopsIntervalAuto")
          : every && l.recurring
            ? t("loopsInterval", { every: span(every) })
            : (l.cron ?? "");
    const next = nextRun(l, now);
    return [
      pace,
      next !== null && t("loopsNext", { when: when(next / 1000) }),
      l.kind === "monitor" && l.ends !== null && t("loopsUntil", { when: when(l.ends / 1000) }),
      t("loopsCreated", { when: ago(l.created / 1000) }),
    ]
      .filter(Boolean)
      .join(" · ");
  }

  /** The Scheduler's form for a new task that does what the loop does, or null when it cannot. */
  export function toTask(session: Session, l: Loop): (() => void) | null {
    const cron = toCron(l);
    return cron ? () => scheduleLoop({ prompt: l.prompt, dir: session.cwd, cron }) : null;
  }
</script>

<script lang="ts">
  import CalendarPlus from "@lucide/svelte/icons/calendar-plus";
  import Eye from "@lucide/svelte/icons/eye";
  import Info from "@lucide/svelte/icons/info";
  import Repeat from "@lucide/svelte/icons/repeat";
  import Popover from "$lib/ui/Popover.svelte";
  import { pill } from "$lib/tile-logic";

  /** `loops`: the session's, still on. */
  let { session, loops }: { session: Session; loops: Loop[] } = $props();

  let open = $state(false);
  let trigger = $state<HTMLButtonElement>();
  const lead = $derived(pill(loops));
  const summary = $derived(loops.map((l) => `${kindLabel(l)} · ${everyLabel(l)} · ${l.prompt || t("loopsDefaultPrompt")}`).join("\n"));
</script>

{#if lead}
  <button
    bind:this={trigger}
    class="loop-pill"
    aria-expanded={open}
    aria-label={t("loopsPill")}
    title={`${t("loopsPill")}\n${summary}`}
    onclick={() => (open = !open)}
  >
    {#if lead.lead.kind === "monitor"}<Eye size={12} aria-hidden="true" />{:else}<Repeat size={12} aria-hidden="true" />{/if}
    {everyLabel(lead.lead)}{#if lead.more}<span class="more">+{lead.more}</span>{/if}
  </button>
  <Popover bind:open {trigger} label={t("loopsPill")} align="end" width={340}>
    {@const now = Date.now()}
    <ul class="loops">
      {#each loops as l (l.id)}
        {@const make = toTask(session, l)}
        <li>
          <span class="kind">
            {#if l.kind === "monitor"}<Eye size={13} aria-hidden="true" />{:else}<Repeat size={13} aria-hidden="true" />{/if}
            <b>{kindLabel(l)}</b>
          </span>
          <span class="prompt">{l.prompt || t("loopsDefaultPrompt")}</span>
          <span class="meta">{detail(l, now)}</span>
          {#if make}
            <button class="btn sm" onclick={() => ((open = false), make())}>
              <CalendarPlus size={14} aria-hidden="true" />{t("loopsToTask")}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
    <p class="reminder"><Info size={14} aria-hidden="true" />{t("loopsReminder")}</p>
  </Popover>
{/if}

<style>
  .loop-pill {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: none;
    color: var(--text);
    font: 600 var(--text-xs) var(--font-ui);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    cursor: pointer;
  }

  .loop-pill:hover {
    border-color: var(--accent);
  }

  .loop-pill :global(svg) {
    color: var(--accent-ink);
  }

  .more {
    color: var(--muted);
  }

  .loops {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .loops li {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }

  .kind {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .kind :global(svg) {
    color: var(--accent-ink);
  }

  .prompt {
    display: -webkit-box;
    overflow: hidden;
    font: var(--text-sm) / 1.45 var(--font-mono);
    overflow-wrap: anywhere;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  .meta {
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .loops .btn {
    margin-top: 4px;
  }

  .reminder {
    display: flex;
    gap: 8px;
    margin: 0;
    padding: 10px 12px;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .reminder :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }
</style>
