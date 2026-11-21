<script lang="ts" generics="Id extends string">
  import type { Component, Snippet } from "svelte";

  type Item = {
    id: Id;
    label: string;
    icon: Component;
    dot?: "warn" | "error" | null;
    /** Said by the dot to a screen reader; without it the dot is decoration. */
    dotLabel?: string;
    count?: number | null;
    /** A rule above it. */
    apart?: boolean;
  };

  let {
    items,
    current,
    label,
    onselect,
    top,
    empty = "",
  }: { items: Item[]; current: Id; label: string; onselect: (id: Id) => void; top?: Snippet; empty?: string } =
    $props();
</script>

<nav class="sections" aria-label={label}>
  {@render top?.()}
  <ul>
    {#each items as item (item.id)}
      <li>
        {#if item.apart}<hr />{/if}
        <button class="item" aria-current={current === item.id ? "page" : undefined} onclick={() => onselect(item.id)}>
          <item.icon size={16} aria-hidden="true" />
          <span class="label">{item.label}</span>
          {#if item.dot && item.dotLabel}
            <span class="dot {item.dot}" role="img" aria-label={item.dotLabel} title={item.dotLabel}></span>
          {:else if item.dot}
            <span class="dot {item.dot}" aria-hidden="true"></span>
          {/if}
          {#if item.count != null}<span class="n">{item.count}</span>{/if}
        </button>
      </li>
    {:else}
      {#if empty}<li class="none">{empty}</li>{/if}
    {/each}
  </ul>
</nav>

<style>
  .sections {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
    padding: 12px 8px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  hr {
    margin: 8px;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 30px;
    padding: 5px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-weight: 500;
    text-align: left;
  }

  .item:hover {
    background: var(--hover);
  }

  .item[aria-current] {
    background: var(--selected);
  }

  .item > :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .item[aria-current] > :global(svg) {
    color: var(--accent-ink);
  }

  .label {
    flex: 1;
    min-width: 0;
  }

  .dot {
    flex-shrink: 0;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--waiting);
  }

  .dot.error {
    background: var(--danger);
  }

  .n {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .none {
    padding: 6px 8px;
    color: var(--muted);
    font-size: var(--text-sm);
  }
</style>
