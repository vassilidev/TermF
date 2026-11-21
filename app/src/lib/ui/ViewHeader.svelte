<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    context = "",
    children,
    actions,
  }: {
    title: string;
    /** Discreet text after the title: what the view is about (a folder, a project). */
    context?: string;
    /** A control after the title, such as a segmented choice or a field. */
    children?: Snippet;
    actions?: Snippet;
  } = $props();
</script>

<header class="view-header">
  <h1>{title}</h1>
  {#if context}<span class="context">{context}</span>{/if}
  {@render children?.()}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  /* One 48px row; a narrow window wraps the actions instead of hiding them. */
  .view-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    flex-shrink: 0;
    min-height: 48px;
    padding: 8px 12px 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  h1 {
    flex-shrink: 0;
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .context {
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    margin-left: auto;
  }
</style>
