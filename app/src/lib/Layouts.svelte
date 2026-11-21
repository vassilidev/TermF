<script lang="ts">
  import AppWindow from "@lucide/svelte/icons/app-window";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Equal from "@lucide/svelte/icons/equal";
  import Grid2x2 from "@lucide/svelte/icons/grid-2x2";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import LayoutPanelLeft from "@lucide/svelte/icons/layout-panel-left";
  import X from "@lucide/svelte/icons/x";
  import Popover from "$lib/ui/Popover.svelte";
  import { t } from "$lib/i18n";
  import { display } from "$lib/keymap.svelte";
  import { shown } from "$lib/project.svelte";
  import { leaves } from "$lib/split-logic";
  import { arrange, currentTab, equalize, layoutOf, openLayout, type Arrangement } from "$lib/tabs.svelte";
  import { app, callLayouts } from "$lib/state.svelte";

  let trigger = $state<HTMLButtonElement>();
  let open = $state(false);
  let name = $state("");

  const tab = $derived(currentTab());
  const arrangements = [
    ["tabs", "t6aOnePerTab", AppWindow],
    ["row", "t6aSideBySide", Columns2],
    ["two", "t6aTwoByTwo", Grid2x2],
    ["grid", "t6aAutoGrid", LayoutGrid],
    ["focus", "t6aFocusLayout", LayoutPanelLeft],
  ] as const;

  /** The layout on screen: the tabs of the current project and their splits. */
  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (await callLayouts("layout_save", { layout: layoutOf(name) })) name = "";
  }

  function lay(how: Arrangement) {
    open = false;
    arrange(how);
  }
</script>

<button bind:this={trigger} class="btn sm quiet" aria-haspopup="dialog" aria-expanded={open} onclick={() => (open = !open)}>
  <LayoutGrid size={14} aria-hidden="true" />{t("layouts")}<ChevronDown size={14} aria-hidden="true" />
</button>

<Popover bind:open {trigger} label={t("layouts")} align="end" width={300}>
  {#if tab && leaves(tab.root).length > 1}
    <h3>{t("t6aArrange")}</h3>
    <ul class="arrange">
      {#each arrangements as [how, label, Icon] (how)}
        <li class="item">
          <button class="row" onclick={() => lay(how)}>
            <Icon size={14} aria-hidden="true" />
            <span class="name">{t(label)}</span>
          </button>
        </li>
      {/each}
      <li class="item">
        <button class="row" onclick={() => ((open = false), equalize())}>
          <Equal size={14} aria-hidden="true" />
          <span class="name">{t("t6aEqualize")}</span>
          {#if display("equalize_splits")}<kbd>{display("equalize_splits")}</kbd>{/if}
        </button>
      </li>
    </ul>
    <h3>{t("t6aSaved")}</h3>
  {/if}
  <ul>
    {#each app.layouts as layout (layout.name)}
      <li class="item">
        <button
          class="row"
          onclick={() => ((open = false), openLayout(layout))}
          title={t("openLayout", { name: layout.name })}
        >
          <LayoutGrid size={14} aria-hidden="true" />
          <span class="name">{layout.name}</span>
          <span class="meta">{layout.sessions.length}</span>
        </button>
        <button
          class="icon-btn sm"
          onclick={() => callLayouts("layout_delete", { name: layout.name })}
          aria-label={t("deleteLayout", { name: layout.name })}
          title={t("deleteLayout", { name: layout.name })}
        >
          <X size={14} aria-hidden="true" />
        </button>
      </li>
    {:else}
      <li class="none">{t("noLayouts")}</li>
    {/each}
  </ul>
  {#if app.sessions.some(shown)}
    <form class="save" onsubmit={save}>
      <input class="field" bind:value={name} placeholder={t("layoutName")} aria-label={t("layoutName")} />
      <button type="submit" class="btn sm" disabled={!name.trim()} title={t("saveLayout")}>{t("save")}</button>
    </form>
  {/if}
</Popover>

<style>
  ul {
    margin: 0;
    padding: 6px;
    list-style: none;
  }

  .arrange {
    border-bottom: 1px solid var(--border);
  }

  h3 {
    margin: 0;
    padding: 10px 14px 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: var(--radius);
  }

  .item:hover {
    background: var(--hover);
  }

  .row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 10px;
    min-width: 0;
    height: 34px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .row :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .item .icon-btn {
    margin-right: 4px;
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .meta {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .none {
    padding: 6px 8px;
    color: var(--muted);
  }

  .save {
    display: flex;
    gap: 6px;
    padding: 8px;
    border-top: 1px solid var(--border);
  }
</style>
