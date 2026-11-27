<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import X from "@lucide/svelte/icons/x";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { healthLabel } from "$lib/claude.svelte";
  import { vault } from "$lib/vault.svelte";
  import { rows, strictNote, type Account, type Health, type Listing, type Row } from "$lib/connectors-logic";

  /** Mounted per project, by its Configuration page. */
  let { project }: { project: string } = $props();

  let listing = $state<Listing | null>(null);
  let account = $state<Account | null>(null);
  let checking = $state(false);
  let busy = $state("");
  let error = $state("");
  let open = $state("");

  const all = $derived(listing ? rows(listing, account) : []);
  const roleOf = (name: string) => listing?.roles.find((r) => r.name === name);

  // svelte-ignore state_referenced_locally
  invoke<Listing>("connectors_list", { project }).then(
    (l) => (listing = l),
    (e) => (error = String(e)),
  );
  check(false);

  async function check(refresh: boolean) {
    checking = true;
    try {
      account = await invoke<Account>("connectors_account", { refresh });
    } catch (e) {
      account = { connectors: [], error: String(e) };
    } finally {
      checking = false;
    }
  }

  async function run(key: string, command: string, args: Record<string, unknown>): Promise<boolean> {
    busy = key;
    error = "";
    try {
      listing = await invoke<Listing>(command, { project, ...args });
      return true;
    } catch (e) {
      error = String(e);
      return false;
    } finally {
      busy = "";
    }
  }

  function save(row: Row, roles: string[], write: boolean) {
    return run(row.name, "connectors_save", { name: row.name, server: null, roles, write, password: null });
  }

  async function give(row: Row) {
    if (await save(row, [], false)) open = row.name;
  }

  function tick(row: Row, role: string, on: boolean) {
    const roles = (row.roles ?? []).filter((r) => r !== role);
    save(row, on ? [...roles, role] : roles, row.write ?? false);
  }

  // Adding a server.
  type Pair = { key: string; value: string; shown: boolean };
  let adding = $state(false);
  const blank = () => ({ name: "", transport: "http", command: "", args: "", url: "" });
  let form = $state(blank());
  let pairs = $state<Pair[]>([{ key: "", value: "", shown: false }]);
  let password = $state("");
  const needsPassword = $derived(!vault.status?.unlocked && pairs.some((p) => p.value.trim()));

  function closeForm() {
    adding = false;
    form = blank();
    pairs = [{ key: "", value: "", shown: false }];
    password = "";
  }

  async function add(e: SubmitEvent) {
    e.preventDefault();
    const map = Object.fromEntries(pairs.filter((p) => p.key.trim()).map((p) => [p.key.trim(), p.value]));
    const has = Object.keys(map).length > 0;
    const server =
      form.transport === "stdio"
        ? {
            type: "stdio",
            command: form.command.trim(),
            ...(form.args.trim() ? { args: form.args.split("\n").map((a) => a.trim()).filter(Boolean) } : {}),
            ...(has ? { env: map } : {}),
          }
        : { type: form.transport, url: form.url.trim(), ...(has ? { headers: map } : {}) };
    const name = form.name.trim();
    const done = await run("add", "connectors_save", {
      name,
      server,
      roles: [],
      write: false,
      password: password || null,
    });
    if (done) {
      closeForm();
      open = name;
    }
  }

  const tone: Record<Health, string> = { connected: "ok", failed: "danger", pending: "warn", needsauth: "warn", unknown: "" };
</script>

<header class="head">
  <div>
    <h2 id="config-connectors">{t("connTitle")}</h2>
    <p class="help">{t("connIntro")}</p>
  </div>
  <button class="btn sm" disabled={checking} onclick={() => check(true)}>
    <RefreshCw size={14} aria-hidden="true" class={checking ? "spin" : ""} />{t("connCheck")}
  </button>
</header>

{#if error}
  <p class="notice error" role="alert">{error}</p>
{/if}

{#if !listing}
  <Skeleton shape="list" rows={3} />
{:else}
  {#if all.length}
    <ul class="list" aria-labelledby="config-connectors">
      {#each all as row (row.name)}
        {@const note = strictNote(row)}
        <li class:open={open === row.name}>
          <div class="line">
            <span class="what">
              <span class="name">{row.label}</span>
              {#if row.target}<span class="mono target">{row.target}</span>{/if}
            </span>
            <span class="badge">{row.account ? t("connOriginAccount") : t("connOriginTermf")}</span>
            <span class="who">
              {#if row.roles === null}
                {t("connEverySession")}
              {:else if row.roles.length}
                {row.roles.join(", ")}
              {:else}
                {t("connNoRole")}
              {/if}
            </span>
            {#if row.write === null}
              <span class="badge">{t("connUnbounded")}</span>
            {:else if row.write}
              <span class="badge warn">{t("connWrite")}</span>
            {:else}
              <span class="badge ok">{t("connReadOnly")}</span>
            {/if}
            {#if row.health}
              <span class="badge {tone[row.health]}">{healthLabel(row.health)}</span>
            {:else if checking && row.account}
              <span class="badge">{t("connChecking")}</span>
            {/if}
            <span class="actions">
              {#if row.configured}
                <button class="btn sm" aria-expanded={open === row.name} onclick={() => (open = open === row.name ? "" : row.name)}>
                  {row.roles === null ? t("connLimit") : t("connEdit")}
                </button>
                <button
                  class="icon-btn"
                  title={t("connRemove")}
                  aria-label={`${t("connRemove")} · ${row.label}`}
                  disabled={!!busy}
                  onclick={() => run(row.name, "connectors_remove", { name: row.name })}><Trash2 size={14} aria-hidden="true" /></button
                >
              {:else}
                <button class="btn sm primary" disabled={!!busy} onclick={() => give(row)}>{t("connGive")}</button>
              {/if}
            </span>
          </div>
          {#if open === row.name && row.configured}
            <div class="edit">
              <fieldset disabled={busy === row.name}>
                <legend>{t("connRoles")}</legend>
                <div class="roles">
                  {#each listing.roles as role (role.name)}
                    <label class="role">
                      <input
                        type="checkbox"
                        checked={row.roles?.includes(role.name) ?? false}
                        onchange={(e) => tick(row, role.name, e.currentTarget.checked)}
                      />
                      {role.name}
                    </label>
                  {/each}
                </div>
              </fieldset>
              {#each (row.roles ?? []).filter((r) => roleOf(r)?.prod) as role (role)}
                <p class="note warn">{t("connProd", { role })}</p>
              {/each}
              {#if note}<p class="note">{t(note)}</p>{/if}
              <label class="check">
                <input
                  type="checkbox"
                  role="switch"
                  class="switch"
                  checked={row.write ?? false}
                  disabled={busy === row.name}
                  onchange={(e) => save(row, row.roles ?? [], e.currentTarget.checked)}
                />
                {t("connAllowWrite")}
              </label>
              <p class="help">{row.write ? t("connWriteHelp") : t("connReadOnlyHelp")}</p>
            </div>
          {/if}
        </li>
      {/each}
      {#if checking && !account}
        <li class="loading"><Skeleton shape="list" rows={2} /></li>
      {/if}
    </ul>
  {:else if checking}
    <Skeleton shape="list" rows={3} />
  {:else}
    <p class="empty">{t("connEmpty")}</p>
  {/if}
  {#if account?.error}
    <p class="help">{t("connAccountError", { error: account.error })}</p>
  {/if}

  {#if adding}
    <form class="add" onsubmit={add}>
      <div class="add-head">
        <h3>{t("connAddTitle")}</h3>
        <button type="button" class="icon-btn" aria-label={t("cancel")} onclick={closeForm}><X size={14} aria-hidden="true" /></button>
      </div>
      <div class="grid">
        <label>
          <span>{t("connName")}</span>
          <input class="field" required pattern="[A-Za-z0-9_\-]+" maxlength="64" bind:value={form.name} />
        </label>
        <label>
          <span>{t("connTransport")}</span>
          <select class="field" bind:value={form.transport}>
            {#each ["http", "sse", "stdio"] as kind (kind)}<option value={kind}>{kind}</option>{/each}
          </select>
        </label>
      </div>
      {#if form.transport === "stdio"}
        <label><span>{t("connCommand")}</span><input class="field mono" required bind:value={form.command} /></label>
        <label><span>{t("connArgs")}</span><textarea class="field mono" rows="2" bind:value={form.args}></textarea></label>
      {:else}
        <label><span>URL</span><input class="field mono" required placeholder="https://" bind:value={form.url} /></label>
      {/if}
      <fieldset class="pairs">
        <legend>{form.transport === "stdio" ? t("connEnv") : t("connHeaders")}</legend>
        {#each pairs as pair, i (i)}
          <div class="pair">
            <input class="field mono" placeholder={t("connKey")} aria-label={t("connKey")} bind:value={pair.key} />
            <input
              class="field mono"
              type={pair.shown ? "text" : "password"}
              placeholder={t("connValue")}
              aria-label={t("connValue")}
              autocomplete="off"
              bind:value={pair.value}
            />
            <button type="button" class="icon-btn" aria-label={t("connShow")} aria-pressed={pair.shown} onclick={() => (pair.shown = !pair.shown)}>
              {#if pair.shown}<EyeOff size={14} aria-hidden="true" />{:else}<Eye size={14} aria-hidden="true" />{/if}
            </button>
          </div>
        {/each}
        <button type="button" class="btn sm quiet" onclick={() => pairs.push({ key: "", value: "", shown: false })}>
          <Plus size={14} aria-hidden="true" />{t("connAddPair")}
        </button>
        <p class="help">{t("connSecrets")}</p>
      </fieldset>
      {#if needsPassword}
        <label><span>{t("connVaultPassword")}</span><input class="field" type="password" autocomplete="current-password" bind:value={password} /></label>
      {/if}
      <div class="add-actions">
        <button type="button" class="btn" onclick={closeForm}>{t("cancel")}</button>
        <button class="btn primary" disabled={busy === "add"}>{t("connAddSubmit")}</button>
      </div>
    </form>
  {:else}
    <button class="btn sm add-open" onclick={() => (adding = true)}><Plus size={14} aria-hidden="true" />{t("connAddTitle")}</button>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  h2 {
    margin: 0 0 6px;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .list {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    list-style: none;
  }

  .list > li + li {
    border-top: 1px solid var(--border);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 6px 12px;
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    font-weight: 500;
  }

  .target {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .who {
    max-width: 220px;
    overflow: hidden;
    color: var(--soft);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .edit {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 4px 12px 14px;
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: 6px;
    padding: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .roles {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }

  .role,
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }

  .note {
    margin: 0;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .note.warn {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .edit .help {
    margin-top: -4px;
  }

  .empty {
    margin: 0;
    padding: 20px;
    border: 1px dashed var(--border);
    border-radius: var(--radius);
    color: var(--muted);
    text-align: center;
  }

  .loading {
    padding: 8px 12px;
  }

  .add-open {
    margin-top: 12px;
  }

  .add {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 16px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .add label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--text-sm);
  }

  .add-head,
  .add-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .add-actions {
    justify-content: flex-end;
  }

  .grid {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
    gap: 10px;
  }

  .pairs {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .pair {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr) auto;
    gap: 6px;
  }

  .pairs .btn {
    align-self: flex-start;
  }

  @media (max-width: 720px) {
    .line {
      flex-wrap: wrap;
    }

    .who {
      max-width: none;
    }
  }
</style>
