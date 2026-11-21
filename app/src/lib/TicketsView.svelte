<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Inbox from "@lucide/svelte/icons/inbox";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Ticket from "@lucide/svelte/icons/ticket";
  import X from "@lucide/svelte/icons/x";
  import RunStatus from "$lib/RunStatus.svelte";
  import TicketRun, { openTile } from "$lib/TicketRun.svelte";
  import { ago, t } from "$lib/i18n";
  import { app, repoName, ui } from "$lib/state.svelte";
  import {
    loadInbox,
    refresh,
    tickets,
    upsert,
    type Resolution,
    type Run,
    type TicketSummary,
  } from "$lib/tickets.svelte";

  let input = $state("");
  let resolving = $state(false);
  /** A resolved ticket waiting to be launched: `from` is where it was asked, the paste field or an inbox row. */
  let pending = $state<{ from: string; resolution: Resolution; repo: string } | null>(null);
  /** The URL of the ticket being launched. */
  let starting = $state("");

  const projects = $derived(app.workspace?.projects ?? []);
  const project = $derived(projects.find((p) => p.id === tickets.scope));
  const projectName = (id: string) => projects.find((p) => p.id === id)?.name ?? id;
  const runs = $derived(tickets.scope ? tickets.runs.filter((r) => r.project === tickets.scope) : tickets.runs);
  const current = $derived(runs.find((r) => r.id === tickets.selected) ?? runs[0]);

  // A deleted project falls back to every ticket.
  $effect(() => {
    if (tickets.scope && app.workspace && !project) {
      tickets.scope = "";
      scoped();
    }
  });

  onMount(() => {
    const unlisten = listen<Run>("run-updated", ({ payload }) => upsert(payload));
    return () => unlisten.then((stop) => stop());
  });

  function scoped() {
    pending = null;
    tickets.inbox = null;
    loadInbox();
  }

  async function resolve(text: string, project: string | null): Promise<Resolution | null> {
    tickets.error = "";
    try {
      return await invoke<Resolution>("tickets_resolve", { input: text, project });
    } catch (e) {
      tickets.error = String(e);
      return null;
    }
  }

  async function preview(e: SubmitEvent) {
    e.preventDefault();
    resolving = true;
    const resolution = await resolve(input.trim(), tickets.scope || null);
    resolving = false;
    pending = resolution && { from: "paste", resolution, repo: resolution.repo ?? "" };
  }

  /** A ticket whose repository is clear starts at once; otherwise the row asks which one. */
  async function launch(ticket: TicketSummary) {
    pending = null;
    starting = ticket.url;
    const resolution = await resolve(ticket.url, ticket.project);
    if (resolution?.repo) return start(resolution, resolution.repo);
    starting = "";
    if (resolution) pending = { from: ticket.url, resolution, repo: "" };
  }

  async function start(resolution: Resolution, repo: string) {
    starting = resolution.ticket.url;
    tickets.error = "";
    try {
      const run = await invoke<Run>("tickets_run", {
        input: resolution.ticket.url,
        project: resolution.project,
        repo,
        cols: 120,
        rows: 32,
      });
      upsert(run);
      if (pending?.from === "paste") input = "";
      pending = null;
      Object.assign(tickets, { tab: "runs", selected: run.id });
      await openTile(run);
    } catch (e) {
      tickets.error = String(e);
    } finally {
      starting = "";
    }
  }

  function focusPaste(el: HTMLInputElement) {
    if (!tickets.paste) return;
    tickets.paste = false;
    tick().then(() => el.focus());
  }
</script>

{#snippet launching(spinning: boolean)}
  {#if spinning}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("ticketLaunching")}{:else}<Play
      size={14}
      aria-hidden="true"
    />{t("ticketLaunch")}{/if}
{/snippet}

{#snippet launchBox()}
  {#if pending}
    {@const r = pending.resolution}
    {@const known = tickets.inbox?.find((ticket) => ticket.url === r.ticket.url)}
    {@const chosen = r.repos.find((repo) => repo.path === pending?.repo)}
    <div class="launch" role="group" aria-label={t("ticketLaunchTitle", { key: r.ticket.key })}>
      <div class="what">
        <span class="name">
          <span class="mono">{r.ticket.key}</span>{known ? ` ${known.title}` : ""}
        </span>
        <span class="desc">
          {t("ticketInProject", { project: projectName(r.project) })}{r.repo && chosen
            ? ` · ${t("ticketInRepo", { repo: chosen.name })}`
            : ""}
        </span>
      </div>
      {#if !r.repos.length}
        <p class="notice"><Info size={14} aria-hidden="true" />{t("ticketNoRepo", { project: projectName(r.project) })}</p>
      {:else if !r.repo}
        <label class="labelled">
          <span>{t("ticketRepoChoose")}</span>
          <select class="field" bind:value={pending.repo}>
            <option value="" disabled>{t("ticketRepoPick")}</option>
            {#each r.repos as repo (repo.path)}
              <option value={repo.path}>{repo.name}</option>
            {/each}
          </select>
        </label>
      {/if}
      <div class="buttons">
        <button class="btn sm" onclick={() => (pending = null)}>{t("cancel")}</button>
        <button
          class="btn sm primary"
          disabled={!pending.repo || !!starting}
          onclick={() => pending && start(r, pending.repo)}
        >
          {@render launching(starting === r.ticket.url)}
        </button>
      </div>
    </div>
  {/if}
{/snippet}

<div class="tickets">
  <div class="toolbar">
    <label class="scope">
      <span>{t("libraryScope")}</span>
      <select class="field" bind:value={tickets.scope} onchange={scoped}>
        <option value="">{t("ticketsAll")}</option>
        {#if projects.length}
          <optgroup label={t("projects")}>
            {#each projects as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </optgroup>
        {/if}
      </select>
    </label>
    <form class="paste" onsubmit={preview}>
      <Ticket size={14} aria-hidden="true" />
      <input
        class="field"
        placeholder={t("ticketPastePlaceholder")}
        aria-label={t("ticketPaste")}
        autocomplete="off"
        spellcheck="false"
        bind:value={input}
        {@attach focusPaste}
      />
      <button class="btn sm" type="submit" disabled={!input.trim() || resolving}>
        {#if resolving}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
        {t("ticketPreview")}
      </button>
    </form>
    <button class="btn sm" onclick={refresh} disabled={tickets.loading}>
      <RefreshCw size={14} aria-hidden="true" class={tickets.loading ? "spin" : ""} />{t("ticketsRefresh")}
    </button>
  </div>

  <div class="tabs" role="tablist" aria-label={t("viewTickets")}>
    <button role="tab" aria-selected={tickets.tab === "inbox"} onclick={() => (tickets.tab = "inbox")}>
      {t("ticketsTabInbox")}{#if tickets.inbox}<span class="count">{tickets.inbox.length}</span>{/if}
    </button>
    <button role="tab" aria-selected={tickets.tab === "runs"} onclick={() => (tickets.tab = "runs")}>
      {t("ticketsTabRuns")}<span class="count">{runs.length}</span>
    </button>
  </div>

  {#if tickets.error || tickets.done || pending?.from === "paste"}
    <div class="notices">
      {#if tickets.error}
        <div class="notice error banner" role="alert">
          <CircleAlert size={14} aria-hidden="true" />
          <span>{tickets.error}</span>
          <button class="icon-btn sm" onclick={() => (tickets.error = "")} aria-label={t("dismiss")} title={t("dismiss")}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      {/if}
      {#if tickets.done}
        <div class="notice ok banner" role="status">
          <Check size={14} aria-hidden="true" />
          <span>{tickets.done}</span>
          <button class="icon-btn sm" onclick={() => (tickets.done = "")} aria-label={t("dismiss")} title={t("dismiss")}>
            <X size={14} aria-hidden="true" />
          </button>
        </div>
      {/if}
      {#if pending?.from === "paste"}{@render launchBox()}{/if}
    </div>
  {/if}

  {#if tickets.tab === "inbox"}
    <div class="pane">
      {#if tickets.inboxError}
        <div class="empty">
          <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{t("ticketsError", { error: tickets.inboxError })}</p>
          <button class="btn" onclick={loadInbox}><RefreshCw size={14} aria-hidden="true" />{t("ticketsRetry")}</button>
        </div>
      {:else if !tickets.inbox}
        <p class="loading muted" role="status">
          <LoaderCircle size={16} aria-hidden="true" class="spin" />{t("ticketsLoading")}
        </p>
      {:else if !tickets.inbox.length && project && !project.repos.length}
        <div class="empty">
          <Inbox size={20} aria-hidden="true" />
          <h2>{t("ticketsNoSource", { project: project.name })}</h2>
          <p>{t("ticketsNoSourceHelp")}</p>
          <button class="btn primary" onclick={() => (ui.editing = project.id)}>
            <Pencil size={14} aria-hidden="true" />{t("ticketsEditProject")}
          </button>
        </div>
      {:else if !tickets.inbox.length}
        <div class="empty">
          <Inbox size={20} aria-hidden="true" />
          <h2>{t("ticketsEmpty")}</h2>
          <p>{project ? t("ticketsEmptyProject", { project: project.name }) : t("ticketsEmptyAll")}</p>
          {#if project}
            <button class="btn" onclick={() => (ui.editing = project.id)}>
              <Pencil size={14} aria-hidden="true" />{t("ticketsEditProject")}
            </button>
          {/if}
        </div>
      {:else}
        <ul class="inbox">
          {#each tickets.inbox as ticket (ticket.url)}
            <li class="ticket">
              <div class="what">
                <span class="name">{ticket.title}</span>
                <span class="desc">
                  <span class="mono">{ticket.key}</span> · {ticket.provider === "github" ? "GitHub" : ticket.provider}
                  · {t("ticketUpdated", { ago: ago(Date.parse(ticket.updated) / 1000) })}
                  {#if !tickets.scope && ticket.project} · {projectName(ticket.project)}{/if}
                </span>
              </div>
              <div class="labels">
                {#each ticket.labels as label (label)}<span class="badge">{label}</span>{/each}
              </div>
              {#if ticket.project}
                <button
                  class="btn sm"
                  title={t("ticketLaunchTitle", { key: ticket.key })}
                  disabled={!!starting}
                  onclick={() => launch(ticket)}
                >
                  {@render launching(starting === ticket.url)}
                </button>
              {:else}
                <span class="badge" title={t("ticketNoProjectTitle")}>{t("ticketNoProject")}</span>
              {/if}
            </li>
            {#if pending?.from === ticket.url}
              <li class="choice">{@render launchBox()}</li>
            {/if}
          {/each}
        </ul>
      {/if}
    </div>
  {:else if !runs.length}
    <div class="pane">
      <div class="empty">
        <Ticket size={20} aria-hidden="true" />
        <h2>{t("runsEmpty")}</h2>
        <p>{t("runsEmptyHelp")}</p>
      </div>
    </div>
  {:else}
    <div class="split">
      <ul class="list" aria-label={t("ticketsTabRuns")}>
        {#each runs as run (run.id)}
          <li class="row" class:current={run === current}>
            <button class="pick" aria-current={run === current} onclick={() => (tickets.selected = run.id)}>
              <span class="name"><span class="mono">{run.ticket.key}</span> {run.title}</span>
              <span class="desc">{repoName(run.repo)}{run.branch ? ` · ${run.branch}` : ""} · {ago(run.updated)}</span>
            </button>
            <RunStatus status={run.status} />
          </li>
        {/each}
      </ul>
      <section class="detail" aria-live="polite">
        {#if current}
          {#key current.id}
            <TicketRun run={current} />
          {/key}
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .tickets {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: var(--chrome);
  }

  .scope {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .scope select {
    width: 200px;
  }

  .paste {
    position: relative;
    display: flex;
    flex: 0 1 520px;
    gap: 6px;
    min-width: 0;
    margin-left: auto;
    color: var(--muted);
  }

  .paste > :global(svg) {
    position: absolute;
    top: 8px;
    left: 9px;
    pointer-events: none;
  }

  .paste .field {
    height: 26px;
    padding-left: 28px;
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--muted);
    font-weight: 500;
    box-shadow: inset 0 -2px 0 transparent;
    transition:
      color var(--dur-fast) ease-out,
      box-shadow var(--dur-fast) ease-out;
  }

  .tabs button:hover {
    color: var(--text);
  }

  .tabs button[aria-selected="true"] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: center;
  }

  .notices {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
  }

  .banner {
    align-items: center;
  }

  .banner > span {
    flex: 1;
  }

  .banner .icon-btn {
    color: inherit;
  }

  .notice.ok {
    background: var(--done-soft);
    color: var(--done);
  }

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
  }

  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.spin) {
      animation: none;
    }
  }

  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 480px;
    margin: 48px auto 0;
    color: var(--muted);
    text-align: center;
  }

  .empty > :global(svg) {
    color: var(--accent-ink);
  }

  .empty h2 {
    margin: 8px 0 6px;
    color: var(--text);
    font-size: var(--text-md);
  }

  .empty p {
    margin: 0;
  }

  .empty .btn {
    margin-top: 14px;
  }

  .inbox {
    max-width: 1100px;
    margin: 0;
    padding: 8px 12px 32px;
    list-style: none;
  }

  .ticket {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
  }

  .ticket:hover {
    background: var(--hover);
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.5;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .labels {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
    max-width: 40%;
  }

  .choice {
    padding: 0 10px 10px;
    border-bottom: 1px solid var(--border);
  }

  .launch {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .labelled {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 340px;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .split {
    display: grid;
    flex: 1;
    grid-template-columns: 380px minmax(0, 1fr);
    min-height: 0;
  }

  .list {
    min-height: 0;
    margin: 0;
    padding: 8px 6px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
    list-style: none;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-right: 8px;
    border-radius: var(--radius);
  }

  .row:hover {
    background: var(--hover);
  }

  .row.current {
    background: var(--selected);
  }

  .pick {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .detail {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .detail > :global(*) {
    max-width: 820px;
  }
</style>
