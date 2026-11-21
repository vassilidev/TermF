<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import X from "@lucide/svelte/icons/x";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import SessionGrid from "$lib/SessionGrid.svelte";
  import Sidebar from "$lib/Sidebar.svelte";
  import TopBar from "$lib/TopBar.svelte";
  import { onShortcut } from "$lib/commands";
  import { lang, t } from "$lib/i18n";
  import { app, init, ui } from "$lib/state.svelte";
  import { files } from "$lib/files.svelte";
  import { library } from "$lib/library.svelte";
  import { ssh } from "$lib/ssh.svelte";
  import { tickets } from "$lib/tickets.svelte";

  onMount(() => {
    document.documentElement.lang = lang;
    return init();
  });
</script>

<svelte:window onkeydowncapture={onShortcut} />

<div class="app">
  <TopBar />
  <Sidebar />
  <div class="main">
    {#if app.error}
      <div class="notice error banner" role="alert">
        <CircleAlert size={14} aria-hidden="true" />
        <span>{app.error}</span>
        <button class="icon-btn sm" onclick={() => (app.error = "")} aria-label={t("dismiss")} title={t("dismiss")}>
          <X size={14} aria-hidden="true" />
        </button>
      </div>
    {/if}
    <!-- Hidden, not unmounted: terminals keep running and keep their scrollback. -->
    <div class="pane" hidden={files.view !== "terminals"}><SessionGrid /></div>
    {#if files.loaded}
      {#await import("$lib/FilesView.svelte") then { default: FilesView }}
        <div class="pane" hidden={files.view !== "files"}><FilesView /></div>
      {/await}
    {/if}
    {#if library.loaded}
      {#await import("$lib/LibraryView.svelte") then { default: LibraryView }}
        <div class="pane" hidden={files.view !== "library"}><LibraryView /></div>
      {/await}
    {/if}
    {#if ssh.loaded}
      {#await import("$lib/SshView.svelte") then { default: SshView }}
        <div class="pane" hidden={files.view !== "ssh"}><SshView /></div>
      {/await}
    {/if}
    {#if tickets.loaded}
      {#await import("$lib/TicketsView.svelte") then { default: TicketsView }}
        <div class="pane" hidden={files.view !== "tickets"}><TicketsView /></div>
      {/await}
    {/if}
  </div>
</div>

{#if ui.palette}
  <CommandPalette />
{/if}

<style>
  .app {
    display: grid;
    grid-template:
      "top top" auto
      "side main" minmax(0, 1fr)
      / 264px minmax(0, 1fr);
    height: 100vh;
  }

  .main {
    grid-area: main;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .banner {
    align-items: center;
    margin: 8px 8px 0;
  }

  .banner span {
    flex: 1;
  }

  .banner .icon-btn {
    color: inherit;
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
