<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Pencil from "@lucide/svelte/icons/pencil";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";
  import { claudeView } from "$lib/claude.svelte";
  import { diffLines, fileName, refreshFiles, repoArg, type Diff, type FileText } from "$lib/claude-native.svelte";

  /** One of Claude Code's Markdown files: shown masked, edited once you ask (the real text), saved after its diff. */
  let {
    path,
    create = false,
    onclose,
  }: {
    path: string;
    /** Not there yet: editing starts at once, and saving creates it. */
    create?: boolean;
    onclose: () => void;
  } = $props();

  let file = $state<FileText | null>(null);
  let editing = $state(false);
  let text = $state("");
  /** What the editor started from; null: a file to create. */
  let base = $state<string | null>(null);
  let preview = $state<Diff | null>(null);
  let error = $state("");
  let busy = $state(false);
  /** Still to be created. */
  let fresh = $state(false);

  $effect(() => {
    void path;
    fresh = create;
    load();
  });

  async function load() {
    editing = false;
    preview = null;
    error = "";
    try {
      file = await invoke<FileText>("claude_file_read", { path, reveal: false, repo: repoArg() });
      if (fresh && !file.readOnly) await edit();
    } catch (e) {
      file = null;
      error = String(e);
    }
  }

  async function edit() {
    error = "";
    try {
      const real = await invoke<FileText>("claude_file_read", { path, reveal: true, repo: repoArg() });
      text = real.text;
      base = fresh ? null : real.text;
      editing = true;
    } catch (e) {
      error = String(e);
    }
  }

  async function showPreview() {
    error = "";
    try {
      preview = await invoke<Diff>("claude_file_preview", { path, text, base, repo: repoArg() });
    } catch (e) {
      preview = null;
      error = String(e);
    }
  }

  async function save() {
    busy = true;
    try {
      await invoke<Diff>("claude_file_save", { path, text, base, repo: repoArg() });
      claudeView.done = t("claudeDgSaved", { path });
      fresh = false;
      await refreshFiles();
      await load();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<section class="viewer" aria-label={fileName(path)}>
  <header>
    <div class="what">
      <h3>{fileName(path)}</h3>
      <span class="mono muted path" title={path}>{path}</span>
    </div>
    {#if file?.readOnly}<span class="badge">{t("claudeViewReadOnly")}</span>{/if}
    {#if file?.masked && !editing}<span class="badge warn">{t("claudeDgMasked")}</span>{/if}
    {#if file && !file.readOnly && !editing}
      <button class="btn sm" onclick={edit}><Pencil size={13} aria-hidden="true" />{t("claudeDgEdit")}</button>
    {/if}
    <button class="icon-btn sm" onclick={onclose} aria-label={t("claudeDgClose")} title={t("claudeDgClose")}>
      <X size={14} aria-hidden="true" />
    </button>
  </header>

  {#if error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}

  {#if editing}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        showPreview();
      }}
    >
      <label class="sr-only" for="claude-file-text">{fileName(path)}</label>
      <textarea id="claude-file-text" class="field mono" bind:value={text} oninput={() => (preview = null)} rows="16" spellcheck="false"></textarea>
      {#if preview}
        {#if preview.diff}
          <pre class="code diff">{#each diffLines(preview.diff) as line, i (i)}<span
                class="l{line[0] === '+' ? ' add' : line[0] === '-' ? ' del' : ''}">{line}
</span>{/each}</pre>
        {:else}
          <p class="notice"><Check size={14} aria-hidden="true" />{t("claudeViewNoChange")}</p>
        {/if}
      {/if}
      <div class="buttons">
        <span class="muted hint">{t("claudeDgEditHint")}</span>
        <button type="button" class="btn sm" onclick={() => (fresh ? onclose() : load())}>{t("cancel")}</button>
        <button type="submit" class="btn sm">{t("claudeViewPreview")}</button>
        <button type="button" class="btn sm primary" disabled={!preview?.diff || busy} onclick={save}>
          {fresh ? t("claudeDgCreate") : t("save")}
        </button>
      </div>
    </form>
  {:else if file}
    {#if file.text.trim()}
      <pre class="code">{file.text}</pre>
    {:else}
      <p class="muted empty">{t("claudeDgEmptyFile")}</p>
    {/if}
  {/if}
</section>

<style>
  .viewer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0 0 20px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .what {
    display: flex;
    flex: 1;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
    white-space: nowrap;
  }

  .path {
    overflow: hidden;
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge.warn {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  textarea {
    height: auto;
    min-height: 220px;
    padding: 8px 10px;
    font-size: var(--text-sm);
    line-height: 1.55;
    resize: vertical;
  }

  .code {
    max-height: 360px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .diff {
    white-space: pre;
  }

  .diff .l {
    display: block;
  }

  .diff .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .diff .del {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .hint {
    flex: 1;
    font-size: var(--text-xs);
  }

  .empty {
    margin: 0;
  }
</style>
