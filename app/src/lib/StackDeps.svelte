<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Search from "@lucide/svelte/icons/search";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { askAI } from "$lib/ask.svelte";
  import { focusSession } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { app, openSession } from "$lib/state.svelte";
  import { stack } from "$lib/stack.svelte";
  import { ending, matches, proposal, rows, tally, worst, type Check, type Filter, type RepoMap, type Row, type Severity } from "$lib/stack-logic";

  type Key = Parameters<typeof t>[0];

  /** A repository's dependencies and runtimes, filtered; each line says how far behind it is and what it risks. */
  let { repo, check, today }: { repo: RepoMap; check: Check | null; today: string } = $props();

  const list = $derived(rows(repo, check, today));
  const counts = $derived(tally(list, today));
  const shown = $derived(list.filter((r) => matches(r, stack.filter, stack.query, today)));
  let open = $state<string | null>(null);
  let asking = $state<{ row: Row; to: string; command: string } | null>(null);
  let confirming = $state(false);
  let error = $state("");

  const severityKey: Record<Severity, Key> = {
    critical: "stackSevCritical",
    high: "stackSevHigh",
    moderate: "stackSevModerate",
    low: "stackSevLow",
    unknown: "stackSevUnknown",
  };
  const bumpKey = { patch: "stackPatch", minor: "stackMinor", major: "stackMajor" } as const;
  const date = (d: string) => new Date(`${d}T00:00:00`).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });

  const filters = $derived([
    { id: "all", label: t("stackFilterAll"), count: counts.all },
    { id: "updates", label: t("stackFilterUpdates"), count: counts.updates },
    { id: "vulnerable", label: t("stackFilterVulnerable"), count: counts.vulnerable },
    { id: "eol", label: t("stackFilterEol"), count: counts.eol },
  ]);

  function update(row: Row) {
    const p = proposal(row);
    const c = row.check;
    if (!p || !c) return;
    if (!p.major && c.command) {
      asking = { row, to: p.to, command: c.command };
      confirming = true;
      return;
    }
    const from = row.version ?? "";
    const draft = p.major ? "stackMajorDraft" : "stackManualDraft";
    hand(row, p.to, t(draft, { name: row.name, from, to: p.to, repo: repo.name, manager: row.dep?.manager ?? "" }));
  }

  /** To devops, in the screen's conversation, with what the line says as context. */
  function hand(row: Row, to: string, draft: string) {
    const c = row.check;
    const lines = [
      `repository: ${repo.path}`,
      row.dep && `manifest: ${row.dep.manifest} (${row.dep.manager}, range "${row.dep.range}"${row.dep.dev ? ", dev" : ""})`,
      `used: ${row.version ?? "?"}; latest: ${c?.latest ?? row.support?.latest ?? "?"}; safe: ${c?.safe ?? "none"}`,
      row.support && `support: cycle ${row.support.cycle ?? "?"}, end ${row.support.eol ?? "?"}${row.support.ended ? " (ended)" : ""}, current LTS ${row.support.currentLts ?? "?"}`,
      ...(c?.vulns ?? []).map((v) => `${v.id} (${v.severity}) ${v.summary}; fixed in ${v.fixed ?? "none"}`),
    ].filter(Boolean);
    askAI({
      role: "devops",
      chips: [{ id: `stack-${row.key}`, kind: "object", label: t("stackChip", { name: row.name, from: row.version ?? "", to, repo: repo.name }), text: lines.join("\n") }],
      draft,
    });
  }

  function upgradeRuntime(row: Row) {
    const s = row.support;
    if (!s) return;
    const to = s.currentLts ?? s.latest ?? "";
    hand(row, to, t("stackRuntimeDraft", { name: row.name, from: s.version, to, repo: repo.name }));
  }

  async function run() {
    const job = asking;
    if (!job) return;
    error = "";
    const id = await openSession(repo.path, null);
    if (id === null) return;
    try {
      await invoke("pty_write", { id, data: `${job.command}\r` });
      await focusSession(id);
    } catch (e) {
      error = String(e);
    }
  }

  function openUrl(url: string) {
    invoke("open_url", { url }).catch((e) => (error = String(e)));
  }
</script>

<div class="deps">
  <div class="bar">
    <Segments label={t("stackFilterLabel")} options={filters} selected={stack.filter} onselect={(id) => {
        stack.filter = id as Filter;
      }} />
    <label class="search">
      <Search size={14} aria-hidden="true" />
      <input class="field" type="search" placeholder={t("stackSearch")} aria-label={t("stackSearch")} bind:value={stack.query} />
    </label>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  {#if !list.length && check}
    <p class="empty">{t("stackNoDeps")}</p>
  {:else}
    <div class="table" role="table" aria-label={repo.name}>
      <div class="head" role="row">
        <span role="columnheader">{t("stackColPackage")}</span>
        <span role="columnheader">{t("stackColUsed")}</span>
        <span role="columnheader">{t("stackColLatest")}</span>
        <span role="columnheader">{t("stackColSupport")}</span>
        <span role="columnheader">{t("stackColVulns")}</span>
        <span role="columnheader">{t("stackColUpdate")}</span>
      </div>
      {#each shown as row (row.key)}
        {@const c = row.check}
        {@const w = worst(c?.vulns ?? [])}
        {@const end = ending(row.support, today)}
        {@const p = proposal(row)}
        <div class="row" class:vulnerable={!!w} data-severity={w} role="row">
          <span class="pkg" role="cell">
            {#if c?.vulns.length}
              <button class="icon-btn sm toggle" aria-expanded={open === row.key} aria-label={row.name} onclick={() => (open = open === row.key ? null : row.key)}>
                <ChevronRight size={14} aria-hidden="true" />
              </button>
            {:else}
              <span class="toggle-space"></span>
            {/if}
            <span class="pkg-name" title={row.dep?.manifest ?? row.support?.product}>{row.name}</span>
            {#if row.dep?.dev}<span class="pill quiet">{t("stackDev")}</span>{/if}
            {#if !row.dep}<span class="pill quiet">{t("stackRuntime")}</span>{/if}
          </span>
          <span class="mono" role="cell">{row.version ?? "–"}</span>
          <span role="cell" class="latest">
            {#if !check && row.dep?.version}
              <span class="bone" aria-hidden="true"></span>
            {:else if c?.latest}
              <span class="mono">{c.latest}</span>
              {#if c.bump}<span class="pill bump-{c.bump}">{t(bumpKey[c.bump])}</span>{/if}
            {:else if row.support?.latest}
              <span class="mono">{row.support.latest}</span>
            {:else if check && row.dep?.version}
              <span class="muted" title={t("stackNotListed")}>–</span>
            {/if}
          </span>
          <span role="cell" class="support">
            {#if row.support}
              <span class="pill support-{end ?? 'ok'}" title={row.support.currentLts ? t("stackCurrentLts", { cycle: row.support.currentLts }) : undefined}>
                {#if end === "ended"}
                  {row.support.eol ? t("stackEnded", { date: date(row.support.eol) }) : t("stackEndedNoDate")}
                {:else if end === "soon" && row.support.eol}
                  {t("stackEndsSoon", { date: date(row.support.eol) })}
                {:else if row.support.eol}
                  {t("stackUntil", { date: date(row.support.eol) })}
                {:else}
                  {row.support.cycle ?? row.support.version}
                {/if}
              </span>
              {#if row.support.lts}<span class="pill quiet">{t("stackLts")}</span>{/if}
            {/if}
          </span>
          <span role="cell">
            {#if w && c}
              <span class="sev sev-{w}">{t(severityKey[w])}{#if c.vulns.length > 1}<span class="more">+{c.vulns.length - 1}</span>{/if}</span>
            {/if}
          </span>
          <span role="cell" class="act">
            {#if p && c}
              <button class="btn sm" class:primary={!!w && !p.major} onclick={() => update(row)}>
                {#if p.major || !c.command}<Sparkles size={13} aria-hidden="true" />{t("stackGiveDevops")}{:else}<CircleArrowUp size={13} aria-hidden="true" />{t("stackUpdate")} <span class="mono">{p.to}</span>{/if}
              </button>
            {:else if !row.dep && end}
              <button class="btn sm" onclick={() => upgradeRuntime(row)}><Sparkles size={13} aria-hidden="true" />{t("stackGiveDevops")}</button>
            {:else if c?.latest && !c.bump}
              <span class="ok">{t("stackUpToDate")}</span>
            {/if}
          </span>
        </div>
        {#if open === row.key && c}
          <ul class="vulns">
            {#each c.vulns as v (v.id)}
              <li>
                <span class="sev sev-{v.severity}">{t(severityKey[v.severity])}</span>
                <button class="link" onclick={() => openUrl(v.url)}>{v.id}<ExternalLink size={11} aria-hidden="true" /></button>
                <span class="summary">{v.summary}</span>
                <span class="fix">{v.fixed ? t("stackFixedIn", { version: v.fixed }) : t("stackNoFix")}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else}
        {#if !check}
          <Skeleton shape="table" rows={6} label={t("stackChecking")} />
        {:else}
          <p class="empty">{t("stackNoMatch")}</p>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<ConfirmDialog bind:open={confirming} title={t("stackUpdateTitle", { name: asking?.row.name ?? "" })} confirm={t("stackUpdateRun")} onconfirm={run}>
  {#if asking}
    {@const fixes = asking.row.check?.safeFixes ?? []}
    <p>{t("stackUpdateBody", { from: asking.row.version ?? "", to: asking.to, kind: t(bumpKey[asking.row.check?.safeBump ?? "patch"]).toLowerCase(), repo: repo.name })}</p>
    <pre class="command">{asking.command}</pre>
    {#if fixes.length}<p class="fixes">{t("stackUpdateFixes", { ids: fixes.join(", ") })}</p>{/if}
    <p class="after">{t("stackUpdateAfter")}</p>
  {/if}
</ConfirmDialog>

<style>
  .deps {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    width: 220px;
  }

  .search :global(svg) {
    position: absolute;
    left: 9px;
    color: var(--muted);
    pointer-events: none;
  }

  .search input {
    width: 100%;
    padding-left: 30px;
  }

  .table {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--panel);
  }

  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(180px, 1.6fr) minmax(80px, 0.7fr) minmax(130px, 1fr) minmax(150px, 1.1fr) minmax(110px, 0.8fr) minmax(150px, 1fr);
    align-items: center;
    gap: 12px;
    padding: 0 14px;
  }

  .head {
    height: 34px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .row {
    min-height: 42px;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .row:last-child {
    border-bottom: 0;
  }

  .row.vulnerable {
    box-shadow: inset 3px 0 0 var(--border-strong);
  }

  .row[data-severity="critical"],
  .row[data-severity="high"] {
    box-shadow: inset 3px 0 0 var(--danger);
  }

  .row[data-severity="moderate"] {
    box-shadow: inset 3px 0 0 var(--waiting);
  }

  .pkg {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .pkg-name {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toggle :global(svg) {
    transition: transform var(--dur-fast) ease;
  }

  .toggle[aria-expanded="true"] :global(svg) {
    transform: rotate(90deg);
  }

  .toggle-space {
    flex: none;
    width: 24px;
  }

  .mono {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .latest,
  .support,
  .act {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .muted {
    color: var(--muted);
  }

  .pill {
    display: inline-flex;
    align-items: center;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
  }

  .pill.quiet {
    background: var(--well);
    color: var(--muted);
    font-weight: 500;
  }

  .bump-patch,
  .bump-minor {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .bump-major {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .support-ended {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .support-soon {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .support-ok {
    background: var(--done-soft);
    color: var(--done);
  }

  .sev {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: var(--text-xs);
    font-weight: 700;
    white-space: nowrap;
  }

  .sev-critical {
    background: var(--danger);
    color: var(--danger-text);
  }

  .sev-high {
    background: var(--danger-soft);
    color: var(--danger);
    box-shadow: inset 0 0 0 1px var(--danger-line);
  }

  .sev-moderate {
    background: var(--waiting-soft);
    color: var(--warning);
    box-shadow: inset 0 0 0 1px var(--waiting-line);
  }

  .sev-low,
  .sev-unknown {
    background: var(--well);
    color: var(--soft);
  }

  .more {
    opacity: 0.8;
    font-weight: 600;
  }

  .ok {
    color: var(--done);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .bone {
    width: 64px;
    height: 12px;
    border-radius: var(--radius-pill);
    background: var(--well);
  }

  .vulns {
    margin: 0;
    padding: 4px 14px 10px 50px;
    border-bottom: 1px solid var(--border);
    background: var(--well);
    list-style: none;
  }

  .vulns li {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    font-size: var(--text-sm);
  }

  .vulns li + li {
    border-top: 1px solid var(--border);
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .link:hover {
    text-decoration: underline;
  }

  .summary {
    overflow: hidden;
    color: var(--soft);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fix {
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .empty,
  .error {
    margin: 0;
    padding: 18px 14px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .error {
    color: var(--danger);
  }

  .command {
    margin: 8px 0;
    padding: 10px 12px;
    overflow-x: auto;
    border-radius: var(--radius);
    background: var(--term-bg);
    color: #e8e8ef;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    white-space: pre-wrap;
    word-break: break-all;
  }

  .fixes,
  .after {
    color: var(--muted);
    font-size: var(--text-sm);
  }
</style>
