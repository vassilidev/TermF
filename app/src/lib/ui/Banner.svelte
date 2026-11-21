<script lang="ts">
  import type { Component, Snippet } from "svelte";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Info from "@lucide/svelte/icons/info";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";

  let {
    kind = "info",
    icon,
    ondismiss,
    action,
    children,
  }: {
    kind?: "error" | "ok" | "warn" | "info";
    icon?: Component;
    ondismiss?: () => void;
    action?: Snippet;
    children: Snippet;
  } = $props();

  const icons = { error: CircleAlert, ok: Check, warn: TriangleAlert, info: Info };
  const Icon = $derived(icon ?? icons[kind]);
</script>

<div
  class="notice banner"
  class:error={kind === "error"}
  class:ok={kind === "ok"}
  class:warn={kind === "warn"}
  role={kind === "error" ? "alert" : kind === "ok" ? "status" : undefined}
>
  <Icon size={14} aria-hidden="true" />
  <span class="text">{@render children()}</span>
  {@render action?.()}
  {#if ondismiss}
    <button class="icon-btn sm" onclick={ondismiss} aria-label={t("dismiss")} title={t("dismiss")}>
      <X size={14} aria-hidden="true" />
    </button>
  {/if}
</div>

<style>
  .banner {
    align-items: center;
  }

  .text {
    flex: 1;
  }

  .icon-btn {
    color: inherit;
  }
</style>
