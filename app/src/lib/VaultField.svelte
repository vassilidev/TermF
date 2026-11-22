<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { getContext, onDestroy } from "svelte";
  import { t } from "$lib/i18n";
  import { onScreen, type ViewId } from "$lib/state.svelte";
  import { copy, failed, vault } from "$lib/vault.svelte";

  /** A value hidden until asked for, hidden again after `seconds`; it lives only here, never in a store. */
  let {
    label,
    load,
    multiline = false,
    seconds = 30,
  }: { label: string; load: () => Promise<string>; multiline?: boolean; seconds?: number } = $props();

  const view = getContext<() => ViewId>("panel");
  let text = $state("");
  let left = $state(0);
  let busy = $state<"" | "reveal" | "copy">("");
  let timer: ReturnType<typeof setInterval> | undefined;
  /** Bumped by every reveal and hide: an answer that comes after a hide (or a newer reveal) is dropped. */
  let request = 0;
  let gone = false;

  function hide() {
    request++;
    clearInterval(timer);
    text = "";
    left = 0;
  }

  async function run(what: "reveal" | "copy") {
    const asked = ++request;
    busy = what;
    vault.error = "";
    try {
      const value = await load();
      if (asked !== request || gone) return;
      if (what === "copy") return void (await copy(value));
      text = value;
      left = seconds;
      clearInterval(timer);
      timer = setInterval(() => {
        if (--left <= 0) hide();
      }, 1000);
    } catch (e) {
      if (asked === request && !gone) vault.error = failed(e);
    } finally {
      if (asked === request) busy = "";
    }
  }

  // Views stay mounted behind the others: leaving this one, or locking, hides the value.
  $effect(() => {
    if (!onScreen(view()) || !vault.status?.unlocked) hide();
  });
  onDestroy(() => {
    gone = true;
    hide();
  });
</script>

<div class="field-row" class:multiline class:open={!!text}>
  <span class="label">{label}</span>
  <div class="value">
    {#if text}
      {#if multiline}
        <pre class="shown">{text}</pre>
      {:else}
        <span class="shown mono">{text}</span>
      {/if}
    {:else}
      <span class="dots" aria-label={t("vaultHidden")}>••••••••••••</span>
    {/if}
  </div>
  <div class="tools">
    {#if text}
      <span class="muted left" role="timer">{t("keysHidesIn", { seconds: left })}</span>
      <button class="icon-btn" aria-label={t("keysHide")} title={t("keysHide")} onclick={hide}>
        <EyeOff size={15} aria-hidden="true" />
      </button>
    {:else}
      <button
        class="icon-btn"
        aria-label={t("vaultReveal", { what: label })}
        title={t("vaultReveal", { what: label })}
        disabled={busy !== ""}
        onclick={() => run("reveal")}
      >
        {#if busy === "reveal"}<LoaderCircle size={15} class="spin" aria-hidden="true" />{:else}<Eye size={15} aria-hidden="true" />{/if}
      </button>
    {/if}
    <button
      class="icon-btn"
      aria-label={t("vaultCopyWhat", { what: label })}
      title={t("vaultCopyWhat", { what: label })}
      disabled={busy !== ""}
      onclick={() => (text ? copy(text) : run("copy"))}
    >
      {#if busy === "copy"}<LoaderCircle size={15} class="spin" aria-hidden="true" />{:else}<Copy size={15} aria-hidden="true" />{/if}
    </button>
  </div>
</div>

<style>
  .shown {
    overflow-wrap: anywhere;
  }

  pre.shown {
    max-height: 320px;
    margin: 0;
    overflow: auto;
    font: var(--text-sm) / 1.55 var(--font-mono);
    white-space: pre-wrap;
  }

  .dots {
    color: var(--muted);
    font: var(--text-sm) var(--font-mono);
    letter-spacing: 1px;
  }

  .left {
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
