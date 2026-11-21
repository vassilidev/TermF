<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { basicSetup } from "codemirror";
  import { EditorState, Prec, type Extension, type Text } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { HighlightStyle, LanguageDescription, syntaxHighlighting } from "@codemirror/language";
  import { languages } from "@codemirror/language-data";
  import { MergeView } from "@codemirror/merge";
  import { tags } from "@lezer/highlight";
  import { lang, t } from "$lib/i18n";
  import {
    ask,
    files,
    isMac,
    openExternal,
    refreshStatus,
    tabKey,
    type Content,
    type Tab,
  } from "$lib/files.svelte";

  const tab = $derived(files.active);

  // Colors come from CSS variables, so the editor follows the app's light/dark tokens.
  const theme = EditorView.theme(
    {
      "&": { height: "100%", backgroundColor: "var(--panel)", color: "var(--text)" },
      ".cm-scroller": { fontFamily: '"JetBrains Mono", Menlo, Consolas, monospace', fontSize: "13px" },
      ".cm-gutters": { backgroundColor: "var(--panel)", color: "var(--muted)", border: "none" },
      ".cm-activeLine, .cm-activeLineGutter": {
        backgroundColor: "color-mix(in srgb, var(--hover) 70%, transparent)",
      },
      ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)" },
      "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
        { backgroundColor: "color-mix(in srgb, var(--accent) 28%, transparent)" },
      ".cm-panels": { backgroundColor: "var(--bg)", color: "var(--text)" },
      ".cm-tooltip": { backgroundColor: "var(--panel)", border: "1px solid var(--border)" },
    },
    { dark: matchMedia("(prefers-color-scheme: dark)").matches },
  );
  const highlight = HighlightStyle.define([
    { tag: [tags.keyword, tags.operatorKeyword, tags.modifier], color: "var(--syn-keyword)" },
    { tag: [tags.string, tags.special(tags.string), tags.regexp], color: "var(--syn-string)" },
    { tag: [tags.comment, tags.meta], color: "var(--syn-comment)", fontStyle: "italic" },
    { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--syn-number)" },
    {
      tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
      color: "var(--syn-function)",
    },
    { tag: [tags.typeName, tags.className, tags.namespace, tags.tagName], color: "var(--syn-type)" },
    { tag: [tags.propertyName, tags.attributeName], color: "var(--syn-property)" },
    { tag: tags.heading, color: "var(--syn-keyword)", fontWeight: "600" },
    { tag: tags.link, textDecoration: "underline" },
    { tag: tags.invalid, color: "var(--danger)" },
  ]);
  const readOnly = [EditorState.readOnly.of(true), EditorView.editable.of(false)];

  // Per tab: editor state (undo history, selection) and the text last saved.
  const states = new Map<string, EditorState>();
  const saved = new Map<string, Text>();

  let host: HTMLDivElement;
  let diffHost: HTMLDivElement;
  let view: EditorView;
  let merge: MergeView | undefined;
  let shown = "";
  let headIssue = $state<Content | null>(null);
  let seq = 0;

  onMount(() => {
    view = new EditorView({ parent: host });
    return () => {
      view.destroy();
      merge?.destroy();
    };
  });

  async function language(name: string): Promise<Extension> {
    const description = LanguageDescription.matchFilename(languages, name);
    return description ? description.load().catch(() => []) : [];
  }

  async function stateFor(tab: Tab, text: string): Promise<EditorState> {
    const key = tabKey(tab);
    const existing = states.get(key);
    if (existing) return existing;
    const state = EditorState.create({
      doc: text,
      extensions: [
        basicSetup,
        theme,
        syntaxHighlighting(highlight),
        await language(tab.name),
        // Keep CRLF files CRLF on save.
        text.includes("\r\n") ? EditorState.lineSeparator.of("\r\n") : [],
        Prec.highest(keymap.of([{ key: "Mod-s", run: () => (save(), true) }])),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) tab.dirty = !u.state.doc.eq(saved.get(key)!);
        }),
      ],
    });
    // Two quick switches may build the same tab twice; keep the first.
    if (!states.has(key)) {
      states.set(key, state);
      saved.set(key, state.doc);
    }
    return states.get(key)!;
  }

  async function show(tab: Tab | null, diff: boolean, line: number | null) {
    const n = ++seq;
    merge?.destroy();
    merge = undefined;
    headIssue = null;
    if (!tab || tab.content.kind !== "text") return;
    const key = tabKey(tab);
    const state = await stateFor(tab, tab.content.text);
    if (n !== seq) return;
    if (shown !== key) {
      if (states.has(shown)) states.set(shown, view.state);
      view.setState(state);
      shown = key;
    }
    if (diff) return showDiff(tab, n);
    if (line !== null) {
      const pos = view.state.doc.line(Math.min(line, view.state.doc.lines)).from;
      view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
      tab.line = null;
    }
    view.focus();
  }

  async function showDiff(tab: Tab, n: number) {
    try {
      const head = await invoke<Content | null>("files_head", { repo: tab.repo, path: tab.path });
      const lang = await language(tab.name);
      if (n !== seq) return;
      if (head && head.kind !== "text") {
        headIssue = head;
        return;
      }
      const extensions = [basicSetup, theme, syntaxHighlighting(highlight), lang, readOnly];
      merge = new MergeView({
        a: { doc: head?.text ?? "", extensions },
        b: { doc: view.state.doc, extensions },
        parent: diffHost,
        collapseUnchanged: { margin: 3, minSize: 6 },
      });
    } catch (e) {
      files.error = String(e);
    }
  }

  async function save() {
    if (!tab || shown !== tabKey(tab)) return;
    const current = tab;
    const key = shown;
    const state = view.state;
    try {
      const write = (mtime: number | null) =>
        invoke<{ kind: "saved"; mtime: number } | { kind: "conflict" }>("files_write", {
          repo: current.repo,
          path: current.path,
          text: state.sliceDoc(),
          mtime,
        });
      let result = await write(current.mtime);
      if (result.kind === "conflict") {
        if (!(await ask(t("fileChanged", { name: current.name })))) return;
        result = await write(null);
      }
      if (result.kind === "saved") current.mtime = result.mtime;
      saved.set(key, state.doc);
      const latest = shown === key ? view.state : (states.get(key) ?? state);
      current.dirty = !latest.doc.eq(state.doc);
      files.error = "";
      refreshStatus();
    } catch (e) {
      files.error = String(e);
    }
  }

  $effect(() => {
    show(tab, tab?.diff ?? false, tab?.line ?? null);
  });

  $effect(() => {
    const open = new Set(files.tabs.map(tabKey));
    for (const key of states.keys()) {
      if (!open.has(key)) {
        states.delete(key);
        saved.delete(key);
        if (key === shown) shown = "";
      }
    }
  });
</script>

<section class="editor">
  {#if tab}
    <header class="toolbar">
      <span class="path" title={tab.path}>{tab.path}</span>
      {#if tab.content.kind === "text"}
        <button aria-pressed={tab.diff} onclick={() => tab && (tab.diff = !tab.diff)}>
          {t("fileDiff")}
        </button>
        <button disabled={!tab.dirty} onclick={save} title={isMac ? "⌘S" : "Ctrl+S"}>
          {t("fileSave")}
        </button>
      {/if}
      <button onclick={() => tab && openExternal(tab)}>{t("fileOpenExternal")}</button>
    </header>
  {/if}

  {#if !tab}
    <p class="message muted">{t("filesEmpty", { key: isMac ? "⌘P" : "Ctrl+P" })}</p>
  {:else}
    {@const issue = tab.content.kind === "text" ? (tab.diff ? headIssue : null) : tab.content}
    {#if issue?.kind === "binary"}
      <p class="message">{t("fileBinary")}</p>
    {:else if issue?.kind === "tooLarge"}
      {@const size = (issue.size / 1048576).toLocaleString(lang, { maximumFractionDigits: 1 })}
      <p class="message">{t("fileTooLarge", { size })}</p>
    {/if}
  {/if}

  <div class="host" bind:this={host} hidden={tab?.content.kind !== "text" || tab.diff}></div>
  {#if tab?.content.kind === "text" && tab.diff && !headIssue}
    <div class="sides muted">
      <span>HEAD</span><span>{t("fileWorkingCopy")}</span>
    </div>
  {/if}
  <div class="host" bind:this={diffHost} hidden={!tab?.diff || !!headIssue}></div>
</section>

<style>
  .editor {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    background: var(--panel);
    --syn-keyword: #cf222e;
    --syn-string: #0a3069;
    --syn-comment: #6e7781;
    --syn-number: #0550ae;
    --syn-function: #8250df;
    --syn-type: #953800;
    --syn-property: #0550ae;
  }

  @media (prefers-color-scheme: dark) {
    .editor {
      --syn-keyword: #ff7b72;
      --syn-string: #a5d6ff;
      --syn-comment: #8b949e;
      --syn-number: #79c0ff;
      --syn-function: #d2a8ff;
      --syn-type: #ffa657;
      --syn-property: #79c0ff;
    }
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
  }

  .path {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font-size: 12px;
  }

  .toolbar button {
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    font-size: 12px;
  }

  .toolbar button[aria-pressed="true"] {
    border-color: var(--accent);
    color: var(--accent);
  }

  .message {
    margin: auto;
    padding: 24px;
    text-align: center;
  }

  .host {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .host[hidden] {
    display: none;
  }

  .host :global(.cm-editor) {
    flex: 1;
    min-height: 0;
  }

  .host :global(.cm-mergeView) {
    flex: 1;
    min-height: 0;
  }

  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 2px 8px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
  }
</style>
