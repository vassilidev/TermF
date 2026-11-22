<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import { ago, t } from "$lib/i18n";
  import type { Blame, FileHistory } from "$lib/git.svelte";
  import { runs, uncommitted } from "$lib/git-logic";

  /** A file's history (renames followed) and blame; a commit picked in either is shown by `onpick`. */
  let {
    dir,
    path,
    tab = $bindable(),
    onpick,
  }: {
    /** The repository or worktree folder. */
    dir: string;
    path: string;
    tab: "history" | "blame";
    onpick: (oid: string) => void;
  } = $props();

  const STEP = 100;
  let history = $state<FileHistory | null>(null);
  let blame = $state<Blame | null>(null);
  let limit = $state(STEP);
  let error = $state("");
  let seq = 0;

  $effect(() => {
    load(dir, path, tab, limit);
  });

  async function load(dir: string, path: string, tab: "history" | "blame", limit: number) {
    const n = ++seq;
    error = "";
    try {
      if (tab === "history") {
        const h = await invoke<FileHistory>("git_history", { repo: dir, path, limit });
        if (n === seq) history = h;
      } else {
        const b = await invoke<Blame>("git_blame", { repo: dir, path });
        if (n === seq) blame = b;
      }
    } catch (e) {
      if (n === seq) error = String(e);
    }
  }

  const short = (oid: string) => oid.slice(0, 7);
  const groups = $derived(blame ? runs(blame.lines) : []);
</script>

<div class="file-view">
  <Tabs
    id="git-file"
    label={path}
    tabs={[
      { id: "history", label: t("git2History") },
      { id: "blame", label: t("git2Blame") },
    ]}
    selected={tab}
    onselect={(id) => (tab = id as "history" | "blame")}
  >
    {#if error}
      <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{error}</span></p>
    {:else if tab === "history"}
      {#if !history}
        <Skeleton rows={6} label={t("gitLoading")} />
      {:else if !history.commits.length}
        <p class="muted small">{t("git2NoHistory")}</p>
      {:else}
        <ol class="history">
          {#each history.commits as c (c.oid)}
            <li>
              <button class="entry" onclick={() => onpick(c.oid)}>
                <span class="subject">{c.subject}</span>
                {#if c.path !== path}<span class="was mono" title={t("git2WasHint")}>{c.path}</span>{/if}
                <span class="muted who">{c.author} · {ago(c.date)}</span>
                <span class="mono muted">{short(c.oid)}</span>
              </button>
            </li>
          {/each}
        </ol>
        {#if history.more}<button class="btn sm more" onclick={() => (limit += STEP)}>{t("gitMore")}</button>{/if}
      {/if}
    {:else if !blame}
      <Skeleton shape="block" height="240px" label={t("gitLoading")} />
    {:else}
      <div class="blame">
        {#each groups as g (g.start)}
          {@const info = blame.commits[g.oid]}
          {@const local = uncommitted(g.oid)}
          <div class="run">
            <div class="who">
              {#if local}
                <span class="muted">{t("git2NotCommitted")}</span>
              {:else}
                <button class="sha mono" onclick={() => onpick(g.oid)} title={info?.subject}>{short(g.oid)}</button>
                <span class="author">{info?.author}</span>
                <span class="muted when">{info ? ago(info.date) : ""}</span>
                <span class="muted subject">{info?.subject}</span>
              {/if}
            </div>
            <div class="code">
              {#each blame.lines.slice(g.start, g.start + g.count) as line, i (i)}
                <div class="bl"><span class="no">{g.start + i + 1}</span><span class="text">{line.text}</span></div>
              {/each}
            </div>
          </div>
        {/each}
      </div>
      {#if blame.cut}<p class="muted small">{t("git2BlameCut", { count: blame.lines.length })}</p>{/if}
    {/if}
  </Tabs>
</div>

<style>
  /* Sized by its content: the detail column scrolls. */
  .file-view {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .small {
    margin: 10px 0 0;
    font-size: var(--text-sm);
  }

  .pre {
    white-space: pre-line;
  }

  .notice {
    margin-top: 10px;
  }

  .history {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: baseline;
    gap: 2px 12px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-bottom: 1px solid var(--border);
    background: none;
    color: var(--text);
    font-size: var(--text-sm);
    text-align: left;
  }

  .entry:hover {
    background: var(--hover);
  }

  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .was {
    grid-column: 1 / -1;
    grid-row: 2;
    color: var(--warning);
  }

  .more {
    display: flex;
    margin: 10px auto 0;
  }

  .blame {
    margin-top: 8px;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: var(--text-xs);
  }

  .run {
    display: grid;
    grid-template-columns: 260px max-content;
    min-width: max-content;
    border-top: 1px solid var(--border);
  }

  .run:first-child {
    border-top: 0;
  }

  .run .who {
    position: sticky;
    left: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    padding: 1px 8px;
    overflow: hidden;
    border-right: 1px solid var(--border);
    background: var(--chrome);
    white-space: nowrap;
  }

  .run .who .subject,
  .run .who .author {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .run .who .subject {
    flex: 1;
  }

  .sha {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    font-size: var(--text-xs);
  }

  .sha:hover {
    text-decoration: underline;
  }

  .code {
    font-family: var(--font-mono);
  }

  .bl {
    display: grid;
    grid-template-columns: 44px max-content;
    line-height: 19px;
  }

  .no {
    padding-right: 10px;
    color: var(--muted);
    text-align: right;
    user-select: none;
  }

  .text {
    padding-right: 12px;
    white-space: pre;
  }
</style>
