<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import SessionGrid from "$lib/SessionGrid.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import { lang, t } from "$lib/i18n";
  import { init } from "$lib/state.svelte";
  import { files, showFiles } from "$lib/files.svelte";

  onMount(() => {
    document.documentElement.lang = lang;
    return init();
  });
</script>

<div class="app">
  <Sidebar />
  <div class="main">
    <nav class="views" aria-label={t("views")}>
      <button aria-pressed={files.view === "terminals"} onclick={() => (files.view = "terminals")}>
        {t("viewTerminals")}
      </button>
      <button aria-pressed={files.view === "files"} onclick={showFiles}>{t("viewFiles")}</button>
    </nav>
    <!-- Hidden, not unmounted: terminals keep running and keep their scrollback. -->
    <div class="pane" hidden={files.view !== "terminals"}><SessionGrid /></div>
    {#if files.loaded}
      {#await import("$lib/FilesView.svelte") then { default: FilesView }}
        <div class="pane" hidden={files.view !== "files"}><FilesView /></div>
      {/await}
    {/if}
  </div>
</div>

<style>
  .app {
    display: grid;
    grid-template-columns: 280px 1fr;
    height: 100vh;
  }

  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .views {
    display: flex;
    gap: 2px;
    padding: 6px 6px 0;
  }

  .views button {
    padding: 3px 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted);
  }

  .views button[aria-pressed="true"] {
    background: var(--hover);
    color: var(--text);
  }

  .pane {
    display: grid;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    flex: 1;
    min-height: 0;
  }

  .pane[hidden] {
    display: none;
  }
</style>
