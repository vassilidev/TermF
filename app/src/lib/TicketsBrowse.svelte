<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import Search from "@lucide/svelte/icons/search";
  import SearchX from "@lucide/svelte/icons/search-x";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { ago, t } from "$lib/i18n";
  import { blocks, type Span } from "$lib/markdown-logic";
  import {
    openUrl,
    type BrowseFilter,
    type BrowsePage,
    type TicketDetail,
    type TicketSummary,
    type TrackerProject,
  } from "$lib/tickets.svelte";

  /**
   * A tracker beyond the inbox: its projects, its tickets by state, assignee, words and sprint, a page at a time,
   * and one ticket in full, to launch a run on. What the tracker gives was written by anyone: it shows as text.
   */
  let {
    project,
    sources,
    sourceName,
    starting,
    onlaunch,
  }: {
    project: string;
    /** The project's sources a browse reads (`jira`, `github`, `mcp:<name>`). */
    sources: string[];
    sourceName: (id: string) => string;
    /** The URL of the ticket being launched. */
    starting: string;
    onlaunch: (ticket: TicketSummary) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let source = $state(sources[0] ?? "");
  let filter = $state<BrowseFilter>({ projects: [], status: "open", assignee: "all", text: "", sprint: false });
  let projects = $state<TrackerProject[] | null>(null);
  let projectsError = $state("");
  let found = $state<TicketSummary[] | null>(null);
  let next = $state<string | null>(null);
  let error = $state("");
  let more = $state(false);
  let chosen = $state<TicketSummary | null>(null);
  let detail = $state<TicketDetail | null>(null);
  let detailError = $state("");

  const mcp = $derived(source.startsWith("mcp:"));
  const jiraLike = $derived(source === "jira" || mcp);

  $effect(() => {
    if (!sources.includes(source)) source = sources[0] ?? "";
  });

  // The source's projects, once per source.
  let projectsOf = "";
  $effect(() => {
    const [p, s] = [project, source];
    if (!s || `${p} ${s}` === projectsOf) return;
    projectsOf = `${p} ${s}`;
    [projects, projectsError] = [null, ""];
    filter.projects = [];
    invoke<TrackerProject[]>("tickets_source_projects", { project: p, source: s }).then(
      (list) => projectsOf === `${p} ${s}` && (projects = list),
      (e) => projectsOf === `${p} ${s}` && ([projects, projectsError] = [[], String(e)]),
    );
  });

  let request = 0;
  async function load(after: string | null) {
    const asked = ++request;
    if (after) more = true;
    else [found, next, error] = [null, null, ""];
    try {
      const browse = { ...filter, text: filter.text.trim() || null, sprint: jiraLike && filter.sprint };
      const page = await invoke<BrowsePage>("tickets_browse", { project, source, browse, next: after });
      if (asked !== request) return;
      found = after ? [...(found ?? []), ...page.tickets] : page.tickets;
      next = page.next;
    } catch (e) {
      if (asked !== request) return;
      error = String(e);
      found ??= [];
    } finally {
      if (asked === request) more = false;
    }
  }

  // The first page again on each change; typing waits for a pause.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const wait = filter.text ? 400 : 0;
    void [project, source, filter.status, filter.assignee, filter.sprint, filter.projects.join(), filter.text];
    clearTimeout(timer);
    if (!source) return;
    timer = setTimeout(() => void load(null), wait);
    return () => clearTimeout(timer);
  });

  let detailRequest = 0;
  async function show(ticket: TicketSummary) {
    chosen = ticket;
    [detail, detailError] = [null, ""];
    const asked = ++detailRequest;
    try {
      const got = await invoke<TicketDetail>("tickets_ticket", {
        project,
        ticket: { provider: ticket.provider, key: ticket.key, url: ticket.url },
      });
      if (asked === detailRequest) detail = got;
    } catch (e) {
      if (asked === detailRequest) detailError = String(e);
    }
  }

  function open(e: MouseEvent, url: string) {
    e.preventDefault();
    openUrl(url);
  }
</script>

{#snippet line(list: Span[])}{#each list as span, i (i)}{#if span.kind === "strong"}<strong>{@render line(span.spans)}</strong
      >{:else if span.kind === "code"}<code>{span.text}</code>{:else if span.kind === "link"}<a
        href={span.url}
        onclick={(e) => open(e, span.url)}>{span.text}</a
      >{:else}{span.text}{/if}{/each}{/snippet}

{#snippet text(body: string)}
  {#each blocks(body) as block, i (i)}
    {#if block.kind === "code"}
      <pre class="mono">{block.text}</pre>
    {:else if block.kind === "list" && block.start === null}
      <ul>{#each block.items as item, j (j)}<li>{@render line(item)}</li>{/each}</ul>
    {:else if block.kind === "list"}
      <ol start={block.start}>{#each block.items as item, j (j)}<li>{@render line(item)}</li>{/each}</ol>
    {:else if block.kind === "paragraph" || block.kind === "heading"}
      <p class:heading={block.kind === "heading"}>{@render line(block.spans)}</p>
    {/if}
  {/each}
{/snippet}

<div class="browse">
  <div class="filters">
    {#if sources.length > 1}
      <Segments
        label={t("tjdSource")}
        options={sources.map((s) => ({ id: s, label: sourceName(s) }))}
        selected={source}
        onselect={(s) => (source = s)}
      />
    {/if}
    <label class="pick">
      <span class="sr-only">{t("tjdProject")}</span>
      <select
        class="field"
        value={filter.projects[0] ?? ""}
        onchange={(e) => (filter.projects = e.currentTarget.value ? [e.currentTarget.value] : [])}
        disabled={!projects}
        aria-busy={!projects}
      >
        <option value="">{projects ? t("tjdAllProjects") : t("tjdProjectsLoading")}</option>
        {#each projects ?? [] as p (p.key)}
          <option value={p.key}>{p.name === p.key ? p.key : `${p.key} · ${p.name}`}</option>
        {/each}
      </select>
    </label>
    <Segments
      label={t("tjdStatus")}
      options={[
        { id: "open", label: t("tjdOpen") },
        { id: "done", label: t("tjdDone") },
        { id: "all", label: t("tjdAll") },
      ]}
      selected={filter.status}
      onselect={(s) => (filter.status = s as BrowseFilter["status"])}
    />
    <label class="pick">
      <span class="sr-only">{t("tjdAssignee")}</span>
      <select class="field" bind:value={filter.assignee}>
        <option value="all">{t("tjdAnyone")}</option>
        <option value="me">{t("tjdMine")}</option>
        <option value="none">{t("tjdNobody")}</option>
      </select>
    </label>
    {#if jiraLike}
      <label class="check"><input type="checkbox" bind:checked={filter.sprint} />{t("tjdSprint")}</label>
    {/if}
    <label class="search">
      <Search size={14} aria-hidden="true" />
      <input class="field" type="search" bind:value={filter.text} placeholder={t("tjdSearch")} aria-label={t("tjdSearch")} spellcheck="false" />
    </label>
  </div>
  {#if projectsError}
    <p class="notice warn small"><CircleAlert size={14} aria-hidden="true" />{t("tjdProjectsFailed", { error: projectsError })}</p>
  {/if}
  {#if mcp}<p class="muted small hint">{t("tjdMcpSlow")}</p>{/if}

  <div class="split">
    <div class="list" aria-busy={found === null}>
      {#if error}
        <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>
      {/if}
      {#if found === null}
        <Skeleton shape="list" rows={8} label={t("tjdLoading")} />
      {:else if !found.length && !error}
        <EmptyState icon={SearchX} title={t("tjdNone")} text={t("tjdNoneHelp")} />
      {:else}
        <ul>
          {#each found as ticket (ticket.url)}
            <li>
              <button class="ticket" class:chosen={chosen?.url === ticket.url} onclick={() => show(ticket)}>
                <span class="what">
                  <span class="name">{ticket.title}</span>
                  <span class="desc">
                    <span class="mono">{ticket.key}</span> · {t("ticketUpdated", { ago: ago(Date.parse(ticket.updated) / 1000) })}
                    {#if ticket.assignees.length}· {t("ticketsUiAssignees", { names: ticket.assignees.join(", ") })}{/if}
                  </span>
                </span>
                {#if ticket.state}<span class="tag">{ticket.state}</span>{/if}
              </button>
            </li>
          {/each}
        </ul>
        {#if next}
          <div class="more">
            <button class="btn" onclick={() => load(next)} disabled={more}>
              {#if more}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("tjdMore")}
            </button>
          </div>
        {/if}
      {/if}
    </div>

    <aside class="detail" aria-label={chosen ? `${chosen.key} ${chosen.title}` : t("tjdPick")}>
      {#if !chosen}
        <p class="muted pick-one">{t("tjdPick")}</p>
      {:else}
        <header>
          <span class="mono key">{chosen.key}</span>
          {#if (detail ?? chosen).state}<span class="tag">{(detail ?? chosen).state}</span>{/if}
        </header>
        <h3>{detail?.title ?? chosen.title}</h3>
        <p class="muted small">
          {#if detail?.author}{t("tjdBy", { author: detail.author })} · {/if}{t("ticketUpdated", { ago: ago(Date.parse(chosen.updated) / 1000) })}
          {#if chosen.assignees.length}· {t("ticketsUiAssignees", { names: chosen.assignees.join(", ") })}{/if}
        </p>
        <div class="buttons">
          <button class="btn primary" onclick={() => chosen && onlaunch(chosen)} disabled={!!starting}>
            {#if starting === chosen.url}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Play size={14} aria-hidden="true" />{/if}{t("tjdLaunch")}
          </button>
          <button class="btn" onclick={() => chosen && openUrl(chosen.url)}>
            <ExternalLink size={14} aria-hidden="true" />{t("tjdOpenIn", { source: sourceName(source) })}
          </button>
        </div>
        {#if detailError}
          <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{detailError}</p>
        {:else if !detail}
          <Skeleton shape="lines" rows={6} />
        {:else}
          {#if detail.labels.length}
            <p class="labels">{#each detail.labels as label (label)}<span class="tag">{label}</span>{/each}</p>
          {/if}
          <div class="body">
            {#if detail.body.trim()}{@render text(detail.body)}{:else}<p class="muted">{t("tjdNoBody")}</p>{/if}
          </div>
          {#if detail.comments.length}
            <h4>{t("tjdComments", { count: detail.comments.length })}</h4>
            <ol class="comments">
              {#each detail.comments as comment, i (i)}
                <li>
                  <p class="muted small">{comment.author ?? "?"} · {ago(Date.parse(comment.created) / 1000)}</p>
                  <div class="body">{@render text(comment.body)}</div>
                </li>
              {/each}
            </ol>
          {/if}
        {/if}
      {/if}
    </aside>
  </div>
</div>

<style>
  .browse {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }

  .pick select {
    max-width: 220px;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .search {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
    min-width: 180px;
    color: var(--muted);
  }

  .search input {
    flex: 1;
  }

  .hint,
  .filters + .notice {
    margin: 6px 12px 0;
  }

  .small {
    font-size: var(--text-sm);
  }

  .split {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.1fr);
    min-height: 0;
  }

  .list,
  .detail {
    min-height: 0;
    overflow-y: auto;
  }

  .list {
    border-right: 1px solid var(--border);
  }

  .list > .notice {
    margin: 10px 12px;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .ticket {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .ticket:hover {
    background: var(--hover);
  }

  .ticket.chosen {
    background: var(--selected, var(--hover));
    box-shadow: inset 2px 0 0 var(--accent);
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
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .more {
    display: flex;
    justify-content: center;
    padding: 12px;
  }

  .detail {
    padding: 16px 20px;
  }

  .pick-one {
    margin-top: 24px;
    text-align: center;
  }

  .detail header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .key {
    color: var(--muted);
  }

  h3 {
    margin: 6px 0 2px;
    font-size: var(--text-lg);
  }

  h4 {
    margin: 18px 0 6px;
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 12px 0;
  }

  .labels {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .body {
    overflow-wrap: anywhere;
    line-height: 1.55;
  }

  .body :global(pre) {
    overflow-x: auto;
    padding: 8px;
    border-radius: var(--radius-sm);
    background: var(--hover);
  }

  .heading {
    font-weight: 600;
  }

  .comments {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .comments li {
    padding-left: 10px;
    border-left: 2px solid var(--border);
  }

  .comments p {
    margin: 0;
  }

  .mono {
    font-family: var(--font-mono);
  }

  @media (max-width: 720px) {
    .split {
      grid-template-columns: 1fr;
    }

    .list {
      border-right: 0;
    }
  }
</style>
