<script lang="ts" module>
  /** What a custom control spreads on itself to be labelled and described like the default input. */
  export type FieldAttrs = { id: string; "aria-describedby"?: string; "aria-invalid"?: true };
</script>

<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLInputAttributes } from "svelte/elements";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import { t } from "$lib/i18n";

  /**
   * A label above, the help or an example below, and an error that says how to fix it (it replaces the help).
   * Without `control`, a text input bound to `value`.
   */
  let {
    label,
    help,
    example,
    error,
    value = $bindable(""),
    id = `field-${Math.random().toString(36).slice(2)}`,
    control,
    ...rest
  }: Omit<HTMLInputAttributes, "value" | "id"> & {
    label: string;
    help?: string;
    example?: string;
    error?: string;
    value?: string;
    id?: string;
    control?: Snippet<[FieldAttrs]>;
  } = $props();

  const describedby = $derived(error || help || example ? `${id}-note` : undefined);
  const attrs = $derived<FieldAttrs>({ id, "aria-describedby": describedby, "aria-invalid": error ? true : undefined });
</script>

<div class="field-box">
  <label for={id}>{label}</label>
  {#if control}
    {@render control(attrs)}
  {:else}
    <input class="field" {...rest} {...attrs} bind:value />
  {/if}
  {#if error}
    <p class="note error" id="{id}-note"><CircleAlert size={13} aria-hidden="true" />{error}</p>
  {:else if help || example}
    <p class="note" id="{id}-note">
      {#if help}<span>{help}</span>{/if}
      {#if example}<span>{t("uiF1Example", { example })}</span>{/if}
    </p>
  {/if}
</div>

<style>
  .field-box {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  label {
    font-weight: 500;
  }

  .note {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .note.error {
    flex-direction: row;
    align-items: flex-start;
    gap: 6px;
    color: var(--danger);
  }

  .note.error :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }
</style>
