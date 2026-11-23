<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import BookCopy from "@lucide/svelte/icons/book-copy";
  import Copy from "@lucide/svelte/icons/copy";
  import Ellipsis from "@lucide/svelte/icons/ellipsis";
  import FolderClosed from "@lucide/svelte/icons/folder-closed";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import Search from "@lucide/svelte/icons/search";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import Popover from "$lib/ui/Popover.svelte";
  import { t } from "$lib/i18n";
  import { copyName, savedGroups, type Connection, type Query } from "$lib/ops-logic";
  import { loadOps, ops } from "$lib/ops.svelte";

  /** The project's saved queries: searched, by folder; each one opened, run, changed, copied, deleted, or made a
   * recipe for the agents. Above them, the form of the one being saved. */
  let {
    project,
    databases,
    onopen,
    onrun,
    notify,
  }: {
    project: string;
    databases: Connection[];
    onopen: (q: Query) => void;
    onrun: (q: Query) => void;
    notify: (text: string) => void;
  } = $props();

  let search = $state("");
  const groups = $derived(savedGroups(ops.queries, search));
  let error = $state("");
  let busy = $state(false);
  let menu = $state<string | null>(null);
  let triggers = $state<Record<string, HTMLElement>>({});
  let removing = $state<Query | null>(null);
  let confirmOpen = $state(false);

  // The form, as typed: tags as one line.
  const editing = $derived(ops.studio.editing);
  let tags = $state("");
  $effect(() => {
    tags = (editing?.query.tags ?? []).join(", ");
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (!editing) return;
    busy = true;
    error = "";
    const q = editing.query;
    const query: Query = {
      name: q.name.trim(),
      sql: q.sql,
      ...(q.description?.trim() ? { description: q.description.trim() } : {}),
      ...(q.folder?.trim() ? { folder: q.folder.trim() } : {}),
      ...(q.connection ? { connection: q.connection } : {}),
    };
    const list = tags
      .split(",")
      .map((x) => x.trim())
      .filter(Boolean);
    if (list.length) query.tags = list;
    try {
      ops.queries = await invoke<Query[]>("ops_query_save", { project, query, previous: editing.previous });
      ops.studio.editing = null;
      notify(t("opsdbSaved", { name: query.name }));
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function duplicate(q: Query) {
    menu = null;
    const name = copyName(
      q.name,
      ops.queries.map((x) => x.name),
      t("opsdbCopyWord"),
    );
    try {
      ops.queries = await invoke<Query[]>("ops_query_save", { project, query: { ...q, name }, previous: null });
      notify(t("opsdbDuplicated", { name }));
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(q: Query) {
    try {
      ops.queries = await invoke<Query[]>("ops_query_delete", { project, name: q.name });
      notify(t("opsdbDeleted", { name: q.name }));
    } catch (e) {
      error = String(e);
    }
  }

  async function recipe(q: Query) {
    menu = null;
    try {
      const name = await invoke<string>("ops_query_recipe", { project, name: q.name });
      await loadOps(project);
      notify(t("opsdbRecipeDone", { name }));
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="saved" aria-label={t("opsdbAsideSaved")}>
  {#if editing}
    <form class="edit" onsubmit={save}>
      <h3>{editing.previous === null ? t("opsdbSaveTitle") : t("opsdbEditTitle", { name: editing.previous })}</h3>
      <label class="labelled">
        <span>{t("opsdbName")}</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="field" bind:value={editing.query.name} required maxlength="100" autofocus />
      </label>
      <label class="labelled">
        <span>{t("opsdbDescription")}</span>
        <input class="field" bind:value={editing.query.description} maxlength="300" />
      </label>
      <div class="pair">
        <label class="labelled">
          <span>{t("opsdbFolder")}</span>
          <input class="field" bind:value={editing.query.folder} maxlength="60" list="ops-folders" />
          <datalist id="ops-folders">
            {#each [...new Set(ops.queries.map((q) => q.folder).filter(Boolean))] as f (f)}<option value={f}></option>{/each}
          </datalist>
        </label>
        <label class="labelled">
          <span>{t("opsdbTags")}</span>
          <input class="field" bind:value={tags} placeholder={t("opsdbTagsHelp")} />
        </label>
      </div>
      <label class="labelled">
        <span>{t("opsdbForDb")}</span>
        <select
          class="field"
          value={editing.query.connection ?? ""}
          onchange={(e) => editing && (editing.query.connection = e.currentTarget.value || undefined)}
        >
          <option value="">{t("opsdbAnyDb")}</option>
          {#each databases as c (c.name)}<option value={c.name}>{c.name} · {c.env}</option>{/each}
        </select>
      </label>
      <pre class="sql">{editing.query.sql}</pre>
      {#if error}<p class="notice error" role="alert">{error}</p>{/if}
      <div class="buttons">
        <button type="button" class="btn sm" onclick={() => ((ops.studio.editing = null), (error = ""))}>{t("cancel")}</button>
        <button class="btn sm primary" disabled={busy || !editing.query.name.trim()}>{t("opsdbSave")}</button>
      </div>
    </form>
  {/if}

  {#if ops.queries.length}
    <label class="search">
      <Search size={13} aria-hidden="true" />
      <input type="search" placeholder={t("opsdbSearch")} aria-label={t("opsdbSearch")} bind:value={search} />
    </label>
  {/if}
  {#if !editing && error}<p class="notice error" role="alert">{error}</p>{/if}

  {#if !ops.queries.length}
    {#if !editing}<p class="empty muted">{t("opsdbSavedEmpty")}</p>{/if}
  {:else if !groups.length}
    <p class="empty muted">{t("opsdbSavedNoMatch")}</p>
  {:else}
    <div class="groups">
      {#each groups as g (g.folder)}
        <div class="group">
          <p class="folder"><FolderClosed size={12} aria-hidden="true" />{g.folder || t("opsdbNoFolder")}</p>
          <ul>
            {#each g.queries as q (q.name)}
              <li>
                <button class="open" onclick={() => onopen(q)} title={t("opsdbOpen")}>
                  <span class="qname">{q.name}</span>
                  {#if q.description}<span class="desc">{q.description}</span>{/if}
                  <code>{q.sql}</code>
                  {#if q.tags?.length || q.connection}
                    <span class="meta">
                      {#if q.connection}<span class="badge">{q.connection}</span>{/if}
                      {#each q.tags ?? [] as tag (tag)}<span class="tag">#{tag}</span>{/each}
                    </span>
                  {/if}
                </button>
                <div class="row-actions">
                  <button class="icon-btn" title={t("opsdbRun")} aria-label={t("opsdbRun")} onclick={() => onrun(q)}>
                    <Play size={13} aria-hidden="true" />
                  </button>
                  <button
                    class="icon-btn"
                    bind:this={triggers[q.name]}
                    aria-label={t("opsdbActions", { name: q.name })}
                    aria-expanded={menu === q.name}
                    onclick={() => (menu = menu === q.name ? null : q.name)}
                  >
                    <Ellipsis size={14} aria-hidden="true" />
                  </button>
                </div>
                {#if menu === q.name}
                  <Popover
                    bind:open={() => menu === q.name, (v) => (menu = v ? q.name : null)}
                    trigger={triggers[q.name]}
                    label={t("opsdbActions", { name: q.name })}
                    align="end"
                    width={250}
                  >
                    <div class="menu" role="menu">
                      <button role="menuitem" onclick={() => ((menu = null), (ops.studio.editing = { query: structuredClone($state.snapshot(q)), previous: q.name }))}>
                        <Pencil size={13} aria-hidden="true" />{t("opsdbEdit")}
                      </button>
                      <button role="menuitem" onclick={() => duplicate(q)}><Copy size={13} aria-hidden="true" />{t("opsdbDuplicate")}</button>
                      <button role="menuitem" onclick={() => recipe(q)}><BookCopy size={13} aria-hidden="true" />{t("opsdbRecipe")}</button>
                      <button role="menuitem" class="danger" onclick={() => ((menu = null), (removing = q), (confirmOpen = true))}>
                        <Trash2 size={13} aria-hidden="true" />{t("opsdbDelete")}
                      </button>
                    </div>
                  </Popover>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>
  {/if}
  {#if removing}
    <ConfirmDialog
      bind:open={confirmOpen}
      title={t("opsdbDeleteTitle", { name: removing.name })}
      confirm={t("opsdbDelete")}
      danger
      onconfirm={() => removing && remove(removing)}
    >
      <p>{t("opsdbDeleteBody")}</p>
    </ConfirmDialog>
  {/if}
</section>

<style>
  .saved {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .edit .labelled {
    font-size: var(--text-xs);
  }

  .pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .sql {
    max-height: 120px;
    margin: 0;
    padding: 8px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px;
    border: 1px solid var(--field-border);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--muted);
  }

  .search:focus-within {
    border-color: var(--focus);
  }

  .search input {
    flex: 1;
    min-width: 0;
    height: 28px;
    border: 0;
    outline: none;
    background: none;
    color: var(--text);
    font: inherit;
    font-size: var(--text-xs);
  }

  .empty {
    margin: 0;
    padding: 12px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .groups {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .folder {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 0 0 4px 6px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    position: relative;
  }

  .open {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    padding: 7px 58px 7px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .open:hover {
    background: var(--hover);
  }

  .qname {
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .desc {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  code {
    display: -webkit-box;
    overflow: hidden;
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tag {
    color: var(--accent-ink);
    font-size: var(--text-xs);
  }

  .row-actions {
    position: absolute;
    top: 5px;
    right: 4px;
    display: flex;
    gap: 2px;
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

  .icon-btn:hover {
    background: var(--raised);
    color: var(--text);
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

  .menu .danger {
    color: var(--danger);
  }
</style>
