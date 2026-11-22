<script lang="ts">
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import { t } from "$lib/i18n";
  import { change, commit, discard, type Preview } from "$lib/claude.svelte";

  /** The diff of a change before it is written: the settings change in waiting, or one given by `preview`. */
  let {
    preview: given,
    error: givenError = "",
    saving: givenSaving = false,
    onsave,
    oncancel,
  }: {
    preview?: Preview | null;
    error?: string;
    saving?: boolean;
    onsave?: () => void;
    oncancel?: () => void;
  } = $props();

  const own = $derived(given !== undefined);
  const preview = $derived(own ? (given ?? null) : change.preview);
  const error = $derived(own ? givenError : change.error);
  const saving = $derived(own ? givenSaving : change.saving);
  const lines = $derived(preview ? preview.diff.replace(/\n$/, "").split("\n") : []);
</script>

{#if preview || error}
  <div class="change" role="region" aria-label={t("claudeMgmtChange")}>
    {#if error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    {#if preview}
      <p class="file">{t("claudeViewPreviewOf", { path: preview.path })}</p>
      {#if preview.diff}
        <pre class="diff">{#each lines as line, i (i)}<span class="l{line[0] === '+' ? ' add' : line[0] === '-' ? ' del' : ''}"
              >{line}
</span>{/each}</pre>
      {:else}
        <p class="notice"><Check size={14} aria-hidden="true" />{t("claudeViewNoChange")}</p>
      {/if}
    {/if}
    <div class="buttons">
      <button type="button" class="btn sm" onclick={oncancel ?? discard}>{t("cancel")}</button>
      <button type="button" class="btn sm primary" disabled={!preview?.diff || saving} onclick={onsave ?? commit}>{t("save")}</button>
    </div>
  </div>
{/if}

<style>
  .change {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 8px 0 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .file {
    margin: 0;
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .diff {
    max-height: 280px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre;
  }

  .l {
    display: block;
  }

  .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .del {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
