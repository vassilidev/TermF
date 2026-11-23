<script lang="ts">
  import Info from "@lucide/svelte/icons/info";

  /**
   * Short help for a term, a setting or a proposal that is not obvious: two lines at most, a `proof` from the user's
   * own data ("212 tests found"), and where to read more.
   */
  let { text, proof, link }: { text: string; proof?: string; link?: { label: string; run: () => void } } = $props();
</script>

<div class="hint" role="note">
  <Info size={14} aria-hidden="true" />
  <p>
    {text}
    {#if proof}<span class="proof">{proof}</span>{/if}
    {#if link}<button class="btn link" onclick={link.run}>{link.label}</button>{/if}
  </p>
</div>

<style>
  .hint {
    display: flex;
    gap: 8px;
    max-width: 60ch;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .hint > :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--muted);
  }

  p {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .proof {
    color: var(--done);
    font-weight: 500;
  }

  .btn.link {
    font-size: inherit;
  }
</style>
