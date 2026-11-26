<script lang="ts">
  import FileText from "@lucide/svelte/icons/file-text";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LayoutPanelLeft from "@lucide/svelte/icons/layout-panel-left";
  import TextSelect from "@lucide/svelte/icons/text-select";
  import X from "@lucide/svelte/icons/x";
  import type { Chip } from "$lib/ask-logic";
  import { t } from "$lib/i18n";

  /** What a screen gives a question, one chip each, taken off by its cross; laid out by the parent's flex row. */
  let { chips, onremove }: { chips: Chip[]; onremove: (chip: Chip) => void } = $props();

  const icons = { view: LayoutPanelLeft, object: FileText, selection: TextSelect };
</script>

<ul aria-label={t("aigContext")}>
  {#each chips as c (c.id)}
    {@const Icon = c.secret ? KeyRound : icons[c.kind]}
    <li class="chip" title={c.secret ? t("aigSecretChip") : (c.text?.slice(0, 300) ?? c.label)}>
      <Icon size={12} aria-hidden="true" />
      <span class="label">{c.label}</span>
      <button type="button" class="x" aria-label={t("aigRemoveChip", { label: c.label })} onclick={() => onremove(c)}>
        <X size={11} aria-hidden="true" />
      </button>
    </li>
  {/each}
</ul>

<style>
  ul {
    display: contents;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    height: 24px;
    padding: 0 4px 0 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .x {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: none;
    color: var(--muted);
  }

  .x:hover {
    background: var(--hover);
    color: var(--text);
  }

  .x:focus-visible {
    outline: 2px solid var(--focus);
  }
</style>
