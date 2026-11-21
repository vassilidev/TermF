<script lang="ts">
  import type { Component, Snippet } from "svelte";

  let {
    icon: Icon,
    title = "",
    text = "",
    children,
  }: { icon?: Component; title?: string; text?: string; children?: Snippet } = $props();
</script>

<div class="empty">
  {#if Icon}<Icon size={20} aria-hidden="true" />{/if}
  {#if title}<h2>{title}</h2>{/if}
  {#if text}<p>{text}</p>{/if}
  {@render children?.()}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    max-width: 480px;
    margin: 48px auto 0;
    color: var(--muted);
    text-align: center;
  }

  .empty > :global(svg) {
    color: var(--accent-ink);
  }

  h2 {
    margin: 8px 0 6px;
    color: var(--text);
    font-size: var(--text-md);
  }

  p {
    margin: 0;
  }

  .empty > :global(.btn) {
    margin-top: 14px;
  }
</style>
