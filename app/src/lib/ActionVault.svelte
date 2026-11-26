<script lang="ts">
  import Fingerprint from "@lucide/svelte/icons/fingerprint-pattern";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import { biometryName, failed, followStatus, quickReady, refreshVault, setStatus, vault, type VaultMethods, type VaultStatus } from "$lib/vault.svelte";

  type Need = "unlock" | "reauth";

  /**
   * What an agent asked for stopped at the vault (an approved action, a key it is given): the card says why (`text`),
   * takes the master password (or Touch ID, or a security key), then `onopen` lets it go on. What is typed here never
   * reaches the agent.
   */
  let {
    need: asked,
    text,
    onopen,
    onrefuse,
  }: { need: Need; text: (need: Need) => string; onopen: () => unknown; onrefuse: () => unknown } = $props();

  // Locked meanwhile (idle, its view left): opening it is what is asked now.
  const need = $derived<Need>(vault.status && !vault.status.unlocked ? "unlock" : asked);
  const withKey = $derived(need === "unlock" && !!vault.status?.keys);
  let password = $state("");
  let pin = $state("");
  let busy = $state(false);
  let error = $state("");

  onMount(() => {
    followStatus().catch(() => {});
    if (!vault.methods)
      invoke<VaultMethods>("vault_methods").then(
        (m) => {
          vault.methods = m;
        },
        () => {},
      );
  });

  async function prove(command: string, args: Record<string, unknown>) {
    busy = true;
    error = "";
    const opening = need === "unlock";
    try {
      setStatus(await invoke<VaultStatus>(command, args));
      if (opening) void refreshVault();
      await onopen();
    } catch (e) {
      error = failed(e);
    } finally {
      password = pin = "";
      busy = false;
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (need === "reauth") prove("vault_reauth", { password });
    else prove("vault_unlock", { password, pin: withKey && vault.methods?.keyPin ? pin : null });
  }

  const quick = () =>
    need === "reauth"
      ? prove("vault_reauth_biometry", { reason: t("vault2ReasonProd") })
      : prove("vault_unlock_biometry", { reason: t("vault2ReasonUnlock") });
</script>

<p class="said">{text(need)}</p>
<form class="prove" onsubmit={submit}>
  {#if quickReady()}
    <button type="button" class="btn sm quick" disabled={busy} onclick={quick}>
      <Fingerprint size={14} aria-hidden="true" />{t(need === "reauth" ? "vault2ProdBiometry" : "vault2OpenWith", { name: biometryName() })}
    </button>
  {/if}
  <label class="labelled secret">
    <span>{t("vaultMasterPassword")}</span>
    <input class="field" type="password" autocomplete="current-password" bind:value={password} required />
  </label>
  {#if withKey && vault.methods?.keyPin}
    <label class="labelled secret">
      <span>{t("vault2Pin")}</span>
      <input class="field" type="password" autocomplete="off" bind:value={pin} minlength="4" required />
    </label>
  {/if}
  {#if busy && withKey}<p class="small touch" role="status">{vault.methods?.keyPin ? t("vault2Touch") : t("vault2WindowsAsks")}</p>{/if}
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  <p class="muted small">{t("vaultCardsNever")}</p>
  <div class="row">
    <button type="button" class="btn sm" disabled={busy} onclick={onrefuse}>
      <ShieldX size={14} aria-hidden="true" />{t("chatDeny")}
    </button>
    <button type="submit" class="btn sm primary" disabled={busy || !password}>
      {#if busy}<LoaderCircle size={14} class="spin" aria-hidden="true" />{:else}<ShieldCheck size={14} aria-hidden="true" />{/if}
      {t(need === "reauth" ? "vaultConfirm" : "vaultUnlock")}
    </button>
  </div>
</form>

<style>
  .said {
    margin: 0;
    font-size: var(--text-sm);
  }

  .prove {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .secret {
    max-width: 360px;
  }

  .quick {
    align-self: flex-start;
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .touch {
    color: var(--accent-ink);
    font-weight: 500;
  }

  .row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
