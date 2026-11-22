<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "$lib/i18n";

  /** A modal question: what is about to happen, then Cancel or the action. Closing it by any means cancels. */
  let {
    open = $bindable(false),
    title,
    confirm,
    danger = false,
    disabled = false,
    onconfirm,
    children,
  }: {
    open?: boolean;
    title: string;
    confirm: string;
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
      <button class="btn" onclick={() => (open = false)}>{t("cancel")}</button>
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
    width: min(480px, calc(100vw - 48px));
    padding: 20px;
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
    background: rgb(10 12 20 / 0.32);
  }

  h2 {
    margin: 0 0 12px;
    font-size: var(--text-lg);
    overflow-wrap: anywhere;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 20px;
    color: var(--muted);
  }

  .body :global(p) {
    margin: 0;
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
