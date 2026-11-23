<script lang="ts">
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import LaneHead from "$lib/LaneHead.svelte";
  import TermOut from "$lib/TermOut.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import StreamSteps, { type Step as Line } from "$lib/ui/StreamSteps.svelte";
  import { agents, showConversation } from "$lib/agents.svelte";
  import { lang, t } from "$lib/i18n";
  import { activity, calls, track } from "$lib/live.svelte";
  import { elapsed, phrase, type ToolAct } from "$lib/live-logic";
  import { repoName } from "$lib/state.svelte";
  import type { Step, TeamRun } from "$lib/team-run-logic";
  import { laneOf } from "$lib/team.svelte";

  /**
   * The lanes of a team run's agents at work, one per step: who, on what, since when, then what it does in plain
   * words, its earlier actions folded in one sentence, a failure with what the terminal printed, the action under way
   * last. An agent whose session asks for a permission comes first: it waits for the user.
   */
  let { run }: { run: TeamRun } = $props();

  // A foreach only holds its tasks: they are what works. A paused or stopped run keeps its step "running" to take it
  // up again, but nothing works on it meanwhile.
  const active = $derived(run.state === "running" || run.state === "waiting");
  const asking = (s: Step) => s.session !== null && agents.sessions.find((a) => a.id === s.session)?.state === "waiting";
  const running = $derived(
    (active ? run.steps.filter((s) => s.state === "running" && s.kind !== "foreach") : []).sort((a, b) => Number(asking(b)) - Number(asking(a))),
  );
  $effect(() => {
    for (const s of running) if (s.session !== null) track(s.session);
  });

  let now = $state(Date.now() / 1000);
  $effect(() => {
    if (!running.length) return;
    now = Date.now() / 1000;
    const timer = setInterval(() => (now = Date.now() / 1000), 10_000);
    return () => clearInterval(timer);
  });

  const clock = (at: number) => new Date(at * 1000).toLocaleTimeString(lang, { hour: "2-digit", minute: "2-digit" });

  function line(act: ToolAct): Line {
    const p = phrase(act.name, act.input);
    const verb = t(p.key as Parameters<typeof t>[0], { ...p.vars, target: "" }).replace(/[«“"]\s*[»”"]/g, "").replace(/\s+/g, " ").trim();
    const state = act.state === "running" ? "now" : act.state === "failed" ? "fail" : "ok";
    return { verb, object: p.target || undefined, at: clock(act.at), state };
  }

  function folded(lines: Line[]): string {
    const first = lines.slice(0, 2).map((l) => [l.verb, l.object].filter(Boolean).join(" "));
    const list = `${first.join(", ")}${lines.length > 2 ? "…" : ""}`;
    return lines.length === 1 ? t("aigFoldedOne", { first: list }) : t("aigFolded", { n: lines.length, first: list });
  }

  /** The earlier actions, folded; from the last failure on (or only the action under way), in full. */
  function stream(step: Step) {
    if (step.session === null) return null;
    const acts = calls(step.session);
    const fail = acts.findLastIndex((a) => a.state === "failed");
    const cut = fail >= 0 && fail >= acts.length - 3 ? fail : acts.findIndex((a) => a.state === "running");
    const split = cut < 0 ? acts.length : cut;
    const live = acts.slice(split).map(line);
    const act = activity[step.session]?.now;
    if (act && act.kind !== "tool") live.push({ verb: t(act.kind === "thinking" ? "liveThinking" : "liveWriting"), state: "now" });
    return { earlier: acts.slice(0, split).map(line), live, failed: fail >= split ? acts[fail] : undefined };
  }

  function meta(step: Step): string {
    if (asking(step)) return t("uiRunAsksPermission");
    const repo = run.plan?.tasks.find((x) => x.id === step.task)?.repo || repoName(run.target.repo);
    if (!step.started) return repo;
    const { key, vars } = elapsed(now - step.started);
    return `${repo} · ${t("liveSince", { time: t(key as Parameters<typeof t>[0], vars) })}`;
  }

  const commandOf = (act: ToolAct) => {
    const input = act.input as { command?: unknown } | null;
    return typeof input?.command === "string" ? input.command : undefined;
  };
</script>

{#each running as step (step.key)}
  {@const who = laneOf(run, step)}
  {@const asks = asking(step)}
  {@const flow = stream(step)}
  {@const session = step.session}
  <article class="lane" class:asks>
    <LaneHead role={who.role} title={who.title} meta={meta(step)} status={asks ? "waiting" : "working"}>
      {#if session !== null}
        <button class="btn sm" class:quiet={!asks} class:primary={asks} onclick={() => showConversation(session)}>
          <MessagesSquare size={14} aria-hidden="true" />{t(asks ? "uiRunAnswer" : "chatThread")}
        </button>
      {/if}
      {#if !asks}<StatusPill status="working" />{/if}
    </LaneHead>
    {#if flow && (flow.earlier.length || flow.live.length)}
      <div class="body">
        {#if flow.earlier.length}<StreamSteps steps={flow.earlier} summary={folded(flow.earlier)} />{/if}
        {#if flow.live.length}<StreamSteps steps={flow.live} live />{/if}
        {#if flow.failed?.output}<TermOut text={flow.failed.output} command={commandOf(flow.failed)} />{/if}
      </div>
    {/if}
  </article>
{/each}

<style>
  .lane {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .lane.asks {
    border-color: var(--waiting-line);
    background: var(--waiting-soft);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding-left: 46px;
  }

  .body :global(.out) {
    margin-top: 4px;
  }
</style>
