<script lang="ts">
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { t } from "$lib/i18n";
  import type { Tally } from "$lib/nav-logic";

  let { tally }: { tally: Tally | undefined } = $props();

  const parts = $derived(
    tally
      ? ([
          ["waiting", tally.waiting, "navWaiting"],
          ["working", tally.working, "navWorking"],
          ["failed", tally.failed, "navFailed"],
        ] as const).filter(([, count]) => count > 0)
      : [],
  );
</script>

{#if parts.length}
  <span class="tally">
    {#each parts as [status, count, key] (status)}
      {@const text = t(key, { count })}
      <span role="img" aria-label={text} title={text}><StatusPill {status} label={String(count)} /></span>
    {/each}
  </span>
{/if}

<style>
  .tally {
    display: inline-flex;
    flex-shrink: 0;
    gap: 2px;
  }

  .tally > span {
    display: inline-flex;
  }

  /* Compact pills for a 30px row; only the amber one keeps its fill, the single signal of urgency. */
  .tally :global(.pill) {
    gap: 2px;
    height: 20px;
    padding: 0 6px 0 5px;
    font-variant-numeric: tabular-nums;
  }

  .tally :global(.pill:not(.waiting)) {
    padding: 0 3px;
    background: none;
  }

  .tally :global(.pill svg) {
    width: 12px;
    height: 12px;
  }
</style>
