<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import ApiConfirm from "$lib/ApiConfirm.svelte";
  import AppNav from "$lib/AppNav.svelte";
  import CommandPalette from "$lib/CommandPalette.svelte";
  import OnboardingView from "$lib/OnboardingView.svelte";
  import ProjectView from "$lib/ProjectView.svelte";
  import RunView from "$lib/RunView.svelte";
  import ReviewView from "$lib/ReviewView.svelte";
  import HelpSheet from "$lib/HelpSheet.svelte";
  import NewProjectView from "$lib/NewProjectView.svelte";
  import UpdateBanner from "$lib/UpdateBanner.svelte";
  import GuidedTour from "$lib/GuidedTour.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import Panel from "$lib/ui/Panel.svelte";
  import { onShortcut } from "$lib/commands";
  import { lang, t } from "$lib/i18n";
  import { MAX, MIN } from "$lib/panes-logic";
  import { dragPanes, resizePanes, watchPanes } from "$lib/panes.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { app, init, onScreen, panes, ui } from "$lib/state.svelte";
  import { tour, watchTour } from "$lib/tour.svelte";
  import { watchAgents } from "$lib/agents.svelte";
  import { watchCi } from "$lib/ci.svelte";
  import { watchOps } from "$lib/ops.svelte";
  import { watchSchedule } from "$lib/schedule.svelte";
  import { sync, watchSync } from "$lib/sync.svelte";
  import { watchTeam } from "$lib/team.svelte";
  import { tickets, watchTickets } from "$lib/tickets.svelte";
  import { setup, watchSetup } from "$lib/onboarding.svelte";
  import { creating } from "$lib/newproject.svelte";
  import { views } from "$lib/views";
  import { visual, watchVisual } from "$lib/visual.svelte";
  import { watchUpdates } from "$lib/release.svelte";

  let area = $state<HTMLElement>();

  onMount(() => {
    document.documentElement.lang = lang;
    const stop = init();
    const stopCi = watchCi();
    const stopSchedule = watchSchedule();
    const stopTickets = watchTickets();
    const stopSetup = watchSetup();
    const stopAgents = watchAgents();
    const stopPanes = watchPanes();
    const stopTour = watchTour();
    const stopTeam = watchTeam();
    const stopOps = watchOps();
    const stopVisual = watchVisual();
    const stopSync = watchSync();
    const stopUpdates = watchUpdates();
    return () => (stop(), stopCi(), stopSchedule(), stopTickets(), stopSetup(), stopAgents(), stopPanes(), stopTour(), stopTeam(), stopOps(), stopVisual(), stopSync(), stopUpdates());
  });

  function drag(e: PointerEvent) {
    if (!area || e.button !== 0) return;
    const handle = e.currentTarget as HTMLElement;
    const box = area.getBoundingClientRect();
    handle.setPointerCapture(e.pointerId);
    dragPanes(true);
    const move = (m: PointerEvent) => resizePanes((m.clientX - box.left) / box.width);
    const end = () => {
      dragPanes(false);
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", end);
      handle.removeEventListener("pointercancel", end);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", end);
    handle.addEventListener("pointercancel", end);
  }

  function resizeKey(e: KeyboardEvent) {
    const to = { ArrowLeft: panes.size - 0.05, ArrowRight: panes.size + 0.05, Home: MIN, End: MAX }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    resizePanes(to);
  }
</script>

<svelte:window onkeydowncapture={onShortcut} />

<div class="app">
  <AppNav />
  <div class="main">
    <ApiConfirm />
    <UpdateBanner />
    {#if app.error || app.workspace?.apiError || app.workspace?.quickTerminalError || app.workspace?.scanTruncated || sync.pulled}
      <div class="banners">
        {#if sync.pulled}
          <Banner ondismiss={() => (sync.pulled = null)}>
            {t("sync2PulledTitle")}
            {#if sync.pulled.restart.includes("confirmCli")}{t("sync2PulledRestart")}{/if}
            {#if sync.pulled.sessions}{t("sync2PulledSessions")}{/if}
          </Banner>
        {/if}
        {#if app.error}
          <Banner kind="error" ondismiss={() => (app.error = "")}>{app.error}</Banner>
        {/if}
        {#if app.workspace?.apiError}
          <Banner><span title={app.workspace.apiError}>{t("apiOff")}</span></Banner>
        {/if}
        {#if app.workspace?.quickTerminalError}
          <Banner>
            {t("shortcutError", { error: app.workspace.quickTerminalError })}
            {#snippet action()}
              <button class="btn sm" onclick={() => showSettings("keys")}>{t("shortcuts")}</button>
            {/snippet}
          </Banner>
        {/if}
        {#if app.workspace?.scanTruncated}
          <Banner>{t("truncated")}</Banner>
        {/if}
      </div>
    {/if}
    <div
      class="panels"
      class:two={panes.other !== null}
      style:--left={panes.size}
      bind:this={area}
    >
      {#each views as view (view.id)}
        {#if view.loaded()}
          {#await view.load() then { default: View }}
            <!-- Hidden, not unmounted: terminals keep running and keep their scrollback. -->
            <Panel view={view.id}><View /></Panel>
          {/await}
        {/if}
      {/each}
      {#if onScreen("project") && ui.editing !== null}
        {#key ui.editing}
          <Panel view="project"><ProjectView /></Panel>
        {/key}
      {/if}
      <!-- Kept once left, like the views: an answer being written or a delivery preview open is still there on return. -->
      {#if tickets.selected !== null}
        {#key tickets.selected}
          <Panel view="run"><RunView id={tickets.selected} /></Panel>
        {/key}
      {/if}
      {#if visual.selected !== null}
        {#key visual.selected}
          <Panel view="review"><ReviewView id={visual.selected} /></Panel>
        {/key}
      {/if}
      {#if onScreen("onboarding") && setup.project !== null}
        {#key setup.project}
          <Panel view="onboarding"><OnboardingView /></Panel>
        {/key}
      {/if}
      <!-- Kept while open: leaving the setup for another view and coming back loses nothing. -->
      {#if creating.open}
        {#key creating.round}
          <Panel view="newProject"><NewProjectView /></Panel>
        {/key}
      {/if}
      {#if panes.other !== null}
        <!-- A focusable separator is a widget (WAI-ARIA window splitter): the arrows move it. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <div
          class="handle"
          role="separator"
          aria-orientation="vertical"
          aria-label={t("t6aResizeHandle")}
          aria-valuenow={Math.round(panes.size * 100)}
          aria-valuemin={MIN * 100}
          aria-valuemax={MAX * 100}
          tabindex="0"
          title={t("t6aResizeHandle")}
          onpointerdown={drag}
          ondblclick={() => resizePanes(0.5)}
          onkeydown={resizeKey}
        ></div>
      {/if}
    </div>
  </div>
</div>

{#if ui.palette}
  <CommandPalette />
{/if}
{#if ui.help}
  <HelpSheet />
{/if}
{#if tour.step !== null}
  <GuidedTour />
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

  .panels {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    min-height: 0;
  }

  .panels.two {
    grid-template-columns: minmax(0, calc((100% - 1px) * var(--left))) 1px minmax(0, 1fr);
  }

  /* A 1px line, wider to grab than to see, over the panels' edges. */
  .handle {
    position: relative;
    z-index: 6;
    grid-area: 1 / 2;
    background: var(--border-strong);
    cursor: col-resize;
    touch-action: none;
  }

  .handle::before {
    content: "";
    position: absolute;
    inset: 0 -4px;
  }

  .handle::after {
    content: "";
    position: absolute;
    inset: 0 -1px;
    transition: background-color var(--dur-fast) ease-out;
  }

  .handle:hover::after,
  .handle:active::after,
  .handle:focus-visible::after {
    background: var(--accent);
  }

  .handle:focus-visible {
    outline: none;
  }
</style>
