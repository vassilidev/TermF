<script lang="ts">
  import GitMerge from "@lucide/svelte/icons/git-merge";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import { invoke } from "@tauri-apps/api/core";
  import { ago, t } from "$lib/i18n";
  import { failed, refreshVault, say, vault } from "$lib/vault.svelte";
  import type { ConflictInfo } from "$lib/vault-logic";

  /** An element both machines changed: both versions are kept, the user chooses. Field names only, never a value. */
  let { key, updated, reauth }: { key: string; updated: number; reauth: () => void } = $props();

  let others = $state<ConflictInfo[]>([]);
  let busy = $state("");

  const names: Record<string, Parameters<typeof t>[0]> = {
    title: "vault3FieldTitle",
    project: "vault3FieldProject",
    username: "vault3FieldUsername",
    urls: "vault3FieldUrls",
    password: "vault3FieldPassword",
    totp: "vault3FieldTotp",
    notes: "vault3FieldNotes",
    agents: "vault3FieldAgents",
    kind: "vault3FieldKind",
    value: "vault3FieldValue",
    reference: "vault3FieldReference",
    description: "vault3FieldDescription",
  };
  const fields = (differs: string[]) => differs.map((d) => (names[d] ? t(names[d]) : d)).join(", ");

  async function load() {
    try {
      others = await invoke<ConflictInfo[]>("vault_conflicts", { key });
    } catch (e) {
      vault.error = failed(e);
    }
  }

  async function keep(id: string | null) {
    busy = id ?? "this";
    try {
      await invoke("vault_resolve", { key, keep: id });
      say(t("vault3Kept"));
      await refreshVault();
    } catch (e) {
      if (/needs the master password again/.test(String(e))) reauth();
      vault.error = failed(e);
    } finally {
      busy = "";
    }
  }

  load();
</script>

<section class="conflict" aria-labelledby="vault-conflict-title">
  <h3 id="vault-conflict-title"><GitMerge size={15} aria-hidden="true" />{t("vault3ConflictTitle")}</h3>
  <p class="muted">{t("vault3ConflictText")}</p>
  <ul class="versions">
    <li>
      <div class="what">
        <span class="name">{t("vault3ThisVersion")}</span>
        <span class="muted">{ago(updated)}</span>
      </div>
      <button class="btn sm" disabled={!!busy} onclick={() => keep(null)}>
        {#if busy === "this"}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}{t("vault3KeepThis")}
      </button>
    </li>
    {#each others as other (other.id)}
      {@const title = other.item?.title ?? other.secret?.name ?? ""}
      <li>
        <div class="what">
          <span class="name">{t("vault3OtherVersion", { when: ago(other.at) })}</span>
          <span class="muted">{[title, other.item?.username].filter(Boolean).join(" · ")}</span>
          {#if other.differs.length}<span class="differs">{t("vault3Differs", { fields: fields(other.differs) })}</span>{/if}
        </div>
        <button class="btn sm" disabled={!!busy} onclick={() => keep(other.id)}>
          {#if busy === other.id}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}{t("vault3KeepOther")}
        </button>
      </li>
    {/each}
  </ul>
</section>

<style>
  .conflict {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--warning);
    border-radius: var(--radius-lg);
    background: var(--waiting-soft);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3 :global(svg) {
    color: var(--warning);
  }

  p {
    margin: 0;
    font-size: var(--text-sm);
  }

  .versions {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .versions li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
  }

  .versions li + li {
    border-top: 1px solid var(--border);
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font-weight: 500;
  }

  .what .muted,
  .differs {
    font-size: var(--text-sm);
  }

  .differs {
    color: var(--warning);
    font-weight: 500;
  }
</style>
