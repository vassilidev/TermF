<script lang="ts" module>
  /** The first letter of a project's name, for its mark. */
  export const initial = (name: string) => ([...name.trim()][0] ?? "?").toUpperCase();
</script>

<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import Layers from "@lucide/svelte/icons/layers";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import ProjectEditor from "$lib/ProjectEditor.svelte";
  import ProjectTally from "$lib/ProjectTally.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import { keys } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { isMac } from "$lib/files.svelte";
  import { tallies } from "$lib/project.svelte";
  import { app, setProject, ui } from "$lib/state.svelte";

  let trigger = $state<HTMLButtonElement>();
  let list = $state<HTMLUListElement>();
  let query = $state("");
  let index = $state(0);

  const projects = $derived(app.workspace?.projects ?? []);
  const current = $derived(projects.find((p) => p.id === app.project));
  const editing = $derived(projects.find((p) => p.id === ui.editing));
  const counts = $derived(tallies());
  const options = $derived.by(() => {
    const q = query.trim().toLowerCase();
    const all: { id: string | null; name: string; repos: { name: string }[] }[] = [
      { id: null, name: t("ticketsAll"), repos: [] },
      ...projects,
    ];
    return q ? all.filter((p) => p.name.toLowerCase().includes(q)) : all;
  });

  $effect(() => {
    void query;
    index = 0;
  });

  $effect(() => {
    if (!ui.switcher) query = "";
  });

  function choose(id: string | null) {
    setProject(id);
    ui.switcher = false;
    trigger?.focus();
  }

  function edit(id: string) {
    ui.switcher = false;
    ui.editing = id;
  }

  function onkeydown(e: KeyboardEvent) {
    const step = { ArrowDown: 1, ArrowUp: -1 }[e.key];
    if (step && options.length) {
      e.preventDefault();
      index = (index + step + options.length) % options.length;
      list?.querySelector(`#switcher-${index}`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && options[index]) {
      e.preventDefault();
      choose(options[index].id);
    }
  }
</script>

<button
  bind:this={trigger}
  class="switcher"
  aria-haspopup="dialog"
  aria-expanded={ui.switcher}
  aria-keyshortcuts={isMac ? "Meta+O" : "Control+O"}
  title="{t('navSwitchProject')} ({keys.switcher})"
  onclick={() => (ui.switcher = !ui.switcher)}
>
  {#if current}
    <span class="mark" aria-hidden="true">{initial(current.name)}</span>
  {:else}
    <span class="mark all" aria-hidden="true"><Layers size={14} /></span>
  {/if}
  <span class="name">{current?.name ?? t("ticketsAll")}</span>
  <ChevronsUpDown size={14} aria-hidden="true" />
</button>

<Popover bind:open={ui.switcher} {trigger} label={t("projects")} width={352}>
  <div class="search">
    <Search size={14} aria-hidden="true" />
    <input
      type="text"
      role="combobox"
      aria-expanded="true"
      aria-controls="switcher-list"
      aria-autocomplete="list"
      aria-activedescendant={options.length ? `switcher-${index}` : undefined}
      aria-label={t("navProjectSearch")}
      placeholder={t("navProjectSearch")}
      autocomplete="off"
      spellcheck="false"
      bind:value={query}
      {onkeydown}
      {@attach (el) => el.focus()}
    />
    <kbd>{keys.switcher}</kbd>
  </div>
  <ul id="switcher-list" role="listbox" aria-label={t("projects")} bind:this={list}>
    {#each options as option, i (option.id)}
      {@const n = projects.findIndex((p) => p.id === option.id) + 1}
      <!-- svelte-ignore a11y_click_events_have_key_events (the search field drives the list with the arrow keys) -->
      <li
        id="switcher-{i}"
        role="option"
        aria-selected={i === index}
        class:divided={option.id !== null && i > 0 && options[i - 1].id === null}
        onclick={() => choose(option.id)}
        onmousemove={() => (index = i)}
      >
        {#if option.id === null}
          <span class="mark all" aria-hidden="true"><Layers size={14} /></span>
        {:else}
          <span class="mark" aria-hidden="true">{initial(option.name)}</span>
        {/if}
        <span class="grow">
          {option.name}
          {#if option.repos.length}<small>{option.repos.map((r) => r.name).join(", ")}</small>{/if}
        </span>
        <ProjectTally tally={counts[option.id ?? "*"]} />
        {#if n > 0 && n <= 9}<kbd>{keys.project(n)}</kbd>{/if}
        {#if option.id === app.project}
          <Check size={14} class="current" aria-label={t("navCurrent")} />
        {/if}
      </li>
    {:else}
      <li class="none">{t("filesNoResults")}</li>
    {/each}
  </ul>
  <footer>
    <button class="btn sm" onclick={() => edit("")}><Plus size={14} aria-hidden="true" />{t("newProject")}</button>
    {#if current}
      <button class="btn sm quiet" onclick={() => edit(current.id)}>
        <Pencil size={14} aria-hidden="true" />{t("ticketsEditProject")}
      </button>
    {/if}
  </footer>
</Popover>

<!-- shortcut: the sidebar's project editor in a popover, until the Project page (T5). -->
<Popover
  bind:open={() => ui.editing !== null, (open) => !open && (ui.editing = null)}
  {trigger}
  label={editing ? t("editProject", { name: editing.name }) : t("newProject")}
  width={360}
  closeOutside={false}
>
  {#key ui.editing}
    <ProjectEditor project={editing} onclose={() => ((ui.editing = null), trigger?.focus())} />
  {/key}
</Popover>

<style>
  .switcher {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 44px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .switcher:hover,
  .switcher[aria-expanded="true"] {
    background: var(--hover);
  }

  .switcher > :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .mark {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 700;
  }

  .mark.all {
    background: var(--hover);
    color: var(--muted);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    height: 40px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 100%;
    border: 0;
    background: none;
    color: var(--text);
  }

  .search input:focus-visible {
    outline: none;
  }

  ul {
    margin: 0;
    padding: 6px;
    overflow-y: auto;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 4px 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  li[aria-selected="true"] {
    background: var(--selected);
  }

  li.divided {
    position: relative;
    margin-top: 9px;
  }

  li.divided::before {
    content: "";
    position: absolute;
    top: -5px;
    right: 8px;
    left: 8px;
    height: 1px;
    background: var(--border);
  }

  .grow {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    line-height: 1.3;
    font-weight: 500;
  }

  small {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  li :global(.current) {
    flex-shrink: 0;
    color: var(--accent-ink);
  }

  .none {
    justify-content: center;
    color: var(--muted);
    cursor: default;
  }

  footer {
    display: flex;
    justify-content: space-between;
    gap: 6px;
    flex-shrink: 0;
    padding: 8px;
    border-top: 1px solid var(--border);
  }

  :global(.popover) > :global(form.editor) {
    margin: 0;
    border: 0;
    box-shadow: none;
  }
</style>
