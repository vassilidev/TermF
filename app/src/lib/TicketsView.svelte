<script lang="ts">
  import { tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Inbox from "@lucide/svelte/icons/inbox";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Ticket from "@lucide/svelte/icons/ticket";
  import TicketRun, { openTile } from "$lib/TicketRun.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { ago, t } from "$lib/i18n";
  import { editProject } from "$lib/project.svelte";
  import { app, repoName } from "$lib/state.svelte";
  import {
    loadInbox,
    refresh,
    tickets,
    upsert,
    type Resolution,
    type Run,
    type RunEvent,
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
  const providers: Record<string, Parameters<typeof t>[0]> = { github: "providerGithub" };
  const sourceNames: Record<string, Parameters<typeof t>[0]> = {
    github: "ticketsGithubIssues",
    githubProject: "ticketsGithubProject",
    jira: "ticketsJira",
  };
  const sourceName = (id: string) =>
    id in sourceNames ? t(sourceNames[id]) : t("ticketsMcpSource", { name: id.replace(/^mcp:/, "") });
  const runs = $derived(tickets.scope ? tickets.runs.filter((r) => r.project === tickets.scope) : tickets.runs);
  const current = $derived(runs.find((r) => r.id === tickets.selected) ?? runs[0]);

  // The inbox follows the current project; showTickets asked for the first one.
  let inboxScope = tickets.scope;
  $effect(() => {
    const scope = tickets.scope;
    if (scope === inboxScope) return;
    inboxScope = scope;
    untrack(scoped);
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
      // The run's log names the session it just opened: its id holds for this launch of the app.
      const events = await invoke<RunEvent[]>("tickets_events", { id: run.id }).catch(() => []);
      for (const e of events) if (e.kind === "session_opened") tickets.sessions[run.id] = e.session;
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
  <ViewHeader title={t("viewTickets")} context={project?.name ?? t("ticketsAll")}>
    {#snippet actions()}
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
    {/snippet}
  </ViewHeader>

  <Tabs
    id="tickets"
    label={t("viewTickets")}
    tabs={[
      { id: "inbox", label: t("ticketsTabInbox"), count: tickets.inbox?.length },
      { id: "runs", label: t("ticketsTabRuns"), count: runs.length },
    ]}
    selected={tickets.tab}
    onselect={(tab) => (tickets.tab = tab as typeof tickets.tab)}
  >
    {#if tickets.error || tickets.done || pending?.from === "paste"}
      <div class="banners">
        {#if tickets.error}
          <Banner kind="error" ondismiss={() => (tickets.error = "")}>{tickets.error}</Banner>
        {/if}
        {#if tickets.done}
          <Banner kind="ok" ondismiss={() => (tickets.done = "")}>{tickets.done}</Banner>
        {/if}
        {#if pending?.from === "paste"}{@render launchBox()}{/if}
      </div>
    {/if}

    {#if tickets.tab === "inbox"}
      <div class="pane">
        {#if tickets.inbox && tickets.inboxErrors.length}
          <div class="source-errors">
            {#each tickets.inboxErrors as failed (failed.source)}
              <p class="notice error" role="alert">
                <CircleAlert size={14} aria-hidden="true" />{t("ticketsSourceFailed", {
                  source: sourceName(failed.source),
                  error: failed.message,
                })}
              </p>
            {/each}
          </div>
        {/if}
        {#if tickets.inboxError}
          <EmptyState>
            <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{t("ticketsError", { error: tickets.inboxError })}</p>
            <button class="btn" onclick={loadInbox}><RefreshCw size={14} aria-hidden="true" />{t("ticketsRetry")}</button>
          </EmptyState>
        {:else if !tickets.inbox}
          <p class="loading muted" role="status">
            <LoaderCircle size={16} aria-hidden="true" class="spin" />{t("ticketsLoading")}
          </p>
        {:else if !tickets.inbox.length}
          <EmptyState
            icon={Inbox}
            title={t("ticketsEmpty")}
            text={project ? t("ticketsEmptyProject", { project: project.name }) : t("ticketsEmptyAll")}
          >
            {#if project}
              <button class="btn" onclick={() => editProject(project.id)}>
                <Pencil size={14} aria-hidden="true" />{t("ticketsEditProject")}
              </button>
            {/if}
          </EmptyState>
        {:else}
          <ul class="inbox">
            {#each tickets.inbox as ticket (ticket.url)}
              <li class="ticket">
                <div class="what">
                  <span class="name">{ticket.title}</span>
                  <span class="desc">
                    <span class="mono">{ticket.key}</span> · {ticket.provider in providers
                      ? t(providers[ticket.provider])
                      : ticket.provider}
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
        <EmptyState icon={Ticket} title={t("runsEmpty")} text={t("runsEmptyHelp")} />
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
              <StatusPill status={run.status} />
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
  </Tabs>
</div>

<style>
  .tickets {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
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

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .source-errors {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-width: 1100px;
    padding: 12px 12px 0;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
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
    font-size: var(--text-sm);
    line-height: 1.5;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    width: 340px;
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }
</style>
