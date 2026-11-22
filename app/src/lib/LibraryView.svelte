<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Import from "@lucide/svelte/icons/import";
  import Info from "@lucide/svelte/icons/info";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import Search from "@lucide/svelte/icons/search";
  import Telescope from "@lucide/svelte/icons/telescope";
  import ConfigTransfer from "$lib/ConfigTransfer.svelte";
  import Conventions from "$lib/Conventions.svelte";
  import LibraryScout from "$lib/LibraryScout.svelte";
  import WorktreeSetup from "$lib/WorktreeSetup.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { t } from "$lib/i18n";
  import { currentProject } from "$lib/files.svelte";
  import { app, shortPath } from "$lib/state.svelte";
  import {
    busy,
    kinds,
    library,
    matches,
    packageLabel,
    type Item,
    type Kind,
    type Layer,
  } from "$lib/library.svelte";

  let items = $state<Item[]>([]);
  let query = $state("");
  let selected = $state<string | null>(null);

  const project = $derived(app.workspace?.projects.find((p) => p.id === library.scope));
  const ofProject = $derived(currentProject());

  // Only a project has conventions and a worktree recipe.
  $effect(() => {
    if (!library.scope && (library.tab === "conventions" || library.tab === "worktrees")) library.tab = "skill";
  });

  async function refresh() {
    const scope = library.scope;
    try {
      const list = await invoke<Item[]>("library_list", { project: scope || null });
      if (scope === library.scope) items = list;
    } catch (e) {
      library.error = String(e);
    }
  }

  $effect(() => {
    void library.scope;
    void library.version;
    refresh();
  });

  const tabLabel: Record<Kind, Parameters<typeof t>[0]> = {
    skill: "kindSkills",
    command: "kindCommands",
    role: "kindRoles",
    preference: "kindPreferences",
    rule: "kindRules",
    workflow: "kindWorkflows",
  };
  const layerLabel: Record<Layer, Parameters<typeof t>[0]> = {
    shipped: "layerShipped",
    package: "stackLayer",
    global: "layerGlobal",
    project: "layerProject",
    repo: "layerRepo",
    run: "layerRun",
  };
  const layerDefault = {
    shipped: "whyShipped",
    package: "stackWhyPackage",
    global: "whyGlobal",
    project: "whyProject",
    repo: "whyRepo",
    run: "whyRepo",
  } as const;

  const visible = $derived(items.filter((i) => matches(i, query)));
  const tabs = $derived([
    ...kinds.map((kind) => ({ id: kind, label: t(tabLabel[kind]), count: visible.filter((i) => i.kind === kind).length })),
    ...(library.scope
      ? [
          { id: "conventions", label: t("conventions"), divider: true },
          { id: "worktrees", label: t("wtTab") },
        ]
      : []),
  ]);
  const shown = $derived(library.tab === "conventions" ? [] : visible.filter((i) => i.kind === library.tab));
  const current = $derived(shown.find((i) => i.name === selected) ?? shown[0]);
  const here = $derived<Layer>(library.scope ? "project" : "global");

  /** From the default of its layer to the most specific toggle, the last one deciding. */
  function reasons(item: Item): string[] {
    const start = item.activatedBy.includes("*")
      ? "stackWhyAlways"
      : item.activatedBy.length
        ? "stackWhyOn"
        : layerDefault[item.layer];
    const list = [t(start)];
    const global = item.toggles.global;
    if (global !== undefined) list.push(t(global ? "whyOnGlobal" : "whyOffGlobal"));
    const own = item.toggles.project;
    if (library.scope && own !== undefined) {
      list.push(t(own ? "whyOnProject" : "whyOffProject", { project: project?.name ?? "" }));
    }
    return list;
  }

  /** `input` is the switch that asked: a refused toggle puts it back. */
  function toggle(item: Item, enabled: boolean | null, input?: HTMLInputElement) {
    return busy(async () => {
      try {
        await invoke("library_toggle", {
          kind: item.kind,
          name: item.name,
          project: library.scope || null,
          enabled,
        });
      } catch (e) {
        if (input) input.checked = item.active;
        throw e;
      } finally {
        await refresh();
      }
    });
  }

  function metaText(value: unknown): string {
    if (Array.isArray(value)) return value.join(", ");
    if (value && typeof value === "object") {
      return Object.entries(value)
        .map(([k, v]) => `${k}: ${typeof v === "object" ? JSON.stringify(v) : v}`)
        .join(" · ");
    }
    return String(value);
  }

  const shownMeta = [
    ["level", "metaLevel"],
    ["check", "metaCheck"],
    ["gate", "metaGate"],
    ["hook", "metaHook"],
    ["model", "metaModel"],
    ["effort", "metaEffort"],
  ] as const;
</script>

<div class="library">
  <ViewHeader title={t("viewLibrary")} context={ofProject ? "" : t("libraryGlobal")}>
    {#if ofProject}
      <div class="segmented" role="group" aria-label={t("libraryScope")}>
        {#each [false, true] as global (global)}
          <button
            aria-pressed={library.global === global}
            onclick={() => ((library.global = global), (library.panel = null))}
          >
            {global ? t("libraryGlobal") : ofProject.name}
          </button>
        {/each}
      </div>
    {/if}
    {#snippet actions()}
      <button class="btn sm" onclick={() => { library.panel = { kind: "scout" }; }}>
        <Telescope size={14} aria-hidden="true" />{t("scoutOpen")}
      </button>
      <button class="btn sm" onclick={() => { library.panel = { kind: "claude" }; }}>
        <Import size={14} aria-hidden="true" />{t("claudeImport")}
      </button>
    {/snippet}
  </ViewHeader>

  <Tabs
    id="library"
    label={t("viewLibrary")}
    {tabs}
    selected={library.panel ? null : library.tab}
    onselect={(tab) => ((library.tab = tab as typeof library.tab), (library.panel = null))}
  >
    {#if library.error}
      <div class="banners">
        <Banner kind="error" ondismiss={() => (library.error = "")}>{library.error}</Banner>
      </div>
    {/if}

    {#if library.panel?.kind === "scout"}
      <LibraryScout />
    {:else if library.panel}
      <ConfigTransfer panel={library.panel} />
    {:else if library.tab === "conventions" && project}
      {#key project.id}
        <Conventions {project} />
      {/key}
    {:else if library.tab === "worktrees" && project}
      {#key project.id}
        <WorktreeSetup {project} />
      {/key}
    {:else}
      <div class="split">
        <div class="list-pane">
          <div class="filter">
            <Search size={14} aria-hidden="true" />
            <input
              class="field"
              type="search"
              placeholder={t("librarySearch")}
              aria-label={t("librarySearch")}
              bind:value={query}
            />
          </div>
          <ul class="list">
            {#each shown as item (item.name)}
              <li class="row" class:current={item === current}>
                <button class="pick" aria-current={item === current} onclick={() => (selected = item.name)}>
                  <span class="name">{item.name}</span>
                  {#if item.description}<span class="desc">{item.description}</span>{/if}
                </button>
                <span class="badge layer-{item.layer}">{t(layerLabel[item.layer])}</span>
                <input
                  class="switch"
                  type="checkbox"
                  role="switch"
                  checked={item.active}
                  disabled={library.busy}
                  aria-label={t("itemActive", { name: item.name })}
                  onchange={(e) => toggle(item, e.currentTarget.checked, e.currentTarget)}
                />
              </li>
            {/each}
          </ul>
        </div>

        <section class="detail" aria-live="polite">
          {#if current}
            {@const item = current}
            <header>
              <div class="title">
                <h2>{item.name}</h2>
                <span class="badge layer-{item.layer}">{t(layerLabel[item.layer])}</span>
              </div>
              {#if item.description}<p class="description">{item.description}</p>{/if}
            </header>

            <div class="state">
              <label class="toggle">
                <input
                  class="switch"
                  type="checkbox"
                  role="switch"
                  checked={item.active}
                  disabled={library.busy}
                  onchange={(e) => toggle(item, e.currentTarget.checked, e.currentTarget)}
                />
                <span>{item.active ? t("itemOn") : t("itemOff")}</span>
              </label>
              {#if item.toggles[here] !== undefined}
                <button class="btn sm quiet" onclick={() => toggle(item, null)} title={t("inheritTitle")}>
                  {t("inherit")}
                </button>
              {/if}
              <ol class="why" aria-label={t("why")}>
                {#each reasons(item) as reason, i (i)}
                  <li>{reason}</li>
                {/each}
              </ol>
              {#if item.activatedBy.length && !item.activatedBy.includes("*")}
                <p class="origin">
                  {t("stackActivatedBy")}
                  {#each item.activatedBy as tag (tag)}<span class="badge">{tag}</span>{/each}
                </p>
              {/if}
              {#if item.package}
                <p class="origin">{t("stackFromPackage", { package: packageLabel(item.package) })}</p>
              {/if}
              {#if item.shadows.length}
                <p class="shadows">
                  {t("whyShadows", {
                    layers: item.shadows.map((l) => t(layerLabel[l]).toLocaleLowerCase()).join(", "),
                  })}
                </p>
              {/if}
            </div>

            <dl class="meta">
              {#if item.appliesTo.length}
                <dt>{t("metaRoles")}</dt>
                <dd>{item.appliesTo.join(", ")}</dd>
              {/if}
              {#each shownMeta as [key, label] (key)}
                {#if item.meta[key] !== undefined && item.kind !== "workflow"}
                  <dt>{t(label)}</dt>
                  <dd>{metaText(item.meta[key])}</dd>
                {/if}
              {/each}
              {#if item.tags.length}
                <dt>{t("metaTags")}</dt>
                <dd>{item.tags.join(", ")}</dd>
              {/if}
              {#if item.runtime}
                <dt>{t("metaRuntime")}</dt>
                <dd>{item.runtime}</dd>
              {/if}
              <dt>{t("metaFile")}</dt>
              <dd class="mono">{item.path ? shortPath(item.path) : t("metaBuiltIn")}</dd>
            </dl>

            {#if item.error}
              <p class="notice"><Info size={14} aria-hidden="true" />{t("itemError", { error: item.error })}</p>
            {/if}

            {#if item.body.trim()}
              <pre class="body">{item.body}</pre>
            {:else}
              <p class="muted">{t("itemEmpty")}</p>
            {/if}
          {:else}
            <EmptyState icon={LibraryBig} text={query.trim() ? t("filesNoResults") : t("libraryEmpty")} />
          {/if}
        </section>
      </div>
    {/if}
  </Tabs>
</div>

<style>
  .library {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .list-pane {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .filter {
    position: relative;
    padding: 10px 10px 6px;
    color: var(--muted);
  }

  .filter :global(svg) {
    position: absolute;
    top: 18px;
    left: 19px;
    pointer-events: none;
  }

  .filter .field {
    padding-left: 28px;
  }

  .list {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0 6px 8px;
    overflow-y: auto;
    list-style: none;
  }

  .layer-project {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .layer-global {
    border-color: var(--border-strong);
    color: var(--text);
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  .description {
    margin: 4px 0 0;
    color: var(--muted);
  }

  .state {
    display: grid;
    grid-template-columns: auto auto 1fr;
    align-items: center;
    gap: 6px 12px;
    margin: 18px 0 16px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    cursor: pointer;
  }

  .state .btn {
    justify-self: start;
  }

  .why,
  .shadows,
  .origin {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .why {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 6px;
    padding: 0;
    list-style: none;
  }

  .why li:not(:last-child)::after {
    content: "→";
    margin-left: 6px;
    color: var(--border-strong);
  }

  .why li:last-child {
    color: var(--text);
  }

  .origin {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 16px;
    margin: 0 0 16px;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .notice {
    margin-bottom: 16px;
  }

  .body {
    margin: 0;
    padding: 14px 16px;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    font: var(--text-sm) / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
