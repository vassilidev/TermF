<script lang="ts">
  import { t } from "$lib/i18n";
  import { pick } from "$lib/onboarding-logic";

  /** The answer field of a question: a text, chips for a multiple choice, a segmented choice for one option or yes/no. */
  let {
    kind,
    options,
    label,
    value,
    onchange,
  }: {
    kind: "choice" | "multi" | "bool" | "text";
    options: string[];
    label: string;
    value: string[];
    onchange: (value: string[]) => void;
  } = $props();
</script>

{#if kind === "text"}
  <input class="field" value={value[0] ?? ""} aria-label={label} oninput={(e) => onchange([e.currentTarget.value])} />
{:else if kind === "multi"}
  <div class="chips" role="group" aria-label={label}>
    {#each options as option (option)}
      <label class="chip" class:on={value.includes(option)}>
        <input type="checkbox" checked={value.includes(option)} onchange={() => onchange(pick({ kind, options }, value, option))} />
        {option}
      </label>
    {/each}
  </div>
{:else}
  {@const choices = kind === "bool" ? ["yes", "no"] : options}
  <div class="segmented" role="radiogroup" aria-label={label}>
    {#each choices as option (option)}
      <button type="button" role="radio" aria-checked={value[0] === option} onclick={() => onchange(pick({ kind, options }, value, option))}>
        {kind === "bool" ? t(option === "yes" ? "obYes" : "obNo") : option}
      </button>
    {/each}
  </div>
{/if}

<style>
  .field {
    width: min(100%, 480px);
  }

  .segmented {
    flex-wrap: wrap;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .chip input {
    margin: 0;
  }

  .chip.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }
</style>
