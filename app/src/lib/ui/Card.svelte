<script lang="ts">
  import type { Snippet } from "svelte";

  /** A bounded group of content: an optional title with its count and actions, then the body. `raised` lifts it (a popover's shadow). */
  let {
    title,
    count,
    actions,
    raised = false,
    flush = false,
    children,
  }: { title?: string; count?: number | null; actions?: Snippet; raised?: boolean; flush?: boolean; children: Snippet } = $props();
</script>

<section class="card" class:raised class:flush>
  {#if title || actions}
    <header>
      {#if title}<h3>{title}{#if count != null}<span class="count">{count}</span>{/if}</h3>{/if}
      {#if actions}<div class="actions">{@render actions()}</div>{/if}
    </header>
  {/if}
  <div class="body">{@render children()}</div>
</section>

<style>
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .raised {
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-2);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--border);
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex: 1;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .count {
    color: var(--muted);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }

  .actions {
    display: flex;
    gap: 6px;
  }

  .body {
    padding: 16px;
  }

  .flush .body {
    padding: 0;
  }
</style>
