<script lang="ts" module>
  export type Tab = { id: string; label: string; count?: number | null; divider?: boolean };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    id,
    label,
    tabs,
    selected,
    onselect,
    children,
  }: {
    id: string;
    label: string;
    tabs: Tab[];
    selected: string | null;
    onselect: (tab: string) => void;
    children: Snippet;
  } = $props();

  const active = $derived(tabs.find((tab) => tab.id === selected));
  // Without a selected tab, the first one keeps the list reachable with Tab.
  const focusable = $derived(active?.id ?? tabs[0]?.id);

  function onkeydown(e: KeyboardEvent) {
    const at = tabs.findIndex((tab) => tab.id === (e.target as HTMLElement).dataset.tab);
    const to = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: tabs.length - 1 }[e.key];
    if (at < 0 || to === undefined) return;
    e.preventDefault();
    const next = tabs[(to + tabs.length) % tabs.length];
    onselect(next.id);
    document.getElementById(`${id}-tab-${next.id}`)?.focus();
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus (focus goes to the tabs themselves) -->
<div class="tabs" role="tablist" aria-label={label} {onkeydown}>
  {#each tabs as tab (tab.id)}
    {#if tab.divider}<span class="divider" aria-hidden="true"></span>{/if}
    <button
      role="tab"
      id="{id}-tab-{tab.id}"
      data-tab={tab.id}
      aria-selected={tab === active}
      aria-controls={active ? `${id}-panel` : undefined}
      tabindex={tab.id === focusable ? 0 : -1}
      onclick={() => onselect(tab.id)}
    >
      {tab.label}{#if tab.count != null}<span class="count">{tab.count}</span>{/if}
    </button>
  {/each}
</div>
<div
  class="panel"
  id="{id}-panel"
  role={active ? "tabpanel" : undefined}
  aria-labelledby={active ? `${id}-tab-${active.id}` : undefined}
>
  {@render children()}
</div>

<style>
  .tabs {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-shrink: 0;
    padding: 0 16px;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  button {
    position: relative;
    flex-shrink: 0;
    height: 38px;
    padding: 0 4px;
    border: 0;
    background: none;
    color: var(--muted);
    font-weight: 500;
    transition: color var(--dur-fast) ease-out;
  }

  button:hover,
  button[aria-selected="true"] {
    color: var(--text);
  }

  button[aria-selected="true"]::after {
    content: "";
    position: absolute;
    inset: auto 2px 0;
    height: 2px;
    border-radius: 2px;
    background: var(--text);
  }

  button:focus-visible {
    outline-offset: -2px;
  }

  .count {
    margin-left: 6px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .divider {
    flex-shrink: 0;
    width: 1px;
    height: 16px;
    margin: 0 6px;
    background: var(--border);
  }

  .panel {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }
</style>
