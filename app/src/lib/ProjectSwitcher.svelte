<script lang="ts" module>
  /** The first letter of a project's name, for its mark. */
  export const initial = (name: string) => ([...name.trim()][0] ?? "?").toUpperCase();
</script>

<script lang="ts">
  import { tick } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import Layers from "@lucide/svelte/icons/layers";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Search from "@lucide/svelte/icons/search";
  import ProjectTally from "$lib/ProjectTally.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import { keys } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { editProject, tallies } from "$lib/project.svelte";
  import { app, setProject, ui } from "$lib/state.svelte";

  /** The navigation's project list, which the search (⌘O) opens over; Escape gives the focus back to it. */
  let { anchor }: { anchor: HTMLElement | undefined } = $props();

  let list = $state<HTMLUListElement>();
  let query = $state("");
  let index = $state(0);

  const projects = $derived(app.workspace?.projects ?? []);
  const current = $derived(projects.find((p) => p.id === app.project));
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

  /** The keyboard lands on the project just picked, in the navigation. */
  async function choose(id: string | null) {
    setProject(id);
    ui.switcher = false;
    await tick();
    anchor?.parentElement?.querySelector<HTMLElement>(".project[aria-current]")?.focus();
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

<Popover bind:open={ui.switcher} trigger={anchor} label={t("projects")} width={352}>
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
    />
    {#if keys.switcher}<kbd>{keys.switcher}</kbd>{/if}
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
        {#if n > 0 && n <= 9 && keys.project(n)}<kbd>{keys.project(n)}</kbd>{/if}
        {#if option.id === app.project}
          <Check size={14} class="current" aria-label={t("navCurrent")} />
        {/if}
      </li>
    {:else}
      <li class="none">{t("filesNoResults")}</li>
    {/each}
  </ul>
  <footer>
    <button class="btn sm" onclick={() => editProject("")}><Plus size={14} aria-hidden="true" />{t("newProject")}</button>
    {#if current}
      <button class="btn sm quiet" onclick={() => editProject(current.id)}>
        <Pencil size={14} aria-hidden="true" />{t("ticketsEditProject")}
      </button>
    {/if}
  </footer>
</Popover>

<style>
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
</style>
