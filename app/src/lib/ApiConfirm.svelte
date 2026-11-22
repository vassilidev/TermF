<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import Banner from "$lib/ui/Banner.svelte";
  import { t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";

  /** A call through the local API that only goes on once the user allows it here. */
  type Confirm = { id: number; action: string; args: Record<string, string> };

  const actions: Record<string, Parameters<typeof t>[0]> = {
    run: "apiIsoRun",
    deliver: "apiIsoDeliver",
    close: "apiIsoClose",
    closeForce: "apiIsoCloseForce",
    team: "apiIsoTeam",
    teamRequest: "apiIsoTeamRequest",
    approve: "apiIsoApprove",
    review: "apiIsoReview",
    allow: "apiIsoAllowAgent",
  };

  let pending = $state<Confirm[]>([]);

  const what = (c: Confirm) => t("apiIsoAsk", { action: t(actions[c.action], c.args) });

  onMount(() => {
    invoke<Confirm[]>("api_confirms").then(
      (list) => (pending = [...list, ...pending.filter((c) => !list.some((l) => l.id === c.id))]),
      (e) => (app.error = String(e)),
    );
    const shown = listen<Confirm>("api-confirm", async ({ payload }) => {
      pending = [...pending.filter((c) => c.id !== payload.id), payload];
      if (document.hasFocus()) return;
      const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
      if (granted) sendNotification({ title: t("apiIsoNotify"), body: what(payload) });
    });
    const gone = listen<Confirm>("api-confirm-done", ({ payload }) => {
      pending = pending.filter((c) => c.id !== payload.id);
    });
    return () => void Promise.all([shown, gone]).then((stops) => stops.forEach((f) => f()));
  });

  function answer(c: Confirm, allow: boolean) {
    pending = pending.filter((p) => p.id !== c.id);
    invoke("api_confirm", { id: c.id, allow }).catch((e) => (app.error = String(e)));
  }
</script>

{#if pending.length}
  <div class="banners">
    {#each pending as c (c.id)}
      <Banner kind="warn" icon={ShieldAlert}>
        {what(c)}
        {#snippet action()}
          <button class="btn sm" onclick={() => answer(c, false)}>{t("chatDeny")}</button>
          <button class="btn sm primary" onclick={() => answer(c, true)}>{t("chatAllow")}</button>
        {/snippet}
      </Banner>
    {/each}
  </div>
{/if}
