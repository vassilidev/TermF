<script lang="ts">
  import { tick } from "svelte";
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Coffee from "@lucide/svelte/icons/coffee";
  import Columns2 from "@lucide/svelte/icons/columns-2";
  import Layers from "@lucide/svelte/icons/layers";
  import Monitor from "@lucide/svelte/icons/monitor";
  import PanelLeft from "@lucide/svelte/icons/panel-left";
  import PanelLeftClose from "@lucide/svelte/icons/panel-left-close";
  import PanelLeftOpen from "@lucide/svelte/icons/panel-left-open";
  import Check from "@lucide/svelte/icons/check";
  import PanelRight from "@lucide/svelte/icons/panel-right";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import X from "@lucide/svelte/icons/x";
  import ProjectSwitcher, { initial } from "$lib/ProjectSwitcher.svelte";
  import ProjectTally from "$lib/ProjectTally.svelte";
  import RemoteList from "$lib/RemoteList.svelte";
  import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
  import { waitingAgents } from "$lib/agents.svelte";
  import { waitingRuns } from "$lib/team.svelte";
  import { keys, nextWaiting } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { aria, display } from "$lib/keymap.svelte";
  import { validProject } from "$lib/nav-logic";
  import { newProject } from "$lib/newproject.svelte";
  import { tallies } from "$lib/project.svelte";
  import { showSettings } from "$lib/settings.svelte";
  import { app, onScreen, panes, setProject, ui, waitingSessions, type ViewId } from "$lib/state.svelte";
  import { summary } from "$lib/sync.svelte";
  import { views, type View } from "$lib/views";
  import Popover from "$lib/ui/Popover.svelte";
  import UsageGauge from "$lib/UsageGauge.svelte";
  import { isMac } from "$lib/files.svelte";
  import { closePanel, openBeside } from "$lib/panes.svelte";
  import { NAV, dragNav } from "$lib/workspace-logic";
  import { foldNav, nav, navRail, setNav, toggleNav } from "$lib/workspace.svelte";

  const machineKey = "termf.machine";

  const waiting = $derived(waitingSessions().length + waitingAgents().length + waitingRuns().length);
  const projectViews = views.filter((v) => v.group === "project");
  const machineViews = views.filter((v) => v.group === "machine");
  let menu = $state<{ view: View; trigger: HTMLElement } | null>(null);
  let menuOpen = $state(false);
  let heading = $state<HTMLElement>();
  let search = $state<HTMLButtonElement>();
  let list = $state<HTMLUListElement>();
  let machineOpen = $state(storedMachine());
  const unfolded = $derived(machineOpen);
  const besideKey = isMac ? "⌘" : "Ctrl";
  const rail = $derived(navRail());
  const foldLabel = $derived(rail ? t("wsUnfoldNav") : t("wsFoldNav"));
  let foldMenu = $state(false);
  let foldButton = $state<HTMLButtonElement>();
  let navEl = $state<HTMLElement>();

  /** Its right edge sizes it; let go under the snap, it folds to icons. Kept once let go. */
  function dragEdge(e: PointerEvent) {
    if (e.button !== 0) return;
    const edge = e.currentTarget as HTMLElement;
    const left = navEl?.getBoundingClientRect().left ?? 0;
    edge.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => setNav(dragNav(nav, m.clientX - left), false);
    const end = () => {
      setNav(nav);
      edge.removeEventListener("pointermove", move);
      edge.removeEventListener("pointerup", end);
      edge.removeEventListener("pointercancel", end);
    };
    edge.addEventListener("pointermove", move);
    edge.addEventListener("pointerup", end);
    edge.addEventListener("pointercancel", end);
  }

  function edgeKey(e: KeyboardEvent) {
    const width = rail ? NAV.rail : nav.width;
    const to = { ArrowLeft: width - 16, ArrowRight: width + 16, Home: NAV.min, End: NAV.max }[e.key];
    if (e.key === "Enter") toggleNav();
    else if (to !== undefined) setNav(dragNav(nav, Math.max(NAV.snap, to)));
    else return;
    e.preventDefault();
  }

  // ⌘O opens its search in the navigation, which shows again for it.
  $effect(() => {
    if (ui.switcher && nav.mode === "hidden") setNav({ ...nav, mode: "full" });
  });

  function storedMachine(): boolean {
    try {
      return localStorage.getItem(machineKey) === "1";
    } catch {
      return false;
    }
  }

  function toggleMachine() {
    machineOpen = !machineOpen;
    try {
      localStorage.setItem(machineKey, machineOpen ? "1" : "0");
    } catch {
      // Storage off: folded again at the next launch.
    }
  }

  // A view of this machine coming on screen unfolds it; folded again by the user, it stays so.
  let machineShown = false;
  $effect(() => {
    const shown = machineViews.some((v) => onScreen(v.id));
    if (shown && !machineShown) machineOpen = true;
    machineShown = shown;
  });

  // Switching project (⌃1…9, the search) may unfold its views out of sight.
  $effect(() => {
    void app.project;
    tick().then(() => {
      const project = list?.querySelector(".project[aria-current]");
      project?.closest("li")?.scrollIntoView({ block: "nearest" });
      // Taller than the list, the row shows its project first.
      project?.scrollIntoView({ block: "nearest" });
    });
  });

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

  const side = (id: ViewId) => (!panes.other || !onScreen(id) ? null : (ui.view === id) === (panes.active === 0) ? "left" : "right");
  const projects = $derived(app.workspace?.projects ?? []);
  const counts = $derived(tallies());
  /** Sync needs the owner (password, conflicts, recovery key, error): a dot on Settings, or on "This machine" folded. */
  const tone = $derived(summary(app.sync).tone);
  const flagged = $derived(tone === "attention" || tone === "error");
  const settingsLabel = $derived(flagged ? `${t("settings")} · ${t("syncNeedsYou")}` : t("settings"));

  // A remembered or current project that no longer exists falls back to every project.
  $effect(() => {
    if (app.workspace && validProject(app.project, projects) !== app.project) setProject(null);
  });
</script>

{#snippet item(view: View)}
  {@const count = view.count?.()}
  {@const at = side(view.id)}
  {@const alert = view.id === "settings" && flagged}
  {@const name = alert ? settingsLabel : t(view.label)}
  <li class:apart={view.apart}>
    <button
      class="nav-item"
      class:beside={at && ui.view !== view.id}
      aria-current={ui.view === view.id ? "page" : undefined}
      aria-label={alert ? settingsLabel : undefined}
      title={view.hint ? `${name} — ${t(view.hint)}` : name}
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
      {#if alert}<span class="dot {tone}" aria-hidden="true"></span>{/if}
    </button>
  </li>
{/snippet}

{#snippet tree()}
  <ul class="views" aria-label={t("ux3t4Views")}>
    {#each projectViews as view (view.id)}
      {@render item(view)}
    {/each}
  </ul>
{/snippet}

<nav class="nav" class:rail bind:this={navEl} hidden={nav.mode === "hidden"} aria-label={t("navLabel")}>
  <div class="top">
    <button
      class="find"
      aria-haspopup="dialog"
      aria-keyshortcuts={aria("palette")}
      onclick={() => (ui.palette = true)}
    >
      <Search size={14} aria-hidden="true" /><span class="label">{t("navSearch")}</span>
      {#if keys.palette}<kbd>{keys.palette}</kbd>{/if}
    </button>
    <button
      bind:this={foldButton}
      class="icon-btn fold"
      aria-expanded={!rail}
      aria-keyshortcuts={aria("toggle_nav")}
      aria-label={foldLabel}
      title="{foldLabel} ({display('toggle_nav')}) · {t('wsFoldHint')}"
      onclick={toggleNav}
      oncontextmenu={(e) => {
        e.preventDefault();
        foldMenu = true;
      }}
    >
      {#if rail}<PanelLeftOpen size={16} aria-hidden="true" />{:else}<PanelLeftClose size={16} aria-hidden="true" />{/if}
    </button>
  </div>
  {#if waiting}
    <button
      class="wait"
      onclick={nextWaiting}
      title={t("nextWaiting")}
      aria-keyshortcuts={aria("next_waiting")}
    >
      <BellRing size={14} aria-hidden="true" /><span class="label">{t("waitingCount", { count: waiting })}</span>
      {#if keys.next}<kbd>{keys.next}</kbd>{/if}
    </button>
  {/if}
  <RemoteList />

  <!-- Out of the scrolling list, so that ⌘O's search always opens in sight, under it. -->
  <div class="group" bind:this={heading}>
    <h2>{t("projects")}</h2>
    <button
      bind:this={search}
      class="icon-btn sm"
      aria-haspopup="dialog"
      aria-expanded={ui.switcher}
      aria-keyshortcuts={aria("switch_project")}
      aria-label={t("navSwitchProject")}
      title={keys.switcher ? `${t("navSwitchProject")} (${keys.switcher})` : t("navSwitchProject")}
      onclick={() => (ui.switcher = !ui.switcher)}
    >
      <Search size={14} aria-hidden="true" />
    </button>
    <button class="icon-btn sm" aria-label={t("newProject")} title={t("newProject")} onclick={newProject}>
      <Plus size={14} aria-hidden="true" />
    </button>
  </div>
  <div class="scroll">
    <ul class="projects" bind:this={list}>
      {#each projects as project, i (project.id)}
        {@const open = app.project === project.id}
        <li>
          <button
            class="nav-item project"
            data-project={project.id}
            aria-current={open ? "true" : undefined}
            aria-keyshortcuts={i < 9 ? aria("project", i + 1) : undefined}
            title={i < 9 && keys.project(i + 1) ? `${t("navSwitchTo", { name: project.name })} (${keys.project(i + 1)})` : project.name}
            onclick={() => setProject(project.id)}
          >
            <span class="mark" aria-hidden="true">{initial(project.name)}</span>
            <span class="label">{project.name}</span>
            <ProjectTally tally={counts[project.id]} />
          </button>
          {#if open}{@render tree()}{/if}
        </li>
      {/each}
      <li>
        <button class="nav-item project" aria-current={app.project === null ? "true" : undefined} title={t("ticketsAll")} onclick={() => setProject(null)}>
          <span class="mark all" aria-hidden="true"><Layers size={13} /></span>
          <span class="label">{t("ticketsAll")}</span>
        </button>
        {#if app.project === null}{@render tree()}{/if}
      </li>
    </ul>
  </div>

  <div class="foot">
    <div class="row">
      <button class="nav-item machine" aria-expanded={unfolded} title={t("settingsContext")} onclick={toggleMachine}>
        <Monitor size={16} aria-hidden="true" />
        <span class="label">{t("settingsContext")}</span>
        <ChevronRight class="chev" size={14} aria-hidden="true" />
      </button>
      <!-- Always in sight, whether this machine is folded or not: a first launch finds the settings. -->
      <button
        class="icon-btn gear"
        class:on={ui.view === "settings"}
        aria-label={settingsLabel}
        title={settingsLabel}
        onclick={() => showSettings()}
      >
        <SettingsIcon size={16} aria-hidden="true" />
        {#if flagged}<span class="dot {tone}" aria-hidden="true"></span>{/if}
      </button>
      <UsageGauge />
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
    {#if unfolded}
      <ul class="views" aria-label={t("settingsContext")}>
        {#each machineViews as view (view.id)}
          {@render item(view)}
        {/each}
      </ul>
    {/if}
  </div>
  <!-- A focusable separator is a widget (WAI-ARIA window splitter): the arrows size it, Enter folds it. -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="edge"
    role="separator"
    aria-orientation="vertical"
    aria-label={t("wsNavWidth")}
    aria-valuenow={rail ? NAV.rail : nav.width}
    aria-valuemin={NAV.rail}
    aria-valuemax={NAV.max}
    tabindex="0"
    title={t("wsNavWidthHint")}
    onpointerdown={dragEdge}
    ondblclick={() => setNav({ ...nav, mode: "full", width: NAV.width })}
    onkeydown={edgeKey}
  ></div>
</nav>

<Popover bind:open={foldMenu} trigger={foldButton} label={t("wsFoldAs")} width={260}>
  <div class="menu" role="menu">
    {#each [["rail", "wsFoldRail"], ["hidden", "wsHideNav"]] as const as [fold, label] (fold)}
      <button class="item" role="menuitemradio" aria-checked={nav.fold === fold} onclick={() => ((foldMenu = false), foldNav(fold))}>
        <span>{t(label)}</span>
        {#if nav.fold === fold}<Check size={14} aria-hidden="true" />{/if}
      </button>
    {/each}
  </div>
</Popover>

<ProjectSwitcher trigger={search} anchor={heading} />

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
    position: relative;
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
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 2px;
    margin: 14px 0 2px;
    padding: 0 2px 0 8px;
  }

  .group h2 {
    flex: 1;
    margin: 0;
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

  /* A project's views and this machine's hang from a thread under their parent's mark. */
  .views {
    position: relative;
    margin: 1px 0 4px;
    padding-left: 17px;
  }

  .views::before {
    content: "";
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 16px;
    width: 1px;
    background: var(--border);
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

  .views .nav-item {
    gap: 9px;
    padding-left: 10px;
  }

  .nav :global(.nav-item > svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .nav :global(.nav-item:hover) {
    background: var(--hover);
  }

  .nav :global(.nav-item[aria-current="page"]) {
    background: var(--selected);
  }

  .nav :global(.nav-item[aria-current="page"] > svg) {
    color: var(--accent-ink);
  }

  /* The current project reads by its filled mark and weight; the highlight stays on the view shown. */
  .project[aria-current] {
    font-weight: 650;
  }

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
    flex-direction: column;
    gap: 1px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }

  .row {
    display: flex;
    align-items: center;
  }

  .row .nav-item {
    flex: 1;
    gap: 6px;
    min-width: 0;
    padding-right: 2px;
  }

  /* Three buttons beside "This machine" leave its name room to show whole. */
  .row .icon-btn {
    width: 24px;
  }

  .gear {
    position: relative;
  }

  .gear.on {
    background: var(--selected);
    color: var(--accent-ink);
  }

  /* The dot sits on the icon: the row has no width to spare for it. */
  .gear .dot {
    position: absolute;
    top: 4px;
    right: 4px;
    box-shadow: 0 0 0 2px var(--chrome);
  }

  .machine :global(.chev) {
    transition: transform var(--dur-fast) ease-out;
  }

  .machine[aria-expanded="true"] :global(.chev) {
    transform: rotate(90deg);
  }

  .foot .views {
    margin-bottom: 0;
  }

  .awake {
    color: var(--warning);
  }

  .dot {
    flex-shrink: 0;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--waiting);
  }

  .dot.error {
    background: var(--danger);
  }

  /* The project's tools, after its daily work. */
  .apart {
    margin-top: 6px;
  }

  .nav[hidden] {
    display: none;
  }

  .top {
    display: flex;
    flex-shrink: 0;
    align-items: flex-end;
    gap: 4px;
  }

  .top .find {
    flex: 1;
    min-width: 0;
  }

  .fold {
    flex-shrink: 0;
    width: 30px;
    height: 30px;
    color: var(--muted);
  }

  /* Wider to grab than to see: a line over the border on hover, as the panels' handle. */
  .edge {
    position: absolute;
    z-index: 7;
    top: 0;
    right: -4px;
    bottom: 0;
    width: 8px;
    cursor: col-resize;
    touch-action: none;
  }

  .edge::after {
    content: "";
    position: absolute;
    inset: 0 3px 0 4px;
    transition: background-color var(--dur-fast) ease-out;
  }

  .edge:hover::after,
  .edge:active::after,
  .edge:focus-visible::after {
    background: var(--accent);
  }

  .edge:focus-visible {
    outline: none;
  }

  .rail .top {
    flex-direction: column-reverse;
    align-items: stretch;
  }

  .rail .fold {
    width: auto;
  }

  .rail .top .find {
    flex: none;
  }

  /* Folded (by hand, or a narrow window): icons only, each named by its tooltip and, for screen readers, its hidden label. */

  .nav.rail {
    align-items: stretch;
  }

  .rail .find,
  .rail .wait {
    justify-content: center;
    padding: 0;
  }

  .rail .find kbd,
  .rail .wait kbd,
  .rail .group h2,
  .nav.rail :global(.side),
  .nav.rail :global(.tally),
  .rail .machine :global(.chev) {
    display: none;
  }

  .nav.rail :global(.label) {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  /* Stacked, both stay in reach: the search is also where Escape gives the focus back after ⌘O. */
  .rail .group {
    flex-direction: column;
    padding: 0;
  }

  .nav.rail :global(.nav-item) {
    position: relative;
    justify-content: center;
    padding: 0;
  }

  .rail .views {
    padding-left: 0;
  }

  .rail .views::before {
    display: none;
  }

  .rail .views .nav-item {
    padding-left: 0;
  }

  /* Under a project, its views; the thread is gone, so a line marks where they end. */
  .rail .projects .views {
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
  }

  .rail .count:not(.hot) {
    display: none;
  }

  .rail .count.hot,
  .rail .views .dot {
    position: absolute;
    top: 2px;
    right: 2px;
    min-width: 16px;
    padding: 0 4px;
    font-size: var(--text-xs);
    line-height: 16px;
  }

  .rail .views .dot {
    top: 6px;
    right: 8px;
    min-width: 0;
    padding: 0;
  }

  .rail .row {
    flex-direction: column;
    align-items: stretch;
    gap: 1px;
  }

  .rail .row .nav-item {
    flex: none;
  }

  .rail .row .icon-btn {
    width: 100%;
    height: 30px;
  }

  /* A scrollbar would take a third of the rail and push the icons off center. */
  .rail .scroll {
    scrollbar-width: none;
  }

  /* Unfolded, this machine lists the settings already. */
  .rail .machine[aria-expanded="true"] ~ .gear {
    display: none;
  }

  .rail .gear .dot {
    top: 6px;
    right: 8px;
  }
</style>
