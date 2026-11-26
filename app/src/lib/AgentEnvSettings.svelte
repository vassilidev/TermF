<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import Plus from "@lucide/svelte/icons/plus";
  import X from "@lucide/svelte/icons/x";
  import Banner from "$lib/ui/Banner.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { t } from "$lib/i18n";

  /** The global allowlist of what agents get of the environment (`[agents] env_allow` in termf.toml). */
  type Settings = { inherit: boolean; allow: string[]; base: string[]; problems: string[] };

  let settings = $state<Settings | null>(null);
  /** The names of the shell's variables no agent gets (never their values); null while the shell is read. */
  let withheld = $state<string[] | null>(null);
  let adding = $state("");
  let error = $state("");

  $effect(() => {
    invoke<Settings>("agentenv_settings").then(
      (s) => (settings = s),
      (e) => (error = String(e)),
    );
    readWithheld();
  });

  function readWithheld() {
    invoke<string[]>("agentenv_withheld").then(
      (names) => (withheld = names),
      (e) => {
        withheld = [];
        error = String(e);
      },
    );
  }

  async function save(allow: string[]): Promise<boolean> {
    error = "";
    try {
      settings = await invoke<Settings>("agentenv_allow_set", { allow });
      adding = "";
      readWithheld();
      return true;
    } catch (e) {
      error = String(e);
      return false;
    }
  }

  function add(e: SubmitEvent) {
    e.preventDefault();
    const pattern = adding.trim();
    if (pattern && settings && !settings.allow.includes(pattern)) save([...settings.allow, pattern]);
  }

  async function give(name: string) {
    if (!settings || settings.allow.includes(name)) return;
    withheld = withheld?.filter((n) => n !== name) ?? null;
    if (await save([...settings.allow, name]))
      toast(t("agentEnvGiven", { name }), { undo: () => settings && save(settings.allow.filter((p) => p !== name)) });
  }
</script>

<h2>{t("agentenvSettings")}</h2>
<p class="help lead">{t("agentenvSettingsHelp")}</p>
{#if error}<Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>{/if}
{#if settings?.inherit}<Banner kind="warn">{t("agentenvInherit")}</Banner>{/if}
{#each settings?.problems ?? [] as problem (problem)}<Banner kind="warn">{problem}</Banner>{/each}

{#if settings}
  {#if !settings.inherit}
    <section class="group" aria-labelledby="agentenv-withheld">
      <h3 id="agentenv-withheld" class="h-sec">
        {t("agentEnvWithheld")}
        {#if withheld?.length}<span class="n">{withheld.length}</span>{/if}
      </h3>
      <p class="help">{t("agentEnvWithheldHelp")}</p>
      {#if withheld === null}
        <Skeleton rows={3} />
      {:else if withheld.length}
        <ul class="withheld">
          {#each withheld as name (name)}
            <li>
              <code title={name}>{name}</code>
              <button class="btn quiet sm" aria-label={t("agentEnvGiveLabel", { name })} onclick={() => give(name)}>
                <Plus size={13} aria-hidden="true" />{t("agentEnvGive")}
              </button>
            </li>
          {/each}
        </ul>
      {:else}
        <p class="muted none">{t("agentEnvWithheldNone")}</p>
      {/if}
    </section>
  {/if}

  <section class="group" aria-labelledby="agentenv-allow">
    <h3 id="agentenv-allow" class="h-sec">{t("agentenvAllow")}</h3>
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
      <p class="muted none">{t("agentenvAllowEmpty")}</p>
    {/if}
    <details class="other">
      <summary><ChevronRight size={14} aria-hidden="true" class="chev" />{t("agentEnvAddOther")}</summary>
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
    </details>
  </section>

  <details class="group base">
    <summary class="h-sec">
      <ChevronRight size={14} aria-hidden="true" class="chev" />{t("agentenvBase")}<span class="n">{settings.base.length}</span>
    </summary>
    <p class="names">{#each settings.base as name (name)}<code>{name}</code>{/each}</p>
    <p class="help">{t("agentenvBaseHelp")}</p>
  </details>
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

  .group {
    margin: 24px 0;
  }

  .h-sec {
    margin: 0;
  }

  .help {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .help.lead {
    max-width: 640px;
    margin: 0 0 4px;
    line-height: 1.5;
  }

  .none {
    margin: 8px 0 0;
    font-size: var(--text-sm);
  }

  /* Many names at once: columns that fill the width, each with its own way to give it. */
  .withheld {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    margin: 12px 0 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--border);
    list-style: none;
    gap: 1px;
  }

  .withheld li {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 4px 4px 4px 10px;
    background: var(--panel);
  }

  .withheld code {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .names {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }

  ul.names li {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 2px 2px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
  }

  p.names {
    gap: 4px;
  }

  p.names code {
    padding: 1px 6px;
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--muted);
  }

  summary {
    display: flex;
    align-items: center;
    gap: 6px;
    width: fit-content;
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  summary :global(.chev) {
    flex-shrink: 0;
    color: var(--muted);
    transition: transform var(--dur-fast) ease-out;
  }

  details[open] > summary :global(.chev) {
    transform: rotate(90deg);
  }

  .base summary {
    align-items: center;
  }

  .other {
    margin-top: 12px;
  }

  .other summary {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .other summary:hover {
    color: var(--text);
  }

  .add {
    display: flex;
    gap: 6px;
    max-width: 360px;
    margin-top: 8px;
  }

  .add input {
    flex: 1;
  }

  .mono,
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
</style>
