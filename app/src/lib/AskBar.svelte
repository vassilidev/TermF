<script lang="ts">
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Avatar from "$lib/ui/Avatar.svelte";
  import { ask, askAI, currentRole } from "$lib/ask.svelte";
  import { roleFor } from "$lib/ask-logic";
  import { t } from "$lib/i18n";
  import { aria, keys } from "$lib/keymap.svelte";
  import { ui, type ViewId } from "$lib/state.svelte";

  /** The one-line "Ask" bar at the bottom of a view: it opens the ⌘I panel, with the screen's role and context. */
  let { view }: { view: ViewId } = $props();

  const role = $derived(ui.view === view ? currentRole() : roleFor(view, ask.given[view]?.role));

  function open() {
    // In the other panel of a split, the click makes this view the current one first.
    ui.view = view;
    void askAI();
  }
</script>

{#if !ask.open}
  <div class="askbar">
    <button class="ask" onclick={open} aria-keyshortcuts={aria("ask_ai")}>
      <Sparkles size={15} aria-hidden="true" class="spark" />
      <span class="ph">{role === "cto" ? t("aigBarCto") : t("aigBarRole", { role })}</span>
      <Avatar {role} size="sm" />
      {#if keys.ask}<kbd>{keys.ask}</kbd>{/if}
    </button>
  </div>
{/if}

<style>
  .askbar {
    flex-shrink: 0;
    padding: 8px 16px 10px;
    border-top: 1px solid var(--border);
    background: var(--bg);
  }

  .ask {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    max-width: 820px;
    height: var(--h-lg);
    margin: 0 auto;
    padding: 0 8px 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
    color: var(--muted);
    text-align: left;
    transition:
      border-color var(--dur-fast) var(--ease-out),
      box-shadow var(--dur-fast) var(--ease-out);
  }

  .ask:hover {
    border-color: var(--accent);
  }

  .ask:focus-visible {
    border-color: var(--focus);
    outline: none;
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .ask :global(.spark) {
    flex-shrink: 0;
    color: var(--accent-ink);
  }

  .ph {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  @media (prefers-reduced-motion: reduce) {
    .ask {
      transition: none;
    }
  }
</style>
