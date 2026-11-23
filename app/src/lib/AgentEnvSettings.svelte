<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import Banner from "$lib/ui/Banner.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";

  /** The global allowlist of what agents get of the environment (`[agents] env_allow` in termf.toml). */
  type Settings = { inherit: boolean; allow: string[]; base: string[]; problems: string[] };

  let settings = $state<Settings | null>(null);
  let adding = $state("");
  let error = $state("");

  $effect(() => {
    invoke<Settings>("agentenv_settings").then(
      (s) => (settings = s),
      (e) => (error = String(e)),
    );
  });

  async function save(allow: string[]) {
    error = "";
    try {
      settings = await invoke<Settings>("agentenv_allow_set", { allow });
      adding = "";
    } catch (e) {
      error = String(e);
    }
  }

  function add(e: SubmitEvent) {
    e.preventDefault();
    const pattern = adding.trim();
    if (pattern && settings && !settings.allow.includes(pattern)) save([...settings.allow, pattern]);
  }
</script>

<h2>{t("agentenvSettings")}</h2>
<p class="help">{t("agentenvSettingsHelp")}</p>
{#if error}<Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>{/if}
{#if settings?.inherit}<Banner kind="warn">{t("agentenvInherit")}</Banner>{/if}
{#each settings?.problems ?? [] as problem (problem)}<Banner kind="warn">{problem}</Banner>{/each}

{#if settings}
  <section class="group" aria-labelledby="agentenv-allow">
    <h3 id="agentenv-allow">{t("agentenvAllow")}</h3>
    {#if settings.allow.length}
      <ul class="names">
        {#each settings.allow as pattern (pattern)}
          <li>
            <code>{pattern}</code>
            <button
              class="icon-btn sm"
              aria-label={`${t("agentenvRemove")} ${pattern}`}
              title={t("agentenvRemove")}
              onclick={() => save(settings!.allow.filter((p) => p !== pattern))}
            >
              <X size={13} aria-hidden="true" />
            </button>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="muted">{t("agentenvAllowEmpty")}</p>
    {/if}
    <form class="add" onsubmit={add}>
      <input
        class="field mono"
        placeholder={t("agentenvAllowPlaceholder")}
        aria-label={t("agentenvAllowPlaceholder")}
        spellcheck="false"
        autocomplete="off"
        bind:value={adding}
      />
      <button class="btn sm" disabled={!adding.trim()}><Plus size={14} aria-hidden="true" />{t("agentenvAllowAdd")}</button>
    </form>
    <p class="help">{t("agentenvAllowHelp")}</p>
  </section>

  <section class="group" aria-labelledby="agentenv-base">
    <h3 id="agentenv-base">{t("agentenvBase")}</h3>
    <p class="base">{#each settings.base as name (name)}<code>{name}</code>{/each}</p>
    <p class="help">{t("agentenvBaseHelp")}</p>
  </section>
{:else if !error}
  <Skeleton shape="form" rows={4} />
{/if}

<style>
  h2 {
    margin: 0 0 12px;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .group {
    margin: 16px 0 24px;
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .names {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }

  .names li {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 2px 2px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
  }

  .add {
    display: flex;
    gap: 6px;
    max-width: 420px;
  }

  .add input {
    flex: 1;
  }

  .mono,
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .base {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 0;
  }

  .base code {
    padding: 1px 6px;
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--muted);
  }
</style>
