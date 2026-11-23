<script lang="ts">
  /** An on/off setting that applies at once (`input.switch` of app.css), its label clickable, an optional hint under it. */
  let {
    checked = $bindable(false),
    label,
    hint,
    disabled = false,
    onchange,
  }: { checked?: boolean; label: string; hint?: string; disabled?: boolean; onchange?: (on: boolean) => void } = $props();
</script>

<label class="switch-row" class:disabled>
  <span class="text">
    <span>{label}</span>
    {#if hint}<span class="hint">{hint}</span>{/if}
  </span>
  <input type="checkbox" role="switch" class="switch" bind:checked {disabled} onchange={() => onchange?.(checked)} />
</label>

<style>
  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-height: var(--h);
    cursor: pointer;
  }

  .disabled {
    cursor: default;
    opacity: 0.55;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .hint {
    color: var(--muted);
    font-size: var(--text-sm);
  }
</style>
