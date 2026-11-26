<script lang="ts">
  import FolderSync from "@lucide/svelte/icons/folder-sync";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import { dismissInvite, invite, showSync, sync, type Scope } from "$lib/sync.svelte";

  let {
    text,
    scope = null,
  }: {
    text: "syncVisibleInviteHome" | "syncVisibleInviteVault" | "syncVisibleInviteEnv";
    /** Ticked in the setup, beside the default ones. */
    scope?: Scope | null;
  } = $props();
</script>

{#if app.sync && !app.sync.enabled && !invite.dismissed}
  <div class="banners">
    <Banner icon={FolderSync} ondismiss={dismissInvite}>
      {t(text)}
      {#snippet action()}
        <button
          class="btn sm"
          onclick={() => {
            sync.suggested = scope;
            showSync();
          }}>{t("syncVisibleInviteAction")}</button
        >
      {/snippet}
    </Banner>
  </div>
{/if}
