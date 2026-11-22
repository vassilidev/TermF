<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import CirclePlay from "@lucide/svelte/icons/circle-play";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import FilePen from "@lucide/svelte/icons/file-pen";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Action from "$lib/Action.svelte";
  import { stepName } from "$lib/TeamRunDetail.svelte";
  import { agents, apiUsd, showConversation } from "$lib/agents.svelte";
  import { lang, t } from "$lib/i18n";
  import { activity, calls, track } from "$lib/live.svelte";
  import { elapsed, feed } from "$lib/live-logic";
  import type { Step, TeamRun } from "$lib/team-run-logic";
  import { costHelp } from "$lib/usage.svelte";

  /** A team run as it happens: what each step at work does now, then the run's story in plain words. */
  let { run }: { run: TeamRun } = $props();

  // A foreach only holds its tasks: they are what works.
  const running = $derived(run.steps.filter((s) => s.state === "running" && s.kind !== "foreach"));
  $effect(() => {
    for (const s of running) if (s.session !== null) track(s.session);
  });

  // "for 3 min" moves on while a step runs, and only then.
  let now = $state(Date.now() / 1000);
  $effect(() => {
    if (!running.length) return;
    now = Date.now() / 1000;
    const timer = setInterval(() => (now = Date.now() / 1000), 10_000);
    return () => clearInterval(timer);
  });

  const moments = $derived(feed(run.steps, calls));
  const FEED = 8;
  let all = $state(false);
  const told = $derived(all ? moments : moments.slice(-FEED));

  const who = (role: string | null) => role ?? "TermF";
  const named = (key: string) => {
    const step = run.steps.find((s) => s.key === key);
    return step ? stepName(step) : key;
  };
  const say = (p: { key: string; vars?: Record<string, string | number> }) => t(p.key as Parameters<typeof t>[0], p.vars);
  const clock = (at: number) => new Date(at * 1000).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });
  /** The step's cost so far: its session's, which grows each turn, until the step adds it up. */
  const spent = (step: Step) => Math.max(step.cost, agents.sessions.find((a) => a.id === step.session)?.cost ?? 0);
</script>

{#if running.length}
  <section class="panel" aria-labelledby="run-now">
    <h3 id="run-now"><span class="pulse" aria-hidden="true"></span>{t("liveNow")}</h3>
    <ul class="now">
      {#each running as step (step.key)}
        {@const act = step.session === null ? null : activity[step.session]?.now}
        {@const cost = spent(step)}
        {@const session = step.session}
        <li>
          <span class="role">{who(step.role)}</span>
          <span class="what">
            <b>{named(step.key)}</b>
            {#if act}<span class="act"><Action {act} /></span>{/if}
          </span>
          <span class="meta">
            {#if step.started}{t("liveSince", { time: say(elapsed(now - step.started)) })}{/if}
            {#if cost}<span title={costHelp()}> · {apiUsd(cost)}</span>{/if}
          </span>
          {#if session !== null}
            <button class="btn sm" onclick={() => showConversation(session)}>
              <MessagesSquare size={14} aria-hidden="true" />{t("liveOpen")}
            </button>
          {/if}
        </li>
      {/each}
    </ul>
  </section>
{/if}

<section class="panel" aria-labelledby="run-feed">
  <h3 id="run-feed">{t("liveFeed")}</h3>
  {#if told.length < moments.length}
    <button class="btn link earlier" onclick={() => (all = true)}>{t("liveMore", { count: moments.length })}</button>
  {/if}
  <ol class="feed">
    {#each told as m}
      <li class={m.kind === "end" ? m.how : m.kind === "act" ? m.act.state : ""}>
        <time datetime={new Date(m.at * 1000).toISOString()}>{clock(m.at)}</time>
        {#if m.kind === "start"}
          <CirclePlay size={14} aria-hidden="true" />
          <span class="said">{t("liveFeedStart", { who: who(m.role), step: named(m.step) })}</span>
        {:else if m.kind === "end"}
          {#if m.how === "done"}<CircleCheck size={14} aria-hidden="true" />{:else if m.how === "failed"}<CircleX
              size={14}
              aria-hidden="true"
            />{:else}<TriangleAlert size={14} aria-hidden="true" />{/if}
          <span class="said">{t(m.how === "done" ? "liveFeedDone" : m.how === "failed" ? "liveFeedFailed" : "liveFeedBlocked", { step: named(m.step) })}</span>
        {:else if m.kind === "edits"}
          <FilePen size={14} aria-hidden="true" />
          <span class="said"><b>{who(m.role)}</b>{t("liveEditMany", { count: m.files.length })}</span>
        {:else}
          {#if m.act.state === "running"}<LoaderCircle size={14} aria-label={t("chatToolRunning")} class="spin" />{:else if m.act.state === "failed"}<CircleX
              size={14}
              aria-label={t("chatToolFailed")}
            />{:else}<Check size={14} aria-hidden="true" />{/if}
          <span class="said"><b>{who(m.role)}</b><Action act={m.act} /></span>
        {/if}
      </li>
    {:else}
      <li class="muted">{t("liveFeedEmpty")}</li>
    {/each}
  </ol>
</section>

<style>
  .panel {
    container-type: inline-size;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 10px;
    font-size: var(--text-base);
    font-weight: 650;
  }

  .pulse {
    position: relative;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    background: var(--working);
  }

  /* A ring that widens and fades, on the compositor: a long run costs no repaint. */
  .pulse::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: var(--working);
    animation: ring 2s var(--ease-out) infinite;
  }

  @keyframes ring {
    from {
      opacity: 0.5;
      transform: scale(1);
    }
    to {
      opacity: 0;
      transform: scale(2.8);
    }
  }

  .now {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .now li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    border-top: 1px solid var(--border);
  }

  .now li:first-child {
    padding-top: 0;
    border-top: 0;
  }

  .role {
    padding: 1px 8px;
    border-radius: var(--radius-pill);
    background: var(--working-soft);
    color: var(--working);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .what b {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .act {
    display: flex;
    min-width: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .meta {
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .earlier {
    margin-bottom: 6px;
  }

  .feed {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  /* A long story only draws what is on screen. */
  .feed li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    content-visibility: auto;
    contain-intrinsic-size: auto 20px;
  }

  .feed li > :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .feed li.done > :global(svg) {
    color: var(--done);
  }

  .feed li.failed > :global(svg),
  .feed li.blocked > :global(svg) {
    color: var(--danger);
  }

  .feed li.running > :global(svg) {
    color: var(--working);
  }

  time {
    flex-shrink: 0;
    width: 40px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .said {
    display: flex;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .said b {
    flex-shrink: 0;
    font-weight: 600;
  }

  @media (prefers-reduced-motion: reduce) {
    .pulse::after {
      display: none;
    }
  }

  @container (max-width: 560px) {
    .now li {
      grid-template-columns: auto minmax(0, 1fr) auto;
    }

    .meta {
      display: none;
    }
  }
</style>
