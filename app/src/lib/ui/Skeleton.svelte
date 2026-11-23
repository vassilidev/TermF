<script lang="ts">
  import { t } from "$lib/i18n";

  /**
   * Stands for content that is loading, in its shape: `lines` of text, a `list` of rows with an avatar, `cards`, a
   * `table` (a header, then rows of cells), a `form` (labels over fields, as in the settings), or one `block` (a diff, a log). Drawn after `delay` ms only, so a quick load never flashes. `label` is what a screen reader
   * hears; `height` sizes a block.
   */
  let {
    shape = "lines",
    rows = 3,
    label,
    delay = 150,
    height = "160px",
  }: { shape?: "lines" | "list" | "cards" | "table" | "form" | "block"; rows?: number; label?: string; delay?: number; height?: string } = $props();

  let shown = $state(false);
  $effect(() => {
    const timer = setTimeout(() => (shown = true), delay);
    return () => clearTimeout(timer);
  });

  // Ragged like real text, the same on every render.
  const widths = [94, 78, 86, 62, 90, 70, 82, 56];
  const width = (i: number, scale = 1) => `${Math.round(widths[i % widths.length] * scale)}%`;
</script>

<div class="skeleton {shape}" role="status" aria-busy="true" data-skeleton>
  <span class="sr-only">{label ?? t("liveLoading")}</span>
  {#if shown}
    <div class="bones" aria-hidden="true">
      {#if shape === "block"}
        <span class="bone" style:height></span>
      {:else}
        {#each { length: rows }, i (i)}
          {#if shape === "list"}
            <div class="row">
              <span class="bone avatar"></span>
              <span class="stack">
                <span class="bone line" style:width={width(i, 0.55)}></span>
                <span class="bone line thin" style:width={width(i + 3, 0.35)}></span>
              </span>
            </div>
          {:else if shape === "table"}
            <div class="cells" class:head={i === 0}>
              <span class="bone line" style:width={width(i, 0.4)}></span>
              <span class="bone line thin" style:width={width(i + 2, 0.25)}></span>
              <span class="bone line thin num" style:width={width(i + 4, 0.15)}></span>
            </div>
          {:else if shape === "form"}
            <div class="pair">
              <span class="bone line thin" style:width={width(i, 0.22)}></span>
              <span class="bone field"></span>
            </div>
          {:else if shape === "cards"}
            <div class="card">
              <span class="bone line" style:width={width(i, 0.5)}></span>
              <span class="bone line thin" style:width={width(i + 1)}></span>
              <span class="bone line thin" style:width={width(i + 2, 0.7)}></span>
            </div>
          {:else}
            <span class="bone line" style:width={width(i)}></span>
          {/if}
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .skeleton {
    width: 100%;
  }

  .bones {
    display: flex;
    flex-direction: column;
    gap: 10px;
    animation: appear var(--dur-slow) var(--ease-out);
  }

  /* A block can fill its parent: `height="100%"`. */
  .block,
  .block .bones {
    height: 100%;
  }

  .table .bones,
  .form .bones {
    gap: 0;
  }

  .cells {
    display: grid;
    grid-template-columns: 2fr 1fr 80px;
    align-items: center;
    gap: 16px;
    height: var(--row);
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
  }

  .cells.head {
    height: 32px;
  }

  .cells .num {
    justify-self: end;
  }

  .pair {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 0;
  }

  .field {
    width: 100%;
    max-width: 420px;
    height: var(--h);
    border-radius: var(--radius);
  }

  .cards .bones {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 12px;
  }

  .bone {
    position: relative;
    display: block;
    overflow: hidden;
    border-radius: var(--radius-sm);
    background: var(--selected);
  }

  /* A soft light passing over, on the compositor: it costs nothing to the page. */
  .bone::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg, transparent, var(--hover), transparent);
    transform: translateX(-100%);
    animation: shimmer 1.6s ease-in-out infinite;
  }

  .line {
    height: 10px;
  }

  .line.thin {
    height: 8px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 32px;
  }

  .avatar {
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-pill);
  }

  .stack {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .card .line:first-child {
    height: 12px;
    margin-bottom: 4px;
  }

  @keyframes shimmer {
    to {
      transform: translateX(100%);
    }
  }

  @keyframes appear {
    from {
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .bone::after {
      display: none;
    }
  }
</style>
