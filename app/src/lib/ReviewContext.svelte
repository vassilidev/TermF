<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import { ticketKey } from "$lib/TeamRunDetail.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { lang, t } from "$lib/i18n";
  import { team } from "$lib/team.svelte";
  import { openUrl, tickets } from "$lib/tickets.svelte";
  import { diffStats, percent, type FileDiff, type Review, type Screen, type Version } from "$lib/visual-logic";

  /** What goes with a screen: its ticket or pull request, why it changed, what changed, its texts; `files`: the code too. */
  let { review, version, screen, files = false }: { review: Review; version: Version; screen: Screen; files?: boolean } = $props();

  /** A diff this long is cut: the file is better read in an editor. */
  const LINES = 400;

  const run = $derived(review.run === null ? undefined : tickets.runs.find((r) => r.id === review.run));
  const why = $derived(review.run === null ? undefined : team.runs.find((r) => r.run === review.run)?.plan?.summary);
  const zones = $derived(screen.rects.length);

  let diffs = $state<FileDiff[] | null>(null);
  let error = $state("");
  // Read again for another version only, not each time a decision updates the review.
  const id = $derived(review.id);
  const n = $derived(version.n);

  $effect(() => {
    if (!files) return;
    let live = true;
    diffs = null;
    error = "";
    invoke<FileDiff[]>("visual_files", { id, version: n }).then(
      (list) => live && (diffs = list),
      (e) => live && (error = String(e)),
    );
    return () => (live = false);
  });

  const justification = (by: string | null) =>
    by === null ? t("review5bUnjustified") : t("review5bJustified", { by: by === "ticket" ? t("review5bByTicket") : by });
</script>

<div class="context">
  {#if review.ticket || run?.prUrl}
    <p class="links">
      {#if review.ticket}
        {#if run?.ticket.url}
          <button class="btn sm quiet" onclick={() => run && openUrl(run.ticket.url)}>
            <ExternalLink size={13} aria-hidden="true" /><span class="mono">{ticketKey(review.ticket)}</span>
          </button>
        {:else}
          <span class="mono">{ticketKey(review.ticket)}</span>
        {/if}
      {/if}
      {#if run?.prUrl}
        {@const pr = run.prUrl}
        <button class="btn sm quiet" onclick={() => openUrl(pr)}>
          <GitPullRequest size={13} aria-hidden="true" />{t("review5bPr")}
        </button>
      {/if}
    </p>
  {/if}
  {#if why}
    <section>
      <h4>{t("review5bWhy")}</h4>
      <p>{why}</p>
    </section>
  {/if}

  <section>
    <h4>{t("review5bChanged")}</h4>
    <p>
      {#if screen.error}
        {t("review5bScreenFailed")}
      {:else if !screen.before && screen.after}
        {t("practicalVisualAfterOnly")}
      {:else if !screen.changed}
        {t("review5bSame")}
      {:else if zones}
        {zones === 1 ? t("review5bZone") : t("review5bZones", { count: zones })} · {t("review5bPixels", { percent: percent(screen.ratio, lang) })}
      {:else}
        {t("review5bTextsOnly")}
      {/if}
    </p>
  </section>

  {#if screen.texts.length}
    <section>
      <h4>{t("review5bTexts", { count: screen.texts.length })}</h4>
      <ul class="texts">
        {#each screen.texts as text, i (i)}
          <li class={text.change}>
            <span class="sign" aria-hidden="true">{text.change === "added" ? "+" : "−"}</span>
            <span class="sr-only">{t(text.change === "added" ? "review5bAdded" : "review5bRemoved")}</span>
            <span class="text">{t("review5bQuote", { text: text.text })}</span>
            <span class="by" class:missing={text.justifiedBy === null}>↳ {justification(text.justifiedBy)}</span>
            {#if text.flagged}
              <span class="flag"><TriangleAlert size={12} aria-hidden="true" />{t("review5bFlagged")}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if files}
    <section>
      <h4>{diffs ? t("review5bFiles", { count: diffs.length }) : t("review5bFilesTitle")}</h4>
      {#if error}
        <p class="notice error">{error}</p>
      {:else if diffs === null}
        <Skeleton label={t("review5bLoading")} />
      {:else if !diffs.length}
        <p class="muted">{t("review5bFilesNone")}</p>
      {:else}
        <ul class="files">
          {#each diffs as file (file.path)}
            {@const stats = diffStats(file.diff)}
            {@const lines = file.diff.replace(/\n$/, "").split("\n")}
            <li>
              <details>
                <summary>
                  <span class="mono path">{file.path}</span>
                  <span class="mono add">+{stats.added}</span>
                  <span class="mono del">−{stats.removed}</span>
                </summary>
                <pre class="diff">{#each lines.slice(0, LINES) as line, i (i)}<span
                      class="l{line[0] === '+' && !line.startsWith('+++') ? ' add' : line[0] === '-' && !line.startsWith('---') ? ' del' : ''}"
                      >{line}
</span>{/each}</pre>
                {#if lines.length > LINES}<p class="muted more">{t("review5bMoreLines", { count: lines.length - LINES })}</p>{/if}
              </details>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .context {
    display: flex;
    flex-direction: column;
    gap: 16px;
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  h4 {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  p {
    margin: 0;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-left: -8px;
  }

  .texts,
  .files {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .texts li {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    column-gap: 4px;
  }

  .texts li > :not(.sign, .sr-only) {
    grid-column: 2;
  }

  .sign {
    font: 600 var(--text-sm) var(--font-mono);
  }

  .added .sign {
    color: var(--done);
  }

  .removed .sign {
    color: var(--danger);
  }

  .removed .text {
    color: var(--muted);
    text-decoration: line-through;
  }

  .text {
    font-weight: 550;
    overflow-wrap: anywhere;
  }

  .by {
    color: var(--muted);
  }

  .by.missing,
  .flag {
    color: var(--warning);
  }

  .flag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  summary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    cursor: pointer;
  }

  .path {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  summary .add {
    color: var(--done);
  }

  summary .del {
    color: var(--danger);
  }

  .diff {
    max-height: 320px;
    margin: 4px 0 0;
    padding: 8px 10px;
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

  .diff .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .diff .del {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .more {
    margin-top: 4px;
  }
</style>
