<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import FileDown from "@lucide/svelte/icons/file-down";
  import FileUp from "@lucide/svelte/icons/file-up";
  import Import from "@lucide/svelte/icons/import";
  import Info from "@lucide/svelte/icons/info";
  import Search from "@lucide/svelte/icons/search";
  import X from "@lucide/svelte/icons/x";
  import ConfigTransfer from "$lib/ConfigTransfer.svelte";
  import Conventions from "$lib/Conventions.svelte";
  import { t } from "$lib/i18n";
  import { app, shortPath } from "$lib/state.svelte";
  import {
    busy,
    exportConfig,
    importConfig,
    kinds,
    library,
    matches,
    type Item,
    type Kind,
    type Layer,
  } from "$lib/library.svelte";

  let items = $state<Item[]>([]);
  let query = $state("");
  let selected = $state<string | null>(null);

  const project = $derived(app.workspace?.projects.find((p) => p.id === library.scope));

  // A deleted project falls back to the global library, and only a project has conventions.
  $effect(() => {
    if (library.scope && app.workspace && !project) library.scope = "";
    if (!library.scope && library.tab === "conventions") library.tab = "skill";
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
    global: "layerGlobal",
    project: "layerProject",
    repo: "layerRepo",
    run: "layerRun",
  };
  const layerDefault = {
    shipped: "whyShipped",
    global: "whyGlobal",
    project: "whyProject",
    repo: "whyRepo",
    run: "whyRepo",
  } as const;

  const visible = $derived(items.filter((i) => matches(i, query)));
  const shown = $derived(library.tab === "conventions" ? [] : visible.filter((i) => i.kind === library.tab));
  const current = $derived(shown.find((i) => i.name === selected) ?? shown[0]);
  const here = $derived<Layer>(library.scope ? "project" : "global");

  /** From the default of its layer to the most specific toggle, the last one deciding. */
  function reasons(item: Item): string[] {
    const list = [t(layerDefault[item.layer])];
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
  <div class="toolbar">
    <label class="scope">
      <span>{t("libraryScope")}</span>
      <select class="field" bind:value={library.scope} onchange={() => (library.panel = null)}>
        <option value="">{t("libraryGlobal")}</option>
        {#if app.workspace?.projects.length}
          <optgroup label={t("projects")}>
            {#each app.workspace.projects as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </optgroup>
        {/if}
      </select>
    </label>
    <div class="tools">
      <button class="btn sm" onclick={() => (library.panel = { kind: "claude" })}>
        <Import size={14} aria-hidden="true" />{t("claudeImport")}
      </button>
      <button class="btn sm" onclick={exportConfig} disabled={library.busy}>
        <FileDown size={14} aria-hidden="true" />{t("configExport")}
      </button>
      <button class="btn sm" onclick={importConfig} disabled={library.busy}>
        <FileUp size={14} aria-hidden="true" />{t("configImport")}
      </button>
    </div>
  </div>

  <div class="tabs" role="tablist" aria-label={t("viewLibrary")}>
    {#each kinds as kind (kind)}
      <button
        role="tab"
        aria-selected={!library.panel && library.tab === kind}
        onclick={() => ((library.tab = kind), (library.panel = null))}
      >
        {t(tabLabel[kind])}<span class="count">{visible.filter((i) => i.kind === kind).length}</span>
      </button>
    {/each}
    {#if library.scope}
      <span class="divider" aria-hidden="true"></span>
      <button
        role="tab"
        aria-selected={!library.panel && library.tab === "conventions"}
        onclick={() => ((library.tab = "conventions"), (library.panel = null))}
      >
        {t("conventions")}
      </button>
    {/if}
  </div>

  {#if library.error}
    <div class="notice error banner" role="alert">
      <CircleAlert size={14} aria-hidden="true" />
      <span>{library.error}</span>
      <button class="icon-btn sm" onclick={() => (library.error = "")} aria-label={t("dismiss")} title={t("dismiss")}>
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  {/if}

  {#if library.panel}
    <ConfigTransfer />
  {:else if library.tab === "conventions" && project}
    {#key project.id}
      <Conventions {project} />
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
          {:else}
            <li class="none">{query.trim() ? t("filesNoResults") : t("libraryEmpty")}</li>
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
          <p class="placeholder muted">{t("libraryPick")}</p>
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .library {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: var(--chrome);
  }

  .scope {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .scope select {
    width: 220px;
  }

  .tools {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 8px;
    overflow-x: auto;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
  }

  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
    height: 34px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--muted);
    font-weight: 500;
    box-shadow: inset 0 -2px 0 transparent;
    transition:
      color var(--dur-fast) ease-out,
      box-shadow var(--dur-fast) ease-out;
  }

  .tabs button:hover {
    color: var(--text);
  }

  .tabs button[aria-selected="true"] {
    color: var(--text);
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: center;
  }

  .divider {
    width: 1px;
    height: 16px;
    margin: 0 6px;
    background: var(--border);
  }

  .banner {
    align-items: center;
    margin: 8px 12px 0;
  }

  .banner span {
    flex: 1;
  }

  .banner .icon-btn {
    color: inherit;
  }

  .split {
    display: grid;
    flex: 1;
    grid-template-columns: 360px minmax(0, 1fr);
    min-height: 0;
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

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-right: 8px;
    border-radius: var(--radius);
  }

  .row:hover {
    background: var(--hover);
  }

  .row.current {
    background: var(--selected);
  }

  .pick {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
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

  .none {
    padding: 8px;
    color: var(--muted);
  }

  .detail {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .detail > * {
    max-width: 820px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
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
  .shadows {
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

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
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

  .placeholder {
    margin-top: 40px;
    text-align: center;
  }
</style>
