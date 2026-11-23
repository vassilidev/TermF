<script lang="ts" module>
  import type { Component } from "svelte";

  export type MenuItem = { label: string; icon?: Component; kbd?: string; danger?: boolean; run: () => void };
</script>

<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import Popover from "$lib/ui/Popover.svelte";

  /**
   * Rows to pick from: hover, the selection (a light indigo ground and an edge), ↑ ↓ Home End to move (the selection
   * follows), Enter to open, and the row's actions on a right click, the context-menu key or Shift+F10.
   */
  let {
    label,
    items,
    key,
    selected = null,
    onselect,
    onopen,
    menu,
    row,
  }: {
    label: string;
    items: T[];
    key: (item: T) => string;
    selected?: string | null;
    onselect?: (item: T) => void;
    onopen?: (item: T) => void;
    menu?: (item: T) => MenuItem[];
    row: Snippet<[T]>;
  } = $props();

  let list = $state<HTMLElement>();
  let menuOpen = $state(false);
  let menuAt = $state<{ trigger: HTMLElement; actions: MenuItem[] }>();

  // Without a selection, the first row keeps the list reachable with Tab.
  const focusable = $derived(items.some((item) => key(item) === selected) ? selected : items[0] ? key(items[0]) : null);

  const rowOf = (id: string) => list?.querySelector<HTMLElement>(`[data-key="${CSS.escape(id)}"]`);

  /** A button, a link or a field inside the row keeps its own clicks. */
  const inside = (e: Event) =>
    (e.target as HTMLElement).closest("button, a, input, select, textarea, [contenteditable], [role=button]") !== null;

  function openMenu(item: T, trigger: HTMLElement) {
    const actions = menu?.(item) ?? [];
    if (!actions.length) return;
    onselect?.(item);
    menuAt = { trigger, actions };
    menuOpen = true;
  }

  function onkeydown(e: KeyboardEvent, item: T, index: number) {
    // Keys typed in a control of the row are that control's.
    if (e.target !== e.currentTarget) return;
    if (e.key === "Enter") {
      e.preventDefault();
      onopen?.(item);
      return;
    }
    if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
      e.preventDefault();
      openMenu(item, e.currentTarget as HTMLElement);
      return;
    }
    const to = { ArrowUp: index - 1, ArrowDown: index + 1, Home: 0, End: items.length - 1 }[e.key];
    if (to === undefined || to < 0 || to >= items.length) return;
    e.preventDefault();
    const next = items[to];
    onselect?.(next);
    rowOf(key(next))?.focus();
  }

  function act(run: () => void) {
    menuOpen = false;
    run();
  }
</script>

<ul class="rows" role="listbox" aria-label={label} bind:this={list}>
  {#each items as item, index (key(item))}
    {@const id = key(item)}
    <li
      class="item"
      role="option"
      data-key={id}
      aria-selected={id === selected}
      tabindex={id === focusable ? 0 : -1}
      onclick={(e) => inside(e) || onselect?.(item)}
      ondblclick={(e) => inside(e) || onopen?.(item)}
      oncontextmenu={(e) => {
        if (!menu || inside(e)) return;
        e.preventDefault();
        openMenu(item, e.currentTarget);
      }}
      onkeydown={(e) => onkeydown(e, item, index)}
    >
      {@render row(item)}
    </li>
  {/each}
</ul>

<Popover bind:open={menuOpen} trigger={menuAt?.trigger} label={label} width={240}>
  {#if menuAt}
    <div class="menu" role="menu">
      {#each menuAt.actions as action (action.label)}
        <button class="action" class:danger={action.danger} role="menuitem" onclick={() => act(action.run)}>
          {#if action.icon}<action.icon size={14} aria-hidden="true" />{/if}
          <span>{action.label}</span>
          {#if action.kbd}<kbd>{action.kbd}</kbd>{/if}
        </button>
      {/each}
    </div>
  {/if}
</Popover>

<style>
  .rows {
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
    list-style: none;
  }

  .item {
    position: relative;
    display: flex;
    align-items: center;
    min-height: var(--row-lg);
    padding: 8px 14px;
    border-top: 1px solid var(--border);
    cursor: default;
    transition: background-color var(--dur-fast) ease;
  }

  .item:first-child {
    border-top: 0;
  }

  .item:hover {
    background: var(--hover);
  }

  .item[aria-selected="true"] {
    background: var(--accent-soft);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .item:focus-visible {
    outline-offset: -2px;
  }

  .menu {
    display: flex;
    flex-direction: column;
    padding: 4px;
  }

  .action {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .action:hover,
  .action:focus-visible {
    background: var(--hover);
    outline: none;
  }

  .action span {
    flex: 1;
  }

  .action.danger {
    color: var(--danger);
  }
</style>
