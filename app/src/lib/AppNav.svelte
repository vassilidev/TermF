<script lang="ts">
  import BellRing from "@lucide/svelte/icons/bell-ring";
  import Coffee from "@lucide/svelte/icons/coffee";
  import Layers from "@lucide/svelte/icons/layers";
  import Search from "@lucide/svelte/icons/search";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import ProjectSwitcher, { initial } from "$lib/ProjectSwitcher.svelte";
  import ProjectTally from "$lib/ProjectTally.svelte";
  import Settings from "$lib/Settings.svelte";
  import { toggleQuickTerminal } from "$lib/QuickTerminal.svelte";
  import { keys, nextWaiting } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { isMac } from "$lib/files.svelte";
  import { validProject } from "$lib/nav-logic";
  import { tallies } from "$lib/project.svelte";
  import { app, setProject, ui, waitingSessions } from "$lib/state.svelte";
  import { groups, views } from "$lib/views";

  const waiting = $derived(waitingSessions().length);
  const projects = $derived(app.workspace?.projects ?? []);
  const counts = $derived(tallies());

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
    aria-keyshortcuts={isMac ? "Meta+K" : "Control+K"}
    onclick={() => (ui.palette = true)}
  >
    <Search size={14} aria-hidden="true" /><span>{t("navSearch")}</span><kbd>{keys.palette}</kbd>
  </button>
  {#if waiting}
    <button
      class="wait"
      onclick={nextWaiting}
      title={t("nextWaiting")}
      aria-keyshortcuts={isMac ? "Meta+J" : "Control+J"}
    >
      <BellRing size={14} aria-hidden="true" /><span>{t("waitingCount", { count: waiting })}</span><kbd>{keys.next}</kbd>
    </button>
  {/if}

  <div class="scroll">
    <h2 class="group">{t("projects")}</h2>
    <ul>
      <li>
        <button class="nav-item" aria-current={app.project === null ? "true" : undefined} onclick={() => setProject(null)}>
          <span class="mark all" aria-hidden="true"><Layers size={13} /></span>
          <span class="label">{t("ticketsAll")}</span>
          <ProjectTally tally={counts["*"]} />
        </button>
      </li>
      {#each projects as project, i (project.id)}
        <li>
          <button
            class="nav-item"
            aria-current={app.project === project.id ? "true" : undefined}
            aria-keyshortcuts={i < 9 ? (isMac ? `Control+${i + 1}` : `Alt+${i + 1}`) : undefined}
            title={i < 9 ? `${t("navSwitchTo", { name: project.name })} (${keys.project(i + 1)})` : undefined}
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
          <li>
            <button class="nav-item" aria-current={ui.view === view.id ? "page" : undefined} onclick={view.show}>
              <view.icon size={16} aria-hidden="true" />
              <span class="label">{t(view.label)}</span>
              {#if count}<span class="count" class:hot={view.hot}>{count}</span>{/if}
            </button>
          </li>
        {/each}
      </ul>
    {/each}
  </div>

  <div class="foot">
    <Settings />
    {#if app.keepAwake?.active}
      <button
        class="icon-btn awake"
        onclick={() => (ui.settings = true)}
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
      aria-keyshortcuts="Control+`"
      title={t("quickTerminalTitle")}
    >
      <SquareTerminal size={16} aria-hidden="true" />
    </button>
  </div>
</nav>

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

  .nav :global(.nav-item:hover) {
    background: var(--hover);
  }

  .nav :global(.nav-item[aria-current]) {
    background: var(--selected);
  }

  .nav :global(.nav-item[aria-current] > svg) {
    color: var(--accent-ink);
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
</style>
