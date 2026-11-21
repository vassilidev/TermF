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
    gap: 3px;
  }

  .tally > span {
    display: inline-flex;
  }

  .tally :global(.pill) {
    gap: 3px;
    height: 20px;
    padding: 0 6px 0 5px;
    font-variant-numeric: tabular-nums;
  }
</style>
