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
  type Key = Parameters<typeof t>[0];

  const actions: Record<string, Key> = {
    run: "apiIsoRun",
    resume: "apiIsoResume",
    deliver: "apiIsoDeliver",
    close: "apiIsoClose",
    closeForce: "apiIsoCloseForce",
    team: "apiIsoTeam",
    teamRequest: "apiIsoTeamRequest",
    approve: "apiIsoApprove",
    review: "apiIsoReview",
    allow: "apiIsoAllowAgent",
    agentStart: "apiIsoAgentStart",
    say: "apiIsoSay",
    open: "apiIsoOpen",
  };
  // Shown in full under the sentence: what the user allows, not only its name.
  const details: [string, Key][] = [
    ["request", "apiIsoRequest"],
    ["project", "apiIsoProject"],
    ["repo", "apiIsoRepo"],
    ["title", "apiIsoSession"],
    ["text", "apiIsoText"],
    ["tool", "apiIsoTool"],
    ["description", "apiIsoDescription"],
    ["input", "apiIsoInput"],
    ["cwd", "apiIsoDir"],
    ["role", "apiIsoRole"],
    ["mode", "apiIsoMode"],
    ["tools", "apiIsoTools"],
    ["model", "apiIsoModel"],
    ["prompt", "apiIsoPrompt"],
  ];

  let pending = $state<Confirm[]>([]);
  // A request may end before the list that still holds it arrives.
  const finished = new Set<number>();

  const what = (c: Confirm) => t("apiIsoAsk", { action: t(actions[c.action], c.args) });
  const shown = (c: Confirm) => details.filter(([key]) => c.args[key]);

  onMount(() => {
    const stops = Promise.all([
      listen<Confirm>("api-confirm", async ({ payload }) => {
        if (finished.has(payload.id)) return;
        pending = [...pending.filter((c) => c.id !== payload.id), payload];
        if (document.hasFocus()) return;
        const granted = (await isPermissionGranted()) || (await requestPermission()) === "granted";
        if (granted) sendNotification({ title: t("apiIsoNotify"), body: what(payload) });
      }),
      listen<Confirm>("api-confirm-done", ({ payload }) => {
        finished.add(payload.id);
        pending = pending.filter((c) => c.id !== payload.id);
      }),
    ]);
    stops
      .then(() => invoke<Confirm[]>("api_confirms"))
      .then(
        (list) => {
          const fresh = list.filter((c) => !finished.has(c.id) && !pending.some((p) => p.id === c.id));
          pending = [...pending, ...fresh];
        },
        (e) => (app.error = String(e)),
      );
    return () => void stops.then((list) => list.forEach((stop) => stop()));
  });

  function answer(c: Confirm, allow: boolean) {
    finished.add(c.id);
    pending = pending.filter((p) => p.id !== c.id);
    invoke("api_confirm", { id: c.id, allow }).catch((e) => (app.error = String(e)));
  }
</script>

{#if pending.length}
  <div class="banners">
    {#each pending as c (c.id)}
      <Banner kind="warn" icon={ShieldAlert}>
        <span class="ask">{what(c)}</span>
        {#if shown(c).length}
          <span class="details">
            {#each shown(c) as [key, label] (key)}
              <span class="label">{t(label)}</span>
              <span class="value">{c.args[key]}</span>
            {/each}
          </span>
        {/if}
        {#snippet action()}
          <button class="btn sm" onclick={() => answer(c, false)}>{t("chatDeny")}</button>
          <button class="btn sm primary" onclick={() => answer(c, true)}>{t("chatAllow")}</button>
        {/snippet}
      </Banner>
    {/each}
  </div>
{/if}

<style>
  .ask {
    display: block;
  }

  .details {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 2px 12px;
    max-height: 10em;
    margin-top: 6px;
    overflow: auto;
    font-size: var(--text-sm);
  }

  .label {
    font-weight: 600;
  }

  .value {
    font-family: var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
