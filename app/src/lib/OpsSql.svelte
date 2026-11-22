<script lang="ts" module>
  /** The read-only check of each database, once per session: the connection test's own (`readonly`). */
  const tested = new Map<string, boolean>();
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import { basicSetup } from "codemirror";
  import { Compartment, EditorState, Prec } from "@codemirror/state";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { languages } from "@codemirror/language-data";
  import { tags } from "@lezer/highlight";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import Check from "@lucide/svelte/icons/check";
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import { isMac } from "$lib/files.svelte";
  import { ago, t } from "$lib/i18n";
  import {
    isSql,
    pastQueries,
    runsOn,
    sortRows,
    tsv,
    type Check as Checked,
    type Connection,
    type Plan,
    type Ran,
  } from "$lib/ops-logic";
  import { loadHistory, ops } from "$lib/ops.svelte";

  let { project, ask }: { project: string; ask: (e: unknown, retry: () => Promise<void>) => boolean } = $props();

  const recipeFor = (c: Connection | undefined) => (c ? ops.recipes.find((r) => isSql(r) && runsOn(r, c)) : undefined);
  const databases = $derived(ops.connections.filter((c) => recipeFor(c)));
  const conn = $derived(databases.find((c) => c.name === ops.sql.connection) ?? databases[0]);
  const recipe = $derived(recipeFor(conn));
  const param = $derived(recipe?.params.find((p) => p.type === "sql")?.name ?? "query");
  const rows = $derived(recipe?.limits.rows ?? 500);
  const past = $derived(pastQueries(ops.history, ops.recipes.filter(isSql).map((r) => r.name)));

  $effect(() => {
    if (conn && ops.sql.connection !== conn.name) untrack(() => (ops.sql.connection = conn.name));
  });


  let view: EditorView | undefined;
  const dialect = new Compartment();
  const theme = EditorView.theme({
    "&": { minHeight: "150px", maxHeight: "320px", backgroundColor: "var(--panel)", color: "var(--text)" },
    "&.cm-focused": { outline: "none" },
    ".cm-scroller": { fontFamily: "var(--font-mono)", fontSize: "13px", fontVariantLigatures: "none" },
    ".cm-gutters": { backgroundColor: "var(--chrome)", color: "var(--muted)", border: "none" },
    ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "color-mix(in srgb, var(--hover) 70%, transparent)" },
    ".cm-cursor": { borderLeftColor: "var(--accent)" },
    ".cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground": {
      backgroundColor: "color-mix(in srgb, var(--accent) 28%, transparent)",
    },
    ".cm-placeholder": { color: "var(--muted)" },
    ".cm-tooltip": { backgroundColor: "var(--raised)", border: "1px solid var(--border)" },
  });
  const highlight = HighlightStyle.define([
    { tag: [tags.keyword, tags.operatorKeyword], color: "var(--syn-keyword)" },
    { tag: [tags.string, tags.special(tags.string)], color: "var(--syn-string)" },
    { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
    { tag: [tags.number, tags.bool, tags.null], color: "var(--syn-number)" },
    { tag: [tags.function(tags.variableName), tags.standard(tags.name)], color: "var(--syn-function)" },
    { tag: [tags.typeName, tags.special(tags.name)], color: "var(--syn-type)" },
  ]);

  async function language(kind: string | undefined) {
    const found = languages.find((l) => l.name === (kind === "mysql" ? "MySQL" : "PostgreSQL"));
    return found ? found.load().catch(() => []) : [];
  }

  function editor(node: HTMLDivElement) {
    view = new EditorView({
      parent: node,
      state: EditorState.create({
        doc: ops.sql.query,
        extensions: [
          basicSetup,
          theme,
          syntaxHighlighting(highlight),
          dialect.of([]),
          EditorView.lineWrapping,
          placeholder(t("opssqlPlaceholder")),
          EditorView.contentAttributes.of({ "aria-label": t("opssqlEditor") }),
          Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => (run(), true) }])),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) ops.sql.query = u.state.doc.toString();
          }),
        ],
      }),
    });
    language(conn?.kind).then((lang) => view?.dispatch({ effects: dialect.reconfigure(lang) }));
    return {
      destroy() {
        view?.destroy();
        view = undefined;
      },
    };
  }

  $effect(() => {
    const kind = conn?.kind;
    language(kind).then((lang) => view?.dispatch({ effects: dialect.reconfigure(lang) }));
  });

  function use(query: string, connection: string) {
    if (databases.some((c) => c.name === connection)) ops.sql.connection = connection;
    ops.sql.query = query;
    view?.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: query } });
    view?.focus();
  }

  // What will run, or why it is refused, as the query is typed.
  let plan = $state<Plan | null>(null);
  let refused = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const askFor = { project, recipe: recipe?.name, connection: conn?.name, params: { [param]: ops.sql.query } };
    clearTimeout(timer);
    if (!askFor.recipe || !askFor.connection || !ops.sql.query.trim()) {
      plan = null;
      refused = "";
      return;
    }
    timer = setTimeout(() => {
      invoke<Plan>("ops_plan", askFor).then(
        (p) => ((plan = p), (refused = "")),
        (e) => ((plan = null), (refused = String(e))),
      );
    }, 300);
  });

  // The connection test's read-only check, reused once per database.
  let writes = $state(false);
  $effect(() => {
    const c = conn;
    if (!c) return;
    const key = `${project}/${c.name}`;
    if (tested.has(key)) {
      writes = tested.get(key) ?? false;
      return;
    }
    writes = false;
    invoke<Checked[]>("ops_test", { project, connection: c, previous: c.name }).then(
      (checks) => {
        const warn = checks.some((x) => x.code === "readonly" && x.warn);
        tested.set(key, warn);
        if (ops.sql.connection === c.name) writes = warn;
      },
      () => {},
    );
  });

  let running = $state(false);
  let error = $state("");
  let notice = $state("");
  let sort = $state<{ column: number; desc: boolean } | null>(null);

  async function run() {
    if (!recipe || !conn || !ops.sql.query.trim() || running) return;
    running = true;
    error = "";
    notice = "";
    try {
      ops.sql.result = await invoke<Ran>("ops_run", {
        project,
        recipe: recipe.name,
        connection: conn.name,
        params: { [param]: ops.sql.query },
        confirm: null,
        proposal: null,
      });
      sort = null;
      loadHistory(project);
    } catch (e) {
      if (!ask(e, run)) error = String(e);
    } finally {
      running = false;
    }
  }

  const result = $derived(ops.sql.result);
  const table = $derived(result?.output.table);
  const shown = $derived(table && sort ? sortRows(table.rows, sort.column, sort.desc) : (table?.rows ?? []));

  function sortBy(column: number) {
    sort = sort?.column === column ? (sort.desc ? null : { column, desc: true }) : { column, desc: false };
  }

  async function copyTable() {
    if (!table) return;
    await navigator.clipboard.writeText(tsv(table)).catch(() => {});
    notice = t("opssqlCopied");
  }

  async function exportCsv() {
    if (!result) return;
    try {
      const path = await save({ defaultPath: `${result.plan.connection}.csv`, filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      await invoke("ops_sql_export", { result: result.id, path });
      notice = t("opssqlExported", { path });
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if !databases.length}
  <p class="notice none">{t("opssqlNoDatabase")}</p>
{:else}
  <div class="console">
    <div class="main">
      <p class="intro muted">{t("opssqlIntro")}</p>
      <div class="bar">
        <label class="inline">
          <span>{t("opssqlConnection")}</span>
          <select class="field" bind:value={ops.sql.connection}>
            {#each databases as c (c.name)}<option value={c.name}>{c.name} · {c.env}</option>{/each}
          </select>
        </label>
        {#if conn}<span class="badge env" class:prod={conn.env === "prod"}>{conn.env}</span>{/if}
        <span class="muted limits">{t("opssqlLimits", { rows, seconds: recipe?.limits.timeout_s ?? 15 })}</span>
        <span class="end muted keys">{t("opssqlRunHint", { keys: isMac ? "⌘↵" : "Ctrl+↵" })}</span>
        <button class="btn primary" disabled={running || !plan} onclick={run}>
          {#if running}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("opssqlRunning")}{:else}<Play
              size={14}
              aria-hidden="true"
            />{t("opssqlRun")}{/if}
        </button>
      </div>
      {#if writes && conn}<p class="notice warn">{t("opsCheckReadonlyWarn", { subject: conn.user ?? "" })}</p>{/if}
      <div class="editor" use:editor></div>
      <div class="check" aria-live="polite">
        {#if refused}
          <p class="refused"><ShieldX size={14} aria-hidden="true" /><span>{t("opssqlRefused", { reason: refused })}</span></p>
        {:else if plan}
          <p class="ok"><Check size={14} aria-hidden="true" /><span>{t("opssqlChecked")}</span></p>
          <pre>{plan.params[param]}</pre>
        {/if}
      </div>
      {#if error}<p class="notice error">{error}</p>{/if}

      {#if result && result.plan.connection === conn?.name}
        {@const out = result.output}
        <section class="result" aria-label={t("opssqlResult")}>
          <div class="result-head">
            <span class="badge" class:ok={out.exit === 0} class:bad={out.exit !== 0}>
              {out.exit === null ? (out.stopped ?? "—") : t("opsExit", { code: out.exit })}
            </span>
            {#if table}<span>{t("opssqlRows", { count: table.rows.length })}</span>{/if}
            <span class="muted">{t("opsDuration", { seconds: (out.millis / 1000).toFixed(1) })}</span>
            <span class="muted">{t("opsMasked", { count: out.masks })}</span>
            {#if out.truncated}<span class="muted">{t("opssqlCut", { count: rows })}</span>{/if}
            {#if table}
              <span class="end"></span>
              <button class="btn sm" onclick={copyTable}><Copy size={13} aria-hidden="true" />{t("opssqlCopy")}</button>
              <button class="btn sm" onclick={exportCsv}><Download size={13} aria-hidden="true" />{t("opssqlExport")}</button>
            {/if}
          </div>
          {#if notice}<p class="muted small" role="status">{notice}</p>{/if}
          {#if table}
            <div class="grid">
              <table>
                <thead>
                  <tr>
                    {#each table.columns as column, i (i)}
                      <th aria-sort={sort?.column === i ? (sort.desc ? "descending" : "ascending") : "none"}>
                        <button onclick={() => sortBy(i)} title={t("opssqlSort", { column })}>
                          <span>{column}</span>
                          {#if sort?.column === i}
                            {#if sort.desc}<ArrowDown size={12} aria-hidden="true" />{:else}<ArrowUp size={12} aria-hidden="true" />{/if}
                          {/if}
                        </button>
                      </th>
                    {/each}
                  </tr>
                </thead>
                <tbody>
                  {#each shown as row, r (r)}
                    <tr>
                      {#each row as cell, c (c)}<td class:null={cell === "NULL"}>{cell}</td>{/each}
                    </tr>
                  {:else}
                    <tr><td class="muted" colspan={table.columns.length}>{t("opssqlNoRows")}</td></tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
          {#if out.errors.trim()}
            <span class="label">{t("opsErrors")}</span>
            <pre class="errors">{out.errors}</pre>
          {/if}
        </section>
      {/if}
    </div>

    <aside aria-label={t("opssqlHistory")}>
      <h3>{t("opssqlHistory")}</h3>
      {#if !past.length}
        <p class="muted small">{t("opssqlHistoryEmpty")}</p>
      {:else}
        <ul>
          {#each past.slice(0, 40) as q (q.query)}
            <li>
              <button onclick={() => use(q.query, q.connection)} title={t("opssqlHistoryUse")}>
                <code>{q.query}</code>
                <span class="muted">{q.connection} · {ago(q.at)}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>
  </div>
{/if}

<style>
  .none {
    margin: 20px 28px;
  }

  .console {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 280px;
    min-height: 0;
  }

  .main {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    min-height: 0;
    padding: 16px 24px 28px;
    overflow-y: auto;
  }

  .intro {
    margin: 0;
    font-size: var(--text-sm);
  }

  .bar,
  .result-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
  }

  .inline select {
    min-width: 220px;
  }

  .end {
    margin-left: auto;
  }

  .keys {
    font-size: var(--text-xs);
  }

  .badge.prod {
    border-color: var(--danger);
    background: var(--danger);
    color: var(--danger-text);
  }

  .badge.ok {
    border-color: var(--done);
    color: var(--done);
  }

  .badge.bad {
    border-color: var(--danger);
    color: var(--danger);
  }

  /* The file editor's colors. */
  .editor {
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    --syn-keyword: #cf222e;
    --syn-string: #0a3069;
    --syn-comment: #6e7781;
    --syn-number: #0550ae;
    --syn-function: #8250df;
    --syn-type: #953800;
  }

  @media (prefers-color-scheme: dark) {
    .editor {
      --syn-keyword: #ff7b72;
      --syn-string: #a5d6ff;
      --syn-comment: #8b949e;
      --syn-number: #79c0ff;
      --syn-function: #d2a8ff;
      --syn-type: #ffa657;
    }
  }

  .editor:focus-within {
    border-color: var(--focus);
    box-shadow: 0 0 0 1px var(--focus);
  }

  .check {
    min-height: 22px;
    font-size: var(--text-sm);
  }

  .check p {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
  }

  .check :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .ok {
    color: var(--done);
  }

  .refused {
    color: var(--danger);
    font-weight: 500;
  }

  pre {
    margin: 6px 0 0;
    padding: 8px 12px;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 4px;
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .grid {
    max-height: 460px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  table {
    min-width: 100%;
    border-collapse: collapse;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  th {
    position: sticky;
    top: 0;
    padding: 0;
    border-bottom: 1px solid var(--border-strong);
    background: var(--chrome);
    text-align: left;
    white-space: nowrap;
  }

  th button {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    background: none;
    color: var(--text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  th button:hover {
    background: var(--hover);
  }

  td {
    max-width: 420px;
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  tbody tr:hover td {
    background: var(--hover);
  }

  td.null {
    color: var(--muted);
    font-style: italic;
  }

  .label {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .errors {
    max-height: 160px;
    overflow-y: auto;
    color: var(--danger);
  }

  aside {
    min-height: 0;
    padding: 16px 12px;
    overflow-y: auto;
    border-left: 1px solid var(--border);
    background: var(--chrome);
  }

  h3 {
    margin: 0 0 8px 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  aside ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  aside button {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  aside button:hover {
    background: var(--hover);
  }

  aside code {
    display: -webkit-box;
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }

  aside span {
    font-size: var(--text-xs);
  }
</style>
