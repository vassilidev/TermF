<script lang="ts">
  import { untrack } from "svelte";
  import LayoutGrid from "@lucide/svelte/icons/layout-grid";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import BroadcastBar from "$lib/BroadcastBar.svelte";
  import Layouts from "$lib/Layouts.svelte";
  import QuickTerminal from "$lib/QuickTerminal.svelte";
  import SessionLauncher from "$lib/SessionLauncher.svelte";
  import SessionTile from "$lib/SessionTile.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { keys } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { allRepos, currentProject, isMac } from "$lib/files.svelte";
  import { shown } from "$lib/project.svelte";
  import { app, load, openLayout, ticked, ui } from "$lib/state.svelte";

  let launcher = $state<HTMLButtonElement>();
  const visible = $derived(app.sessions.filter(shown));
  const cols = $derived(Math.max(1, Math.ceil(Math.sqrt(app.focused === null ? visible.length : 1))));
  const firstRun = $derived(app.workspace !== null && !allRepos().length);

  // Tiles another project hides: none stays maximized or ticked for broadcast, and no launcher stays without its button.
  $effect(() => {
    const ids = new Set(visible.map((s) => s.id));
    if (app.focused !== null && !ids.has(app.focused)) app.focused = null;
    untrack(() => ticked.forEach((id) => !ids.has(id) && ticked.delete(id)));
    if (!ids.size) ui.launcher = false;
  });

  // The launcher was opened for the previous project.
  $effect(() => {
    void app.project;
    ui.launcher = false;
  });

  const shortcuts = $derived([
    [keys.palette, t("palette")],
    [keys.next, t("nextWaiting")],
    [keys.switcher, t("navSwitchProject")],
    [keys.launcher, t("navNewSession")],
    [`${keys.tile(1)}…9`, t("focusTiles")],
    [keys.maximize, t("maximizeActive")],
    [keys.quick, t("quickTerminal")],
  ]);
</script>

<main class="main">
  <ViewHeader title={t("viewTerminals")} context={currentProject()?.name ?? t("ticketsAll")}>
    {#snippet actions()}
      <Layouts />
      {#if visible.length}
        <button
          bind:this={launcher}
          class="btn sm primary"
          aria-haspopup="dialog"
          aria-expanded={ui.launcher}
          aria-keyshortcuts={isMac ? "Meta+T" : "Control+Shift+T"}
          onclick={() => (ui.launcher = !ui.launcher)}
        >
          <Plus size={14} aria-hidden="true" />{t("navNewSession")}<kbd>{keys.launcher}</kbd>
        </button>
      {/if}
    {/snippet}
  </ViewHeader>
  <Popover bind:open={ui.launcher} trigger={launcher} label={t("navNewSession")} align="end" width={480}>
    <SessionLauncher title={t("navNewSession")} onopen={() => (ui.launcher = false)} />
  </Popover>
  <div class="body">
    <QuickTerminal />
    <!-- Every tile stays mounted, those of other projects hidden: their terminals keep running and their scrollback. -->
    <div
      class="grid"
      class:broadcasting={visible.some((s) => ticked.has(s.id))}
      style:--cols={cols}
      hidden={!visible.length}
    >
      {#each app.sessions as session (session.id)}
        <SessionTile {session} index={visible.indexOf(session)} />
      {/each}
    </div>
    {#if visible.length}
      <BroadcastBar />
    {:else if firstRun}
      <EmptyState title={t("noRepos")} text={t("noReposHelp")}>
        <code class="config">{app.workspace?.configDir}/termf.toml</code>
        <button class="btn" onclick={load}><RefreshCw size={14} aria-hidden="true" />{t("reload")}</button>
      </EmptyState>
    {:else if app.workspace}
      <div class="empty">
        <div class="card"><SessionLauncher title={t("emptyTitle")} /></div>
        {#if app.layouts.length}
          <div class="layouts">
            <span>{t("emptyLayouts")}</span>
            {#each app.layouts as layout (layout.name)}
              <button class="btn sm" onclick={() => openLayout(layout)}>
                <LayoutGrid size={14} aria-hidden="true" />{layout.name}
              </button>
            {/each}
          </div>
        {/if}
        <section class="keys" aria-labelledby="shortcuts-title">
          <h2 id="shortcuts-title">{t("shortcuts")}</h2>
          <dl>
            {#each shortcuts as [key, label] (label)}
              <dt><kbd>{key}</kbd></dt>
              <dd>{label}</dd>
            {/each}
          </dl>
        </section>
      </div>
    {/if}
  </div>
</main>

<style>
  .main,
  .body {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
  }

  .body {
    position: relative;
    flex: 1;
  }

  .grid {
    flex: 1;
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    grid-auto-rows: minmax(0, 1fr);
    gap: 8px;
    padding: 8px;
    min-height: 0;
    overflow: hidden;
  }

  .grid[hidden] {
    display: none;
  }

  .main :global(.view-header .primary kbd) {
    border: 0;
    background: rgb(255 255 255 / 0.18);
    color: inherit;
  }

  .empty {
    display: flex;
    flex: 1;
    flex-direction: column;
    align-items: center;
    gap: 20px;
    min-height: 0;
    padding: 40px 24px;
    overflow-y: auto;
  }

  .card {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    width: 100%;
    max-width: 560px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-2);
  }

  .layouts {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    width: 100%;
    max-width: 560px;
    color: var(--muted);
  }

  .keys {
    width: 100%;
    max-width: 560px;
    padding-top: 20px;
    border-top: 1px solid var(--border);
  }

  h2 {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 14px;
    align-items: center;
    margin: 0;
  }

  dt {
    justify-self: start;
  }

  dd {
    margin: 0;
    color: var(--muted);
  }

  .config {
    max-width: 100%;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--hover);
    color: var(--text);
    font: var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
