<script lang="ts">
  import type { Snippet } from "svelte";
  import { ui } from "$lib/state.svelte";

  let {
    open = $bindable(),
    trigger,
    label,
    align = "start",
    width,
    closeOutside = true,
    children,
  }: {
    open: boolean;
    trigger: HTMLElement | undefined;
    label: string;
    /** Which edge of the trigger the popover lines up with. */
    align?: "start" | "end";
    width?: number;
    /** A form with unsaved input keeps its popover open on a click elsewhere. */
    closeOutside?: boolean;
    children: Snippet;
  } = $props();

  let panel = $state<HTMLElement>();
  let style = $state("");

  $effect(() => {
    if (!open || !trigger) return;
    const anchor = trigger;
    const place = () => {
      const r = anchor.getBoundingClientRect();
      const side = align === "start" ? `left: ${r.left}px` : `right: ${innerWidth - r.right}px`;
      style = `${side}; top: ${r.bottom + 4}px; max-height: ${innerHeight - r.bottom - 12}px;${width ? ` width: ${width}px;` : ""}`;
    };
    place();
    addEventListener("resize", place);
    return () => removeEventListener("resize", place);
  });

  // Opened from the keyboard or a click, the popover takes the focus, unless its content already did.
  $effect(() => {
    if (panel && !panel.contains(document.activeElement)) {
      panel.querySelector<HTMLElement>("input, button, [href], [tabindex]:not([tabindex='-1'])")?.focus();
    }
  });

  // A view change or the palette takes the screen.
  let view = ui.view;
  $effect(() => {
    if (ui.palette || ui.view !== view) {
      view = ui.view;
      open = false;
    }
  });

  /** Escape typed elsewhere, in a terminal for instance, is not for the popover. */
  function onkeydown(e: KeyboardEvent) {
    const focus = document.activeElement;
    if (!open || e.key !== "Escape" || (focus !== trigger && !panel?.contains(focus))) return;
    e.preventDefault();
    open = false;
    trigger?.focus();
  }

  function onpointerdown(e: PointerEvent) {
    const target = e.target as Node;
    if (open && closeOutside && !panel?.contains(target) && !trigger?.contains(target)) open = false;
  }
</script>

<svelte:window {onkeydown} {onpointerdown} />

{#if open}
  <!-- Focusable, so that a click on its text keeps the focus, and Escape, inside. -->
  <div class="popover" role="dialog" aria-label={label} tabindex="-1" bind:this={panel} {style}>
    {@render children()}
  </div>
{/if}

<style>
  .popover {
    position: fixed;
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    box-shadow: var(--shadow-3);
    overflow: auto;
    animation: drop var(--dur) var(--ease-out);
  }

  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }
</style>
