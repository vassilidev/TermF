<script lang="ts">
  import { qr, qrPath } from "$lib/qr-logic";

  /** Dark on light whatever the theme: phones read that best. */
  let { text, size = 168, label }: { text: string; size?: number; label: string } = $props();

  const rows = $derived(qr(text));
  const side = $derived(rows.length + 8);
</script>

<svg class="qr" width={size} height={size} viewBox="0 0 {side} {side}" role="img" aria-label={label} shape-rendering="crispEdges">
  <rect width={side} height={side} fill="#fff" />
  <path d={qrPath(rows)} fill="#000" />
</svg>

<style>
  .qr {
    display: block;
    border-radius: var(--radius);
  }
</style>
