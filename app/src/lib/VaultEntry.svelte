<script lang="ts">
  import Bot from "@lucide/svelte/icons/bot";
  import Braces from "@lucide/svelte/icons/braces";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Pencil from "@lucide/svelte/icons/pencil";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { invoke } from "@tauri-apps/api/core";
  import VaultConflict from "$lib/VaultConflict.svelte";
  import VaultField from "$lib/VaultField.svelte";
  import VaultTotp from "$lib/VaultTotp.svelte";
  import { ago, t } from "$lib/i18n";
  import { biometryName, copy, failed, quickReady, setStatus, vault, type VaultStatus } from "$lib/vault.svelte";
  import { openable, type Entry } from "$lib/vault-logic";

  let {
    entry,
    projectName,
    onedit,
    ondelete,
  }: { entry: Entry; projectName: (id: string) => string; onedit: () => void; ondelete: () => void } = $props();

  const item = $derived(entry.type === "item" ? entry.item : null);
  const secret = $derived(entry.type === "secret" ? entry.secret : null);
  const title = $derived(item ? item.title : (secret?.name ?? ""));
  let reauth = $state(false);
  let password = $state("");
  let busy = $state(false);
  /** Proving it with the security key: the field takes its PIN. */
  let withKey = $state(false);
  let touching = $state(false);

  const reveal = (field: "password" | "seed" | "notes") => () =>
    invoke<string>("vault_item_reveal", { id: item?.id, field });

  async function secretValue(): Promise<string> {
    try {
      return await invoke<string>("vault_reveal", { name: secret?.name, scope: secret?.scope, project: secret?.project });
    } catch (e) {
      if (/needs the master password again/.test(String(e))) reauth = true;
      throw e;
    }
  }

  async function prove(command: string, args: Record<string, unknown>) {
    busy = true;
    touching = command === "vault_reauth_key";
    try {
      setStatus(await invoke<VaultStatus>(command, args));
      reauth = withKey = false;
      vault.error = "";
    } catch (e) {
      vault.error = failed(e);
    } finally {
      password = "";
      busy = touching = false;
    }
  }

  function confirm(e: SubmitEvent) {
    e.preventDefault();
    if (withKey) prove("vault_reauth_key", { pin: password });
    else prove("vault_reauth", { password });
  }

  function byKey() {
    // Windows asks for the PIN in its own window.
    // What was typed so far is the master password, not a PIN: it would spend one of the key's tries.
    password = "";
    if (vault.methods?.keyPin) withKey = true;
    else prove("vault_reauth_key", { pin: null });
  }

  function byPassword() {
    password = "";
    withKey = false;
  }

  function open(url: string) {
    const href = openable(url);
    if (href) invoke("open_url", { url: href }).catch((e) => (vault.error = String(e)));
  }
</script>

<article class="entry">
  <header class="head">
    <span class="avatar" class:secret aria-hidden="true">
      {#if secret}<Braces size={18} />{:else}{title.trim().charAt(0).toUpperCase() || "?"}{/if}
    </span>
    <div class="titles">
      <h2 class:mono={secret}>{title}</h2>
      <p class="muted">
        {#if secret}<span class="badge" class:danger={secret.scope === "prod"}>{secret.scope}</span>{/if}
        <span class="badge">{entry.project ? projectName(entry.project) : t("vaultGlobal")}</span>
        {secret ? t("vaultSecret") : item?.kind === "login" ? t("vaultLogin") : t("vaultNote")}
      </p>
    </div>
    {#if item}
      <div class="head-actions">
        <button class="btn sm" onclick={onedit}><Pencil size={14} aria-hidden="true" />{t("vaultEdit")}</button>
        <button class="icon-btn" aria-label={t("delete")} title={t("delete")} onclick={ondelete}>
          <Trash2 size={15} aria-hidden="true" />
        </button>
      </div>
    {/if}
  </header>

  {#if (item?.conflicts ?? secret?.conflicts ?? 0) > 0}
    <VaultConflict
      key={entry.key}
      count={item?.conflicts ?? secret?.conflicts ?? 0}
      updated={item?.updated ?? secret?.updated ?? 0}
      reauth={() => (reauth = true)}
    />
  {/if}

  {#if secret?.description}<p class="description">{secret.description}</p>{/if}

  <div class="fields">
    {#if item?.kind === "login"}
      {#if item.username}
        <div class="field-row">
          <span class="label">{t("vaultUsername")}</span>
          <div class="value"><span class="plain">{item.username}</span></div>
          <div class="tools">
            <button
              class="icon-btn"
              aria-label={t("vaultCopyWhat", { what: t("vaultUsername") })}
              title={t("vaultCopyWhat", { what: t("vaultUsername") })}
              onclick={() => copy(item.username, false)}
            >
              <Copy size={15} aria-hidden="true" />
            </button>
          </div>
        </div>
      {/if}
      {#if item.hasPassword}
        <VaultField label={t("vaultPassword")} load={reveal("password")} />
      {/if}
      {#if item.totp}
        <VaultTotp id={item.id} period={item.totp.period} label={t("vaultTotpCode")} />
        <VaultField label={t("vaultTotpSeed")} load={reveal("seed")} />
      {/if}
      {#each item.urls as url (url)}
        <div class="field-row">
          <span class="label">{t("vaultSite")}</span>
          <div class="value"><span class="plain">{url}</span></div>
          <div class="tools">
            {#if openable(url)}
              <button class="icon-btn" aria-label={t("vaultOpenSite", { url })} title={t("vaultOpenSite", { url })} onclick={() => open(url)}>
                <ExternalLink size={15} aria-hidden="true" />
              </button>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
    {#if item?.hasNotes}
      <VaultField label={item.kind === "note" ? t("vaultNote") : t("vaultNotes")} load={reveal("notes")} multiline seconds={120} />
    {/if}
    {#if secret}
      {#if secret.reference}
        <div class="field-row">
          <span class="label">{t("vaultReference")}</span>
          <div class="value"><span class="mono">{secret.reference}</span></div>
        </div>
      {/if}
      <VaultField label={t("vaultValue")} load={secretValue} />
    {/if}
  </div>

  {#if reauth}
    <form class="reauth" onsubmit={confirm}>
      <label class="labelled">
        {withKey ? t("vault2Pin") : t("vaultProdAgain")}
        <!-- svelte-ignore a11y_autofocus (asked for right after the reveal) -->
        <input
          class="field"
          type="password"
          autocomplete={withKey ? "off" : "current-password"}
          bind:value={password}
          required
          autofocus
        />
      </label>
      <button type="submit" class="btn primary" disabled={busy || !password}>{t("vaultConfirm")}</button>
    </form>
    {#if vault.status?.keys || quickReady()}
      <div class="reauth-other">
        <span class="muted">{t("vault2Or")}</span>
        {#if withKey}
          <button class="btn sm" disabled={busy} onclick={byPassword}>{t("vault2BackToPassword")}</button>
        {:else if vault.status?.keys}
          <button class="btn sm" disabled={busy} onclick={byKey}>{t("vault2ProdKey")}</button>
        {/if}
        {#if quickReady()}
          <button class="btn sm" disabled={busy} onclick={() => prove("vault_reauth_biometry", { reason: t("vault2ReasonProd") })}>
            {t("vault2ProdBiometry", { name: biometryName() })}
          </button>
        {/if}
      </div>
    {/if}
    {#if touching}
      <p class="touch muted" role="status">{vault.methods?.keyPin ? t("vault2Touch") : t("vault2WindowsAsks")}</p>
    {/if}
  {/if}

  <p class="meta muted">
    {#if secret}
      {t("vaultSecretHint")}
    {:else if item?.agents}
      <Bot size={14} aria-hidden="true" />{t("vaultAgentsOn")}
    {:else}
      <ShieldCheck size={14} aria-hidden="true" />{t("vaultAgentsOff")}
    {/if}
    <span aria-hidden="true">·</span>
    {t("vaultUpdated", { when: ago(item?.updated ?? secret?.updated ?? 0) })}
  </p>
</article>

<style>
  .entry {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .avatar {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-size: var(--text-lg);
    font-weight: 650;
  }

  .avatar.secret {
    background: var(--hover);
    color: var(--muted);
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  h2.mono {
    font-size: var(--text-lg);
    letter-spacing: 0;
  }

  .titles p {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .description {
    margin: 0;
  }

  .fields {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .fields :global(.field-row) {
    display: grid;
    grid-template-columns: 140px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    min-height: 48px;
    padding: 8px 8px 8px 14px;
  }

  .fields :global(.field-row + .field-row) {
    border-top: 1px solid var(--border);
  }

  .fields :global(.field-row.multiline.open) {
    grid-template-areas: "label . tools" "value value value";
  }

  .fields :global(.field-row.multiline.open .label) {
    grid-area: label;
  }

  .fields :global(.field-row.multiline.open .value) {
    grid-area: value;
  }

  .fields :global(.field-row.multiline.open .tools) {
    grid-area: tools;
  }

  .fields :global(.field-row .label) {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .fields :global(.field-row .value) {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .fields :global(.field-row .tools) {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .plain {
    display: block;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .reauth {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    max-width: 480px;
  }

  .reauth .labelled {
    flex: 1;
  }

  .reauth-other {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: -8px;
    font-size: var(--text-sm);
  }

  .touch {
    margin: -8px 0 0;
    font-size: var(--text-sm);
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
