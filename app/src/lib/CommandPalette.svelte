<script lang="ts">
  import { onMount } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import Search from "@lucide/svelte/icons/search";
  import { commands, rank, type Command } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { currentProject } from "$lib/files.svelte";
  import type { Group } from "$lib/nav-logic";
  import { ui } from "$lib/state.svelte";

  let dialog: HTMLDialogElement;
  let list: HTMLDivElement;
  let query = $state("");
  let index = $state(0);
  const results = $derived(rank(commands(), query));
  /** The results cut where the group changes, with each one's index among all. */
  const sections = $derived.by(() => {
    const out: { group: Group; items: { command: Command; i: number }[] }[] = [];
    results.forEach((command, i) => {
      if (out.at(-1)?.group !== command.group) out.push({ group: command.group, items: [] });
      out.at(-1)!.items.push({ command, i });
    });
    return out;
  });
  const project = currentProject();
  const titles: Record<Group, string> = {
    projects: t("projects"),
    sessions: t("navSessions"),
    repos: project ? t("navProjectRepos", { project: project.name }) : t("repos"),
    others: t("repos"),
    views: project ? t("ux3t4ProjectViews", { project: project.name }) : t("navViews"),
    machine: t("settingsContext"),
    vault: t("vaultTitle"),
    actions: t("navActions"),
  };
  const previous = document.activeElement as HTMLElement | null;
  let restored = false;

  /** Gives the focus back first, so commands acting on the focused tile find it. */
  function restore() {
    if (restored) return;
    restored = true;
    if (dialog.open) dialog.close();
    previous?.focus();
  }

  onMount(() => {
    Object.assign(ui, { settings: false, switcher: false, launcher: false });
    dialog.showModal();
    return restore;
  });

  $effect(() => {
    void query;
    index = 0;
  });

  function run(command: Command | undefined) {
    if (!command) return;
    restore();
    ui.palette = false;
    command.run();
  }

  function move(to: number) {
    if (!results.length) return;
    index = (to + results.length) % results.length;
    list.querySelector(`#palette-${index}`)?.scrollIntoView({ block: "nearest" });
  }

  function onkeydown(e: KeyboardEvent) {
    const step = { ArrowDown: 1, ArrowUp: -1, PageDown: 8, PageUp: -8 }[e.key];
    if (step) {
      e.preventDefault();
      move(Math.abs(step) > 1 ? Math.max(0, Math.min(results.length - 1, index + step)) : index + step);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(results[index]);
    }
  }
</script>

<dialog
  bind:this={dialog}
  class="palette"
  aria-label={t("palette")}
  onclose={() => {
    restore();
    ui.palette = false;
  }}
  onclick={(e) => e.target === dialog && dialog.close()}
>
  <div class="search">
    <Search size={16} aria-hidden="true" />
    <input
      type="text"
      role="combobox"
      aria-expanded="true"
      aria-controls="palette-list"
      aria-autocomplete="list"
      aria-activedescendant={results.length ? `palette-${index}` : undefined}
      aria-label={t("palette")}
      placeholder={t("palettePlaceholder")}
      autocomplete="off"
      spellcheck="false"
      bind:value={query}
      {onkeydown}
    />
  </div>
  <div id="palette-list" class="list" role="listbox" aria-label={t("palette")} bind:this={list}>
    {#each sections as section (section.group)}
      <div role="group" aria-labelledby="palette-group-{section.group}">
        <div class="group" id="palette-group-{section.group}">{titles[section.group]}</div>
        {#each section.items as { command, i } (command.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events (the input drives the list with the arrow keys) -->
          <div
            id="palette-{i}"
            class="option"
            role="option"
            aria-selected={i === index}
            tabindex="-1"
            onclick={() => run(command)}
            onmousemove={() => (index = i)}
          >
            <command.icon size={16} aria-hidden="true" />
            <span class="label">{command.label}</span>
            {#if command.hint}<span class="hint">{command.hint}</span>{/if}
            {#if command.checked}<Check size={14} aria-hidden="true" />{/if}
            {#if command.keys}<kbd>{command.keys}</kbd>{/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="none">{t("filesNoResults")}</div>
    {/each}
  </div>
  <footer aria-hidden="true">
    <span><kbd>↑</kbd><kbd>↓</kbd> {t("paletteNavigate")}</span>
    <span><kbd>↵</kbd> {t("paletteRun")}</span>
    <span><kbd>esc</kbd> {t("paletteClose")}</span>
  </footer>
</dialog>

<style>
  .palette {
    width: min(600px, calc(100vw - 32px));
    margin: 14vh auto auto;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
    overflow: hidden;
  }

  .palette[open] {
    animation: rise var(--dur) var(--ease-out);
  }

  .palette::backdrop {
    background: rgb(10 12 20 / 0.28);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.99);
    }
  }

  .search {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
  }

  input {
    flex: 1;
    min-width: 0;
    height: 48px;
    border: 0;
    background: none;
    color: var(--text);
    font-size: var(--text-md);
  }

  input:focus-visible {
    outline: none;
  }

  .list {
    max-height: min(420px, 52vh);
    padding: 0 6px 6px;
    overflow-y: auto;
    scroll-padding: 28px 6px 6px;
  }

  .group {
    padding: 10px 10px 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .option,
  .none {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius);
    color: var(--muted);
    cursor: pointer;
    white-space: nowrap;
  }

  .option[aria-selected="true"] {
    background: var(--selected);
    color: var(--text);
  }

  .label {
    flex-shrink: 0;
    max-width: 70%;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--text);
  }

  .hint {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .label + :global(svg),
  .label + kbd {
    margin-left: auto;
  }

  .option :global(svg:not(:first-child)) {
    color: var(--accent-ink);
  }

  .none {
    justify-content: center;
    margin-top: 6px;
    cursor: default;
  }

  footer {
    display: flex;
    gap: 16px;
    padding: 8px 16px;
    border-top: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
  }

  footer kbd {
    margin-right: 2px;
  }
</style>
