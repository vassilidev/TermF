<script lang="ts">
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Check from "@lucide/svelte/icons/check";
  import Pencil from "@lucide/svelte/icons/pencil";
  import { t } from "$lib/i18n";
  import { lineDiff } from "$lib/library.svelte";
  import { keepReport, sync, syncCall, type Conflict, type Report } from "$lib/sync.svelte";

  let { conflict, onback }: { conflict: Conflict; onback: () => void } = $props();

  let editing = $state(false);
  let text = $state("");
  const lines = $derived(lineDiff(conflict.local ?? "", conflict.remote ?? ""));
  // A key is never merged: one of the two pairs is kept.
  const key = $derived(conflict.path.startsWith("ssh/keys/"));

  async function keep(resolution: { keep: "local" | "remote" } | { keep: "merged"; text: string }) {
    const report = await syncCall<Report>("sync_resolve", { path: conflict.path, resolution });
    if (!report) return;
    keepReport(report);
    onback();
  }
</script>

{#snippet side(label: Parameters<typeof t>[0], body: string | null, hidden: "+" | "-", which: "local" | "remote")}
  <div class="side">
    <h4>{t(label)}</h4>
    {#if body === null}
      <p class="deleted">{t("syncDeleted")}</p>
    {:else}
      <pre class="code">{#each lines as line, i (i)}{#if line.op !== hidden}<span class="l" class:differs={line.op !== " "}
            >{line.text}
</span>{/if}{/each}</pre>
    {/if}
    <button class="btn" onclick={() => keep({ keep: which })} disabled={!!sync.busy}>
      <Check size={14} aria-hidden="true" />{t("syncKeep")}
    </button>
  </div>
{/snippet}

<div class="resolver">
  <header>
    <button class="btn sm quiet" onclick={onback}><ArrowLeft size={14} aria-hidden="true" />{t("syncBack")}</button>
    <h3>{t("syncConflictTitle")} <span class="mono">{conflict.path}</span></h3>
  </header>
  <p class="help">
    {conflict.keys.length ? t("syncConflictKeys", { keys: conflict.keys.join(", ") }) : t("syncConflictWhole")}
  </p>

  <div class="sides">
    {@render side("syncLocal", conflict.local, "+", "local")}
    {@render side("syncRemoteSide", conflict.remote, "-", "remote")}
  </div>

  {#if key}
    <p class="help">{t("sync2KeyConflict")}</p>
  {:else if editing}
    <div class="merged">
      <h4>{t("syncMerged")}</h4>
      <p class="help">{t("syncMergeHelp")}</p>
      <textarea class="field code" rows="14" spellcheck="false" bind:value={text} aria-label={t("syncMerged")}></textarea>
      <div class="buttons">
        <button class="btn" onclick={() => (editing = false)}>{t("cancel")}</button>
        <button class="btn primary" onclick={() => keep({ keep: "merged", text })} disabled={!!sync.busy}>
          {t("syncKeepMerged")}
        </button>
      </div>
    </div>
  {:else}
    <button
      class="btn"
      onclick={() => {
        text = conflict.local ?? conflict.remote ?? "";
        editing = true;
      }}
    >
      <Pencil size={14} aria-hidden="true" />{t("syncMergeEdit")}
    </button>
  {/if}
</div>

<style>
  .resolver {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
    max-width: none;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }

  header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h4 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .mono {
    font: 500 var(--text-sm) var(--font-mono);
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .sides {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
    width: 100%;
  }

  .side,
  .merged {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    min-width: 0;
  }

  .merged {
    width: 100%;
  }

  .code {
    width: 100%;
    max-height: 420px;
    margin: 0;
    padding: 10px 0;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre;
  }

  .l {
    display: block;
    padding: 0 12px;
  }

  .differs {
    background: var(--waiting-soft);
  }

  .deleted {
    width: 100%;
    margin: 0;
    padding: 16px 12px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    color: var(--muted);
    font-size: var(--text-sm);
  }

  textarea.code {
    height: auto;
    padding: 10px 12px;
    color: var(--text);
    resize: vertical;
  }

  .buttons {
    display: flex;
    gap: 8px;
    align-self: flex-end;
  }
</style>
