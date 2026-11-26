<script lang="ts">
  import type { Snippet } from "svelte";
  import { ui } from "$lib/state.svelte";

  let {
    open = $bindable(),
    trigger,
    anchor,
    point,
    label,
    align = "start",
    above = false,
    width,
    closeOutside = true,
    children,
  }: {
    open: boolean;
    trigger: HTMLElement | undefined;
    /** Where it opens, when not under the trigger; the focus still comes back to the trigger. */
    anchor?: HTMLElement;
    /** A context menu: at the pointer, kept inside the window. */
    point?: { x: number; y: number } | null;
    label: string;
    /** Which edge of the trigger the popover lines up with. */
    align?: "start" | "end";
    /** Opens over the trigger: for a trigger at the bottom of the window. */
    above?: boolean;
    width?: number;
    /** A form with unsaved input keeps its popover open on a click elsewhere. */
    closeOutside?: boolean;
    children: Snippet;
  } = $props();

  let panel = $state<HTMLElement>();
  let style = $state("");

  $effect(() => {
    if (!open || !trigger) return;
    const at = anchor ?? trigger;
    const place = () => {
      if (point) {
        const w = panel?.offsetWidth ?? width ?? 240;
        const h = panel?.offsetHeight ?? 0;
        const left = Math.max(8, Math.min(point.x, innerWidth - w - 8));
        const top = point.y + h + 8 > innerHeight ? Math.max(8, point.y - h) : point.y;
        style = `left: ${left}px; top: ${top}px; max-height: ${innerHeight - top - 8}px;${width ? ` width: ${width}px;` : ""}`;
        return;
      }
      const r = at.getBoundingClientRect();
      const side = align === "start" ? `left: ${r.left}px` : `right: ${innerWidth - r.right}px`;
      const edge = above
        ? `bottom: ${innerHeight - r.top + 4}px; max-height: ${r.top - 12}px;`
        : `top: ${r.bottom + 4}px; max-height: ${innerHeight - r.bottom - 12}px;`;
      style = `${side}; ${edge}${width ? ` width: ${width}px;` : ""}`;
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
