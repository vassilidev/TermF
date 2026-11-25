<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LayoutDashboard from "@lucide/svelte/icons/layout-dashboard";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { askAI } from "$lib/ask.svelte";
  import { t } from "$lib/i18n";
  import { shown } from "$lib/run-logic";
  import { team } from "$lib/team.svelte";
  import { boardColumns, laneOf } from "$lib/tickets-logic";
  import { tickets as store, type TicketSummary } from "$lib/tickets.svelte";

  /**
   * The tickets as a board: one column per state the source really has, a card per ticket. A card dragged to
   * another column moves the ticket there in its tracker at once (the user's own gesture: no approval), and comes
   * back with the tracker's reason when it refuses. A source without states gets the CTO's help instead.
   */
  let {
    project,
    source,
    sourceName,
    tickets,
    columns,
    movable,
    none,
    chosen,
    starting,
    next,
    more,
    onmore,
    onopen,
    onlaunch,
    onmoved,
  }: {
    project: string;
    source: string;
    sourceName: (id: string) => string;
    /** null while the first page loads. */
    tickets: TicketSummary[] | null;
    /** The source's states, in its order; null while read; empty: it has none to move tickets between. */
    columns: string[] | null;
    movable: boolean;
    /** The source has no states at all: the CTO helps instead. */
    none: boolean;
    chosen: TicketSummary | null;
    starting: string;
    next: string | null;
    more: boolean;
    onmore: () => void;
    onopen: (ticket: TicketSummary) => void;
    onlaunch: (ticket: TicketSummary) => void;
    onmoved: (ticket: TicketSummary, to: string) => void;
  } = $props();

  /** Where a card shows while its move is asked: it comes back if the tracker refuses. */
  let moving = $state<Record<string, string>>({});
  let dragging = $state<TicketSummary | null>(null);
  let over = $state<string | null>(null);

  const stateOf = (ticket: TicketSummary) => moving[ticket.url] ?? ticket.state;
  const lanes = $derived(boardColumns(columns ?? [], (tickets ?? []).map(stateOf)));
  const runOf = (ticket: TicketSummary) => store.runs.find((r) => r.ticket.url === ticket.url && r.status !== "closed");

  async function drop(to: string) {
    const ticket = dragging;
    [dragging, over] = [null, null];
    if (!ticket || !movable || laneOf(stateOf(ticket), lanes) === to) return;
    moving[ticket.url] = to;
    try {
      await invoke("tickets_move", { project, ticket: { provider: ticket.provider, key: ticket.key, url: ticket.url }, to });
      onmoved(ticket, to);
      toast(t("tjdMoved", { key: ticket.key, to }));
    } catch (e) {
      toast(t("tjdMoveRefused", { key: ticket.key, error: String(e) }), { kind: "error" });
    } finally {
      delete moving[ticket.url];
    }
  }

  function organize() {
    askAI({ role: "cto", draft: t("tkbOrganizeDraft", { source: sourceName(source) }) });
  }

  function sort() {
    askAI({ role: "cto", draft: t("tkbSortDraft", { source: sourceName(source) }) });
  }
</script>

{#if none}
  <EmptyState icon={LayoutDashboard} title={t("tkbNoStates")} text={t("tkbNoStatesHelp", { source: sourceName(source) })}>
    <button class="btn primary" onclick={organize}><Sparkles size={14} aria-hidden="true" />{t("tkbOrganize")}</button>
  </EmptyState>
{:else}
  <div class="bar">
    {#if !movable && columns !== null}<span class="muted small">{t("tkbReadOnly")}</span>{/if}
    <span class="grow"></span>
    <button class="btn sm" onclick={sort} disabled={!tickets?.length}><Sparkles size={14} aria-hidden="true" />{t("tkbSort")}</button>
  </div>
  <div class="board" aria-busy={tickets === null || columns === null}>
    {#if tickets === null || columns === null}
      {#each [0, 1, 2, 3] as i (i)}
        <section class="column"><Skeleton shape="cards" rows={3} /></section>
      {/each}
    {:else}
      {#each lanes as lane (lane)}
        {@const cards = tickets.filter((x) => laneOf(stateOf(x), lanes) === lane)}
        <section
          class="column"
          class:over={over === lane}
          aria-label={t("tkbColumn", { name: lane, count: cards.length })}
          ondragover={(e) => {
            if (!dragging || !movable) return;
            e.preventDefault();
            over = lane;
          }}
          ondragleave={() => over === lane && (over = null)}
          ondrop={(e) => {
            e.preventDefault();
            void drop(lane);
          }}
        >
          <header><span class="name">{lane}</span><span class="count">{cards.length}</span></header>
          <ul>
            {#each cards as ticket (ticket.url)}
              {@const run = runOf(ticket)}
              <li>
                <article
                  class="card"
                  class:chosen={chosen?.url === ticket.url}
                  class:pending={moving[ticket.url] !== undefined}
                  draggable={movable}
                  ondragstart={(e) => {
                    dragging = ticket;
                    e.dataTransfer?.setData("text/plain", ticket.url);
                  }}
                  ondragend={() => ([dragging, over] = [null, null])}
                >
                  <button class="open" onclick={() => onopen(ticket)}>
                    <span class="mono key">{ticket.key}</span>
                    <span class="title">{ticket.title}</span>
                  </button>
                  <div class="foot">
                    {#if ticket.assignees.length}<span class="who" title={ticket.assignees.join(", ")}>{ticket.assignees[0]}</span>{/if}
                    {#each ticket.labels.slice(0, 2) as label (label)}<span class="tag">{label}</span>{/each}
                    <span class="grow"></span>
                    {#if run}
                      {@const crew = team.runs.find((r) => r.run === run.id)}
                      {@const pill = shown(run, crew)}
                      <StatusPill status={pill.status as Status} label={pill.label ? t(pill.label) : undefined} />
                    {:else}
                      <button class="icon-btn sm" onclick={() => onlaunch(ticket)} disabled={!!starting} title={t("tjdLaunch")} aria-label={t("tkbLaunch", { key: ticket.key })}>
                        {#if starting === ticket.url}<LoaderCircle size={13} aria-hidden="true" class="spin" />{:else}<Play size={13} aria-hidden="true" />{/if}
                      </button>
                    {/if}
                  </div>
                </article>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/if}
  </div>
  {#if next}
    <div class="more">
      <button class="btn sm" onclick={onmore} disabled={more}>
        {#if more}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("tjdMore")}
      </button>
    </div>
  {/if}
{/if}

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px 0;
  }

  .grow {
    flex: 1;
  }

  .small {
    font-size: var(--text-sm);
  }

  .board {
    display: flex;
    flex: 1;
    gap: 12px;
    min-height: 0;
    padding: 10px 12px 12px;
    overflow-x: auto;
    overscroll-behavior-x: contain;
    scroll-snap-type: x proximity;
  }

  .column {
    display: flex;
    flex: 0 0 272px;
    flex-direction: column;
    min-height: 0;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius, 8px);
    background: var(--chrome);
    scroll-snap-align: start;
  }

  .column.over {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .column header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 4px 8px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .count {
    color: var(--muted);
    font-weight: 500;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 40px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--panel);
    cursor: grab;
  }

  .card:hover {
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }

  .card.chosen {
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .card.pending {
    opacity: 0.7;
  }

  .open {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .key {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .title {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    font-size: var(--text-sm);
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 22px;
  }

  .who {
    max-width: 110px;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag {
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .more {
    display: flex;
    justify-content: center;
    padding: 0 12px 12px;
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>
