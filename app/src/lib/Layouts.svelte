<script lang="ts">
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import X from "@lucide/svelte/icons/x";
  import Popover from "$lib/ui/Popover.svelte";
  import { t } from "$lib/i18n";
  import { app, callLayouts, openLayout } from "$lib/state.svelte";

  let trigger = $state<HTMLButtonElement>();
  let open = $state(false);
  let name = $state("");

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const sessions = app.sessions.map((s) => ({ path: s.cwd, agent: s.agent }));
    if (await callLayouts("layout_save", { layout: { name, sessions } })) name = "";
  }
</script>

<button bind:this={trigger} class="btn sm quiet" aria-haspopup="dialog" aria-expanded={open} onclick={() => (open = !open)}>
  <LayoutGrid size={14} aria-hidden="true" />{t("layouts")}<ChevronDown size={14} aria-hidden="true" />
</button>

<Popover bind:open {trigger} label={t("layouts")} align="end" width={300}>
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
  {#if app.sessions.length}
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
