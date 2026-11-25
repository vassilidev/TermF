<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Search from "@lucide/svelte/icons/search";
  import SearchX from "@lucide/svelte/icons/search-x";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { ago, t } from "$lib/i18n";
  import TicketDetail from "$lib/TicketDetail.svelte";
  import TicketsBoard from "$lib/TicketsBoard.svelte";
  import { bySpace } from "$lib/tickets-logic";
  import {
    type BrowseFilter,
    type BrowsePage,
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

  const mcp = $derived(source.startsWith("mcp:"));
  const jiraLike = $derived(source === "jira" || source.startsWith("jira:") || mcp);
  /** Several spaces read at once: the list groups them. */
  const grouped = $derived(jiraLike && !filter.projects.length && (projects?.length ?? 0) > 1);

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
    // Through an MCP server each read is a paid Claude Code call: its words say the projects instead.
    if (s.startsWith("mcp:")) return void (projects = []);
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

  /** A ticket moved in its panel or on the board: the list shows its new state. */
  function moved(ticket: TicketSummary, to: string) {
    found = (found ?? []).map((x) => (x.url === ticket.url ? { ...x, state: to } : x));
  }

  /** List or board, kept for the next visit. */
  let layout = $state<"list" | "board">(read("termf.tickets.layout") === "board" ? "board" : "list");
  $effect(() => write("termf.tickets.layout", layout));

  /** The source's states, the board's columns, read once per source. */
  let board = $state<{ columns: string[]; movable: boolean; none: boolean } | null>(null);
  let boardOf = "";
  $effect(() => {
    const [p, s, l] = [project, source, layout];
    if (l !== "board" || !s || `${p} ${s}` === boardOf) return;
    boardOf = `${p} ${s}`;
    board = null;
    invoke<{ columns: string[]; movable: boolean; none: boolean }>("tickets_board", { project: p, source: s }).then(
      (b) => boardOf === `${p} ${s}` && (board = b),
      (e) => boardOf === `${p} ${s}` && ([board, error] = [{ columns: [], movable: false, none: false }, String(e)]),
    );
  });

  function read(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }

  function write(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // A private window keeps the choice for this visit only.
    }
  }
</script>


<div class="browse">
  <div class="filters">
    <Segments
      label={t("tkbLayout")}
      options={[
        { id: "list", label: t("tkbList") },
        { id: "board", label: t("tkbBoard") },
      ]}
      selected={layout}
      onselect={(l) => (layout = l as typeof layout)}
    />
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
      onselect={(s) => { filter.status = s as BrowseFilter["status"]; }}
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

  <div class="split" class:wide={layout === "board" && !chosen}>
    {#if layout === "board"}
      <div class="boardpane">
        {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
        <TicketsBoard
          {project}
          {source}
          {sourceName}
          tickets={found}
          columns={board?.columns ?? null}
          movable={board?.movable ?? false}
          none={board?.none ?? false}
          {chosen}
          {starting}
          {next}
          {more}
          onmore={() => load(next)}
          onopen={(ticket) => (chosen = ticket)}
          {onlaunch}
          onmoved={moved}
        />
      </div>
    {:else}
    <div class="list" aria-busy={found === null}>
      {#if error}
        <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>
      {/if}
      {#if found === null}
        <Skeleton shape="list" rows={8} label={t("tjdLoading")} />
      {:else if !found.length && !error}
        <EmptyState icon={SearchX} title={t("tjdNone")} text={t("tjdNoneHelp")} />
      {:else}
        {#each grouped ? bySpace(found) : [{ space: "", tickets: found }] as group (group.space)}
          {#if grouped}<h4 class="space"><span class="mono">{group.space}</span><span class="muted">{group.tickets.length}</span></h4>{/if}
          <ul>
            {#each group.tickets as ticket (ticket.url)}
              <li>
                <button class="ticket" class:chosen={chosen?.url === ticket.url} onclick={() => (chosen = ticket)}>
                  <span class="what">
                    <span class="name">{ticket.title}</span>
                    <span class="desc"
                      ><span class="mono">{ticket.key}</span>{` · ${t("ticketUpdated", { ago: ago(Date.parse(ticket.updated) / 1000) })}`}{ticket.assignees
                        .length
                        ? ` · ${t("ticketsUiAssignees", { names: ticket.assignees.join(", ") })}`
                        : ""}</span
                    ></span
                  >
                  {#if ticket.state}<span class="tag">{ticket.state}</span>{/if}
                </button>
              </li>
            {/each}
          </ul>
        {/each}
        {#if next}
          <div class="more">
            <button class="btn" onclick={() => load(next)} disabled={more}>
              {#if more}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("tjdMore")}
            </button>
          </div>
        {/if}
      {/if}
    </div>
    {/if}

    {#if layout === "list" || chosen}
    <aside class="pane-detail" aria-label={chosen ? `${chosen.key} ${chosen.title}` : t("tjdPick")}>
      {#if !chosen}
        <p class="muted pick-one">{t("tjdPick")}</p>
      {:else}
        {#key chosen.url}
          <TicketDetail {project} ticket={chosen} {starting} {onlaunch} onopen={(ticket) => (chosen = ticket)} onmoved={moved} />
        {/key}
      {/if}
    </aside>
    {/if}
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

  .split.wide {
    grid-template-columns: minmax(0, 1fr);
  }

  .boardpane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    border-right: 1px solid var(--border);
  }

  .list,
  .pane-detail {
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

  .pick-one {
    margin-top: 24px;
    text-align: center;
  }

  .space {
    display: flex;
    gap: 8px;
    margin: 0;
    padding: 10px 12px 4px;
    font-size: var(--text-xs);
    letter-spacing: 0.02em;
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
