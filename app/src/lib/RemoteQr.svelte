<script lang="ts">
  import { fits, qr, qrPath } from "$lib/qr-logic";

  /** Dark on light whatever the theme: phones read that best. Nothing for a text too long for a QR code. */
  let { text, size = 168, label }: { text: string; size?: number; label: string } = $props();

  const rows = $derived(fits(text) ? qr(text) : null);
  const side = $derived((rows?.length ?? 0) + 8);
</script>

{#if rows}
  <svg class="qr" width={size} height={size} viewBox="0 0 {side} {side}" role="img" aria-label={label} shape-rendering="crispEdges">
    <rect width={side} height={side} fill="#fff" />
    <path d={qrPath(rows)} fill="#000" />
  </svg>
{/if}

<style>
  .qr {
    display: block;
    border-radius: var(--radius);
  }
</style>
