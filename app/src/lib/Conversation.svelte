<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import Check from "@lucide/svelte/icons/check";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CirclePause from "@lucide/svelte/icons/circle-pause";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import ShieldQuestionMark from "@lucide/svelte/icons/shield-question-mark";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import Square from "@lucide/svelte/icons/square";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import UserRound from "@lucide/svelte/icons/user-round";
  import ContextGauge from "$lib/ContextGauge.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { upsert, usd, type AgentSession } from "$lib/agents.svelte";
  import { focusSession } from "$lib/commands";
  import {
    blocked,
    merge,
    pending,
    stream,
    summary,
    thread,
    type End,
    type Item,
    type Permission,
    type StoredEvent,
    type Tool,
  } from "$lib/conversation-logic";
  import { t } from "$lib/i18n";
  import { app, shortPath, ui, type Session } from "$lib/state.svelte";

  let { session }: { session: AgentSession } = $props();

  let events = $state.raw<StoredEvent[]>([]);
  let live = $state<string | null>(null);
  let draft = $state("");
  let reasons = $state<Record<string, string>>({});
  let busy = $state("");
  let error = $state("");
  let loadError = $state("");
  let scroller = $state<HTMLElement>();
  let form = $state<HTMLFormElement>();

  const turns = $derived(thread(events));
  const init = $derived(events.findLast((e) => e.kind === "init"));
  const why = $derived(blocked(session.state));
  const turning = $derived(session.state === "working" || session.state === "waiting");
  const asking = $derived(session.state === "waiting" ? pending(events) : null);

  const efforts: Record<string, Parameters<typeof t>[0]> = {
    low: "schedulerEffortLow",
    medium: "schedulerEffortMedium",
    high: "schedulerEffortHigh",
    xhigh: "schedulerEffortXhigh",
    max: "schedulerEffortMax",
  };
  const modes: Record<string, Parameters<typeof t>[0]> = {
    manual: "schedulerPermManual",
    acceptEdits: "schedulerPermAcceptEdits",
    plan: "schedulerPermPlan",
    auto: "schedulerPermAuto",
    dontAsk: "schedulerPermDontAsk",
    bypassPermissions: "schedulerPermBypass",
  };
  const reasonsWhy = {
    working: "chatWhyWorking",
    waiting: "chatWhyWaiting",
    interactive: "chatWhyInteractive",
    budget: "chatWhyBudget",
  } as const;
  const outcomes = {
    interrupted: ["stopped", "chatOutcomeInterrupted"],
    error: ["error", "chatOutcomeError"],
    budget: ["budget", "chatOutcomeBudget"],
  } as const;

  const model = $derived(session.model || (init?.kind === "init" ? init.model : ""));
  const mode = $derived(session.permissionMode || (init?.kind === "init" ? init.permissionMode : ""));
  const left = $derived(session.budget === null ? null : Math.max(0, session.budget - session.cost));

  const atBottom = () => !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;

  async function follow(stick: boolean) {
    if (!stick) return;
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight });
  }

  // Back on the view: what came meanwhile is at the bottom.
  $effect(() => {
    if (ui.view === "conversations") follow(true);
  });

  // Heard while what is stored loads, or failed to load: `merge` drops what both give.
  let buffer: StoredEvent[] | null = [];

  async function load() {
    loadError = "";
    try {
      const stored = await invoke<StoredEvent[]>("agent_events", { id: session.id });
      events = merge(stored, buffer ?? []);
      live = (buffer ?? []).reduce(stream, null);
      buffer = null;
      follow(true);
    } catch (e) {
      loadError = String(e);
    }
  }

  onMount(() => {
    const id = session.id;
    const off = listen<StoredEvent>("agent-event", ({ payload }) => {
      if (payload.session !== id) return;
      if (buffer) return void buffer.push(payload);
      const stick = atBottom();
      live = stream(live, payload);
      if (payload.id) events = merge(events, [payload]);
      follow(stick);
    });
    // Listening first, then reading what is stored: nothing falls in between.
    off.then(load);
    return () => off.then((stop) => stop());
  });

  /** Runs one action at a time; the backend's refusal is shown as it is. */
  async function run<T>(name: string, action: () => Promise<T>): Promise<T | undefined> {
    busy = name;
    error = "";
    try {
      return await action();
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  const id = () => session.id;

  async function send(e: SubmitEvent) {
    e.preventDefault();
    const text = draft.trim();
    if (!text || why || busy) return;
    const sent = await run("send", () => invoke("agent_send", { id: id(), text }).then(() => true));
    if (sent) {
      draft = "";
      follow(true);
    }
  }

  function enter(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      form?.requestSubmit();
    }
  }

  async function settle(name: string, command: string) {
    const updated = await run(name, () => invoke<AgentSession>(command, { id: id() }));
    if (updated) upsert(updated);
  }

  async function takeover() {
    const tile = await run("takeover", () => invoke<Session>("agent_takeover", { id: id() }));
    if (!tile) return;
    if (!app.sessions.some((s) => s.id === tile.id)) app.sessions.push(tile);
    focusSession(tile.id);
  }

  function answer(ask: Permission, allow: boolean) {
    const message = allow ? null : reasons[ask.request]?.trim() || null;
    run("answer", () => invoke("agent_answer", { id: id(), request: ask.request, allow, message }));
  }

  const pretty = (value: unknown) => (typeof value === "string" ? value : JSON.stringify(value, null, 2));
  const firstLine = (text: string) => text.trim().split("\n")[0];
</script>

{#snippet entry(item: Item, now: boolean)}
  {#if item.type === "text"}
    <p class="text">{item.text}</p>
  {:else if item.type === "tool"}
    {@render tool(item, now)}
  {:else if item.type === "permission"}
    {@render ask(item)}
  {:else if item.type === "denied"}
    <p class="sys failed">
      <ShieldX size={14} aria-hidden="true" />
      <span>{t("chatDenied", { tool: item.tool })}<span class="muted">{" · "}{item.message}</span></span>
    </p>
  {:else if item.type === "error"}
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{t("chatProcessError", { message: item.message })}</span></p>
  {:else if item.type === "taken_over"}
    <p class="sys"><SquareTerminal size={14} aria-hidden="true" />{t("chatTakenOver")}</p>
  {:else if item.type === "handed_back"}
    <p class="sys"><Undo2 size={14} aria-hidden="true" />{t("chatHandedBack")}</p>
  {/if}
{/snippet}

{#snippet tool(item: Tool, now: boolean)}
  {@const failed = item.result?.isError}
  <details class="tool" class:failed>
    <summary>
      <span class="call">
        <ChevronRight size={14} aria-hidden="true" class="chevron" />
        {#if item.result === null}
          {#if now}
            <LoaderCircle size={14} aria-label={t("chatToolRunning")} class="spin" />
          {:else}
            <CircleDashed size={14} aria-label={t("chatToolNoResult")} />
          {/if}
        {:else if failed}
          <CircleX size={14} aria-label={t("chatToolFailed")} />
        {:else}
          <Check size={14} aria-hidden="true" />
        {/if}
        <b>{item.name}</b>
        <span class="mono sum">{summary(item.input)}</span>
        {#if item.children.length}<span class="badge">{t("chatSubagent", { count: item.children.length })}</span>{/if}
      </span>
      {#if failed && item.result}<span class="mono why-failed">{firstLine(item.result.output)}</span>{/if}
    </summary>
    <div class="body">
      <p class="label">{t("chatInput")}</p>
      <pre class="mono code">{pretty(item.input)}</pre>
      {#if item.children.length}
        <div class="nested">
          {#each item.children as child (child.key)}{@render entry(child, now)}{/each}
        </div>
      {/if}
      {#if item.result}
        <p class="label">{t("chatOutput")}</p>
        <pre class="mono code" class:error={failed}>{item.result.output}</pre>
        {#if item.result.truncated}<p class="cut">{t("chatTruncated")}</p>{/if}
      {/if}
    </div>
  </details>
{/snippet}

{#snippet ask(item: Permission)}
  {#if item.request !== asking}
    <p class="sys" class:failed={item.answer && !item.answer.allow}>
      {#if item.answer?.allow}<ShieldCheck size={14} aria-hidden="true" />{:else if item.answer}<ShieldX
          size={14}
          aria-hidden="true"
        />{:else}<ShieldQuestionMark size={14} aria-hidden="true" />{/if}
      <span>
        {t("chatPermission", { tool: item.tool })} ·
        {#if item.answer?.allow}{t("chatAllowed")}{:else if item.answer?.message}{t("chatRefusedWhy", {
            message: item.answer.message,
          })}{:else if item.answer}{t("chatRefused")}{:else}{t("chatUnanswered")}{/if}
      </span>
    </p>
  {:else}
    <div class="ask" role="group" aria-label={t("chatPermission", { tool: item.tool })}>
      <p class="ask-title">
        <ShieldQuestionMark size={16} aria-hidden="true" />{t("chatPermission", { tool: item.tool })}
      </p>
      {#if item.description}<p class="ask-text">{item.description}</p>{/if}
      <pre class="mono code">{pretty(item.input)}</pre>
      <div class="answer">
        <input
          class="field"
          placeholder={t("chatReason")}
          aria-label={t("chatReason")}
          bind:value={() => reasons[item.request] ?? "", (v) => (reasons[item.request] = v)}
        />
        <button class="btn sm" disabled={!!busy} onclick={() => answer(item, false)}>
          <ShieldX size={14} aria-hidden="true" />{t("chatDeny")}
        </button>
        <button class="btn sm primary" disabled={!!busy} onclick={() => answer(item, true)}>
          <ShieldCheck size={14} aria-hidden="true" />{t("chatAllow")}
        </button>
      </div>
    </div>
  {/if}
{/snippet}

{#snippet ending(end: End)}
  <div class="end">
    {#if end.outcome !== "done"}
      {@const [status, label] = outcomes[end.outcome]}
      <StatusPill {status} label={t(label)} />
    {/if}
    <span>{t("chatTurnCost", { cost: usd(end.cost), total: usd(end.total) })}</span>
    {#if end.context}<ContextGauge gauge={end.context} />{/if}
  </div>
  {#if end.outcome === "error" && end.error}
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{end.error}</span></p>
  {/if}
{/snippet}

<header class="head">
  <div class="top">
    <h2>{session.title}</h2>
    {#if session.role}<span class="role"><UserRound size={12} aria-hidden="true" />{session.role}</span>{/if}
    <StatusPill status={session.state} />
    <div class="actions">
      {#if turning}
        <button class="btn sm" disabled={!!busy} onclick={() => run("interrupt", () => invoke("agent_interrupt", { id: id() }))}>
          <CirclePause size={14} aria-hidden="true" />{t("chatInterrupt")}
        </button>
      {/if}
      {#if session.state === "idle" || session.state === "stopped" || session.state === "error"}
        <button class="btn sm" disabled={!!busy} onclick={takeover} title={t("chatTakeoverHelp")}>
          {#if busy === "takeover"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<SquareTerminal
              size={14}
              aria-hidden="true"
            />{/if}{t("chatTakeover")}
        </button>
      {/if}
      {#if session.state === "interactive"}
        {#if session.tile !== null}
          <button class="btn sm" onclick={() => focusSession(session.tile!)}>
            <ArrowRight size={14} aria-hidden="true" />{t("chatGoToTile")}
          </button>
        {/if}
        <button class="btn sm" disabled={!!busy} onclick={() => settle("handback", "agent_handback")}>
          {#if busy === "handback"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Undo2
              size={14}
              aria-hidden="true"
            />{/if}{t("chatHandback")}
        </button>
      {/if}
      {#if turning || session.state === "idle"}
        <button class="btn sm" disabled={!!busy} onclick={() => settle("stop", "agent_stop")}>
          <Square size={13} aria-hidden="true" />{t("chatStop")}
        </button>
      {/if}
    </div>
  </div>
  <p class="meta">
    <span class="mono" title={session.cwd}>{shortPath(session.cwd)}</span>
    <span><small>{t("metaModel")}</small> {model || t("schedulerDefault")}</span>
    <span><small>{t("metaEffort")}</small> {session.effort ? (efforts[session.effort] ? t(efforts[session.effort]) : session.effort) : t("schedulerDefault")}</span>
    <span><small>{t("chatMode")}</small> {mode ? (modes[mode] ? t(modes[mode]) : mode) : t("schedulerDefault")}</span>
    <span><small>{t("chatCost")}</small> {usd(session.cost)}</span>
    {#if left !== null && session.budget !== null}
      <span><small>{t("chatBudget")}</small> {t("chatBudgetLeft", { left: usd(left), budget: usd(session.budget) })}</span>
    {/if}
    {#if session.context}<ContextGauge gauge={session.context} />{/if}
  </p>
</header>

{#if error}
  <div class="banners">
    <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
  </div>
{/if}

<div class="thread" bind:this={scroller} aria-label={t("chatThread")} role="log">
  <div class="turns">
    {#if loadError}
      <p class="notice error">
        <CircleAlert size={14} aria-hidden="true" /><span>{t("chatLoadError", { error: loadError })}</span>
        <button class="btn sm" onclick={load}>{t("chatRetry")}</button>
      </p>
    {/if}
    {#each turns as turn (turn.key)}
      {@const now = turning && turn === turns.at(-1) && !turn.end}
      {#if turn.user !== null}<p class="user">{turn.user}</p>{/if}
      {#each turn.items as item (item.key)}{@render entry(item, now)}{/each}
      {#if turn.end}{@render ending(turn.end)}{/if}
    {/each}
    {#if live}<p class="text live">{live}</p>{/if}
  </div>
</div>

<form class="composer" bind:this={form} onsubmit={send}>
  <textarea
    class="field"
    rows="2"
    placeholder={t("chatMessage")}
    aria-label={t("chatMessage")}
    aria-describedby={why ? "chat-why" : undefined}
    bind:value={draft}
    onkeydown={enter}
  ></textarea>
  <button type="submit" class="btn primary" disabled={!!why || !!busy || !draft.trim()}>
    {#if busy === "send"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("chatSend")}
  </button>
  {#if why}<p class="why" id="chat-why">{t(reasonsWhy[why])}</p>{/if}
</form>

<style>
  .head {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
    padding: 14px 24px 12px;
    border-bottom: 1px solid var(--border);
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  h2 {
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .role {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    gap: 4px;
    height: 20px;
    padding: 0 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .role :global(svg) {
    color: var(--accent-ink);
  }

  .actions {
    display: flex;
    flex: 1 1 auto;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 16px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .meta small {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .meta .mono {
    color: var(--muted);
  }

  .banners {
    flex-shrink: 0;
  }

  .thread {
    flex: 1;
    min-height: 0;
    padding: 16px 24px 24px;
    overflow-y: auto;
  }

  .turns {
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-width: 820px;
  }

  .user {
    align-self: flex-end;
    max-width: 85%;
    margin: 12px 0 2px;
    padding: 8px 12px;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .turns > .user:first-child {
    margin-top: 0;
  }

  .text {
    margin: 0;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .live::after {
    content: "▍";
    color: var(--accent);
  }

  .pre {
    white-space: pre-wrap;
  }

  .sys {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .sys :global(svg) {
    flex-shrink: 0;
  }

  .sys.failed {
    color: var(--danger);
  }

  .tool {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--chrome);
  }

  .tool summary {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px 8px;
    list-style: none;
    cursor: pointer;
  }

  .tool summary::-webkit-details-marker {
    display: none;
  }

  .call {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: var(--text-sm);
  }

  .call :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .call :global(.chevron) {
    transition: transform var(--dur-fast) ease-out;
  }

  .tool[open] > summary :global(.chevron) {
    transform: rotate(90deg);
  }

  .call b {
    flex-shrink: 0;
    font-weight: 600;
  }

  .sum {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tool.failed {
    border-color: var(--danger);
  }

  .tool.failed .call :global(svg:not(.chevron)),
  .why-failed {
    color: var(--danger);
  }

  .why-failed {
    overflow: hidden;
    padding-left: 40px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 10px 10px 30px;
  }

  .label {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .code {
    max-height: 320px;
    margin: 0;
    padding: 8px 10px;
    overflow: auto;
    border-radius: var(--radius-sm);
    background: var(--panel);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .code.error {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .cut {
    margin: 0;
    color: var(--warning);
    font-size: var(--text-xs);
  }

  .nested {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 4px 0;
    padding-left: 12px;
    border-left: 2px solid var(--border-strong);
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--waiting-ring);
    border-radius: var(--radius-lg);
    background: var(--waiting-soft);
  }

  .ask-title {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-weight: 600;
  }

  .ask-title :global(svg) {
    color: var(--waiting-ring);
  }

  .ask-text {
    margin: 0;
    font-size: var(--text-sm);
  }

  .answer {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .answer .field {
    flex: 1;
    min-width: 0;
    height: 26px;
  }

  .end {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding-top: 6px;
    border-top: 1px dashed var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .composer {
    display: grid;
    grid-template-columns: minmax(0, 820px) auto;
    align-items: end;
    gap: 6px 8px;
    flex-shrink: 0;
    padding: 10px 24px 12px;
    border-top: 1px solid var(--border);
    background: var(--chrome);
  }

  .composer textarea {
    height: auto;
    min-height: 52px;
    max-height: 200px;
    padding: 6px 8px;
    line-height: 1.5;
    resize: vertical;
  }

  .why {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
  }
</style>
