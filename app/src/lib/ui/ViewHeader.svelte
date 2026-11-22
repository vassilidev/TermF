<script lang="ts">
  import { getContext, type Snippet } from "svelte";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";
  import { closePanel } from "$lib/panes.svelte";
  import { panes, ui, type ViewId } from "$lib/state.svelte";

  /** Set by the panel the view is in. */
  const panel = getContext<(() => ViewId) | undefined>("panel");
  const closable = $derived(!!panel && panes.other !== null);

  let {
    title,
    context = "",
    mono = false,
    children,
    actions,
  }: {
    title: string;
    context?: string;
    mono?: boolean;
    children?: Snippet;
    actions?: Snippet;
  } = $props();
</script>

<header class="view-header" class:aside={closable && panel?.() !== ui.view}>
  <h1>{title}</h1>
  {#if context}<span class="context" class:mono>{context}</span>{/if}
  {@render children?.()}
  {#if actions || closable}
    <div class="actions">
      {@render actions?.()}
      {#if closable && panel}
        <button
          class="icon-btn close"
          aria-label={t("t6bClosePanel")}
          title={t("t6bClosePanel")}
          onclick={() => closePanel(panel())}
        >
          <X size={15} aria-hidden="true" />
        </button>
      {/if}
    </div>
  {/if}
</header>

<style>
  /* One 48px row; a narrow window wraps the actions instead of hiding them. */
  .view-header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    flex-shrink: 0;
    min-height: 48px;
    padding: 8px 12px 8px 16px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  h1 {
    flex-shrink: 0;
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .context {
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex: 1 1 auto;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    min-width: 0;
  }

  /* The panel the keyboard is not in. */
  .aside h1 {
    color: var(--muted);
  }

  .close {
    margin-left: 2px;
  }
</style>
