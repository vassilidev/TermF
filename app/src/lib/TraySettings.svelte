<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";

  type Tray = { icon: boolean; badge: boolean };

  let tray = $state<Tray | null>(null);
  let error = $state("");

  invoke<Tray>("tray_settings").then(
    (s) => (tray = s),
    (e) => (error = String(e)),
  );

  async function set(next: Tray) {
    error = "";
    try {
      tray = await invoke<Tray>("tray_settings_set", { settings: next });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<h2>{t("remoteTrayTitle")}</h2>
{#if tray}
  <label class="check">
    <input type="checkbox" role="switch" class="switch" checked={tray.icon} onchange={(e) => set({ ...tray!, icon: e.currentTarget.checked })} />
    {t("remoteTrayIcon")}
  </label>
  <label class="check">
    <input type="checkbox" role="switch" class="switch" checked={tray.badge} onchange={(e) => set({ ...tray!, badge: e.currentTarget.checked })} />
    {t("remoteTrayBadge")}
  </label>
{:else if !error}
  <Skeleton rows={2} />
{/if}
<p class="help">{t("remoteTrayHelp")}</p>
{#if error}<p class="notice error" role="alert">{error}</p>{/if}

<style>
  /* The page's title in Settings. */
  h2 {
    margin: 0 0 12px;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    cursor: pointer;
  }

  .help {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }
</style>
