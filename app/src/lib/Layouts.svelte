<script lang="ts">
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import Section from "$lib/Section.svelte";
  import { t } from "$lib/i18n";
  import { app, callLayouts, openLayout } from "$lib/state.svelte";

  let name = $state("");
  let saving = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const sessions = app.sessions.map((s) => ({ path: s.cwd, agent: s.agent }));
    if (await callLayouts("layout_save", { layout: { name, sessions } })) {
      name = "";
      saving = false;
    }
  }
</script>

<Section id="layouts" title={t("layouts")} count={app.layouts.length}>
  {#snippet actions()}
    <button
      class="icon-btn sm"
      disabled={!app.sessions.length}
      aria-expanded={saving}
      onclick={() => (saving = !saving)}
      aria-label={t("saveLayout")}
      title={t("saveLayout")}
    >
      <Plus size={14} aria-hidden="true" />
    </button>
  {/snippet}
  {#if saving}
    <form class="save" onsubmit={save}>
      <input
        class="field"
        onkeydown={(e) => e.key === "Escape" && (saving = false)}
        bind:value={name}
        placeholder={t("layoutName")}
        aria-label={t("layoutName")}
        {@attach (el) => el.focus()}
      />
      <button type="submit" class="btn sm primary" disabled={!name.trim() || !app.sessions.length}>
        {t("save")}
      </button>
    </form>
  {/if}
  <ul>
    {#each app.layouts as layout (layout.name)}
      <li class="item">
        <button class="row" onclick={() => openLayout(layout)} title={t("openLayout", { name: layout.name })}>
          <LayoutGrid size={14} aria-hidden="true" />
          <span class="name">{layout.name}</span>
          <span class="meta">{layout.sessions.length}</span>
        </button>
        <button
          class="icon-btn sm reveal"
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
</Section>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    display: flex;
    align-items: center;
    border-radius: var(--radius);
  }

  .item:hover {
    background: var(--hover);
  }

  .item .icon-btn {
    margin-right: 4px;
  }

  .reveal {
    opacity: 0;
  }

  .item:hover .reveal,
  .reveal:focus-visible {
    opacity: 1;
  }

  .row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 10px;
    min-width: 0;
    height: 30px;
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
    padding: 4px 8px;
    color: var(--muted);
  }

  .save {
    display: flex;
    gap: 6px;
    margin-bottom: 6px;
  }
</style>
