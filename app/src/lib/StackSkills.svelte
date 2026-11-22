<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Layers from "@lucide/svelte/icons/layers";
  import { t } from "$lib/i18n";
  import { library, packageLabel, type Item, type RepoStack } from "$lib/library.svelte";
  import { shortPath } from "$lib/state.svelte";

  let {
    project,
    stacks,
    ondecide,
  }: {
    project: string;
    stacks: RepoStack[];
    /** Records the owner's decision in the assistant's journal. */
    ondecide: (decision: "accepted" | "rejected", detail: string) => Promise<void>;
  } = $props();

  let items = $state<Item[]>([]);
  let busy = $state("");
  let error = $state("");

  async function load() {
    try {
      items = await invoke<Item[]>("library_list", { kind: "skill", project });
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void project;
    void library.version;
    load();
  });

  /** What the stack switches on, what installed packages bring, and what the owner removed from either. */
  const shown = $derived(items.filter((i) => i.activatedBy.length || i.layer === "package"));
  const untrusted = $derived(stacks.some((r) => !r.trusted && r.skills.length));

  /** `null` puts back what the stack or the trust decides. */
  async function set(item: Item, enabled: boolean | null) {
    busy = item.name;
    error = "";
    try {
      await invoke("library_toggle", { kind: "skill", name: item.name, project, enabled });
      await ondecide(enabled === false ? "rejected" : "accepted", `${enabled === false ? "-" : "+"}${item.name}`);
      library.version++;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  function origin(item: Item): string {
    if (item.package) return t("stackFromPackage", { package: packageLabel(item.package) });
    return item.activatedBy.includes("*") ? t("stackForEvery") : t("stackActivatedBy");
  }
</script>

<article class="proposal">
  <h3><Layers size={15} aria-hidden="true" />{t("stackSkillsTitle")}</h3>
  <p class="hint">{t("stackSkillsHint")}</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}

  <ul class="detected">
    {#each stacks as repo (repo.path)}
      <li>
        <b title={shortPath(repo.path)}>{repo.name}</b>
        {#each repo.tags as tag (tag.name)}
          <span class="badge" title={tag.source}>{tag.name}{#if tag.version}<span class="version">{tag.version}</span>{/if}</span>
        {:else}
          <span class="muted">{t("stackNoTag")}</span>
        {/each}
      </li>
    {/each}
  </ul>

  <ul class="skills">
    {#each shown as item (item.name)}
      <li class:off={!item.active}>
        <div class="what">
          <b>{item.name}</b>
          <span class="origin">
            {origin(item)}
            {#if !item.package && !item.activatedBy.includes("*")}
              {#each item.activatedBy as tag (tag)}<span class="badge">{tag}</span>{/each}
            {/if}
          </span>
          {#if item.description}<span class="desc">{item.description}</span>{/if}
        </div>
        {#if item.active}
          <button class="btn sm" disabled={!!busy} onclick={() => set(item, false)}>{t("stackRemove")}</button>
        {:else if item.toggles.project === false}
          <span class="muted">{t("stackRemoved")}</span>
          <button class="btn sm" disabled={!!busy} onclick={() => set(item, null)}>{t("stackPutBack")}</button>
        {:else}
          <button class="btn sm" disabled={!!busy} onclick={() => set(item, true)}>{t("stackTurnOn")}</button>
        {/if}
      </li>
    {:else}
      <li class="muted">{t("stackNoSkill")}</li>
    {/each}
  </ul>
  {#if untrusted}<p class="hint">{t("stackUntrusted")}</p>{/if}
</article>

<style>
  .proposal {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .hint,
  .error {
    margin: 0;
    color: var(--muted);
    line-height: 1.5;
  }

  .error {
    color: var(--danger-text);
  }

  .detected,
  .skills {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .detected li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .detected b {
    min-width: 80px;
    font-weight: 600;
  }

  .version {
    margin-left: 4px;
    color: var(--muted);
    font-family: var(--font-mono);
  }

  .skills li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }

  .skills li.muted {
    border-style: dashed;
  }

  .skills li.off .what b {
    color: var(--muted);
  }

  .what {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 8px;
    min-width: 0;
  }

  .what b {
    font-weight: 600;
  }

  .origin {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    color: var(--muted);
  }

  .desc {
    flex-basis: 100%;
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
