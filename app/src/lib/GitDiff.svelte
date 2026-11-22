<script lang="ts">
  import type { Snippet } from "svelte";
  import { t } from "$lib/i18n";
  import type { FileDiff, Hunk } from "$lib/git.svelte";
  import { span } from "$lib/git-logic";

  /** A file's diff, hunk by hunk. With `picked` bound, its changed lines can be picked: a click, or a range with Shift. */
  let {
    diff,
    picked = $bindable(),
    hunk,
  }: {
    diff: FileDiff;
    /** `hunk:line` keys; undefined when lines cannot be picked. */
    picked?: string[];
    /** Actions shown on each hunk's header. */
    hunk?: Snippet<[number, Hunk]>;
  } = $props();

  let anchor: { hunk: number; line: number } | null = null;

  function toggle(h: number, l: number, range: boolean) {
    if (!picked) return;
    const key = `${h}:${l}`;
    if (range && anchor?.hunk === h) {
      const keys = span(h, diff.hunks[h].lines, anchor.line, l);
      picked = [...new Set([...picked, ...keys])];
    } else {
      picked = picked.includes(key) ? picked.filter((k) => k !== key) : [...picked, key];
    }
    anchor = { hunk: h, line: l };
  }
</script>

{#if diff.binary}
  <p class="muted small">{t("git2Binary")}</p>
{:else if diff.cut}
  <p class="muted small">{t("git2TooLarge", { added: diff.added, removed: diff.removed })}</p>
{:else if diff.untracked.length && !diff.hunks.length}
  <p class="muted small">{t("git2UntrackedFolder", { count: diff.untracked.length })}</p>
  <ul class="files mono">
    {#each diff.untracked.slice(0, 50) as file (file)}<li>{file}</li>{/each}
  </ul>
  {#if diff.untracked.length > 50}<p class="muted small">{t("gitChangesMore", { count: diff.untracked.length - 50 })}</p>{/if}
{:else if !diff.hunks.length}
  <p class="muted small">{t("git2NoDiff")}</p>
{:else}
  <div class="diff">
    {#each diff.hunks as h, hi (hi)}
      <div class="hunk-head">
        <span class="mono range">@@ -{h.oldStart},{h.oldLines} +{h.newStart},{h.newLines} @@</span>
        <span class="section mono">{h.section}</span>
        {@render hunk?.(hi, h)}
      </div>
      {#each h.lines as line, li (li)}
        {@const kind = line.kind === "+" ? "add" : line.kind === "-" ? "del" : "ctx"}
        {#if picked && line.kind !== " "}
          {@const on = picked.includes(`${hi}:${li}`)}
          <button
            class="line {kind}"
            class:on
            aria-pressed={on}
            title={t("git2PickHint")}
            onclick={(e) => toggle(hi, li, e.shiftKey)}
          >
            <span class="no">{line.old ?? ""}</span><span class="no">{line.new ?? ""}</span><span class="sign">{line.kind}</span><span class="code">{line.text}{#if line.noEol}<span class="eol">{t("git2NoEol")}</span>{/if}</span>
          </button>
        {:else}
          <div class="line {kind}">
            <span class="no">{line.old ?? ""}</span><span class="no">{line.new ?? ""}</span><span class="sign">{line.kind}</span><span class="code">{line.text}{#if line.noEol}<span class="eol">{t("git2NoEol")}</span>{/if}</span>
          </div>
        {/if}
      {/each}
    {/each}
  </div>
{/if}

<style>
  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .files {
    margin: 6px 0 0;
    padding-left: 18px;
    color: var(--text);
  }

  .diff {
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .hunk-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    min-height: 30px;
    padding: 2px 8px;
    border-block: 1px solid var(--border);
    background: var(--chrome);
    color: var(--muted);
    font-family: var(--font-ui);
  }

  .hunk-head:first-child {
    border-top: 0;
  }

  .section {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hunk-head :global(.btn) {
    font-family: var(--font-ui);
  }

  .line {
    display: grid;
    grid-template-columns: 40px 40px 16px max-content;
    width: 100%;
    min-width: max-content;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: none;
    color: var(--text);
    font: inherit;
    line-height: 19px;
    text-align: left;
  }

  .no {
    padding-right: 8px;
    color: var(--muted);
    text-align: right;
    user-select: none;
  }

  .sign {
    color: var(--muted);
    user-select: none;
  }

  .code {
    padding-right: 12px;
    white-space: pre;
  }

  .add {
    background: color-mix(in srgb, var(--done) 12%, transparent);
  }

  .del {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
  }

  button.line {
    cursor: pointer;
  }

  button.line:hover {
    box-shadow: inset 3px 0 0 var(--border-strong);
  }

  button.line.on {
    box-shadow: inset 3px 0 0 var(--accent);
  }

  button.line.add.on {
    background: color-mix(in srgb, var(--done) 30%, transparent);
  }

  button.line.del.on {
    background: color-mix(in srgb, var(--danger) 30%, transparent);
  }

  .eol {
    margin-left: 8px;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    color: var(--muted);
    font-family: var(--font-ui);
    font-size: var(--text-xs);
  }
</style>
