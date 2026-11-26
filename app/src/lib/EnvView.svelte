<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleDashed from "@lucide/svelte/icons/circle-dashed";
  import CircleX from "@lucide/svelte/icons/circle-x";
  import Copy from "@lucide/svelte/icons/copy";
  import Download from "@lucide/svelte/icons/download";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import PackageCheck from "@lucide/svelte/icons/package-check";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import ScanSearch from "@lucide/svelte/icons/scan-search";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import SyncInvite from "$lib/SyncInvite.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { askAI, askedProject, provide } from "$lib/ask.svelte";
  import { GROUPS, MANAGERS, blank, clean, fresh, groupOf, keptByDefault, keyOf, lines, missing, stateOf, type Entry, type Found, type Group, type Manager } from "$lib/devenv-logic";
  import { addEntries, deleteScript, devenv, install, removeEntries, saveScript, scan, updateEntry } from "$lib/devenv.svelte";
  import { t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";

  type Key = Parameters<typeof t>[0];

  const GROUP_LABEL: Record<Group, Key> = {
    languages: "myenvGroupLanguages",
    cli: "myenvGroupCli",
    apps: "myenvGroupApps",
    editors: "myenvGroupEditors",
  };
  const SOURCE_LABEL: Record<string, Key> = { bins: "myenvSourceBins", apps: "myenvSourceApps", checks: "myenvSourceChecks" };
  const SOURCE_NAME: Record<string, string> = { brew: "Homebrew", gh: "GitHub CLI", rustup: "rustup", volta: "Volta" };
  const managerName = (m: Manager) => (m === "shell" ? t("myenvManagerShell") : m === "manual" ? t("myenvManagerManual") : m);
  const sourceName = (s: string) => (SOURCE_LABEL[s] ? t(SOURCE_LABEL[s]) : (SOURCE_NAME[s] ?? s));

  const windows = navigator.platform.startsWith("Win");
  const entries = $derived(devenv.manifest?.entries ?? []);
  const news = $derived(fresh(devenv.found, entries));
  const lacking = $derived(missing(entries, devenv.found, devenv.scanned));
  const canAsk = $derived(askedProject() !== null);

  // ⌘I: the devops agent answers here, with the list and what the detection found, by name only.
  $effect(() =>
    provide("env", {
      role: "devops",
      chips: [
        ...(entries.length ? [{ id: "env-manifest", kind: "object" as const, label: t("myenvChipManifest", { n: entries.length }), text: lines(entries) }] : []),
        ...(news.length ? [{ id: "env-found", kind: "object" as const, label: t("myenvChipFound", { n: news.length }), text: lines(news) }] : []),
      ],
    }),
  );

  const askTriage = () => void askAI({ role: "devops", draft: t("myenvAskDraft") });
  const askScript = () => void askAI({ role: "devops", draft: t("myenvScriptDraft") });

  async function attempt(run: () => Promise<unknown>): Promise<boolean> {
    devenv.error = "";
    try {
      await run();
      return true;
    } catch (e) {
      devenv.error = String(e);
      return false;
    }
  }

  function detect() {
    devenv.tab = "found";
    scan().catch(() => {});
  }

  // ---------------------------------------------------------------- the list

  let filter = $state<"all" | "missing">("all");
  const shown = $derived(filter === "missing" ? lacking : entries);

  /** The form: a new entry (`previous` null), or the entry `previous` names. */
  let form = $state<{ previous: Entry | null; entry: Entry } | null>(null);
  let saving = $state(false);

  function edit(entry: Entry | null) {
    devenv.tab = "manifest";
    form = { previous: entry, entry: entry ? { ...entry } : blank("brew") };
  }

  function pickManager(m: Manager) {
    if (!form) return;
    // A group still at the old manager's default follows the new one.
    if (form.entry.group === groupOf(form.entry.manager)) form.entry.group = groupOf(m);
    form.entry.manager = m;
  }

  const ownCommand = $derived(form?.entry.manager === "shell" || form?.entry.manager === "manual");
  const formReady = $derived(!!form?.entry.name.trim() && (form.entry.manager !== "shell" || !!form.entry.install?.trim()) && (form.entry.manager !== "mas" || !!form.entry.id?.trim()));

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!form || !formReady) return;
    const { previous } = form;
    const entry = clean(form.entry);
    saving = true;
    const ok = await attempt(() => (previous ? updateEntry(previous, entry) : addEntries([entry])));
    saving = false;
    if (!ok) return;
    form = null;
    toast(t("myenvSaved", { name: entry.name }));
  }

  function toggle(entry: Entry) {
    void attempt(() => updateEntry(entry, { ...entry, active: !entry.active }));
  }

  async function remove(entry: Entry) {
    if (!(await attempt(() => removeEntries([entry])))) return;
    toast(t("myenvRemoved", { name: entry.name }), { undo: () => void attempt(() => addEntries([entry])) });
  }

  // ---------------------------------------------------------------- what the detection found

  /** The user's ticks, by key; untouched, an entry is ticked when a manager installed it. */
  let ticks = $state<Record<string, boolean>>({});
  const ticked = (f: Found) => ticks[keyOf(f)] ?? keptByDefault(f);
  const chosen = $derived(news.filter(ticked));
  const sources = $derived(Object.entries(devenv.sources));

  async function keep() {
    const n = chosen.length;
    if (!(await attempt(() => addEntries(chosen)))) return;
    ticks = {};
    devenv.tab = "manifest";
    toast(t("myenvKept", { n }));
  }

  function tickAll(list: Found[], on: boolean) {
    for (const f of list) ticks[keyOf(f)] = on;
  }

  // ---------------------------------------------------------------- the script

  /** The script shown: the one written from the list (""), or a named one. */
  let script = $state("");
  let text = $state<string | null>(null);
  let copied = $state(false);
  const scripts = $derived(devenv.manifest?.scripts ?? []);

  $effect(() => {
    if (script && !scripts.includes(script)) script = "";
  });

  $effect(() => {
    if (devenv.tab !== "script") return;
    // Read again when the list changes: the script is written from it.
    void devenv.manifest;
    const name = script;
    text = null;
    invoke<string>("env_script", { name: name || null }).then(
      (s) => name === script && (text = s),
      (e) => (devenv.error = String(e)),
    );
  });

  async function copy() {
    await navigator.clipboard.writeText(text ?? "");
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  async function dropScript(name: string) {
    const kept = text ?? "";
    if (!(await attempt(() => deleteScript(name)))) return;
    toast(t("myenvScriptDeleted", { name }), { undo: () => void attempt(() => saveScript(name, kept)) });
  }

  // ---------------------------------------------------------------- install

  /** The script whose run waits for the user's yes: null for the one written from the list. */
  let running = $state<{ name: string | null } | null>(null);
  let confirming = $state(false);

  function askInstall(name: string | null) {
    running = { name };
    confirming = true;
  }

  async function go() {
    const name = running?.name ?? null;
    if (await attempt(() => install(name))) toast(t("myenvStarted"));
  }
</script>

{#snippet stateBadge(entry: Entry)}
  {@const state = stateOf(entry, devenv.found, devenv.scanned)}
  {#if !entry.active}
    <span class="state off">{t("myenvOff")}</span>
  {:else if state === "there"}
    <span class="state ok"><Check size={12} aria-hidden="true" />{t("myenvThere")}</span>
  {:else if state === "missing"}
    <span class="state missing"><CircleX size={12} aria-hidden="true" />{t("myenvMissing")}</span>
  {:else if devenv.scanning}
    <span class="state"><LoaderCircle size={12} aria-hidden="true" class="spin" />{t("myenvChecking")}</span>
  {/if}
{/snippet}

{#snippet who(entry: Entry & { installed?: string | null })}
  <span class="who">
    <b class="name mono">{entry.name}</b>
    {#if entry.version ?? entry.installed}<span class="version mono">{entry.version ?? entry.installed}</span>{/if}
    <span class="badge">{managerName(entry.manager)}</span>
  </span>
{/snippet}

<div class="env">
  <ViewHeader title={t("myenvView")} context={devenv.manifest ? shortPath(devenv.manifest.file) : ""} mono>
    {#snippet actions()}
      <button class="btn sm" onclick={detect} disabled={devenv.scanning}>
        {#if devenv.scanning}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("myenvScanning")}{:else}<ScanSearch size={14} aria-hidden="true" />{t("myenvScan")}{/if}
      </button>
      <button class="btn sm" onclick={() => edit(null)}><Plus size={14} aria-hidden="true" />{t("myenvAdd")}</button>
      {#if canAsk}
        <button class="btn sm" onclick={askScript}><Sparkles size={14} aria-hidden="true" />{t("myenvAskScript")}</button>
      {/if}
      <button class="btn sm primary" onclick={() => askInstall(null)} disabled={windows || !entries.length} title={windows ? t("myenvWindows") : undefined}>
        <Download size={14} aria-hidden="true" />{t("myenvInstall")}
      </button>
    {/snippet}
  </ViewHeader>
  <SyncInvite text="syncVisibleInviteEnv" />

  <Tabs
    id="env"
    label={t("myenvView")}
    tabs={[
      { id: "manifest", label: t("myenvTabManifest"), count: devenv.manifest ? entries.length : null },
      { id: "found", label: t("myenvTabFound"), count: devenv.scanned || devenv.scanning ? news.length : null },
      { id: "script", label: t("myenvTabScript") },
    ]}
    selected={devenv.tab}
    onselect={(tab) => {
      devenv.tab = tab as typeof devenv.tab;
      devenv.error = "";
    }}
  >
    <div class="body">
      {#if devenv.error}<Banner kind="error" ondismiss={() => (devenv.error = "")}>{devenv.error}</Banner>{/if}

      {#if devenv.tab === "manifest"}
        {#if form}
          <form class="form" onsubmit={submit} aria-label={form.previous ? t("myenvFormEdit", { name: form.previous.name }) : t("myenvFormNew")}>
            <h2>{form.previous ? t("myenvFormEdit", { name: form.previous.name }) : t("myenvFormNew")}</h2>
            <div class="grid">
              <label class="labelled">
                {t("myenvFieldManager")}
                <select class="field" value={form.entry.manager} onchange={(e) => pickManager(e.currentTarget.value as Manager)}>
                  {#each MANAGERS as m (m)}<option value={m}>{managerName(m)}</option>{/each}
                </select>
              </label>
              <label class="labelled">
                {t("myenvFieldGroup")}
                <select class="field" bind:value={form.entry.group}>
                  {#each GROUPS as g (g)}<option value={g}>{t(GROUP_LABEL[g])}</option>{/each}
                </select>
              </label>
              <label class="labelled">
                {t("myenvFieldName")}
                <input class="field mono" bind:value={form.entry.name} placeholder={form.entry.manager === "gh" || form.entry.manager === "tap" ? "owner/repo" : ""} spellcheck="false" autocomplete="off" />
              </label>
              {#if form.entry.manager === "mas"}
                <label class="labelled">
                  {t("myenvFieldId")}
                  <input class="field mono" bind:value={form.entry.id} inputmode="numeric" />
                </label>
              {:else}
                <label class="labelled">
                  {t("myenvFieldVersion")}
                  <input class="field mono" bind:value={form.entry.version} spellcheck="false" autocomplete="off" />
                  <span class="note">{t("myenvFieldVersionHelp")}</span>
                </label>
              {/if}
            </div>
            {#if ownCommand}
              <label class="labelled">
                {t("myenvFieldInstall")}
                <textarea class="field mono" rows="2" bind:value={form.entry.install} spellcheck="false" placeholder="curl -fsSL https://… | sh"></textarea>
                <span class="note">{t("myenvFieldInstallHelp")}</span>
              </label>
              <label class="labelled">
                {t("myenvFieldPath")}
                <input class="field mono" bind:value={form.entry.path} spellcheck="false" placeholder="~/.local/bin/…" />
                <span class="note">{t("myenvFieldPathHelp")}</span>
              </label>
            {/if}
            <label class="labelled">
              {t("myenvFieldDescription")}
              <input class="field" bind:value={form.entry.description} />
              <span class="note">{t("myenvFieldDescriptionHelp")}</span>
            </label>
            <div class="row-actions">
              <button class="btn sm primary" type="submit" disabled={!formReady || saving}>
                {#if saving}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("save")}
              </button>
              <button class="btn sm" type="button" onclick={() => (form = null)}>{t("cancel")}</button>
            </div>
          </form>
        {/if}

        {#if !devenv.manifest}
          {#if !devenv.error}<Skeleton shape="list" rows={6} label={t("myenvLoading")} />{/if}
        {:else if !entries.length}
          {#if !form}
            <EmptyState
              icon={PackageCheck}
              title={t("myenvEmpty")}
              text={t("myenvEmptyHelp")}
              action={{ label: t("myenvDetect"), icon: ScanSearch, run: detect }}
              onask={canAsk ? askTriage : undefined}
              askLabel={t("myenvAsk")}
            />
          {/if}
        {:else}
          <div class="toolbar">
            {#if devenv.scanned}
              <Segments
                label={t("myenvFilter")}
                options={[
                  { id: "all", label: t("myenvFilterAll"), count: entries.length },
                  { id: "missing", label: t("myenvFilterMissing"), count: lacking.length },
                ]}
                selected={filter}
                onselect={(id) => (filter = id as typeof filter)}
              />
              {#if lacking.length}
                <p class="summary warn"><TriangleAlert size={14} aria-hidden="true" />{t("myenvMissingCount", { n: lacking.length })}</p>
              {:else}
                <p class="summary ok"><Check size={14} aria-hidden="true" />{t("myenvAllThere")}</p>
              {/if}
            {:else if devenv.scanning}
              <p class="summary"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("myenvCheckingHere")}</p>
            {/if}
          </div>
          {#each GROUPS as group (group)}
            {@const list = shown.filter((e) => e.group === group)}
            {#if list.length}
              <section class="group" aria-labelledby="env-group-{group}">
                <h3 id="env-group-{group}">{t(GROUP_LABEL[group])}<span class="count">{list.length}</span></h3>
                <ul class="rows">
                  {#each list as entry (keyOf(entry))}
                    <li class:off={!entry.active}>
                      <input type="checkbox" checked={entry.active} onchange={() => toggle(entry)} aria-label={t("myenvInScript", { name: entry.name })} />
                      <div class="main">
                        {@render who(entry)}
                        {#if entry.description}<p class="desc">{entry.description}</p>{/if}
                        {#if entry.install}
                          <code class="cmd">{entry.install}</code>
                        {:else if entry.manager === "manual"}
                          <p class="desc hint">{t("myenvNoInstall")}</p>
                        {/if}
                      </div>
                      {@render stateBadge(entry)}
                      <div class="tools">
                        <button class="icon-btn sm" onclick={() => edit(entry)} aria-label={t("myenvEditName", { name: entry.name })} title={t("myenvEditName", { name: entry.name })}>
                          <Pencil size={14} aria-hidden="true" />
                        </button>
                        <button class="icon-btn sm" onclick={() => remove(entry)} aria-label={t("myenvDeleteName", { name: entry.name })} title={t("myenvDeleteName", { name: entry.name })}>
                          <Trash2 size={14} aria-hidden="true" />
                        </button>
                      </div>
                    </li>
                  {/each}
                </ul>
              </section>
            {/if}
          {/each}
          {#if filter === "missing" && !lacking.length}
            <EmptyState icon={Check} title={t("myenvAllThere")} action={{ label: t("myenvFilterAll"), run: () => (filter = "all") }} />
          {/if}
        {/if}
      {:else if devenv.tab === "found"}
        {#if !devenv.scanning && !devenv.scanned}
          <EmptyState
            icon={ScanSearch}
            title={t("myenvFoundBefore")}
            text={t("myenvFoundBeforeHelp")}
            action={{ label: t("myenvDetect"), icon: ScanSearch, run: detect }}
            onask={canAsk ? askTriage : undefined}
            askLabel={t("myenvAsk")}
          />
        {:else}
          {#if sources.length}
            <ul class="sources" aria-label={t("myenvSources")} aria-live="polite">
              {#each sources as [source, done] (source)}
                <li class:fail={!!done?.error} title={done?.error ?? undefined}>
                  {#if !done}<LoaderCircle size={12} aria-hidden="true" class="spin" />{:else if done.error}<CircleX size={12} aria-hidden="true" />{:else}<Check size={12} aria-hidden="true" />{/if}
                  <span>{sourceName(source)}</span>
                  {#if done && !done.error}<span class="n">{done.found.length}</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
          {#each sources.filter(([, done]) => done?.error) as [source, done] (source)}
            <p class="notice error"><CircleX size={14} aria-hidden="true" /><span>{sourceName(source)} · {done?.error}</span></p>
          {/each}

          {#if news.length}
            <div class="found-head">
              <p class="help">{t("myenvFoundHelp")}</p>
              <div class="row-actions">
                {#if canAsk}<button class="btn sm" onclick={askTriage}><Sparkles size={14} aria-hidden="true" />{t("myenvAsk")}</button>{/if}
                <button class="btn sm primary" onclick={keep} disabled={!chosen.length}><Check size={14} aria-hidden="true" />{t("myenvKeep", { n: chosen.length })}</button>
              </div>
            </div>
            {#each GROUPS as group (group)}
              {@const list = news.filter((f) => f.group === group)}
              {#if list.length}
                {@const all = list.every(ticked)}
                <section class="group" aria-labelledby="env-found-{group}">
                  <h3 id="env-found-{group}">
                    {t(GROUP_LABEL[group])}<span class="count">{list.length}</span>
                    <button class="btn link" onclick={() => tickAll(list, !all)}>{all ? t("myenvUntickAll") : t("myenvTickAll")}</button>
                  </h3>
                  <ul class="rows">
                    {#each list as f (keyOf(f))}
                      <li>
                        <input type="checkbox" checked={ticked(f)} onchange={(e) => (ticks[keyOf(f)] = e.currentTarget.checked)} aria-label={t("myenvKeepName", { name: f.name })} />
                        <div class="main">
                          {@render who(f)}
                          {#if f.description}<p class="desc">{f.description}</p>{/if}
                          {#if f.path}<p class="desc mono">{f.path}</p>{/if}
                        </div>
                      </li>
                    {/each}
                  </ul>
                </section>
              {/if}
            {/each}
            {#if devenv.scanning}<Skeleton shape="list" rows={2} label={t("myenvScanning")} />{/if}
          {:else if devenv.scanning}
            <Skeleton shape="list" rows={6} label={t("myenvScanning")} />
          {:else}
            <EmptyState icon={Check} title={t("myenvFoundNone")} action={{ label: t("myenvTabManifest"), run: () => (devenv.tab = "manifest") }} />
          {/if}
        {/if}
      {:else}
        <div class="toolbar">
          {#if scripts.length}
            <Segments
              label={t("myenvTabScript")}
              options={[{ id: "", label: t("myenvScriptGenerated") }, ...scripts.map((s) => ({ id: s, label: s }))]}
              selected={script}
              onselect={(id) => (script = id)}
            />
          {/if}
          <div class="row-actions end">
            <button class="btn sm" onclick={copy} disabled={text === null}>
              {#if copied}<Check size={14} aria-hidden="true" />{t("myenvCopied")}{:else}<Copy size={14} aria-hidden="true" />{t("myenvCopy")}{/if}
            </button>
            {#if script}
              <button class="btn sm" onclick={() => dropScript(script)}><Trash2 size={14} aria-hidden="true" />{t("delete")}</button>
            {/if}
            <button class="btn sm primary" onclick={() => askInstall(script || null)} disabled={windows || text === null || (!script && !entries.length)}>
              <Download size={14} aria-hidden="true" />{t("myenvRunScript")}
            </button>
          </div>
        </div>
        <p class="help">{script ? t("myenvScriptNamedHelp") : t("myenvScriptHelp")}</p>
        {#if text === null}
          <Skeleton shape="block" height="320px" label={t("myenvLoading")} />
        {:else}
          <pre class="code" aria-label={t("myenvTabScript")}>{text}</pre>
        {/if}
      {/if}
    </div>
  </Tabs>
</div>

<ConfirmDialog
  bind:open={confirming}
  title={running?.name ? t("myenvConfirmScriptTitle", { name: running.name }) : t("myenvConfirmTitle")}
  confirm={t("myenvConfirmGo")}
  onconfirm={go}
>
  <p>{t("myenvConfirmBody")}</p>
  {#if !running?.name && lacking.length}
    <p class="lacking">{t("myenvConfirmMissing", { list: lacking.map((e) => e.name).join(", ") })}</p>
  {:else if !running?.name && !devenv.scanned}
    <p class="lacking"><CircleDashed size={13} aria-hidden="true" />{t("myenvConfirmUnknown")}</p>
  {/if}
</ConfirmDialog>

<style>
  .env {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 980px;
    padding: 16px 20px 32px;
  }

  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }

  .summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .summary.warn {
    color: var(--warning);
  }

  .summary.ok {
    color: var(--done);
  }

  .group h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0 6px;
    color: var(--soft);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .group h3 .btn.link {
    margin-left: auto;
    font-weight: 400;
  }

  .count {
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  .rows {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 12px;
  }

  .rows li + li {
    border-top: 1px solid var(--border);
  }

  .rows li.off .who,
  .rows li.off .desc {
    opacity: 0.55;
  }

  .rows input[type="checkbox"] {
    margin-top: 3px;
    accent-color: var(--accent);
  }

  .main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }

  .who {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 8px;
  }

  .name {
    font-size: var(--text-base);
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .version {
    color: var(--muted);
  }

  .desc {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .desc.hint {
    font-style: italic;
  }

  .cmd {
    align-self: flex-start;
    max-width: 100%;
    padding: 2px 6px;
    border-radius: var(--radius);
    background: var(--well);
    font: var(--text-xs) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .state {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .state.ok {
    color: var(--done);
  }

  .state.missing {
    color: var(--danger);
  }

  .tools {
    display: flex;
    flex-shrink: 0;
    gap: 2px;
  }

  .sources {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .sources li {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--panel);
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .sources li :global(svg) {
    color: var(--done);
  }

  .sources li.fail :global(svg) {
    color: var(--danger);
  }

  .sources li :global(svg.spin) {
    color: var(--muted);
  }

  .sources .n {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .found-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .form h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 12px;
  }

  .note {
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .row-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .row-actions.end {
    margin-left: auto;
  }

  .code {
    margin: 0;
    padding: 14px 16px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--well);
    font: var(--text-xs) / 1.55 var(--font-mono);
    font-variant-ligatures: none;
    tab-size: 2;
  }

  .lacking {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--soft);
    font-size: var(--text-sm);
  }
</style>
