<script lang="ts">
  import { onMount, tick, type Snippet } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import AtSign from "@lucide/svelte/icons/at-sign";
  import Check from "@lucide/svelte/icons/check";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CirclePause from "@lucide/svelte/icons/circle-pause";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Megaphone from "@lucide/svelte/icons/megaphone";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import ShieldQuestionMark from "@lucide/svelte/icons/shield-question-mark";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import Square from "@lucide/svelte/icons/square";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import UserRound from "@lucide/svelte/icons/user-round";
  import Action from "$lib/Action.svelte";
  import StreamSteps, { type Step } from "$lib/ui/StreamSteps.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { afterChange, changeOf, diffOf, question, within } from "$lib/ask-logic";
  import { phrase as actPhrase } from "$lib/live-logic";
  import ContextGauge from "$lib/ContextGauge.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { ticketKey } from "$lib/TeamRunDetail.svelte";
  import { apiUsd, budgetUsd, runtimes, showConversation, upsert, usd, type AgentSession } from "$lib/agents.svelte";
  import { budgetLabel, costHelp, resetClock, windowName } from "$lib/usage.svelte";
  import { focusSession } from "$lib/commands";
  import {
    addressed,
    blocked,
    fields,
    mentioned,
    merge,
    pending,
    stream,
    summary,
    thread,
    ticketNamed,
    unsaid,
    type End,
    type Item,
    type NoticeFields,
    type Permission,
    type StoredEvent,
    type Tool,
  } from "$lib/conversation-logic";
  import { lang, t } from "$lib/i18n";
  import type { Item as LibraryItem } from "$lib/library.svelte";
  import { blocks, type Span } from "$lib/markdown-logic";
  import { app, onScreen, shortPath, type Session } from "$lib/state.svelte";
  import { asks as runAsks } from "$lib/team-run-logic";
  import { showTeamRun, stepTitle, team } from "$lib/team.svelte";
  import { tickets } from "$lib/tickets.svelte";

  type Notice = Extract<Item, { type: "notice" }>;

  /**
   * `embedded`: in the Home, without the session's header; `lead` goes in its place and the session's details in a
   * popover. `card` replaces a notice of the thread (`fallback` draws it as usual), `top` goes at its head; `notices`
   * tells the caller which notices the thread has. `examples` false leaves the examples of an empty thread out, for a
   * caller whose `top` already asks something.
   * `bare`: in the ⌘I panel, with no header; the agent's actions fold into one line, a permission is a proposal
   * (Apply, Modify, Ignore) and an applied file change can be undone. `chips` goes above the field; `outgoing` turns
   * what is typed into what is sent, null when it went elsewhere.
   */
  let {
    session,
    embedded = false,
    lead,
    card,
    top,
    notices = $bindable([]),
    examples: offerExamples = true,
    bare = false,
    chips,
    outgoing,
    placeholder,
  }: {
    session: AgentSession;
    embedded?: boolean;
    lead?: Snippet;
    card?: Snippet<[Notice, Snippet<[Notice]>]>;
    top?: Snippet;
    notices?: Notice[];
    examples?: boolean;
    bare?: boolean;
    chips?: Snippet;
    outgoing?: (text: string) => Promise<string | null>;
    placeholder?: string;
  } = $props();

  let events = $state.raw<StoredEvent[]>([]);
  let live = $state<string | null>(null);
  let draft = $state("");
  let reasons = $state<Record<string, string>>({});
  let busy = $state("");
  let error = $state("");
  let loadError = $state("");
  let loading = $state(true);
  let scroller = $state<HTMLElement>();
  let form = $state<HTMLFormElement>();

  const turns = $derived(thread(events));
  /** A long thread draws its last turns only, until asked for all: a stream stays smooth however old the conversation. */
  const WINDOW = 40;
  let all = $state(false);
  const drawn = $derived(all ? turns : turns.slice(-WINDOW));
  $effect(() => {
    notices = turns.flatMap((turn) => turn.items.filter((i): i is Notice => i.type === "notice"));
  });
  let details = $state(false);
  let detailsButton = $state<HTMLElement>();
  const init = $derived(events.findLast((e) => e.kind === "init"));
  const expert = $derived(session.mcp === true && session.role === "expert");
  const cto = $derived(session.mcp === true && !expert);
  /** The project's active roles, the CTO aside: `@role` writes to one of them. */
  let roles = $state<string[]>([]);
  let repo = $state("");
  let picked = $state(0);
  let closed = $state(false);
  let box = $state<HTMLTextAreaElement>();
  const repos = $derived(app.workspace?.projects.find((p) => p.id === session.project)?.repos ?? []);
  const to = $derived(cto ? addressed(draft, roles) : null);
  const offered = $derived(cto && !closed ? mentioned(draft, roles) : []);
  // A message to another agent does not wait for the CTO's turn.
  const why = $derived(to ? null : blocked(session.state));
  const turning = $derived(session.state === "working" || session.state === "waiting");
  const asking = $derived(session.state === "waiting" ? pending(events) : null);
  /** At work with no word yet and no call running: it reads, plans, or reads what a call gave. */
  const thinking = $derived.by(() => {
    if (session.state !== "working" || live) return false;
    const last = turns.at(-1);
    const running = (items: Item[]): boolean => items.some((i) => i.type === "tool" && (i.result === null || running(i.children)));
    return !last?.end && !running(last?.items ?? []);
  });

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
  const runtime = $derived(runtimes[session.runtime]);

  const atBottom = () => !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;

  async function follow(stick: boolean) {
    if (!stick) return;
    await tick();
    scroller?.scrollTo({ top: scroller.scrollHeight });
  }

  // Back on the view: what came meanwhile is at the bottom.
  $effect(() => {
    if (bare || onScreen(embedded ? "home" : "conversations")) follow(true);
  });

  $effect(() => {
    if (!cto) return;
    invoke<LibraryItem[]>("library_list", { kind: "role", project: session.project }).then(
      // A shipped role plays in the team even when it is off for terminals.
      (items) => (roles = items.filter((i) => (i.active || i.layer === "shipped") && i.name !== "cto").map((i) => i.name)),
      () => (roles = []),
    );
  });

  $effect(() => {
    if (!repos.some((r) => r.path === repo)) repo = repos[0]?.path ?? "";
  });

  $effect(() => {
    if (!draft.startsWith("@")) closed = false;
  });

  const termfTools: Record<string, Parameters<typeof t>[0]> = {
    mcp__termf__project: "ctoToolProject",
    mcp__termf__team_runs: "ctoToolTeamRuns",
    mcp__termf__team_run: "ctoToolTeamRun",
    mcp__termf__ticket_runs: "ctoToolTicketRuns",
    mcp__termf__inbox: "ctoToolInbox",
    mcp__termf__waiting: "ctoToolWaiting",
    mcp__termf__start_team_run: "ctoToolStart",
    mcp__termf__answer: "ctoToolAnswer",
    mcp__termf__approve: "ctoToolApprove",
    mcp__termf__request_changes: "ctoToolChanges",
    mcp__termf__stop_team_run: "ctoToolStop",
    mcp__termf__send_message: "ctoToolSend",
    mcp__termf__propose_conventions: "watchToolConventions",
    mcp__termf__run_scout: "watchToolScout",
    mcp__termf__start_onboarding: "watchToolOnboarding",
    mcp__termf__schedule_task: "watchToolSchedule",
    mcp__termf__ask_expert: "expertToolAsk",
    mcp__termf__memory_search: "expertToolSearch",
    mcp__termf__memory_read: "expertToolRead",
    mcp__termf__usage: "recetteCtoToolUsage",
    mcp__termf__ops_recipes: "recetteCtoToolOpsRecipes",
    mcp__termf__ops_run: "recetteCtoToolOpsRun",
  };
  const toolName = (name: string) => (termfTools[name] ? t(termfTools[name]) : name);
  const asks = (tool: string) =>
    termfTools[tool] ? t("ctoAsks", { action: t(termfTools[tool]) }) : t("chatPermission", { tool });

  function said(n: NoticeFields): string {
    const vars = { run: n.run ?? "", step: n.step ? stepTitle(n.step) : "", role: n.role ?? "" };
    const keys = {
      waiting: "ctoNoticeWaiting",
      blocked: "ctoNoticeBlocked",
      step: "ctoNoticeStep",
      direct: "ctoNoticeDirect",
      review: "reviewLoopNotice",
    } as const;
    if (n.about === "direct" && n.run) return t("ctoNoticeBusy", vars);
    if (n.about === "review" && n.step && n.step !== "review") return t("roles2ReviewNotice", vars);
    if (n.about === "queued") return t("expertNoticeQueued", vars);
    return n.about in keys ? t(keys[n.about as keyof typeof keys], vars) : n.text;
  }

  // What a beginner can ask first, on an empty thread; a click fills the field without sending.
  const examples = $derived.by(() => {
    const repo = repos[0]?.name;
    if (expert) return repo ? (["uxfT2ExpertExample1", "uxfT2ExpertExample2", "uxfT2ExpertExample3"] as const).map((k) => t(k, { repo })) : [];
    return [t("uxfT2CtoExample1"), t("uxfT2CtoExample2"), ...(repo ? [t("uxfT2CtoExample3", { repo })] : [])];
  });

  function suggest(text: string) {
    draft = text;
    box?.focus();
  }

  function choose(role: string) {
    draft = `@${role} `;
    picked = 0;
    box?.focus();
  }

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
    } finally {
      loading = false;
    }
  }

  // What streams in is drawn once a frame, however fast the tokens come.
  let queue: StoredEvent[] = [];
  let frame = 0;

  function flush() {
    frame = 0;
    const heard = queue;
    queue = [];
    const stick = atBottom();
    live = heard.reduce(stream, live);
    const stored = heard.filter((e) => e.id);
    if (stored.length) events = merge(events, stored);
    follow(stick);
  }

  onMount(() => {
    const id = session.id;
    const off = listen<StoredEvent>("agent-event", ({ payload }) => {
      if (payload.session !== id) return;
      if (buffer) return void buffer.push(payload);
      queue.push(payload);
      frame ||= requestAnimationFrame(flush);
    });
    // Listening first, then reading what is stored: nothing falls in between.
    off.then(load);
    return () => {
      cancelAnimationFrame(frame);
      off.then((stop) => stop());
    };
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
    let text = draft.trim();
    if (!text || why || busy) return;
    if (outgoing) {
      const out = await run("send", () => outgoing(text));
      if (out === undefined) return;
      if (out === null) {
        draft = "";
        return;
      }
      text = out;
    } else if (to) {
      const { role, text } = to;
      const args = { project: session.project, role, text, repo: repo || null };
      const reached = await run("send", () => invoke<{ session: AgentSession }>("cto_address", args));
      if (reached) {
        draft = "";
        upsert(reached.session);
        showConversation(reached.session.id);
      }
      return;
    }
    const sent = await run("send", () => invoke("agent_send", { id: id(), text }).then(() => true));
    if (sent) {
      draft = "";
      follow(true);
    }
  }

  function enter(e: KeyboardEvent) {
    if (e.key === "Escape" && turning && !offered.length) {
      e.preventDefault();
      e.stopPropagation();
      run("interrupt", () => invoke("agent_interrupt", { id: id() }));
      return;
    }
    if (offered.length && !e.isComposing) {
      const at = Math.min(picked, offered.length - 1);
      const step = { ArrowDown: 1, ArrowUp: offered.length - 1 }[e.key];
      if (step !== undefined) picked = (at + step) % offered.length;
      else if (e.key === "Enter" || e.key === "Tab") choose(offered[at]);
      else if (e.key === "Escape") closed = true;
      else return;
      e.preventDefault();
      return;
    }
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
    return run("answer", () => invoke("agent_answer", { id: id(), request: ask.request, allow, message }).then(() => true));
  }

  /**
   * "Allow". In the ⌘I panel, a change of a project file that existed is read first, so that the toast's Undo (⌘Z)
   * puts it back as it was; nothing else can be undone, and the thread says it was allowed.
   */
  async function allow(ask: Permission) {
    const change = bare ? changeOf(ask.tool, ask.input) : null;
    const at = change && within(change.path, repos.map((r) => r.path));
    let before: string | null = null;
    if (at) {
      const read = await invoke<{ kind: string; text?: string }>("files_read", { repo: at.repo, path: at.rel }).catch(() => null);
      if (read?.kind === "text" && typeof read.text === "string") before = read.text;
    }
    if (!(await answer(ask, true)) || !change || !at || before === null) return;
    const after = afterChange(ask.tool, ask.input, before);
    const what = shortPath(change.path);
    const text = before;
    if (after !== null) toast(t("aigApplied", { what }), { undo: () => restore(at, text, after, what) });
  }

  /** Puts a file back as it was, only while it is still as the change left it: a later change is never lost. */
  async function restore(at: { repo: string; rel: string }, before: string, after: string, what: string) {
    try {
      const now = await invoke<{ kind: string; text?: string; stamp: unknown }>("files_read", { repo: at.repo, path: at.rel });
      if (now.kind !== "text" || now.text !== after) return void toast(t("aigUndoChanged", { what }), { kind: "error" });
      const saved = await invoke<{ kind: string }>("files_write", { repo: at.repo, path: at.rel, text: before, stamp: now.stamp });
      if (saved.kind === "conflict") return void toast(t("aigUndoChanged", { what }), { kind: "error" });
      toast(t("aigUndone", { what }), { kind: "info" });
    } catch (e) {
      toast(String(e), { kind: "error" });
    }
  }


  /** The tools of a turn in a row, as one group of steps: the panel folds them into a line. */
  type Group = { key: string; item: Item } | { key: string; tools: Tool[] };
  function groups(items: Item[]): Group[] {
    const out: Group[] = [];
    for (const item of items) {
      const last = out.at(-1);
      if (item.type === "tool" && last && "tools" in last) last.tools.push(item);
      else if (item.type === "tool") out.push({ key: `g${item.key}`, tools: [item] });
      else out.push({ key: String(item.key), item });
    }
    return out;
  }

  function stepOf(item: Tool, now: boolean): Step {
    const state = item.result === null ? (now ? "now" : undefined) : item.result.isError ? "fail" : "ok";
    if (termfTools[item.name]) return { verb: toolName(item.name), object: phrase(item.input) || undefined, state };
    const p = actPhrase(item.name, item.input);
    const verb = t(p.key as Parameters<typeof t>[0], { ...p.vars, target: "" }).replace(/\s+/g, " ").trim();
    return { verb, object: p.target || undefined, state };
  }

  function folded(steps: Step[]): string {
    const first = steps.slice(0, 2).map((s) => [s.verb, s.object].filter(Boolean).join(" "));
    const list = `${first.join(", ")}${steps.length > 2 ? "…" : ""}`;
    return steps.length === 1 ? t("aigFoldedOne", { first: list }) : t("aigFolded", { n: steps.length, first: list });
  }

  const pretty = (value: unknown) => (typeof value === "string" ? value : JSON.stringify(value, null, 2));
  const firstLine = (text: string) => text.trim().split("\n")[0];

  const knownTickets = $derived([
    ...(tickets.inbox ?? []),
    ...tickets.runs.map((r) => ({ ...r.ticket, title: r.title })),
    ...team.runs.map((r) => ({ key: r.target.ticket, url: "", title: r.target.title })),
  ]);

  function phrase(input: unknown): string {
    const said = fields(input).map(([field, value]) => {
      if (field === "ticket") {
        const known = ticketNamed(value, knownTickets);
        return known ? `${known.key} ${known.title}` : value;
      }
      if (field === "run") {
        const crew = team.runs.find((r) => r.run === Number(value));
        const run = tickets.runs.find((r) => r.id === Number(value));
        if (crew) return `${ticketKey(crew.target.ticket)} ${crew.target.title}`;
        return run ? `${run.ticket.key} ${run.title}` : t("ux3t5Run", { run: value });
      }
      if (field === "repo") return t("ticketInRepo", { repo: value });
      if (field === "workflow") return t("teamRunsWorkflowName", { name: value });
      if (field === "budget") return `${budgetLabel(t("chatBudget"))} ${budgetUsd(Number(value))}`;
      if (field === "id") return `#${value}`;
      return value;
    });
    return said.join(" · ") || summary(input);
  }

  function asked(input: unknown, id: string): string {
    const { run, step } = (input ?? {}) as { run?: unknown; step?: unknown };
    const crew = team.runs.find((r) => r.run === run);
    const ask = crew && runAsks(crew).find((a) => a.step.key === step);
    return ask?.questions.find((q) => q.id === id)?.question ?? id;
  }

  function openLink(e: MouseEvent, url: string) {
    e.preventDefault();
    invoke("open_url", { url }).catch((e) => (error = String(e)));
  }
</script>

{#snippet line(list: Span[])}{#each list as span, i (i)}{#if span.kind === "strong"}<strong>{@render line(span.spans)}</strong
      >{:else if span.kind === "code"}<code>{span.text}</code>{:else if span.kind === "link"}<a
        href={span.url}
        onclick={(e) => openLink(e, span.url)}>{span.text}</a
      >{:else}{span.text}{/if}{/each}{/snippet}

{#snippet rich(text: string)}
  {#each blocks(text, true) as block, i (i)}
    {#if block.kind === "code"}
      <pre class="mono">{block.text}</pre>
    {:else if block.kind === "heading"}
      <svelte:element this={`h${Math.min(block.level + 2, 6)}`} class="heading">{@render line(block.spans)}</svelte:element>
    {:else if block.kind === "table"}
      <div class="table">
        <table>
          <thead><tr>{#each block.head as cell, j (j)}<th style:text-align={block.align[j]}>{@render line(cell)}</th>{/each}</tr></thead>
          <tbody>
            {#each block.rows as row, r (r)}<tr>{#each row as cell, j (j)}<td style:text-align={block.align[j]}>{@render line(cell)}</td>{/each}</tr>{/each}
          </tbody>
        </table>
      </div>
    {:else if block.kind === "list" && block.start === null}
      <ul>{#each block.items as item, j (j)}<li>{@render line(item)}</li>{/each}</ul>
    {:else if block.kind === "list"}
      <ol start={block.start}>{#each block.items as item, j (j)}<li>{@render line(item)}</li>{/each}</ol>
    {:else}
      <p>{@render line(block.spans)}</p>
    {/if}
  {/each}
{/snippet}

{#snippet entry(item: Item, now: boolean)}
  {#if item.type === "text"}
    <div class="text">{@render rich(item.text)}</div>
  {:else if item.type === "tool"}
    {@render tool(item, now)}
  {:else if item.type === "permission"}
    {@render ask(item)}
  {:else if item.type === "denied"}
    <p class="sys failed">
      <ShieldX size={14} aria-hidden="true" />
      <span>{t("chatDenied", { tool: item.tool })}<span class="muted">{" · "}{item.message}</span></span>
    </p>
  {:else if item.type === "limit"}
    {@const win = windowName(item.window)}
    <p class="notice" class:error={item.reached} role="status">
      <CircleAlert size={14} aria-hidden="true" />
      <span>
        {#if item.reached}
          {item.resetsAt ? t("usage2Reached", { window: win, time: resetClock(item.resetsAt) }) : t("usage2ReachedNoTime", { window: win })}
        {:else}
          {item.resetsAt ? t("usage2Near", { window: win, time: resetClock(item.resetsAt) }) : t("usage2NearNoTime", { window: win })}
        {/if}
      </span>
    </p>
  {:else if item.type === "error"}
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{t("chatProcessError", { message: item.message })}</span></p>
  {:else if item.type === "taken_over"}
    <p class="sys"><SquareTerminal size={14} aria-hidden="true" />{t("chatTakenOver")}</p>
  {:else if item.type === "handed_back"}
    <p class="sys"><Undo2 size={14} aria-hidden="true" />{t("chatHandedBack")}</p>
  {:else if item.type === "notice"}
    {#if card}{@render card(item, termfNotice)}{:else}{@render termfNotice(item)}{/if}
  {/if}
{/snippet}

{#snippet termfNotice(item: Notice)}
  {@const opens = ["step", "direct", "queued"].includes(item.about) ? item.conversation : undefined}
  <div class="termf" class:asks={item.about === "waiting" || item.about === "blocked"} role="note">
    <Megaphone size={14} aria-hidden="true" />
    <p>
      <b>{t("ctoNoticeTermf")}</b> · {said(item)}{#if item.detail}<span class="muted pre">{" · "}{item.detail}</span>{/if}
    </p>
    {#if opens}
      <button class="btn sm" onclick={() => showConversation(opens)}>{t("ctoOpen")}</button>
    {:else if item.run && ["waiting", "blocked", "review"].includes(item.about)}
      <button class="btn sm" onclick={() => showTeamRun(item.run!)}>{t("ctoOpen")}</button>
    {/if}
  </div>
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
        {#if termfTools[item.name]}
          <b>{toolName(item.name)}</b>
          <span class="sum">{phrase(item.input)}</span>
        {:else}
          <span class="sum said"><Action act={{ kind: "tool", name: item.name, input: item.input }} /></span>
        {/if}
        {#if item.result === null && now}<span class="state">{t("chatToolRunning")}</span>{/if}
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
        {asks(item.tool)} ·
        {#if item.answer?.allow}{t("chatAllowed")}{:else if item.answer?.message}{t("chatRefusedWhy", {
            message: item.answer.message,
          })}{:else if item.answer}{t("chatRefused")}{:else}{t("chatUnanswered")}{/if}
      </span>
    </p>
  {:else}
    <div class="ask" role="group" aria-label={asks(item.tool)}>
      <p class="ask-title">
        <ShieldQuestionMark size={16} aria-hidden="true" />{asks(item.tool)}
      </p>
      {#if item.description}<p class="ask-text">{item.description}</p>{/if}
      {#if termfTools[item.tool]}
        {@const said = phrase(item.input)}
        {#if said}<p class="ask-what">{said}</p>{/if}
        {#each unsaid(item.input) as [name, entries] (name)}
          <dl class="ask-args">
            <dt>{name === "answers" ? t("teamRunsAnswers") : name}</dt>
            {#each entries as [id, value], i (i)}
              <dd>{#if id}<span class="muted">{asked(item.input, id)}</span>{/if}<span>{value}</span></dd>
            {/each}
          </dl>
        {/each}
        <details class="raw">
          <summary><ChevronRight size={14} aria-hidden="true" class="chevron" />{t("ux3t5Details")}</summary>
          <pre class="mono code">{pretty(item.input)}</pre>
        </details>
      {:else}
        <!-- What the agent wants, in plain words; a file change shows its lines; the call itself in the details. -->
        {@const diff = diffOf(item.tool, item.input)}
        <p class="ask-what"><Action act={{ kind: "tool", name: item.tool, input: item.input }} /></p>
        {#if diff.length}
          <pre class="mono code diff">{#each diff as hunk, h (h)}{#each hunk.del as l, i (i)}<span class="del">- {l}</span
              >{/each}{#each hunk.add as l, i (i)}<span class="add">+ {l}</span>{/each}{/each}</pre>
        {/if}
        <details class="raw">
          <summary><ChevronRight size={14} aria-hidden="true" class="chevron" />{t("ux3t5Details")}</summary>
          <pre class="mono code">{pretty(item.input)}</pre>
        </details>
      {/if}
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
        <button class="btn sm primary" disabled={!!busy} onclick={() => allow(item)}>
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
    {#if runtime}
      <span>{t("adaptersTurnTokens", { input: end.usage.input.toLocaleString(lang), output: end.usage.output.toLocaleString(lang) })}</span>
    {:else}
      <span title={costHelp()}>{t("chatTurnCost", { cost: usd(end.cost), total: usd(end.total) })}</span>
    {/if}
    {#if end.context}<ContextGauge gauge={end.context} />{/if}
  </div>
  {#if end.outcome === "error" && end.error}
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{end.error}</span></p>
  {/if}
{/snippet}

{#snippet actions()}
  {#if turning}
    <button class="btn sm" disabled={!!busy} onclick={() => run("interrupt", () => invoke("agent_interrupt", { id: id() }))}>
      <CirclePause size={14} aria-hidden="true" />{t("chatInterrupt")}
    </button>
  {/if}
  {#if !session.mcp && (session.state === "idle" || session.state === "stopped" || session.state === "error")}
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
{/snippet}

{#snippet meta()}
  <p class="meta">
    <span class="mono" title={session.cwd}>{shortPath(session.cwd)}</span>
    {#if runtime}<span><small>{t("adaptersTool")}</small> {t(runtime)}</span>{/if}
    <span><small>{t("metaModel")}</small> {model || t("schedulerDefault")}</span>
    <span><small>{t("metaEffort")}</small> {session.effort ? (efforts[session.effort] ? t(efforts[session.effort]) : session.effort) : t("schedulerDefault")}</span>
    <span><small>{t("chatMode")}</small> {mode ? (modes[mode] ? t(modes[mode]) : mode) : t("schedulerDefault")}</span>
    {#if !runtime}<span title={costHelp()}><small>{t("chatCost")}</small> {apiUsd(session.cost)}</span>{/if}
    {#if left !== null && session.budget !== null}
      <span><small>{budgetLabel(t("chatBudget"))}</small> {t("chatBudgetLeft", { left: budgetUsd(left), budget: budgetUsd(session.budget) })}</span>
    {/if}
    {#if session.context}<ContextGauge gauge={session.context} />{/if}
  </p>
{/snippet}

{#if bare}
  <!-- In the panel, which says who answers. -->
{:else if embedded}
  <header class="bar">
    {@render lead?.()}
    {#if session.state === "working"}<StatusPill status="working" />{/if}
    <button
      class="icon-btn"
      bind:this={detailsButton}
      aria-expanded={details}
      aria-haspopup="dialog"
      aria-label={t("ux3t1Details")}
      title={t("ux3t1Details")}
      onclick={() => (details = !details)}
    >
      <Ellipsis size={16} aria-hidden="true" />
    </button>
    <Popover bind:open={details} trigger={detailsButton} label={t("ux3t1Details")} align="end" width={340}>
      <div class="details">
        <div class="top">
          <b>{session.title}</b>
          <StatusPill status={session.state} />
        </div>
        {@render meta()}
        <div class="actions">{@render actions()}</div>
      </div>
    </Popover>
  </header>
{:else}
  <header class="head">
    <div class="top">
      <h2>{session.title}</h2>
      {#if session.role}<span class="role"><UserRound size={12} aria-hidden="true" />{session.role}</span>{/if}
      <StatusPill status={session.state} />
      <div class="actions">{@render actions()}</div>
    </div>
    {@render meta()}
  </header>
{/if}

{#if error}
  <div class="banners">
    <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
  </div>
{/if}

<div class="thread" class:embedded class:bare bind:this={scroller} aria-label={t("chatThread")} role="log">
  <div class="turns">
    {@render top?.()}
    {#if loadError}
      <p class="notice error">
        <CircleAlert size={14} aria-hidden="true" /><span>{t("chatLoadError", { error: loadError })}</span>
        <button class="btn sm" onclick={load}>{t("chatRetry")}</button>
      </p>
    {/if}
    {#if loading}
      <Skeleton rows={4} />
    {:else if session.mcp && !turns.length && !loadError}
      <div class="intro">
        <p>{expert ? t("expertIntro") : t("ctoIntro")}</p>
        {#if offerExamples && examples.length}
          <div class="examples" role="group" aria-label={t("uxfT2Examples")}>
            <span aria-hidden="true">{t("uxfT2Examples")}</span>
            {#each examples as example (example)}
              <button type="button" class="btn sm" onclick={() => suggest(example)}>{example}</button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
    {#if drawn.length < turns.length}
      <button class="btn sm quiet earlier" onclick={() => (all = true)}>{t("liveMore", { count: turns.length })}</button>
    {/if}
    {#each drawn as turn (turn.key)}
      {@const now = turning && turn === turns.at(-1) && !turn.end}
      {#if turn.user !== null}<p class="user">{question(turn.user)}</p>{/if}
      {#if bare}
        {@const list = groups(turn.items)}
        {#each list as g, gi (g.key)}
          {#if "tools" in g}
            {@const steps = g.tools.map((tool) => stepOf(tool, now))}
            <StreamSteps {steps} live={now && gi === list.length - 1} summary={folded(steps)} />
          {:else}{@render entry(g.item, now)}{/if}
        {/each}
      {:else}
        {#each turn.items as item (item.key)}{@render entry(item, now)}{/each}
      {/if}
      {#if turn.end}{@render ending(turn.end)}{/if}
    {/each}
    {#if live}<div class="text live">{@render rich(live)}</div>{/if}
    {#if thinking}
      <p class="thinking" role="status"><span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>{t("liveThinking")}</p>
    {/if}
  </div>
</div>

<form class="composer" class:embedded={embedded || bare} class:bare bind:this={form} onsubmit={send}>
  {#if chips}<div class="chips">{@render chips()}</div>{/if}
  {#if offered.length}
    <ul class="mentions" id="cto-roles" role="listbox" aria-label={t("ctoRoles")}>
      {#each offered as role, i (role)}
        {@const on = i === Math.min(picked, offered.length - 1)}
        <li id="cto-role-{role}" role="option" aria-selected={on} class:on>
          <button type="button" tabindex="-1" onclick={() => choose(role)}><AtSign size={13} aria-hidden="true" />{role}</button>
        </li>
      {/each}
    </ul>
  {/if}
  <textarea
    class="field"
    rows="2"
    placeholder={placeholder ?? (cto ? t("ctoMessage") : expert ? t("expertMessage") : t("chatMessage"))}
    aria-label={placeholder ?? (cto ? t("ctoMessage") : expert ? t("expertMessage") : t("chatMessage"))}
    aria-describedby={why ? "chat-why" : undefined}
    aria-controls={offered.length ? "cto-roles" : undefined}
    aria-activedescendant={offered.length ? `cto-role-${offered[Math.min(picked, offered.length - 1)]}` : undefined}
    bind:this={box}
    bind:value={draft}
    onkeydown={enter}
  ></textarea>
  <div class="send">
    {#if to && !bare && to.role !== "expert" && repos.length > 1}
      <select class="field" aria-label={t("ctoRepo")} title={t("ctoRepo")} bind:value={repo}>
        {#each repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
      </select>
    {/if}
    {#if embedded || bare}
      <!-- On the Home the primary action is the latest decision: sending is an icon in the field. -->
      {@const label = to ? t("ctoSendTo", { role: to.role }) : t("chatSend")}
      <button type="submit" class="icon-send" aria-label={label} title={label} disabled={!!why || !!busy || !draft.trim()}>
        {#if busy === "send"}<LoaderCircle size={16} aria-hidden="true" class="spin" />{:else}<ArrowUp size={16} aria-hidden="true" />{/if}
      </button>
    {:else}
      <button type="submit" class="btn primary" disabled={!!why || !!busy || !draft.trim()}>
        {#if busy === "send"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{to
          ? t("ctoSendTo", { role: to.role })
          : t("chatSend")}
      </button>
    {/if}
  </div>
  {#if why}<p class="why" id="chat-why">{t(reasonsWhy[why])}</p>{/if}
</form>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
    height: 52px;
    padding: 0 12px 0 20px;
    border-bottom: 1px solid var(--border);
  }

  .details {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px 14px;
  }

  .details .top b {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .details .actions {
    justify-content: flex-start;
  }

  .details .actions:empty {
    display: none;
  }

  .thread.embedded {
    padding: 20px 28px 24px;
  }

  .thread.embedded .turns {
    gap: 14px;
    max-width: 760px;
  }

  .composer.embedded {
    grid-template: "mentions" auto "field" auto "why" auto / minmax(0, 1fr);
    row-gap: 0;
    padding: 12px 20px 16px;
    background: var(--panel);
  }

  .composer.embedded .mentions {
    grid-area: mentions;
    margin-bottom: 6px;
  }

  .composer.embedded textarea {
    grid-area: field;
    min-height: 58px;
    padding: 8px 48px 8px 12px;
    border-radius: var(--radius-lg);
  }

  .composer.embedded:has(.send select) textarea {
    padding-right: 240px;
  }

  .composer.embedded .send {
    grid-area: field;
    align-self: end;
    justify-self: end;
    margin: 0 8px 8px 0;
  }

  .composer.embedded .why {
    grid-area: why;
    margin-top: 6px;
  }

  /* In the ⌘I panel: narrower, the context chips above the field. */
  .thread.bare {
    padding: 16px;
  }

  .composer.bare {
    grid-template: "chips" auto "mentions" auto "field" auto "why" auto / minmax(0, 1fr);
    padding: 10px 12px 12px;
  }

  .composer.bare .chips {
    grid-area: chips;
    margin-bottom: 8px;
  }

  /* A permission says the whole command or path: it wraps rather than being cut. */
  .ask-what :global(.action) {
    white-space: normal;
    overflow-wrap: anywhere;
  }

  /* In the narrow panel, the reason for a refusal takes its own line above the answers. */
  .thread.bare .answer {
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .thread.bare .answer .field {
    flex-basis: 100%;
  }

  .diff {
    display: flex;
    flex-direction: column;
    max-height: 220px;
    margin: 0;
    overflow: auto;
  }

  .diff .del {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .diff .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .icon-send {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--radius);
    background: var(--accent);
    color: var(--accent-text);
    cursor: pointer;
    transition: background-color var(--dur-fast) ease-out;
  }

  .icon-send:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  .icon-send:disabled {
    background: var(--hover);
    color: var(--muted);
    cursor: default;
  }

  .icon-send:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

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
    display: flex;
    flex-direction: column;
    gap: 10px;
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .text p {
    margin: 0;
    white-space: pre-wrap;
  }

  .text ul,
  .text ol {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding-left: 22px;
  }

  .text li {
    padding-left: 2px;
    white-space: pre-wrap;
  }

  .text li::marker {
    color: var(--muted);
  }

  .text .heading {
    margin: 6px 0 0;
    font-size: var(--text-md);
    font-weight: 600;
    letter-spacing: -0.005em;
  }

  .text h5.heading,
  .text h6.heading {
    font-size: var(--text-base);
  }

  .text .table {
    max-width: 100%;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .text table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
    line-height: 1.45;
  }

  .text th,
  .text td {
    padding: 7px 10px;
    border-top: 1px solid var(--border);
    text-align: left;
    vertical-align: top;
    overflow-wrap: normal;
  }

  .text th {
    border-top: 0;
    background: var(--well);
    color: var(--soft);
    font-weight: 600;
    white-space: nowrap;
  }

  .text tbody tr:hover {
    background: var(--hover);
  }

  .text code {
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: 0.9em;
  }

  .text pre {
    max-height: 420px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--chrome);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .text a {
    color: var(--accent-ink);
    text-decoration: underline;
    text-decoration-thickness: 1px;
    text-underline-offset: 2px;
  }

  .live > :global(:last-child)::after {
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

  .sum.said {
    display: flex;
    color: var(--text);
  }

  .state {
    flex-shrink: 0;
    color: var(--working);
    font-size: var(--text-xs);
  }

  .earlier {
    align-self: center;
  }

  .thinking {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .dots {
    display: inline-flex;
    gap: 3px;
  }

  .dots i {
    width: 5px;
    height: 5px;
    border-radius: var(--radius-pill);
    background: var(--working);
    animation: breathe 1.2s ease-in-out infinite;
  }

  .dots i:nth-child(2) {
    animation-delay: 0.15s;
  }

  .dots i:nth-child(3) {
    animation-delay: 0.3s;
  }

  @keyframes breathe {
    0%,
    100% {
      opacity: 0.25;
      transform: scale(0.8);
    }
    50% {
      opacity: 1;
      transform: scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots i {
      animation: none;
      opacity: 0.6;
    }
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

  .ask-what {
    margin: 0;
    font-weight: 500;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .ask-args {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .ask-args dt {
    font-weight: 600;
  }

  .ask-args dd {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding-left: 10px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .ask-args dd span:last-child {
    font-weight: 500;
  }

  .raw summary {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    font-size: var(--text-sm);
    list-style: none;
    cursor: pointer;
  }

  .raw summary::-webkit-details-marker {
    display: none;
  }

  .raw summary :global(.chevron) {
    transition: transform var(--dur-fast) ease-out;
  }

  .raw[open] summary :global(.chevron) {
    transform: rotate(90deg);
  }

  .raw .code {
    margin-top: 6px;
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
    font-size: var(--text-sm);
  }

  .send {
    display: flex;
    align-items: end;
    gap: 6px;
  }

  .send select {
    width: auto;
    max-width: 180px;
  }

  .intro {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 64ch;
  }

  .intro p {
    margin: 0;
    line-height: 1.55;
  }

  .examples {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .examples span {
    margin-right: 2px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  /* TermF speaks in the conversation: neither the user's bubble nor the agent's text. */
  .termf {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--chrome);
    font-size: var(--text-sm);
  }

  .termf p {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .termf :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .termf.asks {
    border-color: var(--waiting-ring);
    background: var(--waiting-soft);
  }

  .termf.asks :global(svg) {
    color: var(--waiting-ring);
  }

  .mentions {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .mentions button {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--panel);
    color: inherit;
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .mentions .on button {
    border-color: var(--accent);
    background: var(--accent-soft);
    font-weight: 600;
  }
</style>
