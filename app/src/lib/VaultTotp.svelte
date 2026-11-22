<script lang="ts">
  import Copy from "@lucide/svelte/icons/copy";
  import { invoke } from "@tauri-apps/api/core";
  import { getContext, onDestroy } from "svelte";
  import { t } from "$lib/i18n";
  import { onScreen, type ViewId } from "$lib/state.svelte";
  import { copy, failed, vault } from "$lib/vault.svelte";
  import { secondsLeft, spaced } from "$lib/vault-logic";

  /** The code valid now, asked again when its period ends, only while on screen. */
  let { id, period, label }: { id: string; period: number; label: string } = $props();

  const view = getContext<() => ViewId>("panel");
  let code = $state("");
  let now = $state(Date.now());
  let slot = -1;
  let request = 0;
  const left = $derived(secondsLeft(period, now));
  const live = $derived(onScreen(view()) && !!vault.status?.unlocked);

  async function fetch() {
    const asked = ++request;
    try {
      const value = await invoke<string>("vault_item_reveal", { id, field: "totp" });
      if (asked === request) code = value;
    } catch (e) {
      if (asked === request) {
        code = "";
        vault.error = failed(e);
      }
    }
  }

  $effect(() => {
    if (!live) {
      request++;
      code = "";
      slot = -1;
      return;
    }
    const tick = () => {
      now = Date.now();
      const current = Math.floor(now / 1000 / period);
      if (current !== slot) {
        slot = current;
        fetch();
      }
    };
    tick();
    const timer = setInterval(tick, 1000);
    return () => clearInterval(timer);
  });
  onDestroy(() => {
    request++;
    code = "";
  });
</script>

<div class="field-row totp">
  <span class="label">{label}</span>
  <div class="value">
    <span class="code mono" class:soon={left <= 5} aria-live="off">{code ? spaced(code) : "··· ···"}</span>
    <svg class="ring" viewBox="0 0 20 20" aria-hidden="true">
      <circle cx="10" cy="10" r="8" />
      <circle cx="10" cy="10" r="8" class="arc" class:soon={left <= 5} style:stroke-dashoffset={50.27 * (1 - left / period)} />
    </svg>
    <span class="muted left" role="timer">{t("vaultTotpLeft", { seconds: left })}</span>
  </div>
  <div class="tools">
    <button
      class="icon-btn"
      aria-label={t("vaultCopyWhat", { what: label })}
      title={t("vaultCopyWhat", { what: label })}
      disabled={!code}
      onclick={() => copy(code)}
    >
      <Copy size={15} aria-hidden="true" />
    </button>
  </div>
</div>

<style>
  .totp .value {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .code {
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: 0.06em;
    font-variant-numeric: tabular-nums;
  }

  .code.soon {
    color: var(--warning);
  }

  .ring {
    width: 18px;
    height: 18px;
    transform: rotate(-90deg);
  }

  .ring circle {
    fill: none;
    stroke: var(--border-strong);
    stroke-width: 2.5;
  }

  .ring .arc {
    stroke: var(--accent);
    stroke-dasharray: 50.27;
    stroke-linecap: round;
    transition: stroke-dashoffset 1s linear;
  }

  .ring .arc.soon {
    stroke: var(--warning);
  }

  @media (prefers-reduced-motion: reduce) {
    .ring .arc {
      transition: none;
    }
  }

  .left {
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
</style>
