<script lang="ts">
  import { untrack, type Snippet } from "svelte";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";

  let {
    id,
    title,
    count,
    grow = false,
    expand = false,
    actions,
    children,
  }: {
    id: string;
    title: string;
    count?: number;
    /** Takes the space left in the sidebar and scrolls inside it. */
    grow?: boolean;
    /** Opens the section when it becomes true, as if clicked. */
    expand?: boolean;
    actions?: Snippet;
    children: Snippet;
  } = $props();

  const key = "termf.collapsed";

  function collapsed(): string[] {
    try {
      return JSON.parse(localStorage.getItem(key) ?? "[]");
    } catch {
      return [];
    }
  }

  // svelte-ignore state_referenced_locally
  let open = $state(!collapsed().includes(id));

  function toggle() {
    open = !open;
    const ids = new Set(collapsed());
    if (open) ids.delete(id);
    else ids.add(id);
    try {
      localStorage.setItem(key, JSON.stringify([...ids]));
    } catch {
      // Private storage off: the section just won't remember.
    }
  }

  // Not tracking `open`: the user can still fold it afterwards.
  $effect(() => {
    if (expand && !untrack(() => open)) toggle();
  });
</script>

<section class="section" class:grow={grow && open}>
  <div class="head">
    <h2>
      <button aria-expanded={open} aria-controls="section-{id}" onclick={toggle}>
        <ChevronRight size={14} aria-hidden="true" />
        {title}
        {#if count !== undefined}<span class="count">{count}</span>{/if}
      </button>
    </h2>
    {#if open}{@render actions?.()}{/if}
  </div>
  <div id="section-{id}" class="body" hidden={!open}>
    {@render children()}
  </div>
</section>

<style>
  .section {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    padding: 0 8px;
  }

  .section.grow {
    flex: 1 1 auto;
    min-height: 180px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 36px;
    padding-top: 4px;
  }

  h2 {
    flex: 1;
    min-width: 0;
    margin: 0;
  }

  h2 button {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 6px 0 2px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  h2 button:hover {
    color: var(--text);
  }

  h2 button :global(svg) {
    transition: transform var(--dur-fast) var(--ease-out);
  }

  h2 button[aria-expanded="true"] :global(svg) {
    transform: rotate(90deg);
  }

  .count {
    font-weight: 500;
    letter-spacing: 0;
    font-variant-numeric: tabular-nums;
  }

  .body {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .grow .body {
    flex: 1;
  }

  .body[hidden] {
    display: none;
  }
</style>
