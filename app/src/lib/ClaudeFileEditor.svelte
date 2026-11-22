<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Pencil from "@lucide/svelte/icons/pencil";
  import X from "@lucide/svelte/icons/x";
  import ClaudeChange from "$lib/ClaudeChange.svelte";
  import { t } from "$lib/i18n";
  import { claudeView, type Preview } from "$lib/claude.svelte";
  import { fileName, refreshFiles, repoArg, type FileText } from "$lib/claude-native.svelte";

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
  /** The file as the editor read it; null: a file to create. */
  let hash = $state<string | null>(null);
  let preview = $state<Preview | null>(null);
  let error = $state("");
  let busy = $state(false);
  /** Still to be created. */
  let fresh = $state(false);
  /** A skill is known by its folder. */
  const title = $derived(/[/\\]SKILL\.md$/.test(path) ? fileName(path.replace(/[/\\]SKILL\.md$/, "")) : fileName(path));

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
      hash = fresh ? null : real.hash;
      editing = true;
    } catch (e) {
      error = String(e);
    }
  }

  async function showPreview() {
    error = "";
    try {
      preview = await invoke<Preview>("claude_file_preview", { path, text, hash, repo: repoArg() });
    } catch (e) {
      preview = null;
      error = String(e);
    }
  }

  async function save() {
    busy = true;
    try {
      await invoke<Preview>("claude_file_save", { path, text, hash, repo: repoArg() });
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

<section class="viewer" aria-label={title}>
  <header>
    <div class="what">
      <h3>{title}</h3>
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

  {#if editing}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        showPreview();
      }}
    >
      <label class="sr-only" for="claude-file-text">{fileName(path)}</label>
      <textarea
        id="claude-file-text"
        class="field mono"
        bind:value={text}
        oninput={() => ((preview = null), (error = ""))}
        rows="16"
        spellcheck="false"
      ></textarea>
      {#if preview || error}
        <ClaudeChange {preview} {error} saving={busy} onsave={save} oncancel={() => ((preview = null), (error = ""))} />
      {:else}
        <div class="buttons">
          <span class="muted hint">{t("claudeDgEditHint")}</span>
          <button type="button" class="btn sm" onclick={() => (fresh ? onclose() : load())}>{t("cancel")}</button>
          <button type="submit" class="btn sm primary">{t("claudeViewPreview")}</button>
        </div>
      {/if}
    </form>
  {:else}
    {#if error}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    {#if file}
      {#if file.text.trim()}
        <pre class="code">{file.text}</pre>
      {:else}
        <p class="muted empty">{t("claudeDgEmptyFile")}</p>
      {/if}
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
