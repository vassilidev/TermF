<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Bot from "@lucide/svelte/icons/bot";
  import { agentEnv, type Grant } from "$lib/agentenv.svelte";
  import { t } from "$lib/i18n";
  import { failed, say, vault } from "$lib/vault.svelte";

  /** The keys the project's agents get from the vault (`[agents.secrets]`), in the project's Vault view. */
  let { project }: { project: string } = $props();

  let grants = $state<Grant[]>([]);

  $effect(() => {
    // Read again once a request is granted, and when the vault opens (whether each key is there).
    void agentEnv.granted;
    void vault.status?.unlocked;
    invoke<{ grants: Grant[] }>("agentenv_grants", { project }).then(
      (g) => (grants = g.grants),
      (e) => {
        vault.error = failed(e);
      },
    );
  });

  async function remove(g: Grant) {
    try {
      const left = await invoke<{ grants: Grant[] }>("agentenv_revoke", { project, variable: g.variable, global: !g.project });
      grants = left.grants;
      say(t("agentenvRemoved", { variable: g.variable }));
    } catch (e) {
      vault.error = failed(e);
    }
  }
</script>

<section class="keys" aria-labelledby="agent-keys">
  <h3 class="section" id="agent-keys">{t("agentenvKeys")}</h3>
  {#if grants.length}
    <ul>
      {#each grants as g (g.variable)}
        <li>
          <span class="what">
            <span class="line">
              <Bot size={13} aria-hidden="true" />
              <span class="name mono" title={g.variable}>{g.variable}</span>
            </span>
            <span class="line desc">
              <span class="mono to">→ {g.secret} · {g.scope}</span>
              {#if !g.project}<span class="badge">{t("agentenvGlobal")}</span>{/if}
              {#if g.found === false}<span class="badge warn">{t("agentenvMissing")}</span>{/if}
            </span>
          </span>
          <button class="btn sm" aria-label={t("agentenvRemoveLabel", { variable: g.variable })} onclick={() => remove(g)}>
            {t("agentenvRemove")}
          </button>
        </li>
      {/each}
    </ul>
    <p class="muted fine">{t("agentenvKeysHelp")}</p>
  {:else}
    <p class="muted fine">{t("agentenvKeysEmpty")}</p>
  {/if}
</section>

<style>
  .keys {
    margin-top: 8px;
    padding-top: 2px;
    border-top: 1px solid var(--border);
  }

  .section {
    margin: 10px 8px 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 6px 6px 8px;
    border-radius: var(--radius);
  }

  li:hover {
    background: var(--hover);
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    color: var(--muted);
  }

  .name {
    overflow: hidden;
    color: var(--text);
    font-size: var(--text-sm);
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    padding-left: 19px;
    font-size: var(--text-xs);
  }

  .to {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    flex-shrink: 0;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .fine {
    margin: 4px 8px 8px;
    font-size: var(--text-xs);
  }
</style>
