<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "$lib/i18n";

  /**
   * A modal question, only for what cannot be undone (a toast with Undo covers the rest): what is about to happen, then
   * Cancel (or `cancel`, "Keep") or the action, whose label says what is lost. Closing it by any means cancels.
   */
  let {
    open = $bindable(false),
    title,
    confirm,
    cancel,
    danger = false,
    disabled = false,
    onconfirm,
    children,
  }: {
    open?: boolean;
    title: string;
    confirm: string;
    cancel?: string;
    danger?: boolean;
    /** Until the body has what the answer needs, such as a name typed back. */
    disabled?: boolean;
    onconfirm: () => void;
    children?: Snippet;
  } = $props();

  let dialog = $state<HTMLDialogElement>();
  const id = `confirm-${Math.random().toString(36).slice(2)}`;

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog bind:this={dialog} class="confirm" aria-labelledby={id} onclose={() => (open = false)}>
  {#if open}
    <h2 {id}>{title}</h2>
    {#if children}<div class="body">{@render children()}</div>{/if}
    <div class="answers">
      <button class="btn" onclick={() => (open = false)}>{cancel ?? t("cancel")}</button>
      <button
        class="btn {danger ? 'danger' : 'primary'}"
        {disabled}
        onclick={() => {
          open = false;
          onconfirm();
        }}>{confirm}</button
      >
    </div>
  {/if}
</dialog>

<style>
  .confirm {
    width: min(460px, calc(100vw - 48px));
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .confirm[open] {
    animation: rise var(--dur) var(--ease-out);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .confirm[open] {
      animation: none;
    }
  }

  .confirm::backdrop {
    background: var(--scrim, rgb(10 12 20 / 0.3));
  }

  h2 {
    margin: 0;
    padding: 18px 20px 8px;
    font-size: var(--text-lg);
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 0 20px 16px;
    color: var(--soft);
  }

  .body :global(p) {
    margin: 0;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }

  h2 + .answers {
    margin-top: 8px;
  }
</style>
