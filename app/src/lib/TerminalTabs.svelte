<script lang="ts">
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import { initial } from "$lib/ProjectSwitcher.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import { focusSession, keys, openLauncher } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { aria } from "$lib/keymap.svelte";
  import { sessionProjects } from "$lib/project.svelte";
  import { leaves } from "$lib/split-logic";
  import { closeTab, currentTab, entryOf, moveTab, tabName, terminals, visibleTabs, type Tab } from "$lib/tabs.svelte";
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
  let drag: { tab: Tab; x: number } | null = null;
  // The click that ends a drag is not a selection.
  let moved = false;

  const sessionsOf = (tab: Tab) =>
    leaves(tab.root)
      .map((id) => app.sessions.find((s) => s.id === id))
      .filter((s): s is Session => !!s);

  function projectName(tab: Tab): string {
    const s = app.sessions.find((s) => s.id === tab.active);
    const id = s && sessionProjects(s)[0];
    return app.workspace?.projects.find((p) => p.id === id)?.name ?? "";
  }

  const select = async (tab: Tab) => {
    await focusSession(entryOf(tab));
  };

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
    drag = { tab, x: e.clientX };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function over(e: PointerEvent) {
    if (!drag || (!moved && Math.abs(e.clientX - drag.x) < 5)) return;
    moved = true;
    dragged = drag.tab.id;
    const bar = (e.currentTarget as HTMLElement).closest("[role=tablist]");
    const under = [...(bar?.querySelectorAll<HTMLElement>("[data-tab]") ?? [])].find((el) => {
      const r = el.getBoundingClientRect();
      return e.clientX >= r.left && e.clientX < r.right;
    });
    const target = list.find((tab) => tab.id === Number(under?.dataset.tab));
    if (target && target.id !== drag.tab.id) moveTab(drag.tab, target);
  }

  function up() {
    drag = null;
    dragged = null;
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
      <div class="tab" class:on={tab === current} class:dragged={dragged === tab.id} data-tab={tab.id}>
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
            onclick={() => !moved && select(tab)}
            ondblclick={() => (renaming = tab.id)}
            onpointerdown={(e) => down(e, tab)}
            onpointermove={over}
            onpointerup={up}
            onpointercancel={up}
            onkeydown={(e) => onkeydown(e, tab)}
          >
            {#if sessionsOf(tab).some((s) => s.running && s.status === "waiting")}
              <span class="dot" role="img" aria-label={t("statusWaiting")}></span>
            {/if}
            {#if project}<span class="mark" title={project} aria-hidden="true">{initial(project)}</span><span class="sr-only">{project}</span>{/if}
            <span class="name">{tabName(tab)}</span>
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

  [role="tab"] {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
    min-width: 0;
    height: 100%;
    padding: 0 26px 0 12px;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
    white-space: nowrap;
  }

  /* The shortcut keeps the room of the close button, which takes its place on hover and on the current tab. */
  [role="tab"]:has(kbd) {
    padding-right: 6px;
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
    font-size: 10px;
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

  kbd {
    flex-shrink: 0;
    border-bottom-width: 1px;
    background: none;
  }

  .tab:hover kbd,
  .tab.on kbd {
    visibility: hidden;
  }

  .close {
    position: absolute;
    right: 5px;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    opacity: 0;
  }

  .tab:hover .close,
  .tab.on .close {
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
    margin: 0 26px 0 6px;
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
