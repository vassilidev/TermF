<script lang="ts">
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Equal from "@lucide/svelte/icons/equal";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Rows2 from "@lucide/svelte/icons/rows-2";
  import SquarePlus from "@lucide/svelte/icons/square-plus";
  import X from "@lucide/svelte/icons/x";
  import ContextMenu, { menuKey, menuPoint, type ContextItem } from "$lib/ui/ContextMenu.svelte";
  import { initial } from "$lib/ProjectSwitcher.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import SelectionBar from "$lib/ui/SelectionBar.svelte";
  import { focusSession, keys, openLauncher, splitActive } from "$lib/commands";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { aria, display } from "$lib/keymap.svelte";
  import { sessionProjects } from "$lib/project.svelte";
  import { all, none, pick, within } from "$lib/select-logic";
  import { leaves, type Side } from "$lib/split-logic";
  import {
    closeTab,
    currentTab,
    entryOf,
    equalize,
    mergeTab,
    moveTab,
    showSession,
    tabName,
    tabOf,
    terminals,
    visibleTabs,
    type Tab,
  } from "$lib/tabs.svelte";
  import { app, grid, type Session } from "$lib/state.svelte";

  const list = $derived(visibleTabs());
  const current = $derived(currentTab());
  // Under every project, a tab says whose it is.
  const mixed = $derived(app.project === null || grid.all);
  let renaming = $state<number | null>(null);
  let closing = $state<{ tab: Tab; running: number } | null>(null);
  let closeTrigger = $state<HTMLElement>();
  let asking = $state(false);
  let dragged = $state<number | null>(null);
  let drag: { tab: Tab; x: number; y: number } | null = null;
  // The click that ends a drag is not a selection.
  let moved = false;
  /** Dragged below the bar: the split it would go beside, the side, and that half of the split on screen. */
  let drop = $state<{ id: number; side: Side; box: { left: number; top: number; width: number; height: number } } | null>(null);
  /** The splits it could go beside, outlined while it is dragged over them. */
  let targets = $state<DOMRect[]>([]);

  let menuOpen = $state(false);
  let menuAt = $state<{ x: number; y: number } | null>(null);
  let menuTrigger = $state<HTMLElement>();
  let menuItems = $state<ContextItem[]>([]);

  /** A tab's actions at the pointer: its splits, its name, closing it. */
  function openMenu(e: MouseEvent | KeyboardEvent, tab: Tab) {
    e.preventDefault();
    const count = leaves(tab.root).length;
    menuTrigger = e.currentTarget as HTMLElement;
    menuAt = menuPoint(e);
    const beside = (side: Side) => () => splitActive(side, tab.active);
    menuItems = [
      { label: t("t6aSplitRight"), icon: Columns2, kbd: display("split_right"), run: beside("right") },
      { label: t("t6aSplitDown"), icon: Rows2, kbd: display("split_down"), run: beside("down") },
      {
        label: t("wsSplitWith"),
        icon: SquarePlus,
        run: () => {
          showSession(tab.active);
          openLauncher("right");
        },
      },
      ...(count > 1 ? [{ label: t("t6aEqualize"), icon: Equal, kbd: display("equalize_splits"), run: () => equalize(tab) }] : []),
      { label: t("wsRenameTab"), icon: Pencil, divider: true, run: () => (renaming = tab.id) },
      { label: t("t6aCloseTab"), icon: X, run: () => askClose(tab, menuTrigger!) },
    ];
    menuOpen = true;
  }

  const sideLabel = { left: "wsDropLeft", right: "wsDropRight", up: "wsDropUp", down: "wsDropDown" } as const;

  /** The visible split under the pointer, outside the dragged tab, and the edge it is nearest to. */
  function dropAt(x: number, y: number, dragged: Tab) {
    const tiles = [...document.querySelectorAll<HTMLElement>(".panes .tile[data-session]:not([hidden])")].filter(
      (el) => tabOf(Number(el.dataset.session)) !== dragged,
    );
    targets = tiles.map((el) => el.getBoundingClientRect());
    const at = tiles.find((el) => {
      const r = el.getBoundingClientRect();
      return x >= r.left && x < r.right && y >= r.top && y < r.bottom;
    });
    if (!at) return null;
    const r = at.getBoundingClientRect();
    const dx = (x - r.left) / r.width - 0.5;
    const dy = (y - r.top) / r.height - 0.5;
    const side: Side = Math.abs(dx) > Math.abs(dy) ? (dx < 0 ? "left" : "right") : dy < 0 ? "up" : "down";
    const half = {
      left: { left: r.left, top: r.top, width: r.width / 2, height: r.height },
      right: { left: r.left + r.width / 2, top: r.top, width: r.width / 2, height: r.height },
      up: { left: r.left, top: r.top, width: r.width, height: r.height / 2 },
      down: { left: r.left, top: r.top + r.height / 2, width: r.width, height: r.height / 2 },
    }[side];
    return { id: Number(at.dataset.session), side, box: half };
  }

  const sessionsOf = (tab: Tab) =>
    leaves(tab.root)
      .map((id) => app.sessions.find((s) => s.id === id))
      .filter((s): s is Session => !!s);

  function projectName(tab: Tab): string {
    const s = app.sessions.find((s) => s.id === tab.active);
    const id = s && sessionProjects(s)[0];
    return app.workspace?.projects.find((p) => p.id === id)?.name ?? "";
  }

  let picked = $state(none<number>());
  let askingMany = $state(false);
  const ids = $derived(list.map((tab) => tab.id));
  const chosen = $derived(list.filter((tab) => within(picked.keys, ids).includes(tab.id)));
  const chosenSessions = $derived(chosen.flatMap(sessionsOf));
  const running = $derived(chosenSessions.filter((s) => s.running).length);

  /** A first Shift-click ranges from the current tab, as in a browser. */
  const select = async (e: MouseEvent, tab: Tab) => {
    if ((isMac ? e.metaKey : e.ctrlKey) || e.shiftKey) {
      const from = picked.anchor === null && current ? { keys: [], anchor: current.id } : picked;
      askingMany = false;
      picked = pick(from, ids, tab.id, e.shiftKey);
      return;
    }
    await focusSession(entryOf(tab));
  };

  /** The keyboard moves only if the current tab was among those closed. */
  async function closeMany() {
    const tabs = chosen;
    const moving = !!current && tabs.includes(current);
    askingMany = false;
    picked = none();
    let next: number | null = null;
    for (const tab of tabs) next = await closeTab(tab);
    if (!moving) return;
    const left = currentTab();
    const to = next !== null && app.sessions.some((s) => s.id === next) ? next : left ? entryOf(left) : null;
    if (to !== null) focusSession(to);
  }

  async function close(tab: Tab) {
    closing = null;
    asking = false;
    const next = await closeTab(tab);
    if (next !== null) focusSession(next);
  }

  function askClose(tab: Tab, trigger: HTMLElement) {
    const running = sessionsOf(tab).filter((s) => s.running).length;
    if (!running) return close(tab);
    closing = { tab, running };
    closeTrigger = trigger;
    asking = true;
  }

  function rename(tab: Tab, value: string) {
    if (renaming !== tab.id) return;
    renaming = null;
    tab.name = "";
    if (value.trim() !== tabName(tab)) tab.name = value.trim();
  }

  function down(e: PointerEvent, tab: Tab) {
    moved = false;
    if (e.button !== 0) return;
    drag = { tab, x: e.clientX, y: e.clientY };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function over(e: PointerEvent) {
    if (!drag || (!moved && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 5)) return;
    moved = true;
    dragged = drag.tab.id;
    const bar = (e.currentTarget as HTMLElement).closest("[role=tablist]");
    // Below the bar, over the splits: beside one of them, on the edge it is nearest to.
    if (bar && e.clientY > bar.getBoundingClientRect().bottom + 4) {
      drop = dropAt(e.clientX, e.clientY, drag.tab);
      return;
    }
    drop = null;
    targets = [];
    const under = [...(bar?.querySelectorAll<HTMLElement>("[data-tab]") ?? [])].find((el) => {
      const r = el.getBoundingClientRect();
      return e.clientX >= r.left && e.clientX < r.right;
    });
    const target = list.find((tab) => tab.id === Number(under?.dataset.tab));
    if (target && target.id !== drag.tab.id) moveTab(drag.tab, target);
  }

  /** Let go: dropped on a split, the tab joins it. */
  function up() {
    const from = drag?.tab;
    const to = drop;
    cancel();
    if (from && to) {
      mergeTab(from, to.id, to.side);
      focusSession(from.active);
    }
  }

  /** A drag the system took away (another window, a gesture) drops nothing. */
  function cancel() {
    drag = null;
    dragged = null;
    drop = null;
    targets = [];
    setTimeout(() => (moved = false));
  }

  function onkeydown(e: KeyboardEvent, tab: Tab) {
    const at = list.indexOf(tab);
    const to = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: list.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    const next = list[(to + list.length) % list.length];
    terminals.current = next.id;
    document.getElementById(`tab-${next.id}`)?.focus();
  }

  function focusInput(input: HTMLInputElement) {
    input.focus();
    input.select();
  }
</script>

<div class="bar">
  <div class="tabs" role="tablist" aria-label={t("t6aTabs")}>
    {#each list as tab, i (tab.id)}
      {@const count = leaves(tab.root).length}
      {@const project = mixed ? projectName(tab) : ""}
      <div
        class="tab"
        class:on={tab === current}
        class:chosen={chosen.includes(tab)}
        class:dragged={dragged === tab.id}
        data-tab={tab.id}
      >
        {#if renaming === tab.id}
          <input
            class="rename"
            value={tabName(tab)}
            aria-label={t("t6aTabName")}
            use:focusInput
            onblur={(e) => rename(tab, e.currentTarget.value)}
            onkeydown={(e) => {
              if (e.key === "Enter") rename(tab, e.currentTarget.value);
              if (e.key === "Escape") renaming = null;
            }}
          />
        {:else}
          <button
            role="tab"
            id="tab-{tab.id}"
            aria-selected={tab === current}
            tabindex={tab === current ? 0 : -1}
            title="{tabName(tab)} · {t('t6aRename')}"
            aria-keyshortcuts={i < 9 ? aria("tile", i + 1) : undefined}
            onclick={(e) => !moved && select(e, tab)}
            ondblclick={() => (renaming = tab.id)}
            onpointerdown={(e) => down(e, tab)}
            onpointermove={over}
            onpointerup={up}
            onpointercancel={cancel}
            onkeydown={(e) => (menuKey(e) ? openMenu(e, tab) : onkeydown(e, tab))}
            oncontextmenu={(e) => openMenu(e, tab)}
          >
            {#if sessionsOf(tab).some((s) => s.running && s.status === "waiting")}
              <span class="dot" role="img" aria-label={t("statusWaiting")}></span>
            {/if}
            {#if project}<span class="mark" title={project} aria-hidden="true">{initial(project)}</span><span class="sr-only">{project}</span>{/if}
            <span class="name">{tabName(tab)}</span>
            {#if chosen.includes(tab)}<span class="sr-only">{t("t6bChosen")}</span>{/if}
            {#if count > 1}<span class="count" title={t("t6aSplits", { count })}>{count}</span>{/if}
            {#if i < 9}<kbd aria-hidden="true">{keys.tile(i + 1)}</kbd>{/if}
          </button>
        {/if}
        <button
          class="close"
          tabindex="-1"
          aria-label={t("t6aCloseTab")}
          title={t("t6aCloseTab")}
          onclick={(e) => askClose(tab, e.currentTarget)}
        >
          <X size={12} aria-hidden="true" />
        </button>
      </div>
    {/each}
  </div>
  <button
    class="icon-btn sm new"
    onclick={() => openLauncher()}
    aria-label={t("t6aNewTab")}
    title="{t('t6aNewTab')} ({keys.launcher})"
    aria-keyshortcuts={aria("new_session")}
  >
    <Plus size={14} aria-hidden="true" />
  </button>
</div>

{#if chosen.length}
  <SelectionBar
    count={chosen.length}
    total={list.length}
    label={t("t6aTabs")}
    onall={() => ((askingMany = false), (picked = all(picked, ids)))}
    onclear={() => ((askingMany = false), (picked = none()))}
  >
    {#if askingMany}
      <span class="ask" role="alert">{t("t6bCloseTabsAsk", { count: chosenSessions.length, running })}</span>
      <button class="btn sm" onclick={() => (askingMany = false)}>{t("cancel")}</button>
      <button class="btn sm danger" onclick={closeMany}>{t("close")} · {chosen.length}</button>
    {:else}
      <button class="btn sm" onclick={() => (running ? (askingMany = true) : closeMany())}>
        <X size={14} aria-hidden="true" />{t("close")} · {chosen.length}
      </button>
    {/if}
  </SelectionBar>
{/if}

<ContextMenu bind:open={menuOpen} point={menuAt} trigger={menuTrigger} label={t("t6aTabs")} items={menuItems} />

{#if dragged !== null && (drop || targets.length)}
  <!-- Where a tab dragged over the splits would go: each split outlined, the half it would take filled. -->
  {#each targets as r, i (i)}
    <div class="target" style:left="{r.left}px" style:top="{r.top}px" style:width="{r.width}px" style:height="{r.height}px" aria-hidden="true"></div>
  {/each}
  {#if drop}
    <div
      class="zone"
      style:left="{drop.box.left}px"
      style:top="{drop.box.top}px"
      style:width="{drop.box.width}px"
      style:height="{drop.box.height}px"
      aria-hidden="true"
    >
      <span>{t(sideLabel[drop.side])}</span>
    </div>
  {/if}
{/if}

<Popover bind:open={asking} trigger={closeTrigger} label={t("t6aCloseTab")} width={300}>
  {#if closing}
    <div class="confirm">
      <p>{closing.running > 1 ? t("t6aCloseTabAsk", { count: closing.running }) : t("t6aCloseAsk")}</p>
      <div class="buttons">
        <button class="btn sm" onclick={() => (asking = false)}>{t("cancel")}</button>
        <button class="btn sm primary" onclick={() => closing && close(closing.tab)}>{t("t6aCloseTab")}</button>
      </div>
    </div>
  {/if}
</Popover>

<style>
  .bar {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 2px;
    height: 34px;
    padding: 0 8px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .tabs {
    display: flex;
    align-items: stretch;
    align-self: stretch;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: thin;
  }

  .tab {
    position: relative;
    display: flex;
    flex: 0 1 auto;
    align-items: center;
    min-width: 110px;
    max-width: 240px;
    border-right: 1px solid var(--border);
  }

  .tab:first-child {
    border-left: 1px solid var(--border);
  }

  .tab.on {
    flex-shrink: 0;
    background: var(--panel);
  }

  /* The current tab's line, on its bottom edge over the bar's border. */
  .tab.on::after {
    content: "";
    position: absolute;
    inset: auto 0 -1px;
    height: 2px;
    background: var(--accent);
  }

  .tab.dragged {
    opacity: 0.6;
  }

  .target,
  .zone {
    position: fixed;
    z-index: 25;
    border-radius: var(--radius-lg);
    pointer-events: none;
  }

  .target {
    border: 1.5px dashed var(--accent);
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }

  .zone {
    display: grid;
    place-items: center;
    border: 2px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    transition:
      left var(--dur-fast) var(--ease-out),
      top var(--dur-fast) var(--ease-out),
      width var(--dur-fast) var(--ease-out),
      height var(--dur-fast) var(--ease-out);
  }

  .zone span {
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    background: var(--accent);
    color: var(--accent-text);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .tab.chosen {
    background: var(--accent-soft);
  }

  .tab.chosen [role="tab"] {
    color: var(--accent-ink);
  }

  [role="tab"] {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 100%;
    padding: 0 30px 0 12px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    white-space: nowrap;
  }

  /* The shortcut keeps the room of the close button, which takes its place on hover and on the current tab. */
  [role="tab"]:has(kbd) {
    padding-right: 4px;
  }

  [role="tab"]:hover,
  .on [role="tab"] {
    color: var(--text);
  }

  [role="tab"]:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .mark {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .dot {
    flex-shrink: 0;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--waiting);
  }

  .count {
    flex-shrink: 0;
    min-width: 16px;
    padding: 0 4px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: center;
  }

  /* As wide as the close button over it. */
  kbd {
    flex-shrink: 0;
    min-width: 24px;
    border-bottom-width: 1px;
    background: none;
  }

  .tab:hover kbd,
  .tab.on kbd,
  .tab:focus-within kbd {
    visibility: hidden;
  }

  .close {
    position: absolute;
    right: 4px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    opacity: 0;
  }

  .tab:hover .close,
  .tab.on .close,
  .tab:focus-within .close {
    opacity: 1;
  }

  .close:hover {
    background: var(--hover);
    color: var(--text);
  }

  .rename {
    flex: 1;
    min-width: 0;
    height: 24px;
    margin: 0 30px 0 6px;
    padding: 0 6px;
    border: 1px solid var(--focus);
    border-radius: var(--radius-sm);
    background: var(--panel);
    color: var(--text);
    font: inherit;
    font-size: var(--text-sm);
  }

  .new {
    flex-shrink: 0;
    margin-left: 4px;
  }

  .confirm {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    font-size: var(--text-sm);
  }

  .confirm p {
    margin: 0;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
