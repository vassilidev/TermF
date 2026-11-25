<script lang="ts">
  import type { Snippet } from "svelte";
  import PanelRightClose from "@lucide/svelte/icons/panel-right-close";
  import { t } from "$lib/i18n";

  /**
   * A panel docked on the right that pushes the content, the parent being `position: relative`. Where pushing would
   * leave the content less than `ROOM` (a narrow window, two panels side by side), it goes over the content instead,
   * and Escape, its button or a click beside it folds it away. Its edge drags (or the arrows on it, double-click for
   * the default) between `min` and `max`; the width is kept per `id` across launches. `open` false folds it away.
   */
  let {
    id,
    title,
    open = $bindable(true),
    width = 400,
    min = 360,
    max = 560,
    actions,
    footer,
    flush = false,
    children,
  }: {
    id: string;
    title: string;
    open?: boolean;
    width?: number;
    min?: number;
    max?: number;
    actions?: Snippet;
    footer?: Snippet;
    /** The body without its padding, for a content that scrolls on its own (a conversation). */
    flush?: boolean;
    children: Snippet;
  } = $props();

  const storeKey = () => `termf.side.${id}`;
  const clamp = (w: number) => Math.round(Math.min(max, Math.max(min, w)));

  function stored(): number | null {
    try {
      const value = Number(localStorage.getItem(storeKey()));
      return value > 0 ? value : null;
    } catch {
      return null;
    }
  }

  // svelte-ignore state_referenced_locally (the stored width wins over the default once)
  let size = $state(clamp(stored() ?? width));

  const ROOM = 440;
  let aside = $state<HTMLElement>();
  let room = $state(0);
  const over = $derived(room > 0 && room - size < ROOM);
  // Over the content, a strip of it stays beside the panel to click.
  const shown = $derived(over ? Math.min(size, room - 48) : size);

  // The positioned parent's width: a parent shown as `contents` (the ⌘I panel's) has none.
  $effect(() => {
    const parent = aside?.offsetParent;
    if (!parent) return;
    const observer = new ResizeObserver(() => (room = parent.clientWidth));
    observer.observe(parent);
    return () => observer.disconnect();
  });

  function store() {
    try {
      localStorage.setItem(storeKey(), String(size));
    } catch {
      // Storage refused (private mode): the width holds until the next launch.
    }
  }

  function resize(to: number) {
    size = clamp(to);
    store();
  }

  /** The width follows the pointer, and is stored once the drag ends, however it ends. */
  function drag(e: PointerEvent) {
    const grip = e.currentTarget as HTMLElement;
    const right = grip.parentElement!.getBoundingClientRect().right;
    try {
      grip.setPointerCapture(e.pointerId);
    } catch {
      // A pointer the system no longer tracks: the drag still follows it over the grip.
    }
    const move = (m: PointerEvent) => (size = clamp(right - m.clientX));
    const end = () => {
      grip.removeEventListener("pointermove", move);
      for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) grip.removeEventListener(type, end);
      store();
    };
    grip.addEventListener("pointermove", move);
    for (const type of ["pointerup", "pointercancel", "lostpointercapture"]) grip.addEventListener(type, end);
  }

  function close(e: KeyboardEvent) {
    if (over && e.key === "Escape" && !e.defaultPrevented) open = false;
  }

  function onkeydown(e: KeyboardEvent) {
    const to = { ArrowLeft: size + 16, ArrowRight: size - 16, Home: max, End: min }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    resize(to);
  }
</script>

{#if open}
  {#if over}
    <!-- The pointer's way out; the keyboard's is Escape or the fold button. -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="scrim" style:right="{shown}px" onclick={() => (open = false)}></div>
  {/if}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions (Escape in the panel folds it while it covers the content) -->
  <aside class="side" class:over style:width="{shown}px" aria-label={title} bind:this={aside} onkeydown={close}>
    <!-- A focusable separator is a widget (WAI-ARIA window splitter): the arrows move it. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="grip"
      role="separator"
      aria-orientation="vertical"
      aria-label={t("uiF1Resize")}
      aria-valuenow={size}
      aria-valuemin={min}
      aria-valuemax={max}
      tabindex="0"
      title={t("uiF1Resize")}
      onpointerdown={drag}
      ondblclick={() => resize(width)}
      {onkeydown}
    ></div>
    <header>
      <h2>{title}</h2>
      {@render actions?.()}
      <button class="icon-btn" onclick={() => (open = false)} aria-label={t("uiF1Fold")} title={t("uiF1Fold")}>
        <PanelRightClose size={16} aria-hidden="true" />
      </button>
    </header>
    <div class="body" class:flush>{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </aside>
{/if}

<style>
  .side {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-width: 0;
    min-height: 0;
    border-left: 1px solid var(--border);
    background: var(--panel);
    animation: enter var(--dur) var(--ease-drawer);
  }

  @keyframes enter {
    from {
      opacity: 0;
      transform: translateX(16px);
    }
  }

  .side.over {
    position: absolute;
    z-index: 20;
    inset: 0 0 0 auto;
    box-shadow: var(--shadow-3);
  }

  .scrim {
    position: absolute;
    z-index: 19;
    inset: 0;
    background: var(--scrim);
    animation: fade var(--dur) ease-out;
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }

  .grip {
    position: absolute;
    z-index: 1;
    inset: 0 auto 0 -3px;
    width: 6px;
    cursor: col-resize;
    touch-action: none;
  }

  .grip:hover,
  .grip:focus-visible {
    outline: none;
    background: linear-gradient(var(--accent), var(--accent)) center / 2px 100% no-repeat;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    height: 52px;
    padding: 0 12px 0 16px;
    border-bottom: 1px solid var(--border);
  }

  h2 {
    flex: 1;
    margin: 0;
    overflow: hidden;
    font-size: var(--text-md);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    padding: 16px;
    overflow: auto;
  }

  .body.flush {
    gap: 0;
    padding: 0;
    overflow: hidden;
  }

  footer {
    flex-shrink: 0;
    padding: 12px;
    border-top: 1px solid var(--border);
  }
</style>
