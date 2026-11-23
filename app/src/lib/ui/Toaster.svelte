<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Info from "@lucide/svelte/icons/info";
  import X from "@lucide/svelte/icons/x";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { dismiss, hold, toasts, undo } from "$lib/ui/toast.svelte";

  /** Mounted once: the toasts at the bottom of the window. ⌘Z (Ctrl+Z) undoes the newest one, outside a field or a terminal. */
  const icons = { ok: Check, error: CircleAlert, info: Info };
  const undoKey = isMac ? "⌘Z" : "Ctrl+Z";

  function onkeydown(e: KeyboardEvent) {
    if (e.key.toLowerCase() !== "z" || e.shiftKey || e.altKey || !(isMac ? e.metaKey : e.ctrlKey)) return;
    const target = e.target as HTMLElement | null;
    if (target?.closest?.("input, textarea, select, [contenteditable]:not([contenteditable='false']), .xterm")) return;
    const last = toasts.findLast((item) => item.undo);
    if (!last) return;
    e.preventDefault();
    undo(last.id);
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions (hovering only pauses the timers) -->
<ol class="toaster" aria-live="polite" onpointerenter={() => hold(true)} onpointerleave={() => hold(false)}>
  {#each toasts as item (item.id)}
    {@const Icon = icons[item.kind]}
    <li class="toast toast-card {item.kind}" role={item.kind === "error" ? "alert" : "status"}>
      <Icon size={15} aria-hidden="true" />
      <span>{item.text}</span>
      {#if item.undo}
        <button class="btn sm" onclick={() => undo(item.id)}>{t("uiF1Undo")}<kbd>{undoKey}</kbd></button>
      {/if}
      <button class="icon-btn sm" onclick={() => dismiss(item.id)} aria-label={t("dismiss")} title={t("dismiss")}>
        <X size={14} aria-hidden="true" />
      </button>
    </li>
  {/each}
</ol>

<style>
  .toaster {
    position: fixed;
    z-index: 40;
    bottom: 20px;
    left: 50%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
    transform: translateX(-50%);
    pointer-events: none;
  }

  .toast {
    pointer-events: auto;
    animation: rise var(--dur) var(--ease-out);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }

  .toast > :global(svg) {
    flex-shrink: 0;
    color: #62cf8e;
  }

  .toast.error > :global(svg) {
    color: #ff8178;
  }

  .toast.info > :global(svg) {
    color: #a3a5ff;
  }

  .toast .icon-btn {
    color: #b9b9c6;
  }

  .toast .icon-btn:hover {
    background: rgb(255 255 255 / 0.1);
    color: #fff;
  }
</style>
