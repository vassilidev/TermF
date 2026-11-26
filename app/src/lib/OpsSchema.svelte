<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Link2 from "@lucide/svelte/icons/link-2";
  import ListTree from "@lucide/svelte/icons/list-tree";
  import Network from "@lucide/svelte/icons/network";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Table2 from "@lucide/svelte/icons/table-2";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { referencedBy, schemaGroups, type SchemaTable } from "$lib/ops-logic";
  import { loadSchema, ops } from "$lib/ops.svelte";

  /** A database's tables by schema, sorted, their columns, keys, indexes and the keys pointing at them; a click on a name
   * puts it in the query. `ondiagram`: the whole schema drawn, `diagram` while it is. */
  let {
    project,
    connection,
    oninsert,
    diagram,
    ondiagram,
  }: { project: string; connection: string; oninsert: (name: string) => void; diagram: boolean; ondiagram: () => void } = $props();

  const read = $derived(ops.studio.schemas[`${project}/${connection}`]);
  const tables = $derived(Array.isArray(read) ? read : []);
  let filter = $state("");
  let open = $state<Record<string, boolean>>({});
  const shown = $derived(
    tables.filter(
      (tb) =>
        !filter.trim() ||
        `${tb.schema}.${tb.name}`.toLowerCase().includes(filter.trim().toLowerCase()) ||
        tb.columns.some((c) => c.name.toLowerCase().includes(filter.trim().toLowerCase())),
    ),
  );
  const groups = $derived(schemaGroups(shown));
  const incoming = $derived(referencedBy(tables));
  const named = (tb: SchemaTable) => (tb.schema ? `${tb.schema}.${tb.name}` : tb.name);
  const keyLabel = { pk: "opsdbKeyPk", fk: "opsdbKeyFk", unique: "opsdbKeyUnique", index: "opsdbKeyIndex" } as const;
</script>

<section class="schema" aria-label={t("opsdbSchema")}>
  <div class="head">
    <h3>{t("opsdbSchema")}</h3>
    {#if tables.length}<span class="muted count">{t("opsdbSchemaCount", { count: tables.length })}</span>{/if}
    <button
      class="icon-btn end"
      title={t("opstidyDiagram")}
      aria-label={t("opstidyDiagram")}
      aria-pressed={diagram}
      disabled={!tables.length}
      onclick={ondiagram}
    >
      <Network size={13} aria-hidden="true" />
    </button>
    <button
      class="icon-btn"
      title={t("opsdbSchemaRefresh")}
      aria-label={t("opsdbSchemaRefresh")}
      disabled={read === "loading"}
      onclick={() => loadSchema(project, connection, true)}
    >
      <RefreshCw size={13} aria-hidden="true" class={read === "loading" ? "spin" : ""} />
    </button>
  </div>
  {#if tables.length > 6}
    <input class="field filter" type="search" placeholder={t("opsdbSchemaFilter")} aria-label={t("opsdbSchemaFilter")} bind:value={filter} />
  {/if}
  {#if read === "loading" || read === undefined}
    <Skeleton shape="list" rows={6} />
  {:else if !Array.isArray(read)}
    <p class="notice error small">{t("opsdbSchemaError", { error: read.error })}</p>
  {:else if !tables.length}
    <p class="muted small">{t("opsdbSchemaEmpty")}</p>
  {:else if !shown.length}
    <p class="muted small">{t("opsdbSchemaNoMatch")}</p>
  {:else}
    <ul class="tables">
      {#each groups as g (g.schema)}
        {#if groups.length > 1}<li class="group muted">{g.schema}</li>{/if}
        {#each g.tables as tb (named(tb))}
          {@const id = named(tb)}
          {@const from = incoming.get(id) ?? []}
          <li>
            <div class="table-row">
              <button class="toggle" aria-expanded={!!open[id]} aria-label={id} onclick={() => (open[id] = !open[id])}>
                <ChevronRight size={12} aria-hidden="true" class={open[id] ? "turned" : ""} />
              </button>
              <button class="name" title={t("opsdbSchemaInsert", { name: id })} onclick={() => oninsert(id)}>
                <Table2 size={13} aria-hidden="true" /><span>{tb.name}</span><small class="muted">{tb.columns.length}</small>
              </button>
            </div>
            {#if open[id]}
              <ul class="columns">
                {#each tb.columns as c (c.name)}
                  <li>
                    <button class="col" title={t("opsdbSchemaInsert", { name: c.name })} onclick={() => oninsert(c.name)}>
                      <span class="key" title={c.key ? t(keyLabel[c.key]) : undefined}>
                        {#if c.key === "pk"}<KeyRound size={11} aria-label={t("opsdbKeyPk")} />{:else if c.key === "fk"}<Link2
                            size={11}
                            aria-label={t("opsdbKeyFk")}
                          />{:else if c.key}<ListTree size={11} aria-label={t(keyLabel[c.key])} />{/if}
                      </span>
                      <span class="cname">{c.name}</span>
                      <span class="ctype">{c.type}</span>
                      {#if c.references}<span class="ref">→ {c.references.table}{c.references.column ? `.${c.references.column}` : ""}</span>{/if}
                    </button>
                  </li>
                {/each}
              </ul>
              {#if tb.indexes.length}
                <p class="idx-head muted">{t("opsdbIndexes")}</p>
                <ul class="indexes">
                  {#each tb.indexes as ix (ix.name)}
                    <li title={ix.columns}><span>{ix.name}</span>{#if ix.unique}<span class="badge">{t("opsdbKeyUnique")}</span>{/if}</li>
                  {/each}
                </ul>
              {/if}
              {#if from.length}
                <p class="idx-head muted">{t("opstidyReferencedBy")}</p>
                <ul class="indexes">
                  {#each from as f (f)}<li><Link2 size={11} aria-hidden="true" /><span>{f}</span></li>{/each}
                </ul>
              {/if}
            {/if}
          </li>
        {/each}
      {/each}
    </ul>
  {/if}
</section>

<style>
  .schema {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 4px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .count {
    font-size: var(--text-xs);
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .icon-btn.end {
    margin-left: auto;
  }

  .icon-btn[aria-pressed="true"] {
    background: var(--selected);
    color: var(--text);
  }

  .group {
    margin: 8px 0 2px;
    padding-left: 6px;
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .group:first-child {
    margin-top: 0;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }

  .filter {
    font-size: var(--text-xs);
  }

  .small {
    margin: 0 4px;
    font-size: var(--text-xs);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tables {
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
  }

  .table-row {
    display: flex;
    align-items: center;
  }

  .toggle,
  .name,
  .col {
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }

  .toggle {
    display: grid;
    place-items: center;
    width: 20px;
    height: 26px;
    color: var(--muted);
  }

  .toggle :global(svg) {
    transition: transform var(--dur-fast) var(--ease-out);
  }

  .toggle :global(.turned) {
    transform: rotate(90deg);
  }

  .name {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 4px 6px;
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    text-align: left;
  }

  .name small {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }

  .name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .name:hover,
  .col:hover {
    background: var(--hover);
  }

  .columns {
    margin: 0 0 4px 20px;
    border-left: 1px solid var(--border);
  }

  .col {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr) auto;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 3px 6px;
    border-radius: 0 var(--radius) var(--radius) 0;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    text-align: left;
  }

  .key {
    display: grid;
    place-items: center;
    color: var(--warning);
  }

  .cname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ref {
    grid-column: 2 / -1;
    overflow: hidden;
    color: var(--accent-ink);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ctype {
    max-width: 90px;
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .idx-head {
    margin: 2px 0 2px 26px;
    font-size: var(--text-xs);
  }

  .indexes {
    margin: 0 0 6px 26px;
  }

  .indexes li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--muted);
  }

  .indexes span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
