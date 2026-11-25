<script lang="ts">
  import { untrack } from "svelte";
  import FolderSearch from "@lucide/svelte/icons/folder-search";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Megaphone from "@lucide/svelte/icons/megaphone";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import BroadcastBar from "$lib/BroadcastBar.svelte";
  import Layouts from "$lib/Layouts.svelte";
  import QuickTerminal from "$lib/QuickTerminal.svelte";
  import SessionLauncher from "$lib/SessionLauncher.svelte";
  import SessionTile from "$lib/SessionTile.svelte";
  import TerminalTabs from "$lib/TerminalTabs.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { provide } from "$lib/ask.svelte";
  import { keys, openLauncher } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { allRepos, currentProject } from "$lib/files.svelte";
  import { aria, display } from "$lib/keymap.svelte";
  import { shown, tallies } from "$lib/project.svelte";
  import { dividers, rects, resizeAt, type Divider } from "$lib/split-logic";
  import { sessionChip } from "$lib/tile-logic";
  import { currentTab, equalizeAt, openLayout, reconcile, tabOf } from "$lib/tabs.svelte";
  import { app, grid, load, ticked, ui } from "$lib/state.svelte";

  let launcher = $state<HTMLButtonElement>();
  let broadcast = $state<HTMLButtonElement>();
  let panes = $state<HTMLElement>();
  const visible = $derived(app.sessions.filter(shown));
  const project = $derived(currentProject());
  const counts = $derived(tallies());
  const tab = $derived(currentTab());
  const placed = $derived(tab ? (tab.zoom ? new Map([[tab.active, { x: 0, y: 0, w: 1, h: 1 }]]) : rects(tab.root)) : new Map());
  const borders = $derived(tab && !tab.zoom ? dividers(tab.root) : []);
  const firstRun = $derived(app.workspace !== null && !allRepos().length);

  // The ⌘I panel: the CTO, with the session that has the keyboard (its selection comes with ⌘I).
  $effect(() => {
    const active = app.sessions.find((s) => s.id === tab?.active && shown(s));
    return provide("terminals", { chips: active ? [sessionChip(active)] : [] });
  });

  // Every session in one tab, whoever opened it.
  $effect(() => {
    void app.sessions.map((s) => s.id);
    untrack(reconcile);
  });

  // Tiles another project hides: none stays ticked for broadcast, and no launcher or broadcast mode stays without its
  // button.
  $effect(() => {
    const ids = new Set(visible.map((s) => s.id));
    untrack(() => ticked.forEach((id) => !ids.has(id) && ticked.delete(id)));
    if (!ids.size) ui.launcher = grid.broadcast = false;
  });

  // Leaving the broadcast mode forgets its recipients.
  $effect(() => {
    if (!grid.broadcast) untrack(() => ticked.clear());
  });

  // The launcher was opened, and every project's tiles asked for, under the previous project.
  $effect(() => {
    void app.project;
    ui.launcher = grid.all = false;
  });

  const shortcuts = $derived([
    [keys.palette, t("palette")],
    [keys.next, t("nextWaiting")],
    [keys.switcher, t("navSwitchProject")],
    [keys.launcher, t("navNewSession")],
    [display("split_right"), t("t6aSplitRight")],
    [display("split_down"), t("t6aSplitDown")],
    [`${keys.tile(1)}…9`, t("t6aTab")],
    [keys.maximize, t("maximizeActive")],
    [keys.quick, t("quickTerminal")],
  ]);

  /**
   * Drags a border between splits; it follows the pointer from where it started, within the splits' minimum. Anything
   * else that changes the tab meanwhile (a split opening, a shortcut) ends the drag rather than being undone.
   */
  function drag(e: PointerEvent, d: Divider) {
    const target = tab;
    if (!target || !panes || e.button !== 0) return;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const start = target.root;
    let last = target.root;
    const from = d.dir === "row" ? e.clientX : e.clientY;
    const size = d.dir === "row" ? panes.clientWidth * d.box.w : panes.clientHeight * d.box.h;
    const move = (m: PointerEvent) => {
      if (target.root !== last) return end();
      target.root = resizeAt(start, d.path, d.i, ((d.dir === "row" ? m.clientX : m.clientY) - from) / size);
      last = target.root;
    };
    const end = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  }

  const pct = (n: number) => `${n * 100}%`;
</script>

<main class="main">
  <ViewHeader title={t("viewTerminals")} context={project && app.sessions.length ? "" : (project?.name ?? t("ticketsAll"))}>
    {#if project && app.sessions.length}
      <div class="segmented" role="group" aria-label={t("terminalsShown")}>
        {#each [false, true] as all (all)}
          <button aria-pressed={grid.all === all} onclick={() => (grid.all = all)}>
            {all ? t("ticketsAll") : project.name} · {counts[all ? "*" : project.id]?.total ?? 0}
          </button>
        {/each}
      </div>
    {/if}
    {#snippet actions()}
      {#if !firstRun || app.layouts.length}<Layouts />{/if}
      {#if visible.length}
        <button
          bind:this={broadcast}
          class="btn sm quiet toggle"
          aria-pressed={grid.broadcast}
          title={t("uxfT3BroadcastHint")}
          onclick={() => (grid.broadcast = !grid.broadcast)}
        >
          <Megaphone size={14} aria-hidden="true" />{t("broadcast")}
        </button>
        <button
          bind:this={launcher}
          class="btn sm"
          class:primary={!grid.broadcast}
          aria-haspopup="dialog"
          aria-expanded={ui.launcher}
          aria-keyshortcuts={aria("new_session")}
          onclick={() => (ui.launcher ? (ui.launcher = false) : openLauncher())}
        >
          <Plus size={14} aria-hidden="true" />{t("navNewSession")}{#if keys.launcher}<kbd>{keys.launcher}</kbd>{/if}
        </button>
      {/if}
    {/snippet}
  </ViewHeader>
  <Popover bind:open={ui.launcher} trigger={launcher} label={t("navNewSession")} align="end" width={480}>
    <SessionLauncher title={t("navNewSession")} onopen={() => (ui.launcher = false)} />
  </Popover>
  <div class="body">
    <QuickTerminal />
    {#if visible.length}<TerminalTabs />{/if}
    <!-- Every split stays mounted, in one place, those of other tabs hidden: their terminals keep running and their
         scrollback, and moving a split only moves its box. -->
    <div class="panes" bind:this={panes} hidden={!visible.length}>
      {#each app.sessions as session (session.id)}
        <SessionTile
          {session}
          rect={placed.get(session.id)}
          zoomed={!!tab?.zoom && tab.active === session.id}
          alone={typeof tabOf(session.id)?.root !== "object"}
        />
      {/each}
      {#each borders as d (`${d.path.join(".")}:${d.i}`)}
        <!-- svelte-ignore a11y_no_static_element_interactions (the keyboard resizes with its shortcuts) -->
        <div
          class="divider {d.dir}"
          title="{t('t6aResizeHandle')} · {t('wsEqualizeHint')}"
          style:left={d.dir === "row" ? `calc(${pct(d.at)} - 4px)` : `calc(${pct(d.box.x)} + 4px)`}
          style:top={d.dir === "row" ? `calc(${pct(d.box.y)} + 4px)` : `calc(${pct(d.at)} - 4px)`}
          style:width={d.dir === "row" ? "8px" : `calc(${pct(d.box.w)} - 8px)`}
          style:height={d.dir === "row" ? `calc(${pct(d.box.h)} - 8px)` : "8px"}
          onpointerdown={(e) => drag(e, d)}
          ondblclick={() => tab && equalizeAt(tab, d.path)}
        ></div>
      {/each}
    </div>
    {#if visible.length}
      {#if grid.broadcast}<BroadcastBar onexit={() => ((grid.broadcast = false), broadcast?.focus())} />{/if}
    {:else if firstRun}
      <EmptyState icon={FolderSearch} title={t("noRepos")} text={t("noReposHelp")}>
        <code class="config">{app.workspace?.configDir}/termf.toml</code>
        <button class="btn" onclick={load}><RefreshCw size={14} aria-hidden="true" />{t("reload")}</button>
      </EmptyState>
    {:else if app.workspace}
      <div class="empty">
        <div class="card"><SessionLauncher title={t("emptyTitle")} /></div>
        {#if app.layouts.length}
          <div class="layouts">
            <span>{t("emptyLayouts")}</span>
            {#each app.layouts as layout (layout.name)}
              <button class="btn sm" onclick={() => openLayout(layout)}>
                <LayoutGrid size={14} aria-hidden="true" />{layout.name}
              </button>
            {/each}
          </div>
        {/if}
        <section class="keys" aria-labelledby="shortcuts-title">
          <h2 id="shortcuts-title">{t("shortcuts")}</h2>
          <dl>
            {#each shortcuts as [key, label] (label)}
              <dt><kbd>{key}</kbd></dt>
              <dd>{label}</dd>
            {/each}
          </dl>
        </section>
      </div>
    {/if}
  </div>
</main>

<style>
  .main,
  .body {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .body {
    position: relative;
    flex: 1;
  }

  .panes {
    position: relative;
    flex: 1;
    min-height: 0;
    margin: 4px;
    overflow: hidden;
  }

  .panes[hidden] {
    display: none;
  }

  .divider {
    position: absolute;
    z-index: 1;
    touch-action: none;
  }

  .divider.row {
    cursor: col-resize;
  }

  .divider.col {
    cursor: row-resize;
  }

  .divider::after {
    content: "";
    position: absolute;
    inset: 3px;
    border-radius: 1px;
    transition: background var(--dur-fast) ease-out;
  }

  .divider:hover::after,
  .divider:active::after {
    background: var(--accent);
  }

  .toggle[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .main :global(.view-header .primary kbd) {
    border: 0;
    background: rgb(255 255 255 / 0.18);
    color: inherit;
  }

  .empty {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    min-height: 0;
    padding: 40px 24px;
    overflow-y: auto;
  }

  .card {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    width: 100%;
    max-width: 560px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-2);
  }

  .layouts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    width: 100%;
    max-width: 560px;
    color: var(--muted);
  }

  .keys {
    width: 100%;
    max-width: 560px;
    padding-top: 20px;
    border-top: 1px solid var(--border);
  }

  h2 {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 14px;
    align-items: center;
    margin: 0;
  }

  dt {
    justify-self: start;
  }

  dd {
    margin: 0;
    color: var(--muted);
  }

  .config {
    max-width: 100%;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--hover);
    color: var(--text);
    font: var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
