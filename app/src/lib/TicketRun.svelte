<script module lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { focusSession } from "$lib/commands";
  import { app, type Session } from "$lib/state.svelte";
  import { liveSession } from "$lib/tickets-logic";
  import { tickets, type Run } from "$lib/tickets.svelte";

  export function runSession(run: Run): Session | undefined {
    return liveSession(run, app.sessions);
  }

  /** Shows the run's tile. `session-opened` may come after the answer that started the run: then the list is asked. */
  export async function openTile(run: Run) {
    if (!runSession(run)) {
      const list = await invoke<Session[]>("pty_list").catch(() => []);
      for (const s of list) if (!app.sessions.some((o) => o.id === s.id)) app.sessions.push(s);
    }
    const session = runSession(run);
    if (session) await focusSession(session.id);
  }
</script>

<script lang="ts">
  import Archive from "@lucide/svelte/icons/archive";
  import BrushCleaning from "@lucide/svelte/icons/brush-cleaning";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import GitMerge from "@lucide/svelte/icons/git-merge";
  import GitPullRequestClosed from "@lucide/svelte/icons/git-pull-request-closed";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Send from "@lucide/svelte/icons/send";
  import ShieldQuestionMark from "@lucide/svelte/icons/shield-question-mark";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import CiChecks from "$lib/CiChecks.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { repoName, shortPath } from "$lib/state.svelte";
  import { readyToClean, refusal, resumable } from "$lib/tickets-logic";
  import {
    openUrl,
    upsert,
    type Delivery,
    type DeliveryPreview,
    type Resumed,
    type RunEvent,
  } from "$lib/tickets.svelte";

  let { run }: { run: Run } = $props();

  type Step =
    | null
    | { kind: "preview"; preview: DeliveryPreview }
    | { kind: "delivered"; warning: string | null; again: boolean }
    | { kind: "resumed"; warning: string }
    | { kind: "close" }
    | { kind: "refused"; detail: string; cleanup: boolean }
    | { kind: "force" };

  let step = $state<Step>(null);
  /** The action under way. */
  let busy = $state<"" | "preview" | "deliver" | "close" | "resume" | "cleanup">("");
  let stepError = $state("");
  let events = $state<RunEvent[]>([]);

  const session = $derived(runSession(run));
  const open = $derived(run.status === "running" || run.status === "delivered");
  const project = $derived(app.workspace?.projects.find((p) => p.id === run.project)?.name ?? run.project);

  /** How long a run's session may stay "starting" before the trust question is the likely reason. */
  const TRUST_DELAY = 6000;
  /** Claude Code asks whether to trust a folder before any hook runs, so its session stays "starting". */
  let untrusted = $state(false);
  $effect(() => {
    untrusted = false;
    if (session?.status !== "starting") return;
    const timer = setTimeout(() => (untrusted = true), TRUST_DELAY);
    return () => clearTimeout(timer);
  });

  let eventsRequest = 0;

  // Only the latest request fills the log: an older answer may come last.
  $effect(() => {
    void run.updated;
    const request = ++eventsRequest;
    invoke<RunEvent[]>("tickets_events", { id: run.id }).then(
      (list) => {
        if (request === eventsRequest) events = list;
      },
      (e) => {
        if (request === eventsRequest) tickets.error = String(e);
      },
    );
  });

  const short = (commit: string) => commit.slice(0, 7);
  const changeLabel = {
    added: "statusAdded",
    modified: "statusModified",
    deleted: "statusDeleted",
    renamed: "statusRenamed",
  } as const;

  function describe(e: RunEvent): string {
    switch (e.kind) {
      case "worktree_created":
        return t("eventWorktree", { branch: e.branch, base: e.base });
      case "session_opened":
        return t("eventSession");
      case "session_resumed":
        return t("ticketsUiEventResumed");
      case "committed":
        return t("eventCommitted", { commit: short(e.commit), message: e.message.split("\n")[0] });
      case "pushed":
        return t("eventPushed", { branch: e.branch, commit: short(e.commit) });
      case "pr_opened":
        return t("eventPr", { url: e.url });
      case "pr_merged":
        return t("ticketsUiEventMerged");
      case "pr_closed":
        return t("ticketsUiEventPrClosed");
      case "ticket_commented":
        return t("eventCommented");
      case "ticket_moved":
        return t("ticketsUiEventMoved", { to: e.to });
      case "closed":
        return e.dropped.length ? t("eventClosedForce", { dropped: e.dropped.join(", ") }) : t("eventClosed");
      case "failed":
        return t("eventFailed", { error: e.error });
      case "setup": {
        const failed = e.steps.filter((s) => s.outcome === "failed" || s.outcome === "refused").length;
        const count = e.steps.length;
        const text = failed ? t("wtEventSetupFailed", { count, failed }) : t("wtEventSetup", { count });
        return e.saved ? text : `${text} · ${t("wtDetected")}`;
      }
      case "teardown":
        return t("wtEventTeardown");
    }
  }

  const when = (at: number) =>
    new Date(at * 1000).toLocaleString(lang, { dateStyle: "short", timeStyle: "short" });

  async function act(kind: typeof busy, run: () => Promise<void>) {
    busy = kind;
    stepError = "";
    try {
      await run();
    } catch (e) {
      stepError = String(e);
    } finally {
      busy = "";
    }
  }

  const preview = () =>
    act("preview", async () => {
      step = { kind: "preview", preview: await invoke<DeliveryPreview>("tickets_deliver_preview", { id: run.id }) };
    });

  const deliver = (preview: DeliveryPreview) =>
    act("deliver", async () => {
      const again = !!run.prUrl;
      const delivery = await invoke<Delivery>("tickets_deliver", { id: run.id, confirm: preview.fingerprint });
      upsert(delivery.run);
      step = { kind: "delivered", warning: delivery.warning, again };
    });

  /** Closing and cleaning up refuse alike while work is not pushed. */
  const close = (force: boolean, cleanup = false) =>
    act(cleanup ? "cleanup" : "close", async () => {
      try {
        upsert(await invoke<Run>(cleanup ? "tickets_cleanup" : "tickets_close", { id: run.id, force }));
      } catch (e) {
        const refused = refusal(e);
        if (force || refused.kind === "error") throw refused.message;
        step = { kind: "refused", detail: refused.message, cleanup };
        return;
      }
      step = null;
      tickets.done = t(cleanup ? "ticketsUiCleaned" : "runClosed");
    });

  /** Without a warning, straight to the tile, like a launch. */
  const resume = () =>
    act("resume", async () => {
      step = null;
      const resumed = await invoke<Resumed>("tickets_resume", { id: run.id, cols: 120, rows: 32 });
      upsert(resumed.run);
      if (resumed.warning) step = { kind: "resumed", warning: resumed.warning };
      else await openTile(resumed.run);
    });

  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      tickets.done = t("runLinkCopied");
    } catch (e) {
      tickets.error = String(e);
    }
  }
</script>

{#snippet link(url: string)}
  <span class="link-row">
    <span class="mono">{url}</span>
    <button class="icon-btn sm" onclick={() => openUrl(url)} aria-label={t("ticketsUiOpenLink")} title={t("ticketsUiOpenLink")}>
      <ExternalLink size={13} aria-hidden="true" />
    </button>
    <button class="icon-btn sm" onclick={() => copy(url)} aria-label={t("runCopyLink")} title={t("runCopyLink")}>
      <Copy size={13} aria-hidden="true" />
    </button>
  </span>
{/snippet}

{#snippet failure()}
  {#if stepError}
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{stepError}</p>
  {/if}
{/snippet}

<header>
  <div class="title">
    <StatusPill status={run.status} label={readyToClean(run) ? t("ticketsUiReady") : undefined} />
    <span class="mono muted">{run.ticket.key}</span>
  </div>
  <h2>{run.title}</h2>
</header>

{#if readyToClean(run)}
  {@const merged = run.prState === "merged"}
  <section class="ask ready" aria-labelledby="ready-title">
    <p class="ok" id="ready-title">
      {#if merged}<GitMerge size={14} aria-hidden="true" />{:else}<GitPullRequestClosed
          size={14}
          aria-hidden="true"
        />{/if}
      {t(merged ? "ticketsUiMergedTitle" : "ticketsUiClosedTitle")}
    </p>
    <p>{t("ticketsUiCleanHelp")}</p>
    <div class="buttons">
      <button class="btn sm primary" onclick={() => close(false, true)} disabled={!!busy}>
        {#if busy === "cleanup"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<BrushCleaning
            size={14}
            aria-hidden="true"
          />{/if}
        {t("ticketsUiClean")}
      </button>
    </div>
  </section>
{/if}

<div class="actions">
  {#if session}
    <button class="btn sm" onclick={() => openTile(run)}>
      <SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}
    </button>
  {:else if resumable(run)}
    <button class="btn sm" onclick={resume} disabled={!!busy}>
      {#if busy === "resume"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<RotateCcw
          size={14}
          aria-hidden="true"
        />{/if}
      {t("ticketsUiResume")}
    </button>
  {/if}
  {#if open}
    <button class="btn sm" aria-expanded={step?.kind === "preview"} onclick={preview} disabled={!!busy}>
      {#if busy === "preview"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Send
          size={14}
          aria-hidden="true"
        />{/if}
      {t("runDeliver")}
    </button>
  {/if}
  {#if run.status !== "closed"}
    <button
      class="btn sm"
      aria-expanded={step?.kind === "close" || step?.kind === "refused" || step?.kind === "force"}
      onclick={() => ((step = { kind: "close" }), (stepError = ""))}
    >
      <Archive size={14} aria-hidden="true" />{t("runClose")}
    </button>
  {/if}
</div>
{#if open && !session}
  <p class="muted small">{t("runNoSession")}</p>
{/if}
{#if untrusted}
  <div class="trust">
    <Banner kind="warn" icon={ShieldQuestionMark}>
      {t("ticketsUiTrust")}
      {#snippet action()}
        <button class="btn sm" onclick={() => openTile(run)}>
          <SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}
        </button>
      {/snippet}
    </Banner>
  </div>
{/if}

{#if step?.kind === "preview"}
  {@const p = step.preview}
  <section class="ask" aria-labelledby="deliver-title">
    <h3 id="deliver-title">{t("deliverTitle")}</h3>
    {#if p.files.length}
      <p class="sub">{t("deliverFiles", { count: p.files.length })}</p>
      <ul class="changes">
        {#each p.files as file (file.path)}
          <li>
            <span class="change {file.change}">{t(changeLabel[file.change])}</span>
            <span class="mono">{file.from ? `${file.from} → ${file.path}` : file.path}</span>
          </li>
        {/each}
      </ul>
    {/if}
    {#if p.commits.length}
      <p class="sub">{t("deliverCommits", { count: p.commits.length })}</p>
      <ul class="changes">
        {#each p.commits as commit (commit.commit)}
          <li><span class="mono muted">{short(commit.commit)}</span><span>{commit.subject}</span></li>
        {/each}
      </ul>
    {/if}
    {#if !p.files.length && !p.commits.length}
      <p class="notice"><Info size={14} aria-hidden="true" />{t("deliverNothing")}</p>
    {/if}
    <p class="how">
      {run.prUrl
        ? t("deliverHowAgain", { branch: run.branch ?? "" })
        : t("deliverHow", { branch: run.branch ?? "", target: (run.base ?? "").replace(/^origin\//, "") })}
    </p>
    <p class="how muted">
      {t("deliverFingerprint", { fingerprint: p.fingerprint.split(":").map(short).join(":") })}
    </p>
    {@render failure()}
    <div class="buttons">
      <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
      {#if stepError}
        <button class="btn sm" onclick={preview} disabled={!!busy}>{t("deliverAgain")}</button>
      {/if}
      <button class="btn sm primary" onclick={() => deliver(p)} disabled={!!busy}>
        {#if busy === "deliver"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
        {t("deliverConfirm")}
      </button>
    </div>
  </section>
{:else if step?.kind === "delivered"}
  <section class="ask done" role="status">
    <p class="ok"><Check size={14} aria-hidden="true" />{t(step.again ? "deliveredAgain" : "delivered")}</p>
    {#if run.prUrl}{@render link(run.prUrl)}{/if}
    {#if step.warning}
      <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{step.warning}</p>
    {/if}
    <div class="buttons">
      <button class="btn sm" onclick={() => (step = null)}>{t("dismiss")}</button>
    </div>
  </section>
{:else if step?.kind === "resumed"}
  <section class="ask done" role="status">
    <p class="ok"><Check size={14} aria-hidden="true" />{t("ticketsUiResumed")}</p>
    <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{step.warning}</p>
    <div class="buttons">
      <button class="btn sm" onclick={() => (step = null)}>{t("dismiss")}</button>
      {#if session}
        <button class="btn sm primary" onclick={() => openTile(run)}>
          <SquareTerminal size={14} aria-hidden="true" />{t("runOpenTile")}
        </button>
      {/if}
    </div>
  </section>
{:else if step?.kind === "close"}
  <section class="ask confirm" role="alert">
    <span>{t("closeConfirm")}</span>
    {@render failure()}
    <div class="buttons">
      <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
      <button class="btn sm primary" onclick={() => close(false)} disabled={!!busy}>
        {#if busy === "close"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
        {t("closeRun")}
      </button>
    </div>
  </section>
{:else if step?.kind === "refused"}
  <section class="ask confirm warn" role="alert">
    <p class="refused">
      <TriangleAlert size={14} aria-hidden="true" />{t(step.cleanup ? "ticketsUiCleanRefused" : "closeRefused")}
    </p>
    <p class="mono muted">{step.detail}</p>
    <div class="buttons">
      <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
      {#if open}<button class="btn sm" onclick={preview} disabled={!!busy}>{t("runDeliver")}</button>{/if}
      <button class="btn sm" onclick={() => { step = { kind: "force" }; }}>{t("closeForce")}</button>
    </div>
  </section>
{:else if step?.kind === "force"}
  <section class="ask confirm" role="alert">
    <span>{t("closeForceConfirm", { branch: run.branch ?? "", repo: repoName(run.repo) })}</span>
    {@render failure()}
    <div class="buttons">
      <button class="btn sm" onclick={() => (step = null)}>{t("cancel")}</button>
      <button class="btn sm danger" onclick={() => close(true)} disabled={!!busy}>
        {#if busy === "close"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
        {t("closeForceRun")}
      </button>
    </div>
  </section>
{:else if stepError}
  <div class="ask">{@render failure()}</div>
{/if}

<dl class="meta">
  <dt>{t("runMetaTicket")}</dt>
  <dd>{@render link(run.ticket.url)}</dd>
  <dt>{t("runMetaProject")}</dt>
  <dd>{project}</dd>
  <dt>{t("runMetaRepo")}</dt>
  <dd>{repoName(run.repo)} <span class="mono muted">{shortPath(run.repo)}</span></dd>
  {#if run.branch}
    <dt>{t("runMetaBranch")}</dt>
    <dd>
      <span class="mono">{run.branch}</span>
      {#if run.base}<span class="muted"> {t("runBranchFrom", { base: run.base })}</span>{/if}
    </dd>
  {/if}
  {#if run.worktree && run.status !== "closed"}
    <dt>{t("runMetaWorktree")}</dt>
    <dd class="mono">{shortPath(run.worktree)}</dd>
  {/if}
  <dt>{t("runMetaPr")}</dt>
  <dd>{#if run.prUrl}{@render link(run.prUrl)}{:else}—{/if}</dd>
  <dt>{t("runMetaCreated")}</dt>
  <dd>{ago(run.created)}</dd>
  <dt>{t("runMetaUpdated")}</dt>
  <dd>{ago(run.updated)}</dd>
</dl>

{#if run.prUrl}<CiChecks {run} {session} />{/if}

<h3>{t("runJournal")}</h3>
<ol class="journal">
  {#each events as event (event.id)}
    <li class:failed={event.kind === "failed"}>
      <time datetime={new Date(event.at * 1000).toISOString()}>{when(event.at)}</time>
      <span>{describe(event)}</span>
    </li>
  {:else}
    <li class="muted">{t("runJournalEmpty")}</li>
  {/each}
</ol>

<style>
  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 6px 0 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 16px 0 0;
  }

  .small {
    margin: 8px 0 0;
    font-size: var(--text-sm);
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .ask.warn {
    border-color: var(--waiting-ring);
    background: var(--waiting-soft);
  }

  .ask.ready {
    background: var(--done-soft);
  }

  .trust {
    margin-top: 12px;
  }

  .ask p {
    margin: 0;
  }

  .sub {
    color: var(--muted);
    font-weight: 600;
  }

  .changes {
    max-height: 220px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .changes li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 2px 0;
  }

  .change {
    flex-shrink: 0;
    width: 64px;
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .added {
    color: var(--done);
  }

  .modified,
  .renamed {
    color: var(--warning);
  }

  .deleted {
    color: var(--danger);
  }

  .how {
    line-height: 1.5;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }

  .ok,
  .refused {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }

  .ok {
    color: var(--done);
  }

  .refused {
    color: var(--warning);
  }

  .link-row {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: baseline;
    gap: 6px 16px;
    margin: 20px 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .mono {
    overflow-wrap: anywhere;
  }

  .journal {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .journal li {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 12px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
    overflow-wrap: anywhere;
  }

  .journal time {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .journal .failed {
    color: var(--danger);
  }
</style>
