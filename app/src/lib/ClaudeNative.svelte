<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import FileText from "@lucide/svelte/icons/file-text";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import Plus from "@lucide/svelte/icons/plus";
  import Puzzle from "@lucide/svelte/icons/puzzle";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import ClaudeFileEditor from "$lib/ClaudeFileEditor.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { t } from "$lib/i18n";
  import { askAI } from "$lib/ask.svelte";
  import { app } from "$lib/state.svelte";
  import type { Item, Kind } from "$lib/library.svelte";
  import { change, claudeView, effective, kindLabel, propose, scopeLabel, type Folder, type Overview } from "$lib/claude.svelte";
  import { attempt, diffLines, refreshFiles, repoArg, type ExportPreview } from "$lib/claude-native.svelte";

  let { o }: { o: Overview } = $props();

  type FolderKind = Folder["kind"];
  const KINDS: FolderKind[] = ["skills", "agents", "commands", "output-styles", "rules"];
  let only = $state<FolderKind | "">("");
  const folders = $derived(o.extensions.filter((f) => !only || f.kind === only));

  /** The file shown, and whether it is to create. */
  let open = $state<{ path: string; create: boolean } | null>(null);
  let panel = $state<"" | "create" | "export">("");
  let deleting = $state<{ name: string; path: string; kind: FolderKind } | null>(null);
  let confirmDelete = $state(false);

  let draft = $state({ kind: "skills" as FolderKind, scope: "user" as "user" | "project", name: "", description: "" });
  let formError = $state("");

  const EXPORTABLE: Kind[] = ["skill", "command", "role", "preference", "rule"];
  const libraryLabel: Record<string, Parameters<typeof t>[0]> = {
    skill: "kindSkills",
    command: "kindCommands",
    role: "kindRoles",
    preference: "kindPreferences",
    rule: "kindRules",
  };
  let library = $state<Item[] | null>(null);
  let pick = $state("");
  let target = $state<"user" | "project">("user");
  let exportPreview = $state<ExportPreview | null>(null);
  let replace = $state(false);
  const picked = $derived(library?.find((i) => `${i.kind}:${i.name}` === pick));

  /** `skillOverrides` (code.claude.com/docs/en/skills): "off" hides a skill from Claude and from the / menu. */
  const override = (name: string) => effective(o, `skillOverrides.${name}`);

  /** Switching off goes where Claude Code's /skills menu writes (the local scope), or the user's without a
   * repository; switching on removes the entry from the scope that decides. */
  function toggleSkill(name: string) {
    const current = override(name);
    if (current?.value === "off" && current.scope !== "managed") {
      propose("skills", current.scope, { op: "set", key: `skillOverrides.${name}`, value: null });
    } else {
      propose("skills", claudeView.repo ? "local" : "user", { op: "set", key: `skillOverrides.${name}`, value: "off" });
    }
  }

  function show(path: string, create = false) {
    panel = "";
    open = { path, create };
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    formError = "";
    try {
      const path = await invoke<string>("claude_item_create", { ...draft, repo: repoArg() });
      await refreshFiles();
      draft = { ...draft, name: "", description: "" };
      show(path);
    } catch (err) {
      formError = String(err);
    }
  }

  /** The form's what and where, handed to the AI as a draft to read before it goes: nothing is written yet. */
  function writeWithClaude() {
    const base = draft.scope === "project" && claudeView.repo ? `${claudeView.repo}/.claude` : o.claudeDir;
    const name = draft.name.trim() || "…";
    askAI({ draft: t("restClaudeWriteDraft", { name, kind: kindLabel(draft.kind), dir: `${base}/${draft.kind}`, description: draft.description.trim() }) });
  }

  async function startExport() {
    open = null;
    panel = "export";
    exportPreview = null;
    formError = "";
    const items = await attempt(() => invoke<Item[]>("library_list", { kind: null, repo: repoArg(), project: app.project }));
    library = (items ?? []).filter((i) => EXPORTABLE.includes(i.kind));
    if (!picked) pick = library[0] ? `${library[0].kind}:${library[0].name}` : "";
  }

  const exportArgs = () => ({ kind: picked?.kind, name: picked?.name, to: target, repo: repoArg(), project: app.project });

  async function previewExport(e: SubmitEvent) {
    e.preventDefault();
    formError = "";
    replace = false;
    try {
      exportPreview = await invoke<ExportPreview>("claude_export_preview", exportArgs());
    } catch (err) {
      exportPreview = null;
      formError = String(err);
    }
  }

  async function doExport() {
    try {
      const dest = await invoke<string>("claude_export_item", { ...exportArgs(), overwrite: replace });
      toast(t("claudeDgExported", { name: picked?.name ?? "", path: dest }));
      exportPreview = null;
      panel = "";
      await refreshFiles();
    } catch (err) {
      formError = String(err);
    }
  }

  async function remove() {
    if (!deleting) return;
    const { path, name } = deleting;
    const done = await attempt(() => invoke<{ trashed: string; index: string | null }>("claude_file_delete", { path, repo: repoArg() }));
    if (done) {
      toast(t("claudeDgTrashed", { name, path: done.trashed }));
      if (open?.path === path) open = null;
      await refreshFiles();
    }
  }
</script>

<div class="toolbar">
  <div class="segmented" role="group" aria-label={t("claudeDgKindFilter")}>
    <button aria-pressed={only === ""} onclick={() => (only = "")}>{t("claudeRedesignAll")}</button>
    {#each KINDS as kind (kind)}
      <button aria-pressed={only === kind} onclick={() => (only = kind)}>{kindLabel(kind)}</button>
    {/each}
  </div>
  <span class="grow"></span>
  <button class="btn sm" aria-expanded={panel === "export"} onclick={() => (panel === "export" ? (panel = "") : startExport())}>
    <LibraryBig size={14} aria-hidden="true" />{t("claudeDgFromLibrary")}
  </button>
  <button class="btn sm" onclick={writeWithClaude} title={t("restClaudeWriteHelp")}>
    <Sparkles size={14} aria-hidden="true" />{t("lib2Write")}
  </button>
  <button
    class="btn sm primary"
    aria-expanded={panel === "create"}
    onclick={() => {
      open = null;
      formError = "";
      panel = panel === "create" ? "" : "create";
    }}
  >
    <Plus size={14} aria-hidden="true" />{t("claudeDgNew")}
  </button>
</div>

{#if panel === "create"}
  <form class="form" onsubmit={create}>
    <h3>{t("claudeDgNewTitle")}</h3>
    <div class="row">
      <label class="labelled">
        <span>{t("claudeDgKind")}</span>
        <select class="field" bind:value={draft.kind}>
          {#each KINDS as kind (kind)}<option value={kind}>{kindLabel(kind)}</option>{/each}
        </select>
      </label>
      <label class="labelled">
        <span>{t("claudeViewScope")}</span>
        <select class="field" bind:value={draft.scope}>
          <option value="user">{scopeLabel("user")}</option>
          <option value="project" disabled={!claudeView.repo}>{scopeLabel("project")}</option>
        </select>
      </label>
      <label class="labelled">
        <span>{t("claudeDgName")}</span>
        <input class="field mono" bind:value={draft.name} required pattern="[a-z0-9_][a-z0-9_\-]*" maxlength="64" placeholder="review-pr" spellcheck="false" autocomplete="off" />
      </label>
      <label class="labelled grow">
        <span>{t("claudeDgDescription")}</span>
        <input class="field" bind:value={draft.description} placeholder={t("claudeDgDescriptionHint")} />
      </label>
    </div>
    {#if formError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{formError}</p>{/if}
    <div class="buttons">
      <span class="muted hint">{t("claudeDgNewHint")}</span>
      <button type="button" class="btn sm" onclick={() => (panel = "")}>{t("cancel")}</button>
      <button type="button" class="btn sm" onclick={writeWithClaude} title={t("restClaudeWriteHelp")}>
        <Sparkles size={14} aria-hidden="true" />{t("lib2Write")}
      </button>
      <button type="submit" class="btn sm primary">{t("claudeDgCreate")}</button>
    </div>
  </form>
{/if}

{#if panel === "export"}
  <form class="form" onsubmit={previewExport}>
    <h3>{t("claudeDgExportTitle")}</h3>
    <p class="muted lead">{t("claudeDgExportHelp")}</p>
    {#if library && !library.length}
      <p class="muted">{t("claudeDgLibraryEmpty")}</p>
    {:else}
      <div class="row">
        <label class="labelled grow">
          <span>{t("claudeDgLibraryItem")}</span>
          <select class="field" bind:value={pick} onchange={() => (exportPreview = null)}>
            {#each EXPORTABLE as kind (kind)}
              {#if library?.some((i) => i.kind === kind)}
                <optgroup label={t(libraryLabel[kind])}>
                  {#each library.filter((i) => i.kind === kind) as item (item.name)}
                    <option value="{item.kind}:{item.name}">{item.name}{item.description ? ` — ${item.description}` : ""}</option>
                  {/each}
                </optgroup>
              {/if}
            {/each}
          </select>
        </label>
        <label class="labelled">
          <span>{t("claudeDgExportTo")}</span>
          <select class="field" bind:value={target} onchange={() => (exportPreview = null)}>
            <option value="user">{scopeLabel("user")}</option>
            <option value="project" disabled={!claudeView.repo}>{scopeLabel("project")}</option>
          </select>
        </label>
      </div>
    {/if}
    {#if formError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{formError}</p>{/if}
    {#if exportPreview}
      <p class="file">{t("claudeViewPreviewOf", { path: exportPreview.dest })}</p>
      {#if exportPreview.exists}
        <p class="notice warn"><CircleAlert size={14} aria-hidden="true" />{t("claudeDgExportExists")}</p>
      {/if}
      {#if exportPreview.diff}
        <pre class="code diff">{#each diffLines(exportPreview.diff) as line, i (i)}<span
              class="l{line[0] === '+' ? ' add' : line[0] === '-' ? ' del' : ''}">{line}
</span>{/each}</pre>
      {:else}
        <p class="notice">{t("claudeViewNoChange")}</p>
      {/if}
      {#if exportPreview.exists}
        <label class="check"><input type="checkbox" bind:checked={replace} />{t("claudeDgExportReplace")}</label>
      {/if}
    {/if}
    <div class="buttons">
      <span class="grow"></span>
      <button type="button" class="btn sm" onclick={() => (panel = "")}>{t("cancel")}</button>
      <button type="submit" class="btn sm" disabled={!picked}>{t("claudeViewPreview")}</button>
      <button type="button" class="btn sm primary" disabled={!exportPreview || (exportPreview.exists && !replace)} onclick={doExport}>
        {t("claudeDgExport")}
      </button>
    </div>
  </form>
{/if}

{#if change.origin === "skills"}
  <ClaudeChange />
{/if}

{#if open}
  <ClaudeFileEditor path={open.path} create={open.create} onclose={() => (open = null)} />
{/if}

{#each folders as f (f.path)}
  <section class="group" aria-label="{kindLabel(f.kind)} · {scopeLabel(f.scope)}">
    <header>
      <h4>{kindLabel(f.kind)}<span class="count">{f.items.length}</span></h4>
      <span class="badge scope">{scopeLabel(f.scope)}</span>
      <span class="mono muted path" title={f.path}>{f.path}</span>
    </header>
    {#if f.items.length}
      <ul class="rows">
        {#each f.items as item (item.path)}
          <li class:current={open?.path === item.path}>
            <button class="main" onclick={() => show(item.path)}>
              <span class="name mono">{f.kind === "commands" ? `/${item.name}` : item.name}</span>
              <span class="desc">{item.description ?? ""}</span>
            </button>
            {#if f.kind === "skills"}
              {@const state = override(item.name)}
              {#if state && state.value !== "on"}<span class="badge off" title="skillOverrides · {scopeLabel(state.scope)}">{state.value === "off" ? t("claudeDgSkillOff") : String(state.value)}</span>{/if}
              <button class="btn sm quiet" onclick={() => toggleSkill(item.name)} disabled={state?.scope === "managed"} title={t("claudeDgSkillToggleHelp")}>
                {state?.value === "off" ? t("claudeDgEnable") : t("claudeDgDisable")}
              </button>
            {/if}
            <button class="icon-btn sm" onclick={() => show(item.path)} aria-label={t("claudeDgOpen", { name: item.name })} title={t("claudeDgOpen", { name: item.name })}>
              <FileText size={14} aria-hidden="true" />
            </button>
            <button
              class="icon-btn sm danger"
              onclick={() => {
                deleting = { name: item.name, path: item.path, kind: f.kind };
                confirmDelete = true;
              }}
              aria-label={t("claudeDgDelete", { name: item.name })}
              title={t("claudeDgDelete", { name: item.name })}
            >
              <Trash2 size={14} aria-hidden="true" />
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="empty-line">{t("claudeViewEmptyFolder")}</p>
    {/if}
  </section>
{:else}
  <EmptyState
    icon={Puzzle}
    text={t("claudeViewNone")}
    action={{ label: t("claudeDgNew"), icon: Plus, run: () => ((open = null), (panel = "create")) }}
    onask={writeWithClaude}
    askLabel={t("lib2Write")}
  />
{/each}

<ConfirmDialog
  bind:open={confirmDelete}
  title={t("claudeDgDeleteTitle", { name: deleting?.name ?? "" })}
  confirm={t("claudeRedesignDeleteConfirm")}
  danger
  onconfirm={remove}
>
  <p>{deleting?.kind === "skills" ? t("claudeDgDeleteSkillText") : t("claudeDgDeleteText")}</p>
</ConfirmDialog>

<style>
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
  }

  .grow {
    flex: 1;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0 0 20px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .form h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .lead {
    max-width: 72ch;
    margin: -4px 0 0;
    font-size: var(--text-sm);
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
  }

  .row .labelled {
    width: 180px;
  }

  .row .labelled.grow {
    flex: 1;
    min-width: 220px;
  }

  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hint {
    flex: 1;
    font-size: var(--text-xs);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .file {
    margin: 0;
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
  }

  .code {
    max-height: 280px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre;
  }

  .diff .l {
    display: block;
  }

  .diff .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .diff .del {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .group {
    margin-bottom: 18px;
  }

  .group header {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin-bottom: 6px;
  }

  h4 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
    white-space: nowrap;
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 500;
    line-height: 16px;
    text-align: center;
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.scope {
    min-width: 64px;
    justify-content: center;
  }

  .rows {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    list-style: none;
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 8px 0 0;
  }

  .rows li + li {
    border-top: 1px solid var(--border);
  }

  .rows li.current {
    background: var(--hover);
  }

  .main {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(140px, 220px) minmax(0, 1fr);
    align-items: center;
    gap: 16px;
    min-width: 0;
    min-height: 38px;
    padding: 6px 12px;
    border: 0;
    background: none;
    color: var(--text);
    text-align: left;
  }

  .main:hover {
    background: var(--hover);
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
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.off {
    border-color: transparent;
    background: var(--hover);
    color: var(--muted);
  }

  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
  }

  .empty-line {
    margin: 4px 0 0;
    color: var(--muted);
  }
</style>
