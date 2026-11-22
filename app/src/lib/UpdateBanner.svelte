<script lang="ts">
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { dismissUpdate, updates } from "$lib/release.svelte";
  import { settings, showSettings } from "$lib/settings.svelte";
  import { ui } from "$lib/state.svelte";

  const available = $derived(updates.state?.available ?? null);
  // Not over the page that already shows it.
  const shown = $derived(
    !!available && updates.dismissed !== available.version && !(ui.view === "settings" && settings.section === "updates"),
  );
</script>

{#if shown && available}
  <div class="banners">
    <Banner icon={CircleArrowUp} ondismiss={dismissUpdate}>
      {t("releaseBanner", { version: available.version })}
      {#snippet action()}
        <button class="btn sm" onclick={() => showSettings("updates")}>{t("releaseSeeNotes")}</button>
      {/snippet}
    </Banner>
  </div>
{/if}
