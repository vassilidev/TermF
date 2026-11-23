<script lang="ts" module>
  import type { MenuItem } from "$lib/ui/List.svelte";

  /** `divider`: a line above it, to group the actions. */
  export type ContextItem = MenuItem & { divider?: boolean; disabled?: boolean };

  /** Where a right click, or the context-menu key and Shift+F10 on the focused object, opens the menu. */
  export function menuPoint(e: MouseEvent | KeyboardEvent): { x: number; y: number } {
    if (e instanceof MouseEvent && (e.clientX || e.clientY)) return { x: e.clientX, y: e.clientY };
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return { x: r.left + 8, y: r.bottom };
  }

  /** The keys that ask for a context menu without a mouse. */
  export const menuKey = (e: KeyboardEvent) => e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey);
</script>

<script lang="ts">
  import Popover from "$lib/ui/Popover.svelte";

  /**
   * An object's actions at the pointer, as Finder and VS Code show them: ↑ ↓ to move, Enter to run, Escape to close
   * with the focus back where it was (`trigger`).
   */
  let {
    open = $bindable(),
    point,
    trigger,
    label,
    items,
  }: {
    open: boolean;
    point: { x: number; y: number } | null;
    trigger: HTMLElement | undefined;
    label: string;
    items: ContextItem[];
  } = $props();

  function act(run: () => void) {
    open = false;
    trigger?.focus({ preventScroll: true });
    run();
  }

  function onkeydown(e: KeyboardEvent) {
    const buttons = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const to = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: buttons.length - 1 }[e.key];
    if (to === undefined || !buttons.length) return;
    e.preventDefault();
    buttons[(to + buttons.length) % buttons.length].focus();
  }
</script>

<Popover bind:open {trigger} {point} {label} width={264}>
  <!-- svelte-ignore a11y_interactive_supports_focus (its items take the focus) -->
  <div class="menu" role="menu" aria-label={label} tabindex="-1" {onkeydown}>
    {#each items as item, i (item.label)}
      {#if item.divider && i > 0}<div class="divider" role="separator"></div>{/if}
      <button class="action" class:danger={item.danger} role="menuitem" disabled={item.disabled} onclick={() => act(item.run)}>
        {#if item.icon}<item.icon size={14} aria-hidden="true" />{:else}<span class="no-icon" aria-hidden="true"></span>{/if}
        <span class="text">{item.label}</span>
        {#if item.kbd}<kbd>{item.kbd}</kbd>{/if}
      </button>
    {/each}
  </div>
</Popover>

<style>
  .menu {
    display: flex;
    flex-direction: column;
    padding: 4px;
    outline: none;
  }

  .action {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .action:hover:not(:disabled),
  .action:focus-visible {
    background: var(--hover);
    outline: none;
  }

  .action :global(svg),
  .no-icon {
    flex-shrink: 0;
    width: 14px;
    color: var(--muted);
  }

  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action.danger {
    color: var(--danger);
  }

  .action.danger :global(svg) {
    color: currentColor;
  }

  .divider {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
