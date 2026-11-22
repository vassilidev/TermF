<script lang="ts">
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Coffee from "@lucide/svelte/icons/coffee";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Layers from "@lucide/svelte/icons/layers";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import PanelRight from "@lucide/svelte/icons/panel-right";
  import Search from "@lucide/svelte/icons/search";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import X from "@lucide/svelte/icons/x";
  import ProjectSwitcher, { initial } from "$lib/ProjectSwitcher.svelte";
  import ProjectTally from "$lib/ProjectTally.svelte";
  import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
  import { waitingAgents } from "$lib/agents.svelte";
  import { keys, nextWaiting } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { aria } from "$lib/keymap.svelte";
  import { validProject } from "$lib/nav-logic";
  import { tallies } from "$lib/project.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { app, onScreen, panes, setProject, ui, waitingSessions, type ViewId } from "$lib/state.svelte";
  import { summary } from "$lib/sync.svelte";
  import { groups, views, type View } from "$lib/views";
  import Popover from "$lib/ui/Popover.svelte";
  import { isMac } from "$lib/files.svelte";
  import { closePanel, openBeside } from "$lib/panes.svelte";

  const waiting = $derived(waitingSessions().length + waitingAgents().length);
  const settingsView = views.find((v) => v.id === "settings")!;
  /** The view whose menu is open (right click, or the context menu key). */
  let menu = $state<{ view: View; trigger: HTMLElement } | null>(null);
  let menuOpen = $state(false);
  const besideKey = isMac ? "⌘" : "Ctrl";

  /** A view is shown where the keyboard is; with ⌘ (Ctrl elsewhere), in the other panel. */
  function pick(e: MouseEvent, view: View) {
    if (isMac ? e.metaKey : e.ctrlKey) openBeside(view.id);
    else view.show();
  }

  function openMenu(e: MouseEvent, view: View) {
    e.preventDefault();
    menu = { view, trigger: e.currentTarget as HTMLElement };
    menuOpen = true;
  }

  function act(run: () => void) {
    menuOpen = false;
    run();
  }

  /** Which side a view on screen is on, with two panels. */
  const side = (id: ViewId) => (!panes.other || !onScreen(id) ? null : (ui.view === id) === (panes.active === 0) ? "left" : "right");
  const projects = $derived(app.workspace?.projects ?? []);
  const counts = $derived(tallies());
  /** Sync needs the owner (password, conflicts, recovery key, error): a dot on Settings. */
  const tone = $derived(summary(app.sync).tone);
  const flagged = $derived(tone === "attention" || tone === "error");
  const settingsLabel = $derived(flagged ? `${t("settings")} · ${t("syncNeedsYou")}` : t("settings"));

  // A remembered or current project that no longer exists falls back to every project.
  $effect(() => {
    if (app.workspace && validProject(app.project, projects) !== app.project) setProject(null);
  });
</script>

<nav class="nav" aria-label={t("navLabel")}>
  <ProjectSwitcher />
  <button
    class="find"
    aria-haspopup="dialog"
    aria-keyshortcuts={aria("palette")}
    onclick={() => (ui.palette = true)}
  >
    <Search size={14} aria-hidden="true" /><span>{t("navSearch")}</span>
    {#if keys.palette}<kbd>{keys.palette}</kbd>{/if}
  </button>
  {#if waiting}
    <button
      class="wait"
      onclick={nextWaiting}
      title={t("nextWaiting")}
      aria-keyshortcuts={aria("next_waiting")}
    >
      <BellRing size={14} aria-hidden="true" /><span>{t("waitingCount", { count: waiting })}</span>
      {#if keys.next}<kbd>{keys.next}</kbd>{/if}
    </button>
  {/if}

  <div class="scroll">
    <h2 class="group">{t("projects")}</h2>
    <ul>
      <li>
        <button class="nav-item" aria-current={app.project === null ? "true" : undefined} onclick={() => setProject(null)}>
          <span class="mark all" aria-hidden="true"><Layers size={13} /></span>
          <span class="label">{t("ticketsAll")}</span>
        </button>
      </li>
      {#each projects as project, i (project.id)}
        <li>
          <button
            class="nav-item"
            aria-current={app.project === project.id ? "true" : undefined}
            aria-keyshortcuts={i < 9 ? aria("project", i + 1) : undefined}
            title={i < 9 && keys.project(i + 1) ? `${t("navSwitchTo", { name: project.name })} (${keys.project(i + 1)})` : undefined}
            onclick={() => setProject(project.id)}
          >
            <span class="mark" aria-hidden="true">{initial(project.name)}</span>
            <span class="label">{project.name}</span>
            <ProjectTally tally={counts[project.id]} />
          </button>
        </li>
      {/each}
    </ul>

    {#each groups as [group, title] (group)}
      <h2 class="group">{t(title)}</h2>
      <ul>
        {#each views.filter((v) => v.group === group) as view (view.id)}
          {@const count = view.count?.()}
          {@const at = side(view.id)}
          <li>
            <button
              class="nav-item"
              class:beside={at && ui.view !== view.id}
              aria-current={ui.view === view.id ? "page" : undefined}
              data-view={view.id}
              onclick={(e) => pick(e, view)}
              oncontextmenu={(e) => openMenu(e, view)}
            >
              <view.icon size={16} aria-hidden="true" />
              <span class="label">{t(view.label)}</span>
              {#if at}
                {@const Side = at === "left" ? PanelLeft : PanelRight}
                <Side class="side" size={13} aria-hidden="true" />
                {#if ui.view !== view.id}<span class="sr-only">{t("t6bBeside")}</span>{/if}
              {/if}
              {#if count}<span class="count" class:hot={view.hot}>{count}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/each}
  </div>

  <div class="foot">
    <button
      class="nav-item"
      class:beside={side("settings") && ui.view !== "settings"}
      aria-current={ui.view === "settings" ? "page" : undefined}
      aria-label={settingsLabel}
      title={settingsLabel}
      data-view="settings"
      onclick={(e) => pick(e, settingsView)}
      oncontextmenu={(e) => openMenu(e, settingsView)}
    >
      <SettingsIcon size={16} aria-hidden="true" />{t("settings")}
      {#if flagged}<span class="dot {tone}" aria-hidden="true"></span>{/if}
    </button>
    {#if app.keepAwake?.active}
      <button
        class="icon-btn awake"
        onclick={() => showSettings("awake")}
        aria-label={t("keepAwakeActive")}
        title={t("keepAwakeActive")}
      >
        <Coffee size={16} aria-hidden="true" />
      </button>
    {/if}
    <button
      class="icon-btn"
      onclick={toggleQuickTerminal}
      aria-label={t("quickTerminal")}
      aria-keyshortcuts={aria("quick_terminal")}
      title={keys.quick ? `${t("quickTerminal")} (${keys.quick})` : t("quickTerminal")}
    >
      <SquareTerminal size={16} aria-hidden="true" />
    </button>
  </div>
</nav>

<Popover bind:open={menuOpen} trigger={menu?.trigger} label={menu ? t(menu.view.label) : ""} width={240}>
  {#if menu}
    {@const id = menu.view.id}
    <div class="menu">
      <button class="item" disabled={id === ui.view} onclick={() => act(() => openBeside(id))}>
        <Columns2 size={14} aria-hidden="true" /><span>{t("t6bOpenBeside")}</span>
        <kbd>{t("t6bClick", { key: besideKey })}</kbd>
      </button>
      {#if panes.other !== null && onScreen(id)}
        <button class="item" onclick={() => act(() => closePanel(id))}>
          <X size={14} aria-hidden="true" /><span>{t("t6bClosePanel")}</span>
        </button>
      {/if}
    </div>
  {/if}
</Popover>

<style>
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    padding: 8px;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .find,
  .wait {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 8px;
    height: 30px;
    margin-top: 4px;
    padding: 0 6px 0 9px;
    border-radius: var(--radius);
    text-align: left;
  }

  .find span,
  .wait span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .find {
    border: 1px solid var(--border);
    background: var(--panel);
    color: var(--muted);
    transition: border-color var(--dur-fast) ease-out;
  }

  .find:hover {
    border-color: var(--border-strong);
  }

  .wait {
    border: 0;
    background: var(--waiting);
    color: var(--waiting-text);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .wait:hover {
    filter: brightness(1.05);
  }

  .wait kbd {
    border-color: rgb(46 29 0 / 0.25);
    background: rgb(255 255 255 / 0.35);
    color: var(--waiting-text);
  }

  .scroll {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    margin: 0 -8px;
    padding: 0 8px 8px;
    overflow-y: auto;
  }

  .group {
    margin: 14px 0 2px;
    padding: 0 8px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 1px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .nav :global(.nav-item) {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-weight: 500;
    text-align: left;
  }

  .nav :global(.nav-item > svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .nav :global(.nav-item:hover),
  .nav :global(.nav-item[aria-expanded="true"]) {
    background: var(--hover);
  }

  .nav :global(.nav-item[aria-current]) {
    background: var(--selected);
  }

  .nav :global(.nav-item[aria-current] > svg) {
    color: var(--accent-ink);
  }

  /* On screen in the other panel. */
  .nav .nav-item.beside {
    background: var(--hover);
  }

  .menu {
    display: flex;
    flex-direction: column;
    padding: 4px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .item:hover:not(:disabled) {
    background: var(--hover);
  }

  .item span {
    flex: 1;
  }

  .item > :global(svg) {
    color: var(--muted);
  }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mark {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .mark.all {
    background: var(--hover);
    color: var(--muted);
  }

  [aria-current] .mark:not(.all) {
    background: var(--accent);
    color: var(--accent-text);
  }

  .count {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .count.hot {
    padding: 0 6px;
    border-radius: var(--radius-pill);
    background: var(--waiting);
    color: var(--waiting-text);
    font-weight: 600;
    line-height: 18px;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }

  .foot :global(.nav-item) {
    flex: 1;
  }

  .awake {
    color: var(--warning);
  }

  .dot {
    width: 7px;
    height: 7px;
    margin-left: auto;
    border-radius: 50%;
    background: var(--waiting);
  }

  .dot.error {
    background: var(--danger);
  }
</style>
