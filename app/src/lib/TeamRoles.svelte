<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import LibraryBig from "@lucide/svelte/icons/library-big";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { library, showLibrary, type Item, type Layer } from "$lib/library.svelte";
  import { access, initials } from "$lib/team-logic";

  /** Mounted per project, by its Configuration page. */
  let { project }: { project: string } = $props();

  let roles = $state<Item[] | null>(null);
  let busy = $state(false);
  let error = $state("");

  const layerLabel: Record<Layer, Parameters<typeof t>[0]> = {
    shipped: "layerShipped",
    package: "stackLayer",
    global: "layerGlobal",
    project: "layerProject",
    repo: "layerRepo",
    run: "layerRun",
  };

  /** The last request: an answer to an earlier one, arriving late, is dropped. */
  let asked = 0;

  // Again on each library change: a toggle here, or an edit in the Library view.
  $effect(() => {
    void library.version;
    const n = ++asked;
    invoke<Item[]>("library_list", { kind: "role", project }).then(
      (list) => {
        if (n === asked) roles = list;
      },
      (e) => {
        if (n !== asked) return;
        roles = [];
        error = String(e);
      },
    );
  });

  /** As in the Library: `enabled` null inherits; a refused toggle puts the switch back. */
  async function toggle(role: Item, enabled: boolean | null, input?: HTMLInputElement) {
    busy = true;
    error = "";
    try {
      await invoke("library_toggle", { kind: "role", name: role.name, project, enabled });
    } catch (e) {
      if (input) input.checked = role.active;
      error = String(e);
    } finally {
      library.version++;
      busy = false;
    }
  }

  function editRoles() {
    Object.assign(library, { global: false, tab: "role", panel: null });
    showLibrary();
  }

  const text = (value: unknown) => (typeof value === "string" && value ? value : t("t5None"));
</script>

<section aria-labelledby="config-roles">
  <header>
    <h3 id="config-roles">{t("metaRoles")} {#if roles?.length}<small>{roles.length}</small>{/if}</h3>
    <button class="btn sm" onclick={editRoles}>
      <LibraryBig size={14} aria-hidden="true" />{t("t5EditRoles")}
    </button>
  </header>
  {#if error}
    <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
  {/if}
  {#if roles?.length}
    <table class="roster">
      <colgroup>
        <col />
        <col class="model" />
        <col class="access" />
        <col class="layer" />
        <col class="on" />
      </colgroup>
      <thead>
        <tr>
          <th scope="col">{t("kindRole")}</th>
          <th scope="col">{t("metaModel")}</th>
          <th scope="col">{t("t5Access")}</th>
          <th scope="col">{t("t5Layer")}</th>
          <th scope="col">{t("itemOn")}</th>
        </tr>
      </thead>
      <tbody>
        {#each roles as role (role.name)}
          {@const writes = access(role.meta)}
          <tr class:off={!role.active}>
            <td>
              <div class="who">
                <span class="mark" aria-hidden="true">{initials(role.name)}</span>
                <div>
                  <b>{role.name}</b>
                  {#if role.description}<small title={role.description}>{role.description}</small>{/if}
                </div>
              </div>
            </td>
            <td><span class="tag">{text(role.meta.model)}</span></td>
            <td class="muted" class:mono={Array.isArray(writes)}>
              {Array.isArray(writes) ? writes.join(", ") : writes ? t("t5ReadOnly") : t("t5None")}
            </td>
            <td><span class="badge layer-{role.layer}">{t(layerLabel[role.layer])}</span></td>
            <td>
              <div class="active">
                <input
                  class="switch"
                  type="checkbox"
                  role="switch"
                  checked={role.active}
                  disabled={busy}
                  aria-label={t("itemActive", { name: role.name })}
                  onchange={(e) => toggle(role, e.currentTarget.checked, e.currentTarget)}
                />
                {#if role.toggles.project !== undefined}
                  <button class="btn sm quiet" disabled={busy} onclick={() => toggle(role, null)} title={t("inheritTitle")}>
                    {t("inherit")}
                  </button>
                {/if}
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if roles}
    <p class="muted">{t("libraryEmpty")}</p>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h3 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  /* Fixed columns: the description, not the switches, gives way in a narrow window. */
  .roster {
    width: 100%;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    border-spacing: 0;
    background: var(--panel);
    table-layout: fixed;
  }

  .model {
    width: 92px;
  }

  .access {
    width: 22%;
  }

  .layer {
    width: 88px;
  }

  .on {
    width: 116px;
  }

  th {
    height: 34px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: left;
    white-space: nowrap;
  }

  td {
    height: 52px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
    overflow-wrap: anywhere;
  }

  td.muted {
    font-size: var(--text-sm);
  }

  tr:last-child td {
    border-bottom: 0;
  }

  .off .who b,
  .off .tag {
    opacity: 0.6;
  }

  .who {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .who div {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    line-height: 1.35;
  }

  .who b,
  .who small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .who b {
    font-weight: 600;
  }

  .who small {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .mark {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--hover);
    color: var(--text);
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .tag {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 7px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    font-size: var(--text-xs);
    font-weight: 600;
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

  .active {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
