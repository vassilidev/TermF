<script lang="ts">
  import { tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Archive from "@lucide/svelte/icons/archive";
  import BrushCleaning from "@lucide/svelte/icons/brush-cleaning";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Inbox from "@lucide/svelte/icons/inbox";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Ticket from "@lucide/svelte/icons/ticket";
  import Users from "@lucide/svelte/icons/users";
  import TeamOptions from "$lib/TeamOptions.svelte";
  import TicketRun, { openTile } from "$lib/TicketRun.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import SelectionBar from "$lib/ui/SelectionBar.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { isMac } from "$lib/files.svelte";
  import { ago, t } from "$lib/i18n";
  import { editProject } from "$lib/project.svelte";
  import { all, none, pick, settle, within, type Outcome } from "$lib/select-logic";
  import { app, repoName } from "$lib/state.svelte";
  import { budgetError, type TeamRun } from "$lib/team-run-logic";
  import { showTeamRun, upsertRun } from "$lib/team.svelte";
  import { readyToClean, refusal } from "$lib/tickets-logic";
  import {
    launchTicket,
    loadInbox,
    openUrl,
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
  let pending = $state<{ from: string; resolution: Resolution; repo: string; team?: boolean } | null>(null);
  let teamWorkflow = $state("");
  let teamBudget = $state<number | null>(null);
  /** The URL of the ticket being launched. */
  let starting = $state("");

  const projects = $derived(app.workspace?.projects ?? []);
  const project = $derived(projects.find((p) => p.id === tickets.scope));
  const projectName = (id: string) => projects.find((p) => p.id === id)?.name ?? id;
  const sourceNames: Record<string, Parameters<typeof t>[0]> = {
    github: "ticketsGithubIssues",
    githubProject: "ticketsGithubProject",
    jira: "ticketsJira",
  };
  const sourceName = (id: string) =>
    id in sourceNames ? t(sourceNames[id]) : t("ticketsMcpSource", { name: id.replace(/^mcp:/, "") });
  const runs = $derived(tickets.scope ? tickets.runs.filter((r) => r.project === tickets.scope) : tickets.runs);
  const current = $derived(runs.find((r) => r.id === tickets.selected) ?? runs[0]);

  let pickedTickets = $state(none<string>());
  let pickedRuns = $state(none<number>());
  const launchable = $derived((tickets.inbox ?? []).filter((ticket) => ticket.project).map((ticket) => ticket.url));
  const chosenTickets = $derived(within(pickedTickets.keys, launchable));
  const chosenRuns = $derived(runs.filter((r) => pickedRuns.keys.includes(r.id)));
  const closable = $derived(chosenRuns.filter((r) => r.status !== "closed"));
  const cleanable = $derived(chosenRuns.filter(readyToClean));
  let asking = $state<"close" | "cleanup" | null>(null);
  let bulk = $state(false);
  /** Why the last bulk action left some picked, one per list. */
  let leftTickets = $state("");
  let leftRuns = $state("");

  const toggleKey = (e: MouseEvent) => (isMac ? e.metaKey : e.ctrlKey);

  /** The box's own state follows the selection, not the click. */
  function pickTicket(e: MouseEvent, url: string) {
    e.preventDefault();
    leftTickets = "";
    pickedTickets = pick(pickedTickets, launchable, url, e.shiftKey);
  }

  function pickRun(e: MouseEvent, id: number) {
    e.preventDefault();
    leftRuns = "";
    asking = null;
    pickedRuns = pick(pickedRuns, runs.map((r) => r.id), id, e.shiftKey);
  }

  function rowClick(e: MouseEvent, run: () => void) {
    if (!(toggleKey(e) || e.shiftKey) || (e.target as Element).closest("button:not(.pick), a, input, select")) return;
    run();
  }

  /** Why some stay picked: a reason the user can act on (`kept`), then failures, whose errors the banner shows. */
  function leftOf(out: { kept: unknown[]; failed: unknown[] }, keptLabel: Parameters<typeof t>[0]) {
    return [
      out.kept.length ? t(keptLabel, { count: out.kept.length }) : "",
      out.failed.length ? t("t6bFailed", { count: out.failed.length }) : "",
    ].filter(Boolean).join(" ");
  }

  /** One after the other; a ticket whose repository must be chosen stays picked, to launch from its row. */
  async function launchPicked() {
    const list = (tickets.inbox ?? []).filter((ticket) => chosenTickets.includes(ticket.url));
    bulk = true;
    pending = null;
    const results: [string, Outcome][] = [];
    for (const ticket of list) {
      starting = ticket.url;
      // `resolve` clears the error of the ticket before: each one is kept here.
      const resolution = await resolve(ticket.url, ticket.project);
      if (!resolution) {
        results.push([ticket.url, { error: tickets.error }]);
        continue;
      }
      if (!resolution.repo) {
        results.push([ticket.url, "kept"]);
        continue;
      }
      try {
        upsert(await invoke<Run>("tickets_run", { input: ticket.url, project: resolution.project, repo: resolution.repo, cols: 120, rows: 32 }));
        results.push([ticket.url, "done"]);
      } catch (e) {
        results.push([ticket.url, { error: String(e) }]);
      }
    }
    starting = "";
    bulk = false;
    const out = settle(results);
    tickets.error = out.errors.join(" · ");
    pickedTickets = { keys: [...out.kept, ...out.failed], anchor: null };
    if (out.done) tickets.done = t("t6bLaunched", { count: out.done });
    leftTickets = leftOf(out, "t6bLaunchLeft");
  }

  /** Never by force: a run with work not pushed stays picked, to open and decide on its own. */
  async function closePicked(kind: "close" | "cleanup") {
    const list = kind === "close" ? closable : cleanable;
    asking = null;
    bulk = true;
    const results: [number, Outcome][] = [];
    for (const run of list) {
      try {
        upsert(await invoke<Run>(kind === "close" ? "tickets_close" : "tickets_cleanup", { id: run.id, force: false }));
        results.push([run.id, "done"]);
      } catch (e) {
        const why = refusal(e);
        results.push([run.id, why.kind === "error" ? { error: why.message } : "kept"]);
      }
    }
    bulk = false;
    const out = settle(results);
    if (out.errors.length) tickets.error = out.errors.join(" · ");
    pickedRuns = { keys: [...out.kept, ...out.failed], anchor: null };
    if (out.done) tickets.done = t(kind === "close" ? "t6bClosed" : "t6bCleaned", { count: out.done });
    leftRuns = leftOf(out, "t6bRefused");
  }

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

  /** The team starts in a new run of the ticket, which opens no session of its own: the workflow and budget first. */
  async function launchTeam(ticket: TicketSummary) {
    pending = null;
    starting = ticket.url;
    const resolution = await resolve(ticket.url, ticket.project);
    starting = "";
    if (resolution) pending = { from: ticket.url, resolution, repo: resolution.repo ?? "", team: true };
  }

  async function startTeam(resolution: Resolution, repo: string) {
    starting = resolution.ticket.url;
    tickets.error = "";
    let run: Run | null = null;
    try {
      run = await invoke<Run>("tickets_run", { input: resolution.ticket.url, project: resolution.project, repo, session: false });
      upsert(run);
      const options = { workflow: teamWorkflow || undefined, budget: teamBudget ?? undefined };
      const started = await invoke<TeamRun>("team_start", { run: run.id, options });
      upsertRun(started);
      pending = null;
      showTeamRun(started.run);
    } catch (e) {
      tickets.error = String(e);
      // The ticket run exists: its detail starts the team again.
      if (run) {
        pending = null;
        Object.assign(tickets, { tab: "runs", selected: run.id });
      }
    } finally {
      starting = "";
    }
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
      {#if pending.team}
        <TeamOptions project={r.project} bind:workflow={teamWorkflow} bind:budget={teamBudget} />
      {/if}
      <div class="buttons">
        <button class="btn sm" onclick={() => (pending = null)}>{t("cancel")}</button>
        {#if pending.team}
          <button
            class="btn sm primary"
            disabled={!pending.repo || !!starting || !!budgetError(teamBudget)}
            onclick={() => pending && startTeam(r, pending.repo)}
          >
            {#if starting === r.ticket.url}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Users
                size={14}
                aria-hidden="true"
              />{/if}{t("teamRunsLaunchLong")}
          </button>
        {:else}
        <button
          class="btn sm primary"
          disabled={!pending.repo || !!starting}
          onclick={() => pending && start(r, pending.repo)}
        >
          {@render launching(starting === r.ticket.url)}
        </button>
        {/if}
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
          {@const unset = tickets.configured === false && project}
          <EmptyState
            icon={Inbox}
            title={unset ? t("ticketsUiNoSource", { project: unset.name }) : t("ticketsEmpty")}
            text={unset
              ? t("ticketsUiNoSourceHelp")
              : project
                ? t("ticketsEmptyProject", { project: project.name })
                : t("ticketsEmptyAll")}
          >
            {#if project}
              <button class="btn" onclick={() => editProject(project.id)}>
                <Pencil size={14} aria-hidden="true" />{t("ticketsEditProject")}
              </button>
            {/if}
          </EmptyState>
        {:else}
          {#if chosenTickets.length}
            <div class="sticky">
              <SelectionBar
                count={chosenTickets.length}
                total={launchable.length}
                label={t("ticketsTabInbox")}
                onall={() => ((leftTickets = ""), (pickedTickets = all(pickedTickets, launchable)))}
                onclear={() => ((leftTickets = ""), (pickedTickets = none()))}
              >
                {#if leftTickets}<span class="note">{leftTickets}</span>{/if}
                <button class="btn sm primary" disabled={bulk || !!starting} onclick={launchPicked}>
                  {@render launching(bulk)}{#if !bulk}{` · ${chosenTickets.length}`}{/if}
                </button>
              </SelectionBar>
            </div>
          {/if}
          <ul class="inbox" class:picking={chosenTickets.length > 0}>
            {#each tickets.inbox as ticket (ticket.url)}
              <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions (the box does it from the keyboard) -->
              <li
                class="ticket"
                class:picked={chosenTickets.includes(ticket.url)}
                onclick={(e) => ticket.project && rowClick(e, () => pickTicket(e, ticket.url))}
              >
                {#if ticket.project}
                  <input
                    class="box"
                    type="checkbox"
                    checked={chosenTickets.includes(ticket.url)}
                    aria-label={t("t6bPick", { name: `${ticket.key} ${ticket.title}` })}
                    onclick={(e) => pickTicket(e, ticket.url)}
                  />
                {:else}
                  <span class="box" aria-hidden="true"></span>
                {/if}
                <div class="what">
                  <span class="name">{ticket.title}</span>
                  <span class="desc">
                    <span class="mono">{ticket.key}</span> · {sourceName(ticket.source)}
                    · {t("ticketUpdated", { ago: ago(Date.parse(ticket.updated) / 1000) })}
                    {#if ticket.assignees.length}
                      · {t("ticketsUiAssignees", { names: ticket.assignees.join(", ") })}
                    {/if}
                    {#if !tickets.scope && ticket.project} · {projectName(ticket.project)}{/if}
                  </span>
                </div>
                <div class="labels">
                  {#each ticket.labels as label (label)}<span class="badge">{label}</span>{/each}
                </div>
                <button
                  class="icon-btn sm"
                  onclick={() => openUrl(ticket.url)}
                  aria-label={t("ticketsUiOpenLink")}
                  title={t("ticketsUiOpenLink")}
                >
                  <ExternalLink size={14} aria-hidden="true" />
                </button>
                {#if ticket.project}
                  <button
                    class="btn sm"
                    title={t("teamRunsLaunchTitle", { key: ticket.key })}
                    disabled={!!starting}
                    onclick={() => launchTeam(ticket)}
                  >
                    <Users size={14} aria-hidden="true" />{t("teamRunsWithTeam")}
                  </button>
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
        <EmptyState icon={Ticket} title={t("runsEmpty")} text={t("runsEmptyHelp")}>
          <div class="empty-actions">
            <button class="btn" onclick={() => (tickets.tab = "inbox")}>
              <Inbox size={14} aria-hidden="true" />{t("ticketsTabInbox")}
            </button>
            <button class="btn" onclick={launchTicket}>
              <Ticket size={14} aria-hidden="true" />{t("ticketLaunchLong")}
            </button>
          </div>
        </EmptyState>
      </div>
    {:else}
      {#if chosenRuns.length}
        <SelectionBar
          count={chosenRuns.length}
          total={runs.length}
          label={t("ticketsTabRuns")}
          onall={() => ((leftRuns = ""), (asking = null), (pickedRuns = all(pickedRuns, runs.map((r) => r.id))))}
          onclear={() => ((leftRuns = ""), (asking = null), (pickedRuns = none()))}
        >
          {#if asking}
            {@const count = (asking === "close" ? closable : cleanable).length}
            <span class="ask" role="alert">{t(asking === "close" ? "t6bCloseAsk" : "t6bCleanAsk", { count })}</span>
            <button class="btn sm" onclick={() => (asking = null)}>{t("cancel")}</button>
            <button class="btn sm danger" onclick={() => asking && closePicked(asking)}>
              {t(asking === "close" ? "closeRun" : "ticketsUiClean")} · {count}
            </button>
          {:else}
            {#if leftRuns}<span class="note">{leftRuns}</span>{/if}
            {#if bulk}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
            <button class="btn sm" disabled={bulk || !cleanable.length} onclick={() => (asking = "cleanup")}>
              <BrushCleaning size={14} aria-hidden="true" />{t("ticketsUiClean")} · {cleanable.length}
            </button>
            <button class="btn sm" disabled={bulk || !closable.length} onclick={() => (asking = "close")}>
              <Archive size={14} aria-hidden="true" />{t("closeRun")} · {closable.length}
            </button>
          {/if}
        </SelectionBar>
      {/if}
      <div class="split">
        <ul class="list" class:picking={chosenRuns.length > 0} aria-label={t("ticketsTabRuns")}>
          {#each runs as run (run.id)}
            <li class="row" class:current={run === current} class:picked={pickedRuns.keys.includes(run.id)}>
              <input
                class="box"
                type="checkbox"
                checked={pickedRuns.keys.includes(run.id)}
                aria-label={t("t6bPick", { name: `${run.ticket.key} ${run.title}` })}
                onclick={(e) => pickRun(e, run.id)}
              />
              <button
                class="pick"
                aria-current={run === current}
                onclick={(e) => (toggleKey(e) || e.shiftKey ? pickRun(e, run.id) : (tickets.selected = run.id))}
              >
                <span class="name"><span class="mono">{run.ticket.key}</span> {run.title}</span>
                <span class="desc">{repoName(run.repo)}{run.branch ? ` · ${run.branch}` : ""} · {ago(run.updated)}</span>
              </button>
              <StatusPill status={run.status} label={readyToClean(run) ? t("ticketsUiReady") : undefined} />
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

  .sticky {
    position: sticky;
    z-index: 1;
    top: 0;
  }

  .row .box {
    margin-left: 8px;
  }

  /* In place of the box of a ticket with no project: the titles stay aligned. */
  span.box {
    flex-shrink: 0;
    width: 14px;
  }

  .empty-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 14px;
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
