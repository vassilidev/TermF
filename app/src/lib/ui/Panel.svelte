<script lang="ts">
  import { setContext, type Snippet } from "svelte";
  import AskBar from "$lib/AskBar.svelte";
  import { panes, ui, type ViewId } from "$lib/state.svelte";

  let { view, children }: { view: ViewId; children: Snippet } = $props();

  // A conversation has its own field, and a setup asks for all the attention: no "Ask" bar there.
  const bar = $derived(!["conversations", "newProject", "onboarding", "components"].includes(view));

  // Its view header offers to close the panel.
  setContext("panel", () => view);

  const two = $derived(panes.other !== null);
  const place = $derived(ui.view === view ? panes.active : panes.other === view ? 1 - panes.active : null);

  /** A click or the focus in the other panel moves the keyboard, and the view-wide shortcuts, there. */
  function enter() {
    if (panes.other === view) ui.view = view;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions (pointer and focus only follow where the user went) -->
<div
  class="pane"
  class:active={two && ui.view === view}
  class:right={two && place === 1}
  hidden={place === null}
  data-panel={view}
  tabindex="-1"
  onfocusin={enter}
  onpointerdown={enter}
>
  {@render children()}
  {#if bar}<AskBar {view} />{/if}
</div>

<style>
  .pane {
    position: relative;
    display: grid;
    grid-template: minmax(0, 1fr) auto / minmax(0, 1fr);
    grid-area: 1 / 1;
    min-width: 0;
    min-height: 0;
  }

  .pane.right {
    grid-column: 3;
  }

  .pane[hidden] {
    display: none;
  }

  .pane:focus {
    outline: none;
  }

  /* Where the keyboard is, when there are two panels: the same line as the current tab's. */
  .pane.active::before {
    content: "";
    position: absolute;
    z-index: 5;
    inset: 0 0 auto;
    height: 2px;
    background: var(--accent);
    pointer-events: none;
  }
</style>
