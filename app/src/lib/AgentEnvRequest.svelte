<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { dropSecretRequest, type SecretRequest } from "$lib/agentenv.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { followStatus, showVault, vault } from "$lib/vault.svelte";

  /**
   * An agent's request for a key, as the Home shows it: what for, where to get one, then the key pasted here (kept in
   * the project's vault, never shown to the agent) or a refusal.
   */
  let { request: r, primary = true }: { request: SecretRequest; primary?: boolean } = $props();

  const who = $derived(r.agent.role ?? r.agent.title);
  const project = $derived(app.workspace?.projects.find((p) => p.id === r.project)?.name ?? r.project);
  const host = $derived.by(() => {
    try {
      return r.signupUrl ? new URL(r.signupUrl).host : "";
    } catch {
      return "";
    }
  });

  let value = $state("");
  let password = $state("");
  let busy = $state(false);
  let error = $state("");
  // The app's own: it follows the vault opened or locked anywhere, this card included.
  const status = $derived(vault.status);
  const locked = $derived(!!status?.initialized && !status.unlocked);

  $effect(() => {
    followStatus().catch(() => {});
  });

  async function answer(give: boolean) {
    busy = true;
    error = "";
    try {
      await invoke("agentenv_answer", { id: r.id, value: give ? value : null, password: give && locked ? password : null });
      dropSecretRequest(r.id);
    } catch (e) {
      const text = String(e);
      error = /wrong master password/.test(text) ? t("vaultWrongPassword") : text;
    } finally {
      busy = false;
      password = "";
    }
  }
</script>

<article class="request" data-wait={`secret:${r.id}`} tabindex="-1">
  <header>
    <KeyRound size={15} aria-hidden="true" />
    <b>{t("agentenvAsk", { who, variable: r.variable })}</b>
  </header>
  <p class="purpose">{t("agentenvFor", { purpose: r.purpose })}</p>
  {#if r.signupUrl && host}
    <button class="btn link signup" onclick={() => invoke("open_url", { url: r.signupUrl }).catch((e) => (error = String(e)))}>
      <ExternalLink size={13} aria-hidden="true" />{t("agentenvSignup", { host })}
    </button>
  {/if}
  {#if status && !status.initialized}
    <p class="notice">{t("agentenvNoVault")}</p>
  {:else}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        answer(true);
      }}
    >
      <label class="labelled">
        <span>{t("agentenvPaste")} · <code>{r.variable}</code></span>
        <input class="field mono" type="password" autocomplete="off" spellcheck="false" bind:value />
      </label>
      {#if locked}
        <label class="labelled">
          <span>{t("agentenvPassword")}</span>
          <input class="field" type="password" autocomplete="current-password" bind:value={password} />
        </label>
      {/if}
    </form>
  {/if}
  <p class="muted small">{t("agentenvWhere", { project })}</p>
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  <footer>
    <span class="acts">
      <button class="btn sm" disabled={busy} onclick={() => answer(false)}>{t("agentenvRefuse")}</button>
      {#if status && !status.initialized}
        <button class="btn sm" class:primary onclick={showVault}>{t("agentenvOpenVault")}</button>
      {:else}
        <button class="btn sm" class:primary disabled={busy || !value.trim() || (locked && !password)} onclick={() => answer(true)}>
          {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("agentenvGive")}
        </button>
      {/if}
    </span>
  </footer>
</article>

<style>
  .request {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--waiting-ring);
    border-radius: var(--radius-lg);
    background: var(--waiting-soft);
    color: var(--text);
    font-size: var(--text-sm);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-base);
  }

  header :global(svg) {
    flex-shrink: 0;
  }

  header b {
    font-weight: 650;
    overflow-wrap: anywhere;
  }

  .purpose {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .signup {
    align-self: flex-start;
  }

  form {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 12px;
  }

  form .labelled {
    flex: 1 1 240px;
    max-width: 360px;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .notice {
    margin: 0;
  }

  footer {
    display: flex;
    align-items: center;
  }

  .acts {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }
</style>
