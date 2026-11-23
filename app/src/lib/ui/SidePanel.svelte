<script lang="ts">
  import type { Snippet } from "svelte";
  import PanelRightClose from "@lucide/svelte/icons/panel-right-close";
  import { t } from "$lib/i18n";

  /**
   * A panel docked on the right that pushes the content (above it under 1100 px, the parent being `position: relative`).
   * Its edge drags (or the arrows on it, double-click for the default) between `min` and `max`; the width is kept per
   * `id` across launches. `open` false folds it away.
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

  function resize(to: number) {
    size = clamp(to);
    try {
      localStorage.setItem(storeKey(), String(size));
    } catch {
      // Storage refused (private mode): the width holds until the next launch.
    }
  }

  function drag(e: PointerEvent) {
    const grip = e.currentTarget as HTMLElement;
    const right = grip.parentElement!.getBoundingClientRect().right;
    grip.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => resize(right - m.clientX);
    const up = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", up);
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", up);
  }

  function onkeydown(e: KeyboardEvent) {
    const to = { ArrowLeft: size + 16, ArrowRight: size - 16, Home: max, End: min }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    resize(to);
  }
</script>

{#if open}
  <aside class="side" style:width="{size}px" aria-label={title}>
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
    <div class="body">{@render children()}</div>
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

  /* Narrow window: over the content rather than squeezing it. */
  @media (max-width: 1100px) {
    .side {
      position: absolute;
      z-index: 20;
      inset: 0 0 0 auto;
      max-width: calc(100% - 48px);
      box-shadow: var(--shadow-3);
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

  footer {
    flex-shrink: 0;
    padding: 12px;
    border-top: 1px solid var(--border);
  }
</style>
