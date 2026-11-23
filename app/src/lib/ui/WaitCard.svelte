<script lang="ts">
  import type { Snippet } from "svelte";
  import BellRing from "@lucide/svelte/icons/bell-ring";

  /**
   * An agent is blocked by the user: the one loud thing of a screen, always first, in amber, with the answer right
   * there (`action`, or `children` for a field). Amber means this and nothing else.
   */
  let {
    title,
    meta,
    action,
    children,
  }: { title: string; meta?: string; action?: { label: string; kbd?: string; run: () => void }; children?: Snippet } = $props();
</script>

<section class="wait" aria-label={title}>
  <span class="mark" aria-hidden="true"><BellRing size={15} /></span>
  <div class="text">
    <h3>{title}</h3>
    {#if meta}<p class="meta">{meta}</p>{/if}
    {#if children}<div class="answer">{@render children()}</div>{/if}
  </div>
  {#if action}
    <button class="btn" onclick={action.run}>{action.label}{#if action.kbd}<kbd>{action.kbd}</kbd>{/if}</button>
  {/if}
</section>

<style>
  .wait {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--waiting-line);
    border-radius: var(--radius-md);
    background: var(--waiting-soft);
  }

  .mark {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    align-self: flex-start;
    width: 28px;
    height: 28px;
    border-radius: 50%;
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
    line-height: 28px;
  }

  .meta {
    margin: 0;
    color: var(--soft);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .answer {
    margin-top: 10px;
  }
</style>
