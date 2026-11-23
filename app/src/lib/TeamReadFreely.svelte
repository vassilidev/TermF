<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "$lib/i18n";

  /** Whether the project's team reads and tests without asking: mounted by its Configuration page. */
  let { project }: { project: string } = $props();

  let on = $state<boolean | null>(null);
  let error = $state("");

  // svelte-ignore state_referenced_locally
  invoke<boolean>("team_read_freely", { project }).then(
    (value) => (on = value),
    (e) => (error = String(e)),
  );

  async function set(next: boolean) {
    error = "";
    try {
      on = await invoke<boolean>("team_read_freely", { project, on: next });
    } catch (e) {
      error = String(e);
    }
  }
</script>

{#if on !== null}
  <div class="reads">
    <label class="check">
      <input
        type="checkbox"
        role="switch"
        class="switch"
        checked={on}
        aria-describedby="team-reads-help"
        onchange={(e) => set(e.currentTarget.checked)}
      />
      {t("practicalTeamReadFreely")}
    </label>
    <p id="team-reads-help" class="help">{t("practicalTeamReadFreelyHelp")}</p>
  </div>
{/if}
{#if error}<p class="notice error" role="alert">{error}</p>{/if}

<style>
  .reads {
    margin: 12px 0 20px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    font-weight: 500;
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }
</style>
