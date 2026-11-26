<script lang="ts" module>
  /** The read-only check of each database, once per session: the connection test's own (`readonly`). */
  const tested = new Map<string, boolean>();
</script>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import { basicSetup } from "codemirror";
  import { Compartment, EditorState, Prec } from "@codemirror/state";
  import { EditorView, keymap, placeholder } from "@codemirror/view";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { languages } from "@codemirror/language-data";
  import { tags } from "@lezer/highlight";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Check from "@lucide/svelte/icons/check";
  import ChevronDown from "@lucide/svelte/icons/chevron-down";
  import Copy from "@lucide/svelte/icons/copy";
  import Database from "@lucide/svelte/icons/database";
  import Download from "@lucide/svelte/icons/download";
  import Gauge from "@lucide/svelte/icons/gauge";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageCircleQuestion from "@lucide/svelte/icons/message-circle-question";
  import Play from "@lucide/svelte/icons/play";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import OpsChart from "$lib/OpsChart.svelte";
  import OpsDba from "$lib/OpsDba.svelte";
  import OpsDiagram from "$lib/OpsDiagram.svelte";
  import OpsSaved from "$lib/OpsSaved.svelte";
  import OpsSchema from "$lib/OpsSchema.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { isMac } from "$lib/files.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import {
    COPIES,
    EXPORTS,
    EXTENSIONS,
    completions,
    exportedQuery,
    firstTable,
    isSql,
    pastQueries,
    refusal,
    sortRows,
    type Check as Checked,
    type ExportFormat,
    type Plan,
    type Query,
  } from "$lib/ops-logic";
  import { loadHistory, loadSchema, ops, runSql, sqlRecipe } from "$lib/ops.svelte";

  let { project, ask }: { project: string; ask: (e: unknown, retry: () => Promise<void>) => boolean } = $props();

  const recipeFor = sqlRecipe;
  const databases = $derived(ops.connections.filter((c) => recipeFor(c)));
  // None until picked: the databases show first, at the top.
  const conn = $derived(databases.find((c) => c.name === ops.sql.connection));
  const recipe = $derived(recipeFor(conn));
  const param = $derived(recipe?.params.find((p) => p.type === "sql")?.name ?? "query");
  const rows = $derived(recipe?.limits.rows ?? 500);
  const past = $derived(pastQueries(ops.history, ops.recipes.filter(isSql)));
  const schema = $derived.by(() => {
    const read = conn ? ops.studio.schemas[`${project}/${conn.name}`] : undefined;
    return Array.isArray(read) ? read : [];
  });

  const reasons: Record<string, Parameters<typeof t>[0]> = {
    database: "opssqlWhyDatabase",
    empty: "opssqlWhyEmpty",
    control: "opssqlWhyControl",
    backslash: "opssqlWhyBackslash",
    executable: "opssqlWhyExecutable",
    unreadable: "opssqlWhyUnreadable",
    several: "opssqlWhySeveral",
    notRead: "opssqlWhyNotRead",
    explainAnalyze: "opssqlWhyExplainAnalyze",
    explainOnly: "opssqlWhyExplainOnly",
    into: "opssqlWhyInto",
    locks: "opssqlWhyLocks",
    function: "opssqlWhyFunction",
  };
  /** A refusal in the interface's language, from its code; any other error as it came. */
  function reason(e: unknown): string {
    const r = refusal(e);
    if (!r) return String(e);
    return reasons[r.code] ? t(reasons[r.code], { subject: r.subject }) : r.message;
  }

  // The schema of the database shown, once per session; a locked vault asks for its password, then reads it.
  $effect(() => {
    const name = conn?.name;
    if (!name) return;
    untrack(async () => {
      await loadSchema(project, name);
      const read = ops.studio.schemas[`${project}/${name}`];
      if (read && typeof read === "object" && "error" in read) ask(read.error, () => loadSchema(project, name, true));
    });
  });

  let view: EditorView | undefined;
  const dialect = new Compartment();
  const completion = new Compartment();
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
    ".cm-tooltip-autocomplete ul li[aria-selected]": { backgroundColor: "var(--selected)", color: "var(--text)" },
    ".cm-completionDetail": { color: "var(--muted)", fontStyle: "normal", marginLeft: "8px" },
  });
  const highlight = HighlightStyle.define([
    { tag: [tags.keyword, tags.operatorKeyword], color: "var(--syn-keyword)" },
    { tag: [tags.string, tags.special(tags.string)], color: "var(--syn-string)" },
    { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
    { tag: [tags.number, tags.bool, tags.null], color: "var(--syn-number)" },
    { tag: [tags.function(tags.variableName), tags.standard(tags.name)], color: "var(--syn-function)" },
    { tag: [tags.typeName, tags.special(tags.name)], color: "var(--syn-type)" },
  ]);

  const dialects: Record<string, string> = {
    postgres: "PostgreSQL",
    mysql: "MySQL",
    mariadb: "MariaDB SQL",
    sqlite: "SQLite",
    clickhouse: "SQL",
  };
  async function language(kind: string | undefined) {
    const found = languages.find((l) => l.name === dialects[kind ?? "postgres"]);
    return found ? found.load().catch(() => []) : [];
  }

  type Context = { matchBefore(re: RegExp): { from: number; to: number } | null; explicit: boolean };
  /** The schema's tables and columns, next to the dialect's own keywords. */
  function schemaCompletion(items: ReturnType<typeof completions>) {
    const source = (ctx: Context) => {
      const word = ctx.matchBefore(/[\w.]+/);
      if (!word || (word.from === word.to && !ctx.explicit)) return null;
      return { from: word.from, options: items, validFor: /^[\w.]*$/ };
    };
    return EditorState.languageData.of(() => [{ autocomplete: source }]);
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
          completion.of([]),
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

  $effect(() => {
    const items = completions(schema);
    view?.dispatch({ effects: completion.reconfigure(schemaCompletion(items)) });
  });

  function use(query: string, connection?: string) {
    if (connection && databases.some((c) => c.name === connection)) ops.sql.connection = connection;
    ops.sql.query = query;
    view?.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: query } });
    view?.focus();
  }

  function insert(name: string) {
    if (!view) return;
    view.dispatch(view.state.replaceSelection(name));
    view.focus();
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
        (e) => ((plan = null), (refused = reason(e))),
      );
    }, 300);
  });

  /** The console's verdict on a query the DBA gave: `null` when it reads. */
  async function check(sql: string): Promise<string | null> {
    if (!recipe || !conn) return null;
    return invoke<Plan>("ops_plan", { project, recipe: recipe.name, connection: conn.name, params: { [param]: sql } }).then(
      () => null,
      (e) => reason(e),
    );
  }

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
  let flash = $state("");
  let flashTimer: ReturnType<typeof setTimeout> | undefined;
  let sort = $state<{ column: number; desc: boolean } | null>(null);

  /** A word that the action was done, for a few seconds. */
  function notify(text: string) {
    flash = text;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (flash = ""), 5000);
  }

  async function run() {
    if (!recipe || !conn || !ops.sql.query.trim() || running) return;
    running = true;
    error = "";
    notice = "";
    try {
      await runSql(project, conn, ops.sql.query);
      sort = null;
    } catch (e) {
      if (!ask(e, run)) error = reason(e);
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

  const formats: Record<ExportFormat, Parameters<typeof t>[0]> = {
    csv: "opsdbFmtCsv",
    tsv: "opsdbFmtTsv",
    json: "opsdbFmtJson",
    ndjson: "opsdbFmtNdjson",
    markdown: "opsdbFmtMarkdown",
    sql: "opsdbFmtSql",
    xlsx: "opsdbFmtXlsx",
  };
  let copyOpen = $state(false);
  let exportOpen = $state(false);
  let copyButton = $state<HTMLElement>();
  let exportButton = $state<HTMLElement>();
  let exporting = $state(false);

  async function copyAs(format: ExportFormat) {
    copyOpen = false;
    if (!result) return;
    try {
      const text = await invoke<string>("ops_sql_copy", { result: result.id, format });
      await navigator.clipboard.writeText(text);
      notice = t("opsdbCopied", { format: t(formats[format]) });
    } catch (e) {
      error = String(e);
    }
  }

  /** The file the user picks for `format`, or null. */
  async function pick(format: ExportFormat, base: string): Promise<string | null> {
    const ext = EXTENSIONS[format];
    return save({ defaultPath: `${base}.${ext}`, filters: [{ name: t(formats[format]), extensions: [ext] }] });
  }

  async function exportAs(format: ExportFormat) {
    exportOpen = false;
    if (!result) return;
    try {
      const name = firstTable(result.plan.params[param] ?? "");
      const path = await pick(format, `${result.plan.connection}-${name}`);
      if (!path) return;
      await invoke("ops_sql_export", { result: result.id, path, format, name });
      notice = t("opsdbExported", { path });
    } catch (e) {
      error = String(e);
    }
  }

  let exportFormat = $state<ExportFormat>("csv");
  async function exportAll() {
    exportOpen = false;
    if (!result) return;
    const query = exportedQuery(result, param);
    const name = firstTable(query);
    try {
      const path = await pick(exportFormat, `${result.plan.connection}-${name}`);
      if (!path) return;
      exporting = true;
      const done = await invoke<{ rows: number; truncated: boolean }>("ops_sql_export_all", {
        project,
        connection: result.plan.connection,
        query,
        path,
        format: exportFormat,
        name,
      });
      notice = t(done.truncated ? "opsdbExportedAllCut" : "opsdbExportedAll", { rows: done.rows.toLocaleString(lang), path });
      loadHistory(project);
    } catch (e) {
      if (!ask(e, exportAll)) error = reason(e);
    } finally {
      exporting = false;
    }
  }

  function saveQuery(sql: string) {
    ops.studio.editing = { query: { name: "", sql, connection: conn?.name }, previous: null };
    ops.studio.aside = "saved";
  }

  let shownAs = $state<"table" | "chart">("table");
  let chart = $state<ReturnType<typeof OpsChart>>();

  async function exportChart(format: "png" | "svg") {
    exportOpen = false;
    if (!result) return;
    try {
      const base = `${result.plan.connection}-${firstTable(result.plan.params[param] ?? "")}`;
      const label = t(format === "png" ? "sqlcsExportPng" : "sqlcsExportSvg");
      const path = await save({ defaultPath: `${base}.${format}`, filters: [{ name: label, extensions: [format] }] });
      if (!path) return;
      const bytes = await chart?.image(format);
      if (!bytes) return;
      await invoke("ops_chart_save", { path, bytes: Array.from(bytes) });
      notice = t("opsdbExported", { path });
    } catch (e) {
      error = String(e);
    }
  }

  function queryTable(name: string) {
    ops.sql.query = `SELECT * FROM ${name}`;
    ops.studio.view = "query";
  }

  function askTable(name: string) {
    dba?.ask("write", t("sqlcsAskTable", { table: name }));
  }

  let dba = $state<ReturnType<typeof OpsDba>>();
  let dbaPane = $state<HTMLElement>();

  /** The DBA's conversation in sight, its field ready. */
  async function askDba() {
    ops.studio.aside = "dba";
    await tick();
    dbaPane?.querySelector("textarea")?.focus();
  }
  const tabs = [
    { id: "dba", label: "opsdbAsideDba" },
    { id: "saved", label: "opsdbAsideSaved" },
    { id: "recent", label: "opsdbAsideRecent" },
  ] as const;
</script>

{#if !databases.length}
  <p class="notice none">{t("opssqlNoDatabase")}</p>
{:else if !conn}
  <div class="pick"><EmptyState icon={Database} text={t("opstidyPickDb")} /></div>
{:else}
  <div class="host">
    <div class="studio">
      <aside class="left">
        <OpsSchema
          {project}
          connection={conn.name}
          oninsert={insert}
          diagram={ops.studio.view === "schema"}
          ondiagram={() => (ops.studio.view = ops.studio.view === "schema" ? "query" : "schema")}
        />
      </aside>
      <div class="main">
        {#if ops.studio.view === "schema"}
          {@const read = ops.studio.schemas[`${project}/${conn.name}`]}
          <div class="bar">
            <button class="btn sm quiet back" onclick={() => (ops.studio.view = "query")}><ArrowLeft size={14} aria-hidden="true" />{t("sqlcsQuery")}</button>
          </div>
          {#if read === "loading" || read === undefined}
            <Skeleton rows={6} />
          {:else if !Array.isArray(read)}
            <p class="notice error">{t("opsdbSchemaError", { error: read.error })}</p>
          {:else if !read.length}
            <p class="muted">{t("opsdbSchemaEmpty")}</p>
          {:else}
            {#key `${project}/${conn.name}`}<OpsDiagram tables={read} onquery={queryTable} onask={askTable} />{/key}
          {/if}
        {:else}
          <div class="bar">
            <b class="mono">{conn.name}</b>
            <span class="badge env" class:prod={conn.env === "prod"}>{conn.env}</span>
            <span class="muted">{t("opssqlLimits", { rows, seconds: recipe?.limits.timeout_s ?? 15 })}</span>
          </div>
          <p class="intro muted">{t("opssqlIntro")}</p>
          {#if writes}<p class="notice warn">{t("opsCheckReadonlyWarn", { subject: conn.user ?? "" })}</p>{/if}
          <div class="editor" use:editor></div>
          <div class="tools">
            <button class="btn sm" class:primary={!ops.sql.query.trim()} onclick={askDba}>
              <Sparkles size={13} aria-hidden="true" />{t("opsdbAsk")}
            </button>
            <div class="buttons">
              <button class="btn sm" disabled={!plan} onclick={() => dba?.ask("explain", ops.sql.query)}>
                <MessageCircleQuestion size={13} aria-hidden="true" />{t("opsdbExplain")}
              </button>
              <button class="btn sm" disabled={!plan} onclick={() => dba?.ask("optimize", ops.sql.query)}>
                <Gauge size={13} aria-hidden="true" />{t("opsdbOptimize")}
              </button>
              <button class="btn sm" disabled={!ops.sql.query.trim()} onclick={() => saveQuery(ops.sql.query)}>
                <Bookmark size={13} aria-hidden="true" />{t("opsdbSave")}
              </button>
              <span class="muted keys">{t("opssqlRunHint", { keys: isMac ? "⌘↵" : "Ctrl+↵" })}</span>
              <button class="btn" class:primary={!!ops.sql.query.trim()} disabled={running || !plan} onclick={run}>
                {#if running}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("opssqlRunning")}{:else}<Play
                    size={14}
                    aria-hidden="true"
                  />{t("opssqlRun")}{/if}
              </button>
            </div>
          </div>
          <div class="check" aria-live="polite">
            {#if refused}
              <p class="refused"><ShieldX size={14} aria-hidden="true" /><span>{t("opssqlRefused", { reason: refused })}</span></p>
            {:else if plan}
              <p class="ok"><Check size={14} aria-hidden="true" /><span>{t("opssqlChecked")}</span></p>
              <pre>{plan.params[param]}</pre>
            {/if}
          </div>
          {#if flash}<p class="flash" role="status">{flash}</p>{/if}
          {#if error}<p class="notice error">{error}</p>{/if}

          {#if running && !(result && result.plan.connection === conn?.name)}
            <div class="loading" aria-hidden="true">
              {#each [0, 1, 2, 3, 4] as i (i)}<span style:width="{92 - i * 9}%"></span>{/each}
            </div>
          {/if}
          {#if result && result.plan.connection === conn?.name}
            {@const out = result.output}
            <section class="result" class:stale={running} aria-label={t("opssqlResult")}>
              <div class="result-head">
                <span class="badge" class:ok={out.exit === 0} class:danger={out.exit !== 0}>
                  {out.exit === null ? (out.stopped ?? "—") : t("opsExit", { code: out.exit })}
                </span>
                {#if table}<span>{t("opssqlRows", { count: table.rows.length })}</span>{/if}
                <span class="muted">{t("opsDuration", { seconds: (out.millis / 1000).toFixed(1) })}</span>
                <span class="muted">{t("opsMasked", { count: out.masks })}</span>
                {#if out.truncated}<span class="muted">{t("opsTruncated")}</span>{/if}
                {#if table?.columns.length}
                  <span class="end"></span>
                  <div class="segmented" role="group" aria-label={t("sqlcsShownAs")}>
                    <button aria-pressed={shownAs === "table"} onclick={() => (shownAs = "table")}>{t("sqlcsTable")}</button>
                    <button aria-pressed={shownAs === "chart"} onclick={() => (shownAs = "chart")}>{t("sqlcsChart")}</button>
                  </div>
                  <button class="btn sm" bind:this={copyButton} aria-expanded={copyOpen} onclick={() => (copyOpen = !copyOpen)}>
                    <Copy size={13} aria-hidden="true" />{t("opsdbCopy")}<ChevronDown size={12} aria-hidden="true" />
                  </button>
                  <button class="btn sm" bind:this={exportButton} aria-expanded={exportOpen} disabled={exporting} onclick={() => (exportOpen = !exportOpen)}>
                    {#if exporting}<LoaderCircle size={13} aria-hidden="true" class="spin" />{t("opsdbExporting")}{:else}<Download
                        size={13}
                        aria-hidden="true"
                      />{t("opsdbExport")}<ChevronDown size={12} aria-hidden="true" />{/if}
                  </button>
                {/if}
              </div>
              {#if copyOpen}
                <Popover bind:open={copyOpen} trigger={copyButton} label={t("opsdbCopy")} align="end" width={200}>
                  <div class="menu" role="menu">
                    {#each COPIES as f (f)}<button role="menuitem" onclick={() => copyAs(f)}>{t(formats[f])}</button>{/each}
                  </div>
                </Popover>
              {/if}
              {#if exportOpen}
                <Popover bind:open={exportOpen} trigger={exportButton} label={t("opsdbExport")} align="end" width={360}>
                  <div class="menu" role="menu">
                    <div class="formats">
                      {#each EXPORTS as f (f)}<button role="menuitem" onclick={() => exportAs(f)}>{t(formats[f])}</button>{/each}
                    </div>
                    <hr />
                    <div class="all">
                      <div class="all-row">
                        <button role="menuitem" class="strong" onclick={exportAll}>
                          <Sparkles size={13} aria-hidden="true" />{t("opsdbExportAll", { rows: (100000).toLocaleString(lang) })}
                        </button>
                        <select class="field" bind:value={exportFormat} aria-label={t("opsdbExport")}>
                          {#each EXPORTS as f (f)}<option value={f}>{t(formats[f])}</option>{/each}
                        </select>
                      </div>
                      <small class="muted">{t("opsdbExportAllHelp", { shown: rows })}</small>
                    </div>
                    {#if shownAs === "chart"}
                      <hr />
                      <div class="formats">
                        <button role="menuitem" onclick={() => exportChart("png")}>{t("sqlcsExportPng")}</button>
                        <button role="menuitem" onclick={() => exportChart("svg")}>{t("sqlcsExportSvg")}</button>
                      </div>
                    {/if}
                  </div>
                </Popover>
              {/if}
              {#if notice}<p class="muted small" role="status">{notice}</p>{/if}
              {#if table && !table.columns.length}
                <p class="muted">{t("opssqlNoRows")}</p>
              {:else if table && shownAs === "chart"}
                {#key result.id}<OpsChart bind:this={chart} {table} />{/key}
              {:else if table}
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
        {/if}
      </div>

      <aside class="right">
        <div class="segmented aside-tabs" role="group" aria-label={t("opssqlHistory")}>
          {#each tabs as tab (tab.id)}
            <button aria-pressed={ops.studio.aside === tab.id} onclick={() => (ops.studio.aside = tab.id)}>
              {t(tab.label)}{#if tab.id === "saved" && ops.queries.length}<span class="count">{ops.queries.length}</span>{/if}
            </button>
          {/each}
        </div>
        {#if conn}
          <div class="pane-dba" hidden={ops.studio.aside !== "dba"} bind:this={dbaPane}>
            {#key `${project}/${conn.name}`}
              <OpsDba
                bind:this={dba}
                {project}
                {conn}
                {check}
                {ask}
                onuse={(sql) => use(sql)}
                onrun={(sql) => (use(sql), run())}
                onsave={saveQuery}
              />
            {/key}
          </div>
        {/if}
        {#if ops.studio.aside === "saved"}
          <OpsSaved
            {project}
            {databases}
            {notify}
            onopen={(q: Query) => use(q.sql, q.connection)}
            onrun={(q: Query) => (use(q.sql, q.connection), run())}
          />
        {:else if ops.studio.aside === "recent"}
          {#if !past.length}
            <p class="muted small">{t("opssqlHistoryEmpty")}</p>
          {:else}
            <ul class="recent">
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
        {/if}
      </aside>
    </div>
  </div>
{/if}

<style>
  /* What the studio's columns depend on: its own width, ⌘I open or not. */
  .host {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    container: studio / inline-size;
  }

  .none {
    margin: 20px 28px;
  }

  .pick {
    flex: 1;
    overflow-y: auto;
  }

  .studio {
    display: grid;
    flex: 1;
    grid-template-columns: 232px minmax(0, 1fr) 340px;
    min-height: 0;
  }

  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    padding: 16px 12px;
    overflow-y: auto;
    background: var(--chrome);
  }

  .left {
    border-right: 1px solid var(--border);
  }

  .right {
    border-left: 1px solid var(--border);
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

  .main > * {
    flex-shrink: 0;
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

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px 12px;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-left: auto;
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

  .check pre {
    margin-top: 6px;
  }

  .ok {
    color: var(--done);
  }

  .refused {
    color: var(--danger);
    font-weight: 500;
  }

  pre {
    margin: 0;
    padding: 8px 12px;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .flash {
    align-self: flex-start;
    margin: 0;
    padding: 5px 10px;
    border-radius: var(--radius-pill);
    background: var(--done-soft);
    color: var(--done);
    font-size: var(--text-xs);
    font-weight: 500;
    animation: arrive var(--dur) var(--ease-out);
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(-3px);
    }
  }

  .loading {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .loading span {
    height: 10px;
    border-radius: 3px;
    background: var(--hover);
    animation: shimmer 1.2s ease-in-out infinite;
  }

  @keyframes shimmer {
    50% {
      opacity: 0.45;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .loading span,
    .flash {
      animation: none;
    }
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 4px;
    transition: opacity var(--dur) var(--ease-out);
  }

  .result.stale {
    opacity: 0.55;
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .menu {
    display: flex;
    flex-direction: column;
    padding: 4px;
  }

  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .menu button:hover {
    background: var(--hover);
  }

  .formats {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }

  .menu hr {
    width: 100%;
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .all {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px;
  }

  .all-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .all-row select {
    width: 96px;
  }

  .all .strong {
    flex: 1;
    font-weight: 600;
  }

  .all small {
    padding: 0 8px;
    font-size: var(--text-xs);
    line-height: 1.45;
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
    font-variant-numeric: tabular-nums;
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

  .aside-tabs {
    align-self: stretch;
    display: flex;
  }

  .aside-tabs button {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }

  .count {
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  /* The conversation's thread and field go to the pane's edges, as in the ⌘I panel. */
  .pane-dba {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    margin: 0 -12px -16px;
  }

  .pane-dba[hidden] {
    display: none;
  }

  .recent {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .recent button {
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

  .recent button:hover {
    background: var(--hover);
  }

  .recent code {
    display: -webkit-box;
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }

  .recent span {
    font-size: var(--text-xs);
  }

  @media (max-width: 1180px) {
    .studio {
      grid-template-columns: 200px minmax(0, 1fr) 300px;
    }
  }

  /* Too narrow for three columns: the schema, the query and the history one above the other. */
  @container studio (max-width: 760px) {
    .studio {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr) minmax(0, 2fr) minmax(0, 2fr);
    }

    .left {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }

    .right {
      border-top: 1px solid var(--border);
      border-left: 0;
    }

    .main {
      padding: 12px 16px 16px;
    }
  }
</style>
