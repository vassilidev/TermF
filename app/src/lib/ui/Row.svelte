<script lang="ts">
  import type { Snippet } from "svelte";

  /** A row's content: something before (an avatar, an icon), a title and its second line, something after (a state, a time). */
  let {
    title,
    sub,
    lead,
    trail,
  }: { title: string; sub?: string; lead?: Snippet; trail?: Snippet } = $props();
</script>

<div class="row-content">
  {@render lead?.()}
  <div class="text">
    <span class="title">{title}</span>
    {#if sub}<span class="sub">{sub}</span>{/if}
  </div>
  {#if trail}<div class="trail">{@render trail()}</div>{/if}
</div>

<style>
  .row-content {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .title,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .title {
    font-weight: 500;
  }

  .sub {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .trail {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
</style>
