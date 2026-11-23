<script lang="ts">
  import Fingerprint from "@lucide/svelte/icons/fingerprint-pattern";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LifeBuoy from "@lucide/svelte/icons/life-buoy";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import LockKeyhole from "@lucide/svelte/icons/lock-keyhole";
  import Plus from "@lucide/svelte/icons/plus";
  import ScrollText from "@lucide/svelte/icons/scroll-text";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Usb from "@lucide/svelte/icons/usb";
  import X from "@lucide/svelte/icons/x";
  import { invoke } from "@tauri-apps/api/core";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { osOf } from "$lib/release-logic";
  import { biometryName, failed, needsConfirm, say, setStatus, vault, type KeyInfo, type VaultStatus } from "$lib/vault.svelte";

  let { onclose, onaudit }: { onclose: () => void; onaudit: () => void } = $props();

  const DAYS = [1, 3, 7, 14, 30];
  let keys = $state<KeyInfo[]>([]);
  let typed = $state("");
  let busy = $state("");
  let error = $state("");
  /** The form the error belongs to: the password's shows under its fields. */
  let errorAt = $state("");
  let adding = $state(false);
  let name = $state("");
  let pin = $state("");
  let changing = $state(false);
  let current = $state("");
  let password = $state("");
  let again = $state("");
  let removing = $state<KeyInfo | null>(null);
  let asking = $state(false);

  const status = $derived(vault.status);
  // Read when the status changes; once 5 minutes are gone the backend says so and the status comes back.
  const confirmed = $derived((status?.confirmedUntil ?? 0) > Date.now() / 1000);
  const days = $derived(DAYS.includes(status?.quickDays ?? 7) ? DAYS : [...DAYS, status?.quickDays ?? 7].sort((a, b) => a - b));
  const os = osOf(navigator.platform);
  const when = (unix: number) => new Date(unix * 1000).toLocaleString(lang, { dateStyle: "medium", timeStyle: "short" });

  async function load() {
    try {
      keys = await invoke<KeyInfo[]>("vault_keys");
    } catch (e) {
      error = failed(e);
    }
  }

  async function run(what: string, action: () => Promise<void>) {
    busy = what;
    error = "";
    errorAt = what;
    try {
      await action();
    } catch (e) {
      if (needsConfirm(e)) {
        error = t("vault2ConfirmAgain");
        setStatus(await invoke<VaultStatus>("vault_status"));
      } else error = failed(e);
    } finally {
      busy = "";
    }
  }

  const submit = (what: string, action: () => Promise<void>) => (e: SubmitEvent) => {
    e.preventDefault();
    run(what, action);
  };

  const confirm = submit("confirm", async () => {
    try {
      setStatus(await invoke<VaultStatus>("vault_reauth", { password: typed }));
    } finally {
      typed = "";
    }
  });

  const addKey = submit("add", async () => {
    try {
      const added = await invoke<{ key: KeyInfo; recoveryKey: string | null }>("vault_key_add", {
        name,
        pin: vault.methods?.keyPin ? pin : null,
      });
      if (added.recoveryKey) vault.recoveryKey = { key: added.recoveryKey, reason: "first" };
      adding = false;
      name = "";
      setStatus(await invoke<VaultStatus>("vault_status"));
      await load();
    } finally {
      pin = "";
    }
  });

  function remove() {
    const key = removing;
    if (!key) return;
    run("remove", async () => {
      setStatus(await invoke<VaultStatus>("vault_key_remove", { id: key.id }));
      await load();
    });
  }

  async function biometry(days: number | null, control: HTMLInputElement | HTMLSelectElement) {
    await run("biometry", async () => {
      setStatus(await invoke<VaultStatus>("vault_biometry_set", { days, reason: t("vault2ReasonEnable") }));
    });
    // Refused or cancelled: the control shows what is, not what was asked.
    const now = vault.status?.quickDays ?? null;
    if (control instanceof HTMLInputElement) control.checked = now !== null;
    else if (now !== null) control.value = String(now);
  }

  // The current password first, whatever was confirmed before: a vault left open must not change hands.
  const change = submit("password", async () => {
    try {
      if (password !== again) throw t("vaultPasswordsDiffer");
      setStatus(await invoke<VaultStatus>("vault_reauth", { password: current }));
      setStatus(await invoke<VaultStatus>("vault_password_change", { password }));
      changing = false;
      password = again = "";
      say(t("vault2PasswordChanged"));
    } finally {
      current = "";
    }
  });

  const recovery = () =>
    run("recovery", async () => {
      vault.recoveryKey = { key: await invoke<string>("vault_recovery_new"), reason: "renewed" };
      setStatus(await invoke<VaultStatus>("vault_status"));
    });

  load();
</script>

{#snippet spinner(what: string)}
  {#if busy === what}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}
{/snippet}

{#if status}
  <section class="security" aria-labelledby="vault-security-title">
    <header class="head">
      <span class="avatar" aria-hidden="true"><ShieldCheck size={18} /></span>
      <div class="titles">
        <h2 id="vault-security-title">{t("vault2SecurityTitle")}</h2>
        <p class="muted">{status.keys ? t("vault2PasswordAndKey") : t("vault2PasswordOnly")}</p>
      </div>
      <button class="icon-btn" aria-label={t("close")} title={t("close")} onclick={onclose}><X size={16} aria-hidden="true" /></button>
    </header>

    {#if error && errorAt !== "password"}<p class="notice error" role="alert">{error}</p>{/if}


    <div class="card">
      <h3><ScrollText size={15} aria-hidden="true" />{t("restVaultAudit")}</h3>
      <p class="muted">{t("restVaultAuditIntro")}</p>
      <div><button class="btn sm" data-testid="vault-audit" onclick={onaudit}>{t("restVaultAuditOpen")}</button></div>
    </div>

    {#if !confirmed}
      <form class="confirm" onsubmit={confirm}>
        <label class="labelled">
          {t("vault2ConfirmText")}
          <!-- svelte-ignore a11y_autofocus (nothing here changes without it) -->
          <input class="field" type="password" autocomplete="current-password" bind:value={typed} required autofocus />
        </label>
        <button type="submit" class="btn primary" disabled={!!busy || !typed}>{@render spinner("confirm")}{t("vaultConfirm")}</button>
      </form>
    {/if}

    <div class="card">
      <h3><KeyRound size={15} aria-hidden="true" />{t("vault2Keys")}</h3>
      <p class="muted">{status.keys ? t("vault2PasswordAndKeyHint") : t("vault2PasswordOnlyHint")}</p>
      {#if keys.length}
        <ul class="keys">
          {#each keys as key (key.id)}
            <li>
              <Usb size={15} aria-hidden="true" />
              <span class="name">{key.name}</span>
              <span class="muted">{t("vault2KeyAdded", { when: ago(key.created) })}</span>
              <button
                class="icon-btn"
                aria-label={t("vault2KeyRemove", { name: key.name })}
                title={t("vault2KeyRemove", { name: key.name })}
                disabled={!confirmed || !!busy}
                onclick={() => {
                  removing = key;
                  asking = true;
                }}
              >
                <Trash2 size={14} aria-hidden="true" />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if keys.length === 1}<p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("vault2Backup")}</p>{/if}
      {#if adding}
        <form class="inline" onsubmit={addKey}>
          <div class="row">
            <label class="labelled">
              {t("vault2KeyName")}
              <!-- svelte-ignore a11y_autofocus (opened by its button) -->
              <input class="field" bind:value={name} placeholder={t("vault2KeyNamePlaceholder")} maxlength="64" required autofocus />
            </label>
            {#if vault.methods?.keyPin}
              <label class="labelled">
                {t("vault2Pin")}
                <input class="field" type="password" autocomplete="off" bind:value={pin} minlength="4" required />
              </label>
            {/if}
          </div>
          <div class="buttons">
            <button type="button" class="btn" onclick={() => (adding = false)} disabled={!!busy}>{t("cancel")}</button>
            <button type="submit" class="btn primary" disabled={!!busy || !name.trim() || (vault.methods?.keyPin && !pin)}>
              {@render spinner("add")}{t("vault2AddKey")}
            </button>
          </div>
          {#if busy === "add"}
            <p class="touch" role="status">{vault.methods?.keyPin ? t("vault2TouchTwice") : t("vault2WindowsAsks")}</p>
          {/if}
        </form>
      {:else}
        <div><button class="btn sm" disabled={!confirmed || !!busy} onclick={() => (adding = true)}><Plus size={14} aria-hidden="true" />{t("vault2AddKey")}</button></div>
      {/if}
    </div>

    <div class="card">
      <h3>
        <Fingerprint size={15} aria-hidden="true" />{biometryName() ||
          (os === "mac" ? t("vault2TouchId") : os === "windows" ? t("vault2WindowsHello") : t("vault2Biometry"))}
      </h3>
      {#if vault.methods?.biometry}
        <label class="toggle">
          <input
            type="checkbox"
            class="switch"
            checked={status.quickDays !== null}
            disabled={!confirmed || !!busy}
            onchange={(e) => biometry(e.currentTarget.checked ? 7 : null, e.currentTarget)}
          />
          {t("vault2BiometryOn", { name: biometryName() })}
          {@render spinner("biometry")}
        </label>
        {#if status.quickDays !== null}
          <label class="toggle muted">
            {t("vault2BiometryDays")}
            <select class="field" value={status.quickDays} disabled={!confirmed || !!busy} onchange={(e) => biometry(Number(e.currentTarget.value), e.currentTarget)}>
              {#each days as d (d)}<option value={d}>{t("vault2Days", { n: d })}</option>{/each}
            </select>
          </label>
          <p class="muted fine">{status.quickUntil ? t("vault2BiometryUntil", { date: when(status.quickUntil) }) : t("vault2BiometryPending")}</p>
        {/if}
        <p class="muted fine">{t("vault2BiometryHint", { name: biometryName() })}</p>
        <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{vault.methods.biometry === "touchId" ? t("vault2HonestMac") : t("vault2HonestWindows")}</p>
      {:else}
        <p class="muted">
          {os === "mac" ? t("restVaultBiometryNoneMac") : os === "windows" ? t("restVaultBiometryNoneWindows") : t("vault2BiometryNone")}
        </p>
      {/if}
    </div>

    <div class="card">
      <h3><LockKeyhole size={15} aria-hidden="true" />{t("vault2Password")}</h3>
      {#if changing}
        <form class="inline" onsubmit={change}>
          <label class="labelled current">
            {t("restVaultCurrentPassword")}
            <!-- svelte-ignore a11y_autofocus (opened by its button) -->
            <input class="field" type="password" autocomplete="current-password" bind:value={current} required autofocus />
          </label>
          <div class="row">
            <label class="labelled">
              {t("vault2NewPassword")}
              <input class="field" type="password" autocomplete="new-password" minlength="8" bind:value={password} required />
            </label>
            <label class="labelled">
              {t("vaultPasswordAgain")}
              <input class="field" type="password" autocomplete="new-password" bind:value={again} required />
            </label>
          </div>
          {#if error && errorAt === "password"}<p class="notice error" role="alert">{error}</p>{/if}
          <div class="buttons">
            <button type="button" class="btn" onclick={() => ((changing = false), (current = password = again = ""), (error = ""))} disabled={!!busy}>
              {t("cancel")}
            </button>
            <button type="submit" class="btn primary" disabled={!!busy || !current || !password}>{@render spinner("password")}{t("vault2ChangePassword")}</button>
          </div>
        </form>
      {:else}
        <div><button class="btn sm" data-testid="vault-password-change" disabled={!!busy} onclick={() => ((changing = true), (error = ""))}>{t("vault2ChangePassword")}</button></div>
      {/if}
    </div>

    <div class="card">
      <h3><LifeBuoy size={15} aria-hidden="true" />{t("vault2Recovery")}</h3>
      <p class="muted">{status.recovery ? t("vault2RecoveryOn") : t("vault2RecoveryOff")}</p>
      <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("vault2RecoveryLost")}</p>
      <div>
        <button class="btn sm" disabled={!confirmed || !!busy} onclick={recovery}>
          {@render spinner("recovery")}{status.recovery ? t("vault2RecoveryNew") : t("vault2RecoveryCreate")}
        </button>
      </div>
    </div>
  </section>
{/if}

<ConfirmDialog bind:open={asking} title={t("vault2RemoveTitle", { name: removing?.name ?? "" })} confirm={t("vault2KeyRemove", { name: removing?.name ?? "" })} danger onconfirm={remove}>
  <p>{t("vault2RemoveText")}</p>
  {#if keys.length === 1}<p>{t("vault2RemoveLast")}</p>{/if}
</ConfirmDialog>

<style>
  .security {
    display: flex;
    flex-direction: column;
    gap: 16px;
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
  }

  .titles p {
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  .confirm {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
  }

  .confirm .labelled {
    flex: 1;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3 :global(svg) {
    color: var(--muted);
  }

  .card p {
    margin: 0;
  }

  .fine {
    font-size: var(--text-sm);
  }

  .keys {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .keys li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    padding: 4px 0;
  }

  .keys li + li {
    border-top: 1px solid var(--border);
  }

  .keys li :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .keys .name {
    font-weight: 500;
  }

  .keys .muted {
    flex: 1;
    font-size: var(--text-sm);
  }

  .inline {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .row .labelled {
    flex: 1 1 200px;
  }

  .current {
    max-width: calc(50% - 5px);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .touch {
    color: var(--accent-ink);
    font-weight: 500;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .toggle .field {
    width: auto;
    height: 28px;
  }
</style>
