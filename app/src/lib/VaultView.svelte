<script lang="ts">
  import Bot from "@lucide/svelte/icons/bot";
  import Braces from "@lucide/svelte/icons/braces";
  import Copy from "@lucide/svelte/icons/copy";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import Plus from "@lucide/svelte/icons/plus";
  import Fingerprint from "@lucide/svelte/icons/fingerprint-pattern";
  import Search from "@lucide/svelte/icons/search";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import StickyNote from "@lucide/svelte/icons/sticky-note";
  import Timer from "@lucide/svelte/icons/timer";
  import VaultIcon from "@lucide/svelte/icons/lock-keyhole";
  import { invoke } from "@tauri-apps/api/core";
  import { getContext } from "svelte";
  import VaultEntry from "$lib/VaultEntry.svelte";
  import VaultForm from "$lib/VaultForm.svelte";
  import VaultRecoveryKey from "$lib/VaultRecoveryKey.svelte";
  import VaultSecurity from "$lib/VaultSecurity.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { t } from "$lib/i18n";
  import { app, type ViewId } from "$lib/state.svelte";
  import {
    biometryName,
    copy,
    failed,
    openEntry,
    quickReady,
    refreshVault,
    setStatus,
    vault,
    type VaultStatus,
    type VaultView,
  } from "$lib/vault.svelte";
  import { entries, host, sections, visible, type Filter, type ItemInfo, type Kind } from "$lib/vault-logic";

  /** Mounted twice: in the current project's views ("vault") and under "This machine" ("vaultAll"). */
  const view = getContext<() => ViewId>("panel")() as VaultView;
  const scope = $derived(view === "vault" ? app.project : null);

  let query = $state("");
  let filter = $state<Filter>("all");
  let editing = $state<{ item: ItemInfo | null; kind: Kind } | null>(null);
  let deleting = $state(false);
  let password = $state("");
  let again = $state("");
  let busy = $state(false);
  let error = $state("");
  let copying = $state("");
  let security = $state(false);
  let pin = $state("");
  let recovering = $state(false);
  let recoveryText = $state("");

  const projectName = (id: string) => app.workspace?.projects.find((p) => p.id === id)?.name ?? id;
  const status = $derived(vault.status);
  const asksKey = $derived(!!status?.keys);
  const all =$derived(entries(vault.items, vault.secrets));
  const seen = $derived(visible(all, scope, "all", ""));
  const list = $derived(visible(all, scope, filter, query, projectName));
  const cut = $derived(sections(list, app.project, projectName));
  const selected = $derived(seen.find((e) => e.key === vault.selected[view]) ?? null);
  const context = $derived(
    view === "vaultAll" ? t("vaultEverything") : scope ? t("vaultScope", { project: projectName(scope) }) : t("vaultAllProjects"),
  );
  const minutes = $derived([1, 5, 10, 15, 30, 60, 240, status?.autoLockMinutes ?? 10].filter((m, i, l) => l.indexOf(m) === i).sort((a, b) => a - b));

  function select(key: string) {
    vault.selected[view] = key;
    editing = null;
    security = false;
  }

  function create(kind: Kind) {
    editing = { item: null, kind };
    security = false;
  }

  async function unlock(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    error = "";
    try {
      if (!status?.initialized) {
        if (password !== again) throw t("vaultPasswordsDiffer");
        setStatus(await invoke<VaultStatus>("vault_init", { password, remember: false }));
      } else if (recovering) {
        if (password !== again) throw t("vaultPasswordsDiffer");
        const done = await invoke<{ status: VaultStatus; recoveryKey: string }>("vault_recover", { recoveryKey: recoveryText, password });
        vault.recoveryKey = { key: done.recoveryKey, reason: "recovered" };
        recovering = false;
        setStatus(done.status);
      } else {
        const keyPin = asksKey && vault.methods?.keyPin ? pin : null;
        setStatus(await invoke<VaultStatus>("vault_unlock", { password, pin: keyPin }));
      }
      await refreshVault();
    } catch (e) {
      error = failed(e);
    } finally {
      password = again = pin = recoveryText = "";
      busy = false;
    }
  }

  async function unlockQuick() {
    busy = true;
    error = "";
    try {
      setStatus(await invoke<VaultStatus>("vault_unlock_biometry", { reason: t("vault2ReasonUnlock") }));
      await refreshVault();
    } catch (e) {
      error = failed(e);
    } finally {
      busy = false;
    }
  }

  async function lock() {
    editing = null;
    try {
      setStatus(await invoke<VaultStatus>("vault_lock"));
    } catch (e) {
      vault.error = String(e);
    }
  }

  async function remove() {
    if (selected?.type !== "item") return;
    try {
      await invoke("vault_item_delete", { id: selected.item.id });
      vault.selected[view] = null;
      await refreshVault();
    } catch (e) {
      vault.error = failed(e);
    }
  }

  async function autoLock(minutes: number) {
    try {
      setStatus(await invoke<VaultStatus>("vault_auto_lock_set", { minutes }));
    } catch (e) {
      vault.error = String(e);
    }
  }

  async function quickCopy(item: ItemInfo, field: "password" | "totp") {
    copying = `${item.id}:${field}`;
    vault.error = "";
    try {
      await copy(await invoke<string>("vault_item_reveal", { id: item.id, field }));
    } catch (e) {
      vault.error = failed(e);
    } finally {
      copying = "";
    }
  }

  // Locked by hand, idle or asleep: what the form holds must go too.
  $effect(() => {
    if (!status?.unlocked) {
      editing = null;
      security = false;
    }
  });
</script>

<div class="vault">
  <ViewHeader title={t("vaultTitle")} {context}>
    {#snippet actions()}
      {#if status?.unlocked}
        <button class="btn sm primary" onclick={() => create("login")}><Plus size={14} aria-hidden="true" />{t("vaultNew")}</button>
        <button class="btn sm" aria-pressed={security} onclick={() => ((security = !security), (editing = null))}>
          <ShieldCheck size={14} aria-hidden="true" />{t("vault2Security")}
        </button>
        <button class="btn sm" onclick={lock}><Lock size={14} aria-hidden="true" />{t("vaultLock")}</button>
      {/if}
    {/snippet}
  </ViewHeader>

  {#if vault.error}
    <div class="banners">
      <Banner kind="error" ondismiss={() => (vault.error = "")}>{vault.error}</Banner>
    </div>
  {/if}

  {#if vault.recoveryKey}
    <div class="pane"><VaultRecoveryKey /></div>
  {:else if !status}
    <div class="pane">
      <p class="loading muted" role="status"><LoaderCircle size={16} aria-hidden="true" class="spin" />{t("vaultLoading")}</p>
    </div>
  {:else if !status.unlocked}
    <div class="pane">
      <EmptyState
        icon={status.initialized ? Lock : VaultIcon}
        title={recovering ? t("vault2RecoverTitle") : status.initialized ? t("vaultLocked") : t("vaultCreateTitle")}
        text={recovering
          ? t("vault2RecoverText")
          : status.initialized
            ? t("vaultLockedText", { minutes: status.autoLockMinutes })
            : t("vaultCreateText")}
      >
        <form class="unlock" onsubmit={unlock}>
          {#if status.initialized && !recovering && quickReady()}
            <button type="button" class="btn primary quick-open" disabled={busy} onclick={unlockQuick}>
              <Fingerprint size={16} aria-hidden="true" />{t("vault2OpenWith", { name: biometryName() })}
            </button>
            <p class="or muted"><span>{t("vault2Or")}</span></p>
          {/if}
          {#if recovering}
            <label class="labelled">
              {t("vault2Recovery")}
              <!-- svelte-ignore a11y_autofocus (opened by its link) -->
              <input
                class="field mono"
                bind:value={recoveryText}
                placeholder={t("vault2RecoveryPlaceholder")}
                autocomplete="off"
                spellcheck="false"
                required
                autofocus
              />
            </label>
          {/if}
          <label class="labelled">
            {status.initialized && !recovering ? t("vaultMasterPassword") : t("vaultNewMasterPassword")}
            <!-- svelte-ignore a11y_autofocus (the only thing to do on this screen) -->
            <input
              class="field"
              type="password"
              autocomplete={status.initialized && !recovering ? "current-password" : "new-password"}
              minlength={status.initialized && !recovering ? undefined : 8}
              bind:value={password}
              required
              autofocus={!recovering}
            />
          </label>
          {#if !status.initialized || recovering}
            <label class="labelled">
              {t("vaultPasswordAgain")}
              <input class="field" type="password" autocomplete="new-password" bind:value={again} required />
            </label>
          {:else if asksKey && vault.methods?.keyPin}
            <label class="labelled">
              {t("vault2Pin")}
              <input class="field" type="password" autocomplete="off" bind:value={pin} minlength="4" required />
            </label>
          {/if}
          {#if error}<p class="notice error" role="alert">{error}</p>{/if}
          <button type="submit" class="btn" class:primary={!quickReady() || recovering} disabled={busy || !password}>
            {#if busy}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}
            {recovering ? t("vault2RecoverRun") : status.initialized ? t("vaultUnlock") : t("vaultCreate")}
          </button>
          {#if busy && asksKey && status.initialized && !recovering}
            <p class="touch" role="status">{vault.methods?.keyPin ? t("vault2Touch") : t("vault2WindowsAsks")}</p>
          {/if}
          {#if !status.initialized}<p class="muted fine">{t("vaultCreateWarning")}</p>{/if}
          {#if recovering}
            <button type="button" class="btn quiet" onclick={() => ((recovering = false), (error = ""))}>{t("cancel")}</button>
          {:else if status.initialized && status.recovery}
            <button type="button" class="btn quiet sm forgot" onclick={() => ((recovering = true), (error = ""))}>{t("vault2Forgot")}</button>
          {/if}
        </form>
      </EmptyState>
    </div>
  {:else}
    <div class="split">
      <div class="side">
        <div class="search">
          <Search size={14} aria-hidden="true" />
          <input
            class="field"
            type="search"
            placeholder={t("vaultSearch")}
            aria-label={t("vaultSearch")}
            bind:value={query}
            onkeydown={(e) => {
              const first = cut[0]?.entries[0];
              if (e.key === "Enter" && first) select(first.key);
            }}
          />
        </div>
        <div class="segmented filters" role="radiogroup" aria-label={t("vaultFilter")}>
          <button role="radio" aria-checked={filter === "all"} onclick={() => (filter = "all")}>{t("vaultAll")}</button>
          <button role="radio" aria-checked={filter === "login"} onclick={() => (filter = "login")}>{t("vaultLogins")}</button>
          <button role="radio" aria-checked={filter === "note"} onclick={() => (filter = "note")}>{t("vaultNotesFilter")}</button>
          {#if vault.secrets.length}
            <button role="radio" aria-checked={filter === "secret"} onclick={() => (filter = "secret")}>{t("vaultSecrets")}</button>
          {/if}
        </div>
        <div class="scroll">
          {#if !seen.length}
            <p class="muted empty">{t("vaultEmpty")}</p>
          {:else if !list.length}
            <p class="muted empty">{t("vaultNoMatch")}</p>
          {/if}
          {#each cut as section (section.project)}
            <h3 class="section">{section.project ? projectName(section.project) : t("vaultGlobal")}</h3>
            <ul class="list">
              {#each section.entries as entry (entry.key)}
                <li class="row" class:current={entry.key === selected?.key && !editing}>
                  <button class="pick" aria-current={entry.key === selected?.key} onclick={() => select(entry.key)}>
                    <span class="line">
                      <span class="kind" aria-hidden="true">
                        {#if entry.type === "secret"}<Braces size={13} />{:else if entry.item.kind === "note"}<StickyNote size={13} />{:else}<KeyRound size={13} />{/if}
                      </span>
                      <span class="name" class:mono={entry.type === "secret"}>{entry.type === "item" ? entry.item.title : entry.secret.name}</span>
                      {#if entry.type === "item" && entry.item.totp}<span class="badge">{t("vaultBadge2fa")}</span>{/if}
                      {#if entry.type === "item" && entry.item.agents}<span class="badge" title={t("vaultAgentsOn")}><Bot size={11} aria-label={t("vaultAgentsOn")} /></span>{/if}
                      {#if entry.type === "secret"}<span class="badge">{entry.secret.scope}</span>{/if}
                    </span>
                    <span class="desc">
                      {#if entry.type === "secret"}
                        {entry.secret.description || entry.secret.reference || t("vaultSecret")}
                      {:else if entry.item.kind === "note"}
                        {t("vaultNote")}
                      {:else}
                        {entry.item.username || (entry.item.urls[0] ? host(entry.item.urls[0]) : t("vaultLogin"))}
                      {/if}
                    </span>
                  </button>
                  {#if entry.type === "item" && entry.item.kind === "login"}
                    <span class="quick">
                      {#if entry.item.totp}
                        {@const what = t("vaultCopyCodeOf", { title: entry.item.title })}
                        <button class="icon-btn sm" aria-label={what} title={what} disabled={copying !== ""} onclick={() => quickCopy(entry.item, "totp")}>
                          {#if copying === `${entry.item.id}:totp`}<LoaderCircle size={13} class="spin" aria-hidden="true" />{:else}<Timer size={13} aria-hidden="true" />{/if}
                        </button>
                      {/if}
                      {#if entry.item.hasPassword}
                        {@const what = t("vaultCopyPasswordOf", { title: entry.item.title })}
                        <button class="icon-btn sm" aria-label={what} title={what} disabled={copying !== ""} onclick={() => quickCopy(entry.item, "password")}>
                          {#if copying === `${entry.item.id}:password`}<LoaderCircle size={13} class="spin" aria-hidden="true" />{:else}<Copy size={13} aria-hidden="true" />{/if}
                        </button>
                      {/if}
                    </span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/each}
        </div>
        <label class="autolock muted">
          <Timer size={13} aria-hidden="true" />
          {t("vaultAutoLock")}
          <select class="field" value={status.autoLockMinutes} onchange={(e) => autoLock(Number(e.currentTarget.value))}>
            {#each minutes as m (m)}<option value={m}>{t("vaultMinutes", { n: m })}</option>{/each}
          </select>
        </label>
      </div>

      <div class="detail">
        {#if security}
          <VaultSecurity onclose={() => (security = false)} />
        {:else if editing}
          {#key editing}
            <VaultForm
              item={editing.item}
              kind={editing.kind}
              project={scope}
              onsaved={(saved) => {
                editing = null;
                // Moved to a project this view does not show: the whole vault shows it.
                if (view === "vault" && scope !== null && saved.project !== null && saved.project !== scope) {
                  openEntry(saved.id, saved.project);
                } else vault.selected[view] = saved.id;
              }}
              oncancel={() => (editing = null)}
            />
          {/key}
        {:else if selected}
          {#key selected.key}
            <VaultEntry
              entry={selected}
              {projectName}
              onedit={() => selected?.type === "item" && (editing = { item: selected.item, kind: selected.item.kind })}
              ondelete={() => (deleting = true)}
            />
          {/key}
        {:else}
          <EmptyState icon={VaultIcon} title={seen.length ? t("vaultPick") : t("vaultEmptyTitle")} text={t("vaultPickText")}>
            <div class="starts">
              <button class="btn" onclick={() => create("login")}><KeyRound size={14} aria-hidden="true" />{t("vaultNewLogin")}</button>
              <button class="btn" onclick={() => create("note")}><StickyNote size={14} aria-hidden="true" />{t("vaultNewNote")}</button>
            </div>
          </EmptyState>
        {/if}
      </div>
    </div>
  {/if}

  {#if vault.notice}
    <p class="toast" role="status">{vault.notice}</p>
  {/if}
</div>

<ConfirmDialog
  bind:open={deleting}
  title={t("vaultDeleteTitle", { title: selected?.type === "item" ? selected.item.title : "" })}
  confirm={t("delete")}
  danger
  onconfirm={remove}
>
  <p>{t("vaultDeleteText")}</p>
</ConfirmDialog>

<style>
  .vault {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
  }

  .unlock {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(340px, 100%);
    margin-top: 8px;
    text-align: left;
  }

  .fine {
    margin: 0;
    font-size: var(--text-xs);
    text-align: center;
  }

  .quick-open {
    justify-content: center;
    height: 36px;
  }

  .or {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: var(--text-sm);
  }

  .or::before,
  .or::after {
    flex: 1;
    border-top: 1px solid var(--border);
    content: "";
  }

  .touch {
    margin: 0;
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 500;
    text-align: center;
  }

  .forgot {
    align-self: center;
    color: var(--muted);
  }

  .side {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .search {
    position: relative;
    display: flex;
    align-items: center;
    padding: 10px 10px 6px;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    left: 19px;
  }

  .search .field {
    padding-left: 28px;
  }

  .filters {
    margin: 0 10px 6px;
    align-self: flex-start;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    padding: 0 6px 8px;
    overflow-y: auto;
  }

  .section {
    margin: 10px 8px 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .split > .side .list {
    margin: 0;
    padding: 0;
    border: 0;
    background: none;
    list-style: none;
  }

  .list .row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-right: 6px;
    border-radius: var(--radius);
  }

  .list .row:hover {
    background: var(--hover);
  }

  .list .row.current {
    background: var(--selected);
  }

  .list .pick {
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

  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .kind {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--muted);
  }

  .name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.mono {
    font-size: var(--text-sm);
  }

  .desc {
    overflow: hidden;
    padding-left: 19px;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.35;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .quick {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity var(--dur-fast) ease-out;
  }

  .row:hover .quick,
  .row.current .quick,
  .quick:focus-within {
    opacity: 1;
  }

  .empty {
    margin: 16px 10px;
    font-size: var(--text-sm);
  }

  .autolock {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .autolock .field {
    width: auto;
    height: 26px;
    margin-left: auto;
  }

  .starts {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
  }

  .toast {
    position: absolute;
    bottom: 16px;
    left: 50%;
    z-index: 5;
    max-width: calc(100% - 32px);
    margin: 0;
    padding: 8px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--raised);
    box-shadow: var(--shadow-2);
    font-size: var(--text-sm);
    transform: translateX(-50%);
  }
</style>
