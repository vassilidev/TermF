<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";
  import { app, openSession } from "$lib/state.svelte";

  type Layout = { name: string; sessions: { path: string; agent: string | null }[] };

  let layouts = $state<Layout[]>([]);
  let name = $state("");

  async function call(command: string, args: Record<string, unknown> = {}): Promise<boolean> {
    try {
      layouts = await invoke<Layout[]>(command, args);
      return true;
    } catch (e) {
      app.error = String(e);
      return false;
    }
  }

  onMount(() => {
    call("layouts");
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    const sessions = app.sessions.map((s) => ({ path: s.cwd, agent: s.agent }));
    if (await call("layout_save", { layout: { name, sessions } })) name = "";
  }

  async function open(layout: Layout) {
    for (const s of layout.sessions) await openSession(s.path, s.agent);
  }
</script>

<section>
  <h2>{t("layouts")}</h2>
  <ul>
    {#each layouts as layout (layout.name)}
      <li class="item">
        <button
          class="row"
          onclick={() => open(layout)}
          title={t("openLayout", { name: layout.name })}
        >
          <span class="name">{layout.name}</span>
          <span class="count">{layout.sessions.length}</span>
        </button>
        <button
          class="icon"
          onclick={() => call("layout_delete", { name: layout.name })}
          aria-label={t("deleteLayout", { name: layout.name })}
          title={t("deleteLayout", { name: layout.name })}
        >
          ×
        </button>
      </li>
    {:else}
      <li class="muted empty">{t("noLayouts")}</li>
    {/each}
  </ul>
  <form class="save" onsubmit={save}>
    <input bind:value={name} placeholder={t("layoutName")} aria-label={t("layoutName")} />
    <button type="submit" disabled={!name.trim() || !app.sessions.length}>{t("save")}</button>
  </form>
</section>

<style>
  h2 {
    margin: 12px 8px 6px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    display: flex;
    align-items: center;
  }

  .row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
  }

  .row:hover,
  .icon:hover {
    background: var(--hover);
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    color: var(--muted);
    font-size: 11px;
  }

  .icon {
    width: 22px;
    height: 22px;
    flex-shrink: 0;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--muted);
  }

  .empty {
    padding: 4px 8px;
  }

  .save {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }

  .save input {
    flex: 1;
    min-width: 0;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .save button {
    padding: 5px 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
  }
</style>
