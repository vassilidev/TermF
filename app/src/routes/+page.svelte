<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import ApiConfirm from "$lib/ApiConfirm.svelte";
  import AppNav from "$lib/AppNav.svelte";
  import AskPanel from "$lib/AskPanel.svelte";
  import UpdateBanner from "$lib/UpdateBanner.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import Panel from "$lib/ui/Panel.svelte";
  import Toaster from "$lib/ui/Toaster.svelte";
  import { onShortcut } from "$lib/commands";
  import { lang, t } from "$lib/i18n";
  import { MAX, MIN } from "$lib/panes-logic";
  import { dragPanes, resizePanes, watchPanes } from "$lib/panes.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { app, init, onScreen, panes, ui } from "$lib/state.svelte";
  import { tour, watchTour } from "$lib/tour.svelte";
  import { watchAgents } from "$lib/agents.svelte";
  import { watchUsage } from "$lib/usage.svelte";
  import { watchLive } from "$lib/live.svelte";
  import { watchCi } from "$lib/ci.svelte";
  import { watchOps } from "$lib/ops.svelte";
  import { watchAgentEnv } from "$lib/agentenv.svelte";
  import { watchSchedule } from "$lib/schedule.svelte";
  import { sync, watchSync } from "$lib/sync.svelte";
  import { watchTeam } from "$lib/team.svelte";
  import { tickets, watchTickets } from "$lib/tickets.svelte";
  import { setup, watchSetup } from "$lib/onboarding.svelte";
  import { creating } from "$lib/newproject.svelte";
  import { views } from "$lib/views";
  import { visual, watchVisual } from "$lib/visual.svelte";
  import { watchTray } from "$lib/remote.svelte";
  import { watchUpdates } from "$lib/release.svelte";
  import { watchMenu } from "$lib/menu";

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
    const stopAgentEnv = watchAgentEnv();
    const stopVisual = watchVisual();
    const stopSync = watchSync();
    const stopTray = watchTray();
    const stopUpdates = watchUpdates();
    const stopMenu = watchMenu();
    const stopUsage = watchUsage();
    const stopLive = watchLive();
    return () => (stop(), stopCi(), stopSchedule(), stopTickets(), stopSetup(), stopAgents(), stopPanes(), stopTour(), stopTeam(), stopOps(), stopAgentEnv(), stopVisual(), stopSync(), stopTray(), stopUpdates(), stopMenu(), stopUsage(), stopLive());
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
    <div class="stage">
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
          {#await import("$lib/ProjectView.svelte") then { default: ProjectView }}
            <Panel view="project"><ProjectView /></Panel>
          {:catch error}
            <Panel view="project"><Banner kind="error">{String(error)}</Banner></Panel>
          {/await}
        {/key}
      {/if}
      <!-- Kept once left, like the views: an answer being written or a delivery preview open is still there on return. -->
      {#if tickets.selected !== null}
        {#key tickets.selected}
          {#await import("$lib/RunView.svelte") then { default: RunView }}
            <Panel view="run"><RunView id={tickets.selected} /></Panel>
          {:catch error}
            <Panel view="run"><Banner kind="error">{String(error)}</Banner></Panel>
          {/await}
        {/key}
      {/if}
      {#if visual.selected !== null}
        {#key visual.selected}
          {#await import("$lib/ReviewView.svelte") then { default: ReviewView }}
            <Panel view="review"><ReviewView id={visual.selected} /></Panel>
          {:catch error}
            <Panel view="review"><Banner kind="error">{String(error)}</Banner></Panel>
          {/await}
        {/key}
      {/if}
      {#if onScreen("onboarding") && setup.project !== null}
        {#key setup.project}
          {#await import("$lib/OnboardingView.svelte") then { default: OnboardingView }}
            <Panel view="onboarding"><OnboardingView /></Panel>
          {:catch error}
            <Panel view="onboarding"><Banner kind="error">{String(error)}</Banner></Panel>
          {/await}
        {/key}
      {/if}
      <!-- Kept while open: leaving the setup for another view and coming back loses nothing. -->
      {#if creating.open}
        {#key creating.round}
          {#await import("$lib/NewProjectView.svelte") then { default: NewProjectView }}
            <Panel view="newProject"><NewProjectView /></Panel>
          {:catch error}
            <Panel view="newProject"><Banner kind="error">{String(error)}</Banner></Panel>
          {/await}
        {/key}
      {/if}
      {#if onScreen("components")}
        {#await import("$lib/ComponentsView.svelte") then { default: ComponentsView }}
          <Panel view="components"><ComponentsView /></Panel>
        {/await}
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
    <AskPanel />
    </div>
  </div>
</div>

<Toaster />
{#if ui.palette}
  {#await import("$lib/CommandPalette.svelte") then { default: CommandPalette }}
    <CommandPalette />
  {:catch error}
    <div class="load-error"><Banner kind="error" ondismiss={() => (ui.palette = false)}>{String(error)}</Banner></div>
  {/await}
{/if}
{#if ui.help}
  {#await import("$lib/HelpSheet.svelte") then { default: HelpSheet }}
    <HelpSheet />
  {:catch error}
    <div class="load-error"><Banner kind="error" ondismiss={() => (ui.help = false)}>{String(error)}</Banner></div>
  {/await}
{/if}
{#if tour.step !== null}
  {#await import("$lib/GuidedTour.svelte") then { default: GuidedTour }}
    <GuidedTour />
  {:catch error}
    <div class="load-error"><Banner kind="error" ondismiss={() => (tour.step = null)}>{String(error)}</Banner></div>
  {/await}
{/if}

<style>
  .load-error {
    position: fixed;
    inset: auto 16px 16px auto;
    z-index: 20;
  }

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

  /* The views, and the ⌘I panel docked on their right. */
  .stage {
    position: relative;
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .panels {
    display: grid;
    flex: 1;
    min-width: 0;
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
