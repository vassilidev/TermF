<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { onDestroy } from "svelte";
  import { files } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { copySecret } from "$lib/ssh.svelte";

  /** A private key, hidden until asked for; it lives only in this component, never in a store. */
  let {
    load,
    encrypted = false,
    name,
  }: { load: (passphrase: string | null) => Promise<string>; encrypted?: boolean; name: string } = $props();

  const SHOW_FOR = 60;
  let text = $state("");
  let passphrase = $state("");
  let left = $state(0);
  let busy = $state(false);
  let error = $state("");
  let timer: ReturnType<typeof setInterval> | undefined;

  function hide() {
    clearInterval(timer);
    text = "";
    left = 0;
  }

  async function reveal() {
    busy = true;
    error = "";
    try {
      text = await load(encrypted ? passphrase : null);
      left = SHOW_FOR;
      clearInterval(timer);
      timer = setInterval(() => {
        if (--left <= 0) hide();
      }, 1000);
    } catch (e) {
      error = String(e);
    } finally {
      passphrase = "";
      busy = false;
    }
  }

  // The SSH view stays mounted behind the others: leaving it hides the key.
  $effect(() => {
    if (files.view !== "ssh") hide();
  });
  onDestroy(hide);
</script>

{#if text}
  <div class="secret">
    <pre class="key">{text}</pre>
    <div class="bar">
      <span class="warn"><TriangleAlert size={13} aria-hidden="true" />{t("keysRevealWarning")}</span>
      <span class="muted left" role="timer">{t("keysHidesIn", { seconds: left })}</span>
      <button class="btn sm" onclick={() => copySecret(text)}>
        <Copy size={14} aria-hidden="true" />{t("keysCopyPrivate")}
      </button>
      <button class="btn sm" onclick={hide}><EyeOff size={14} aria-hidden="true" />{t("keysHide")}</button>
    </div>
  </div>
{:else}
  <form
    class="masked"
    onsubmit={(e) => {
      e.preventDefault();
      reveal();
    }}
  >
    <span class="dots" aria-hidden="true">••••••••••••••••••••••••</span>
    <span class="muted label">{t("keysHidden")}</span>
    {#if encrypted}
      <input
        class="field"
        type="password"
        autocomplete="off"
        placeholder={t("sshPassphraseFor", { name })}
        aria-label={t("sshPassphraseFor", { name })}
        bind:value={passphrase}
      />
    {/if}
    <button type="submit" class="btn sm" disabled={busy || (encrypted && !passphrase)}>
      {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Eye size={14} aria-hidden="true" />{/if}
      {t("keysRevealRun")}
    </button>
    {#if error}<p class="notice error">{error}</p>{/if}
  </form>
{/if}

<style>
  .secret,
  .masked {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
  }

  .key {
    max-height: 280px;
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    font: var(--text-xs) / 1.55 var(--font-mono);
    white-space: pre;
  }

  .bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-top: 1px solid var(--border);
    background: var(--panel);
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    font-size: var(--text-xs);
  }

  .warn {
    display: inline-flex;
    flex: 1 1 220px;
    align-items: center;
    gap: 5px;
    color: var(--warning);
    font-weight: 500;
  }

  .left {
    font-variant-numeric: tabular-nums;
  }

  .masked {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
  }

  .dots {
    color: var(--muted);
    font: var(--text-sm) var(--font-mono);
    letter-spacing: 1px;
  }

  .label {
    flex: 1 1 auto;
    font-size: var(--text-sm);
  }

  .masked .field {
    flex: 0 1 240px;
  }

  .masked .notice {
    flex-basis: 100%;
  }
</style>
