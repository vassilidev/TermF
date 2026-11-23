<script lang="ts" module>
  export type Segment = { id: string; label: string; count?: number | null };
</script>

<script lang="ts">
  /** A filter among a few options (`.segmented` of app.css): one radio group, the arrows move and pick. */
  let {
    label,
    options,
    selected,
    onselect,
  }: { label: string; options: Segment[]; selected: string; onselect: (id: string) => void } = $props();

  let group = $state<HTMLElement>();

  function onkeydown(e: KeyboardEvent) {
    const at = options.findIndex((o) => o.id === selected);
    const to = { ArrowLeft: at - 1, ArrowUp: at - 1, ArrowRight: at + 1, ArrowDown: at + 1, Home: 0, End: options.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    const next = options[(to + options.length) % options.length];
    onselect(next.id);
    group?.querySelector<HTMLElement>(`[data-segment="${CSS.escape(next.id)}"]`)?.focus();
  }
</script>

<div class="segmented" role="radiogroup" aria-label={label} tabindex="-1" bind:this={group} {onkeydown}>
  {#each options as option (option.id)}
    <button
      type="button"
      role="radio"
      data-segment={option.id}
      aria-checked={option.id === selected}
      tabindex={option.id === selected ? 0 : -1}
      onclick={() => onselect(option.id)}
    >
      {option.label}{#if option.count != null}<span class="count">{option.count}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .segmented:focus {
    outline: none;
  }

  .count {
    margin-left: 5px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
