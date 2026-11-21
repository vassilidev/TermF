<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import AppNav from "$lib/AppNav.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import { onShortcut } from "$lib/commands";
  import { lang, t } from "$lib/i18n";
  import { app, init, ui } from "$lib/state.svelte";
  import { watchCi } from "$lib/ci.svelte";
  import { watchSchedule } from "$lib/schedule.svelte";
  import { watchTickets } from "$lib/tickets.svelte";
  import { views } from "$lib/views";

  onMount(() => {
    document.documentElement.lang = lang;
    const stop = init();
    const stopCi = watchCi();
    const stopSchedule = watchSchedule();
    const stopTickets = watchTickets();
    return () => (stop(), stopCi(), stopSchedule(), stopTickets());
  });
</script>

<svelte:window onkeydowncapture={onShortcut} />

<div class="app">
  <AppNav />
  <div class="main">
    {#if app.error || app.workspace?.apiError || app.workspace?.quickTerminalError || app.workspace?.scanTruncated}
      <div class="banners">
        {#if app.error}
          <Banner kind="error" ondismiss={() => (app.error = "")}>{app.error}</Banner>
        {/if}
        {#if app.workspace?.apiError}
          <Banner><span title={app.workspace.apiError}>{t("apiOff")}</span></Banner>
        {/if}
        {#if app.workspace?.quickTerminalError}
          <Banner>{t("shortcutError", { error: app.workspace.quickTerminalError })}</Banner>
        {/if}
        {#if app.workspace?.scanTruncated}
          <Banner>{t("truncated")}</Banner>
        {/if}
      </div>
    {/if}
    {#each views as view (view.id)}
      {#if view.loaded()}
        {#await view.load() then { default: View }}
          <!-- Hidden, not unmounted: terminals keep running and keep their scrollback. -->
          <div class="pane" hidden={ui.view !== view.id}><View /></div>
        {/await}
      {/if}
    {/each}
  </div>
</div>

{#if ui.palette}
  <CommandPalette />
{/if}

<style>
  .app {
    display: grid;
    grid-template: minmax(0, 1fr) / var(--nav-width) minmax(0, 1fr);
    height: 100vh;
  }

  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
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
