<script lang="ts">
  import type { Snippet } from "svelte";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";

  let {
    count,
    total,
    label,
    onall,
    onclear,
    children,
  }: {
    count: number;
    total: number;
    label: string;
    onall: () => void;
    onclear: () => void;
    children: Snippet;
  } = $props();

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    onclear();
  }
</script>

<!-- svelte-ignore a11y_interactive_supports_focus (Escape is for its own controls) -->
<div class="selection" role="toolbar" aria-label={label} tabindex="-1" {onkeydown}>
  <label class="every">
    <input
      type="checkbox"
      checked={count > 0 && count === total}
      indeterminate={count > 0 && count < total}
      onchange={onall}
    />
    <span>{t("t6bSelectAll")}</span>
  </label>
  <span class="count" aria-live="polite">{t("claudeRedesignSelected", { count })}</span>
  <div class="actions">{@render children()}</div>
  <button
    class="icon-btn sm"
    aria-label={t("claudeRedesignClearSelection")}
    title={t("claudeRedesignClearSelection")}
    onclick={onclear}
  >
    <X size={14} aria-hidden="true" />
  </button>
</div>

<style>
  .selection {
    display: flex;
    flex-shrink: 0;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    min-height: 40px;
    padding: 6px 8px 6px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--accent-soft);
    font-size: var(--text-sm);
  }

  .selection:focus {
    outline: none;
  }

  .every {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    cursor: pointer;
  }

  .every input {
    margin: 0;
  }

  .count {
    color: var(--accent-ink);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .actions {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 0;
  }

  /* What the last action left picked, and the question a destructive one asks. */
  .actions :global(.note) {
    color: var(--muted);
  }

  .actions :global([role="alert"]) {
    margin-right: 4px;
    font-weight: 500;
  }
</style>
