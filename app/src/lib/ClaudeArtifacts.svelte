<script lang="ts">
  import { approxUsd } from "$lib/usage.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ChevronsUpDown from "@lucide/svelte/icons/chevrons-up-down";
  import CircleStop from "@lucide/svelte/icons/circle-stop";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Images from "@lucide/svelte/icons/images";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Search from "@lucide/svelte/icons/search";
  import SettingsIcon from "@lucide/svelte/icons/settings";
  import Share2 from "@lucide/svelte/icons/share-2";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { focusSession } from "$lib/commands";
  import { ago, lang, t } from "$lib/i18n";
  import { projectsOf } from "$lib/nav-logic";
  import { showSettings } from "$lib/settings.svelte";
  import { app, repoName, type Session } from "$lib/state.svelte";
  import { tickets } from "$lib/tickets.svelte";
  import { GALLERY, claudeView, copyText, openUrl, type Artifact, type Artifacts, type Failure } from "$lib/claude.svelte";
  import {
    filterArtifacts,
    nextSort,
    selection,
    sortArtifacts,
    toggleAll,
    updatedDay,
    type Filter,
    type Sort,
    type SortKey,
  } from "$lib/claude-logic";

  const data = $derived(claudeView.artifacts);
  const refresh = $derived(data?.refresh);
  const deletion = $derived(data?.deletion);
  const projects = $derived(app.workspace?.projects ?? []);
  let busy = $state("");

  let filter = $state<Filter>({ text: "", owner: "", project: "" });
  let sort = $state<Sort>({ key: "updated", dir: "desc" });
  let selected = $state<string[]>([]);
  const origin = (a: Artifact) => (a.cwd ? repoName(a.cwd) : "");
  const rows = $derived(
    data
      ? sortArtifacts(
          filterArtifacts(data.artifacts, filter, (cwd) => projectsOf(cwd, projects, tickets.runs)),
          sort,
          origin,
        )
      : [],
  );
  const chosen = $derived(rows.filter((a) => selected.includes(a.id)));
  const picked = $derived(selection(selected, rows));
  const filtered = $derived(!!(filter.text || filter.owner || filter.project));
  let allBox = $state<HTMLInputElement>();
  $effect(() => {
    if (allBox) allBox.indeterminate = picked === "some";
  });

  /** The deletion's result stays on screen until dismissed. */
  let dismissed = $state(-1);
  const outcome = $derived(
    new Map((deletion && deletion.started !== dismissed ? deletion.items : []).map((d) => [d.id, d])),
  );
  const finished = $derived(
    deletion && !deletion.running && deletion.items.length && deletion.started !== dismissed ? deletion : null,
  );
  const failedCount = $derived(finished?.items.filter((d) => d.outcome === "failed").length ?? 0);

  let confirming = $state<Artifact[] | null>(null);
  let dialog = $state<HTMLDialogElement>();
  $effect(() => {
    if (confirming && dialog && !dialog.open) dialog.showModal();
  });

  const failures: Record<Failure, Parameters<typeof t>[0]> = {
    toolMissing: "claudeViewFailToolMissing",
    notStarted: "claudeViewFailNotStarted",
    timeout: "claudeViewFailTimeout",
    budget: "claudeViewFailBudget",
    needsYou: "claudeViewFailNeedsYou",
    closed: "claudeViewFailClosed",
    invalid: "claudeViewFailInvalid",
    spawn: "claudeViewFailSpawn",
    cancelled: "claudeViewFailCancelled",
  };
  const deleteFailures: Partial<Record<Failure, Parameters<typeof t>[0]>> = {
    ...failures,
    budget: "claudeRedesignFailBudget",
  };

  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD", maximumFractionDigits: 3 });
  const cost = (usd: number | null) => (usd === null ? "" : t("claudeViewCost", { cost: approxUsd(money.format(usd)) }));

  /** The account's date when it reads as one, as the account gave it otherwise. */
  function updated(a: Artifact): { text: string; title: string } | null {
    if (!a.updated) return null;
    const day = updatedDay(a.updated, lang);
    return day ? { text: day.text, title: ago(day.at / 1000) } : { text: a.updated, title: a.updated };
  }

  const columns: { key: SortKey; label: Parameters<typeof t>[0] }[] = [
    { key: "title", label: "claudeRedesignTitle" },
    { key: "origin", label: "claudeViewRepo" },
    { key: "owner", label: "claudeRedesignOwner" },
    { key: "updated", label: "claudeRedesignUpdated" },
  ];
  const owners: [Filter["owner"], Parameters<typeof t>[0]][] = [
    ["", "claudeRedesignAll"],
    ["me", "claudeViewMine"],
    ["shared", "claudeViewShared"],
  ];
  const ariaSort = (key: SortKey) =>
    sort.key !== key ? "none" : sort.dir === "asc" ? "ascending" : "descending";

  async function call(name: string, args: Record<string, unknown> = {}): Promise<boolean> {
    busy = name;
    try {
      claudeView.artifacts = await invoke<Artifacts>(name, args);
      return true;
    } catch (e) {
      claudeView.error = String(e);
      return false;
    } finally {
      busy = "";
    }
  }

  async function edit(artifact: Artifact) {
    busy = `edit:${artifact.id}`;
    try {
      const s = await invoke<Session>("artifacts_edit", { url: artifact.url });
      if (!app.sessions.some((o) => o.id === s.id)) app.sessions.push(s);
      await focusSession(s.id);
    } catch (e) {
      claudeView.error = String(e);
    } finally {
      busy = "";
    }
  }

  function share(artifact: Artifact) {
    openUrl(artifact.url);
    claudeView.done = t("claudeViewShareOpened");
  }

  /** TermF asks first, unless you chose yourself not to be asked. */
  function askDelete(list: Artifact[]) {
    if (!list.length) return;
    if (data?.deleteWithoutConfirmation) remove(list, false);
    else confirming = list;
  }

  async function remove(list: Artifact[], confirmed: boolean) {
    dialog?.close();
    const ids = list.map((a) => a.id);
    if (await call("artifacts_delete", { urls: list.map((a) => a.url), confirmed }))
      selected = selected.filter((id) => !ids.includes(id));
  }
</script>

<div class="artifacts">
  <header class="page-head">
    <div class="titles">
      <h2>{t("claudeViewTabArtifacts")}</h2>
      <p class="lead">{t("claudeViewArtifactsHelp")}</p>
    </div>
    <div class="head-actions">
      <button class="btn sm" onclick={() => openUrl(GALLERY)}>
        <Images size={14} aria-hidden="true" />{t("claudeViewGallery")}
      </button>
      {#if refresh?.running}
        <button class="btn sm" onclick={() => call("artifacts_cancel")} disabled={!!busy}>
          <CircleStop size={14} aria-hidden="true" />{t("claudeViewStop")}
        </button>
      {:else}
        <button class="btn sm primary" onclick={() => call("artifacts_refresh")} disabled={!!busy}>
          <RefreshCw size={14} aria-hidden="true" />{t("claudeViewArtifactsRefresh")}
        </button>
      {/if}
      <button class="btn sm quiet" onclick={() => showSettings("claude")} title={t("claudeViewAuto", { hours: data?.refreshHours || 6 })}>
        <SettingsIcon size={14} aria-hidden="true" />{t("settings")}
      </button>
    </div>
  </header>

  <div class="status">
    {#if refresh?.running && refresh.revealed}
      <Banner kind="warn" icon={SquareTerminal}>
        {t("claudeViewNeedsYou")}
        {#snippet action()}
          {#if refresh?.session != null}
            <button class="btn sm" onclick={() => refresh?.session != null && focusSession(refresh.session)}>{t("claudeViewShowTile")}</button>
          {/if}
        {/snippet}
      </Banner>
    {:else if refresh?.running}
      <p class="notice" role="status">
        <LoaderCircle size={14} aria-hidden="true" class="spin" />
        <span>{t("claudeViewListing")}{refresh.cost !== null ? ` · ${cost(refresh.cost)}` : ""}</span>
      </p>
    {:else if refresh?.last?.failure}
      <Banner kind="error">{t(failures[refresh.last.failure], { detail: refresh.last.detail ?? "" })}</Banner>
    {:else if data?.listed}
      <p class="listed">
        {t("claudeViewListed", { ago: ago(data.listed), count: refresh?.last?.count ?? data.artifacts.length })}{refresh?.last?.cost != null
          ? ` · ${cost(refresh.last.cost)}`
          : ""}
      </p>
    {/if}

    {#if deletion?.running && deletion.revealed}
      <Banner kind="warn" icon={SquareTerminal}>
        {t("claudeViewNeedsYou")}
        {#snippet action()}
          {#if deletion?.session != null}
            <button class="btn sm" onclick={() => deletion?.session != null && focusSession(deletion.session)}>{t("claudeViewShowTile")}</button>
          {/if}
        {/snippet}
      </Banner>
    {:else if deletion?.running}
      <p class="notice" role="status">
        <LoaderCircle size={14} aria-hidden="true" class="spin" />
        <span>{t("claudeRedesignDeletingNotice", { count: deletion.items.length })}</span>
      </p>
    {:else if finished}
      {@const why = finished.failure && deleteFailures[finished.failure]}
      <Banner kind={failedCount ? "error" : "ok"} ondismiss={() => (dismissed = finished.started)}>
        {failedCount
          ? t("claudeRedesignNotDeleted", { count: failedCount })
          : t("claudeRedesignDeleted", { count: finished.items.length })}
        {#if why}{" "}{t(why, { detail: finished.detail ?? "" })}{/if}
      </Banner>
    {/if}
  </div>

  {#if !data}
    <Skeleton shape="list" rows={6} />
  {:else if !data.artifacts.length}
    <EmptyState icon={Images} title={t("claudeViewNoArtifacts")} text={t("claudeViewNoArtifactsHelp")} />
  {:else}
    <div class="toolbar">
      <div class="search">
        <Search size={14} aria-hidden="true" />
        <input
          class="field"
          type="search"
          placeholder={t("navSearch")}
          aria-label={t("claudeRedesignFilter")}
          bind:value={filter.text}
          spellcheck="false"
          autocomplete="off"
        />
      </div>
      <div class="segmented" role="group" aria-label={t("claudeRedesignOwnerFilter")}>
        {#each owners as [owner, label] (owner)}
          <button aria-pressed={filter.owner === owner} onclick={() => (filter.owner = owner)}>{t(label)}</button>
        {/each}
      </div>
      {#if projects.length}
        <select class="field project" bind:value={filter.project} aria-label={t("claudeRedesignProjectFilter")}>
          <option value="">{t("ticketsAll")}</option>
          {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      {/if}
      {#if chosen.length}
        <div class="bulk" role="group" aria-label={t("claudeRedesignSelected", { count: chosen.length })}>
          <span class="count">{t("claudeRedesignSelected", { count: chosen.length })}</span>
          <button class="btn sm danger-quiet" onclick={() => askDelete(chosen)} disabled={!!busy || deletion?.running}>
            <Trash2 size={14} aria-hidden="true" />{t("claudeViewDelete")}
          </button>
          <button class="icon-btn" onclick={() => (selected = [])} aria-label={t("claudeRedesignClearSelection")} title={t("claudeRedesignClearSelection")}>
            <X size={15} aria-hidden="true" />
          </button>
        </div>
      {/if}
    </div>

    <div class="table">
      <table aria-label={t("claudeViewTabArtifacts")}>
        <thead>
          <tr>
            <th class="check">
              <input
                type="checkbox"
                bind:this={allBox}
                checked={picked === "all"}
                disabled={!rows.length}
                onchange={() => (selected = toggleAll(selected, rows))}
                aria-label={t("claudeRedesignSelectAll")}
              />
            </th>
            {#each columns as col (col.key)}
              <th class={col.key} aria-sort={ariaSort(col.key)}>
                <button class="sort" class:on={sort.key === col.key} onclick={() => (sort = nextSort(sort, col.key))}>
                  {t(col.label)}
                  {#if sort.key !== col.key}
                    <ChevronsUpDown size={13} aria-hidden="true" />
                  {:else if sort.dir === "asc"}
                    <ArrowUp size={13} aria-hidden="true" />
                  {:else}
                    <ArrowDown size={13} aria-hidden="true" />
                  {/if}
                </button>
              </th>
            {/each}
            <th class="actions"><span class="sr-only">{t("claudeRedesignActions")}</span></th>
          </tr>
        </thead>
        <tbody>
          {#each rows as a (a.id)}
            {@const state = outcome.get(a.id)}
            {@const pending = state?.outcome === "pending" && deletion?.running}
            {@const when = updated(a)}
            <tr class:chosen={selected.includes(a.id)} class:pending>
              <td class="check">
                <input
                  type="checkbox"
                  checked={selected.includes(a.id)}
                  disabled={pending}
                  onchange={(e) =>
                    (selected = e.currentTarget.checked ? [...selected, a.id] : selected.filter((id) => id !== a.id))}
                  aria-label={t("claudeRedesignSelect", { title: a.title ?? t("claudeViewUntitled") })}
                />
              </td>
              <td class="title">
                <span class="name" class:untitled={!a.title} title={a.title ?? a.url}>{a.title ?? t("claudeViewUntitled")}</span>
                {#if state?.outcome === "failed" && !deletion?.running}
                  <span class="row-error">{state.error ? t("claudeRedesignRowFailedWhy", { error: state.error }) : t("claudeRedesignRowFailed")}</span>
                {/if}
              </td>
              <td class="origin" title={a.cwd ?? ""}>{#if a.cwd}{origin(a)}{:else}<span class="none">—</span>{/if}</td>
              <td class="owner">
                {#if a.owner}
                  <span class="badge" class:accent={a.owner === "me"}>{a.owner === "me" ? t("claudeViewMine") : t("claudeViewShared")}</span>
                {:else}<span class="none">—</span>{/if}
              </td>
              <td class="updated" title={when?.title ?? ""}>{#if when}{when.text}{:else}<span class="none">—</span>{/if}</td>
              <td class="actions">
                {#if pending}
                  <span class="deleting" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("claudeRedesignDeleting")}</span>
                {:else}
                  <div class="row-actions">
                    <button class="icon-btn" onclick={() => openUrl(a.url)} aria-label={t("claudeViewOpen")} title={t("claudeViewOpen")}>
                      <ExternalLink size={15} aria-hidden="true" />
                    </button>
                    <button class="icon-btn" onclick={() => copyText(a.url, t("claudeViewLinkCopied"))} aria-label={t("claudeViewCopyLink")} title={t("claudeViewCopyLink")}>
                      <Copy size={15} aria-hidden="true" />
                    </button>
                    <button class="icon-btn" onclick={() => share(a)} aria-label={t("claudeViewShare")} title={t("claudeViewShareTitle")}>
                      <Share2 size={15} aria-hidden="true" />
                    </button>
                    <button class="icon-btn" onclick={() => edit(a)} disabled={!!busy} aria-label={t("claudeViewEditArtifact")} title={t("claudeViewEditArtifactTitle")}>
                      <Pencil size={15} aria-hidden="true" />
                    </button>
                    <button class="icon-btn delete" onclick={() => askDelete([a])} disabled={!!busy || deletion?.running} aria-label={t("claudeViewDelete")} title={t("claudeViewDelete")}>
                      <Trash2 size={15} aria-hidden="true" />
                    </button>
                  </div>
                {/if}
              </td>
            </tr>
          {:else}
            <tr>
              <td colspan="6" class="no-match">
                {t("claudeRedesignNoMatch")}
                {#if filtered}
                  <button class="btn sm quiet" onclick={() => (filter = { text: "", owner: "", project: "" })}>{t("claudeRedesignClearFilters")}</button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<dialog bind:this={dialog} class="confirm" aria-labelledby="artifacts-delete-title" onclose={() => (confirming = null)}>
  {#if confirming}
    <h2 id="artifacts-delete-title">
      {confirming.length === 1
        ? t("claudeRedesignDeleteOne", { title: confirming[0].title ?? t("claudeViewUntitled") })
        : t("claudeRedesignDeleteMany", { count: confirming.length })}
    </h2>
    {#if confirming.length > 1}
      <ul class="doomed">
        {#each confirming.slice(0, 5) as a (a.id)}<li>{a.title ?? t("claudeViewUntitled")}</li>{/each}
        {#if confirming.length > 5}<li class="more">{t("claudeRedesignDeleteMore", { count: confirming.length - 5 })}</li>{/if}
      </ul>
    {/if}
    <p>{t("claudeRedesignDeleteText")}</p>
    <div class="answers">
      <button class="btn" onclick={() => dialog?.close()}>{t("cancel")}</button>
      <button class="btn danger" onclick={() => confirming && remove(confirming, true)}>
        <Trash2 size={14} aria-hidden="true" />{t("claudeRedesignDeleteConfirm")}
      </button>
    </div>
  {/if}
</dialog>

<style>
  .artifacts {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .page-head {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 12px 24px;
  }

  .titles {
    flex: 1 1 360px;
    min-width: 0;
  }

  h2 {
    margin: 0 0 4px;
    font-size: var(--text-xl);
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .lead {
    max-width: 68ch;
    margin: 0;
    color: var(--muted);
  }

  .head-actions {
    display: flex;
    gap: 8px;
  }

  .status {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .status:empty {
    display: none;
  }

  .notice {
    align-items: center;
  }

  .listed {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .search {
    position: relative;
    flex: 0 1 260px;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    top: 8px;
    left: 9px;
    pointer-events: none;
  }

  .search .field {
    padding-left: 29px;
  }

  .segmented button {
    height: 24px;
  }

  .project {
    width: auto;
    max-width: 200px;
  }

  .bulk {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    padding: 2px 2px 2px 10px;
    border-radius: var(--radius);
    background: var(--accent-soft);
  }

  .bulk .count {
    margin-right: 4px;
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  .danger-quiet {
    border-color: transparent;
    background: var(--panel);
    color: var(--danger);
  }

  .table {
    container-type: inline-size;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    overflow: hidden;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    table-layout: fixed;
  }

  th.check {
    width: 40px;
  }

  th.origin {
    width: 18%;
  }

  th.owner {
    width: 150px;
  }

  th.updated {
    width: 116px;
  }

  th.actions {
    width: 152px;
  }

  /* A narrow window keeps the title readable: the owner, then the folder, give way (their filters stay). */
  @container (max-width: 760px) {
    .owner {
      display: none;
    }
  }

  @container (max-width: 620px) {
    .origin {
      display: none;
    }
  }

  th {
    height: 34px;
    padding: 0 10px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
  }

  th.check,
  td.check {
    padding: 0 0 0 14px;
  }

  .sort {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    margin-left: -6px;
    padding: 4px 6px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: inherit;
    font-weight: inherit;
  }

  .sort:hover,
  .sort.on {
    color: var(--text);
  }

  .sort:hover {
    background: var(--hover);
  }

  .sort :global(svg) {
    flex-shrink: 0;
    opacity: 0.55;
  }

  .sort.on :global(svg) {
    color: var(--accent-ink);
    opacity: 1;
  }

  td {
    height: 44px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  tbody tr:hover td {
    background: var(--hover);
  }

  tbody tr.chosen td {
    background: var(--accent-soft);
  }

  tr.pending td:not(.actions) {
    color: var(--muted);
  }

  .title {
    white-space: normal;
  }

  .name {
    display: block;
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.untitled {
    color: var(--muted);
    font-style: italic;
    font-weight: 400;
  }

  .row-error {
    display: block;
    overflow: hidden;
    color: var(--danger);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  td.origin {
    color: var(--muted);
  }

  td.updated {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .none {
    color: var(--border-strong);
  }

  td.actions {
    padding: 0 8px 0 0;
    text-align: right;
  }

  .row-actions {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }

  .icon-btn.delete:hover:not(:disabled) {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .deleting {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding-right: 6px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .no-match {
    height: 64px;
    color: var(--muted);
    text-align: center;
  }

  .no-match .btn {
    margin-left: 6px;
  }

  .confirm {
    width: min(440px, calc(100vw - 48px));
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .confirm[open] {
    animation: rise var(--dur) var(--ease-out);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.98);
    }
  }

  .confirm::backdrop {
    background: var(--scrim, rgb(10 12 20 / 0.3));
  }

  .confirm h2 {
    margin: 0 0 12px;
    font-size: var(--text-lg);
    overflow-wrap: anywhere;
  }

  .doomed {
    margin: 0 0 12px;
    padding: 8px 12px 8px 28px;
    border-radius: var(--radius);
    background: var(--hover);
  }

  .doomed li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .doomed .more {
    color: var(--muted);
    list-style: none;
  }

  .confirm p {
    margin: 0 0 20px;
    color: var(--muted);
  }

  .answers {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
