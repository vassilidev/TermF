<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Archive from "@lucide/svelte/icons/archive";
  import Check from "@lucide/svelte/icons/check";
  import FilePen from "@lucide/svelte/icons/file-pen";
  import History from "@lucide/svelte/icons/history";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import ScanText from "@lucide/svelte/icons/scan-text";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import GitDiff from "$lib/GitDiff.svelte";
  import { t } from "$lib/i18n";
  import { openAt } from "$lib/design.svelte";
  import type { Change, CommitDetail, DiffLine, FileDiff, Pick, Worktree } from "$lib/git.svelte";
  import { changeKind, joinMessage, picks as toPicks, sides, splitMessage, within } from "$lib/git-logic";

  type Notice = { kind: "ok" | "warn"; text: string };

  /** A worktree's changes: conflicts, staged and not, each file's diff, staging by file, hunk or line, the commit. */
  let {
    wt,
    repo,
    busy,
    act,
    ours,
    theirs,
    onfile,
  }: {
    wt: Worktree;
    /** The repository's folder, where TermF's editor opens files. */
    repo: string;
    busy: string;
    act: (name: string, run: () => Promise<Notice | void>) => Promise<void>;
    /** What each side of a conflict is: a branch or a commit. */
    ours: string;
    theirs: string;
    onfile: (path: string, tab: "history" | "blame") => void;
  } = $props();

  const groups = $derived(sides(wt.changes));
  let sel = $state<{ path: string; staged: boolean } | null>(null);
  let diff = $state<FileDiff | null>(null);
  let diffFor = "";
  let diffError = $state("");
  let picked = $state<string[]>([]);
  let subject = $state("");
  let body = $state("");
  let amend = $state(false);
  /** `filled`: the message Amend put in the form, if it did. */
  let amendOf = $state<{ head: string; remote: string | null; filled: { subject: string; body: string } | null } | null>(null);
  let stashing = $state<{ message: string; untracked: boolean } | null>(null);
  let discard = $state<{
    open: boolean;
    path: string;
    picks: Pick[] | null;
    id: string;
    lines: DiffLine[];
    untracked: string[];
    staged: boolean;
  } | null>(null);

  const row = $derived(sel ? (sel.staged ? groups.staged : groups.unstaged).find((c) => c.path === sel?.path) : undefined);
  const kindLabel = {
    conflict: "gitChangeConflict",
    untracked: "gitChangeUntracked",
    added: "gitChangeAdded",
    deleted: "gitChangeDeleted",
    renamed: "gitChangeRenamed",
    modified: "gitChangeModified",
  } as const;
  const kindLetter = { conflict: "!", untracked: "?", added: "A", deleted: "D", renamed: "R", modified: "M" } as const;

  let seen = "";
  $effect(() => {
    const path = wt.path;
    untrack(() => {
      if (path === seen) return;
      seen = path;
      [sel, diff, picked, subject, body, amend, amendOf, stashing] = [null, null, [], "", "", false, null, null];
      diffFor = "";
    });
  });

  let seq = 0;
  // Read again whenever the worktree is read again, so the diff never lags behind the list.
  $effect(() => {
    const [s, r, dir] = [sel, row, wt.path];
    untrack(() => {
      if (s && !r) sel = null;
      else load(dir, s, r);
    });
  });

  async function load(dir: string, s: typeof sel, r: Change | undefined) {
    const n = ++seq;
    if (!s || !r) {
      [diff, picked, diffFor] = [null, [], ""];
      return;
    }
    try {
      const d = await invoke<FileDiff>("git_diff", { repo: dir, path: s.path, from: s.staged ? r.from : null, staged: s.staged });
      if (n !== seq) return;
      const key = `${s.staged}:${s.path}:${d.id}`;
      if (key !== diffFor) picked = [];
      [diff, diffFor, diffError] = [d, key, ""];
    } catch (e) {
      if (n === seq) [diff, diffError] = [null, String(e)];
    }
  }

  function show(c: Change, staged: boolean) {
    sel = sel?.path === c.path && sel.staged === staged ? null : { path: c.path, staged };
  }

  const stage = (c: Change) =>
    act(`stage:${c.path}`, async () => {
      await invoke("git_stage", { repo: wt.path, path: c.path, picks: null, id: null });
    });

  const unstage = (c: Change) =>
    act(`unstage:${c.path}`, async () => {
      await invoke("git_unstage", { repo: wt.path, path: c.path, from: c.from, picks: null, id: null });
    });

  /** Stages (or unstages, on the staged side) picked lines or whole hunks of the file shown. */
  function stageParts(parts: Pick[]) {
    if (!diff || !sel) return;
    const [{ path, staged }, id, from] = [sel, diff.id, row?.from ?? null];
    return act("lines", async () => {
      if (staged) await invoke("git_unstage", { repo: wt.path, path, from, picks: parts, id });
      else await invoke("git_stage", { repo: wt.path, path, picks: parts, id });
      picked = [];
    });
  }

  /** What discarding would lose, read before asking: the file's (or the picked) changed lines, or its untracked files. */
  async function askDiscard(c: Change, parts: Pick[] | null) {
    let d = sel && !sel.staged && sel.path === c.path ? diff : null;
    if (!d) {
      try {
        d = await invoke<FileDiff>("git_diff", { repo: wt.path, path: c.path, from: null, staged: false });
      } catch (e) {
        return act("discard", () => Promise.reject(e));
      }
    }
    const lines = d.hunks.flatMap((h, hi) =>
      h.lines.filter((l, li) => l.kind !== " " && (!parts || parts.some((p) => p.hunk === hi && (!p.lines || p.lines.includes(li))))),
    );
    const staged = c.index !== "." && c.index !== "?";
    discard = { open: true, path: c.path, picks: parts, id: d.id, lines, untracked: d.untracked, staged };
  }

  function confirmDiscard() {
    const d = discard;
    if (!d) return;
    act(`discard:${d.path}`, async () => {
      await invoke("git_discard", { repo: wt.path, path: d.path, picks: d.picks, id: d.id });
      picked = [];
      return { kind: "ok", text: t("git2Discarded", { path: d.path }) };
    });
  }

  async function toggleAmend() {
    const filled = amendOf?.filled;
    amendOf = null;
    if (!amend) {
      // HEAD's message goes with Amend, unless it was edited: it would otherwise make a new commit.
      if (filled && subject === filled.subject && body === filled.body) [subject, body] = ["", ""];
      return;
    }
    await act("amend", async () => {
      const head = await invoke<CommitDetail>("git_show", { repo: wt.path, rev: "HEAD" });
      const filled = !subject.trim() && !head.remote ? splitMessage(head.message) : null;
      if (filled) ({ subject, body } = filled);
      amendOf = { head: head.commit.oid, remote: head.remote, filled };
    });
    if (!amendOf) amend = false;
  }

  function commit() {
    const [message, amended, head] = [joinMessage(subject, body), amend, amendOf?.head ?? null];
    act("commit", async () => {
      const oid = await invoke<string>("git_commit", { repo: wt.path, message, amend: amended, head: amended ? head : null });
      [subject, body, amend, amendOf] = ["", "", false, null];
      return { kind: "ok", text: t(amended ? "git2Amended" : "git2Committed", { sha: oid.slice(0, 7) }) };
    });
  }

  function stash() {
    const s = stashing;
    if (!s) return;
    act("stash", async () => {
      await invoke("git_stash_push", { repo: wt.path, message: s.message, untracked: s.untracked });
      stashing = null;
      return { kind: "ok", text: t("git2Stashed") };
    });
  }

  const resolve = (c: Change, how: "ours" | "theirs" | "edited") =>
    act(`resolve:${c.path}`, async () => {
      await invoke("git_resolve", { repo: wt.path, path: c.path, how });
      return { kind: "ok", text: t("git2ResolvedFile", { path: c.path }) };
    });

  /** The folder of this worktree inside the repository's, which TermF's editor can reach; null when outside. */
  const inRepo = $derived(within(repo, wt.path));
  const edit = (c: Change) => inRepo !== null && openAt(repo, inRepo ? `${inRepo}/${c.path}` : c.path, 1);
  const tracked = (c: Change) => c.index !== "?" && c.index !== "A";
  const canCommit = $derived(!!subject.trim() && !groups.conflicts.length && (amend ? !!amendOf && !amendOf.remote : groups.staged.length > 0));
</script>

{#snippet fileRow(c: Change, staged: boolean)}
  {@const kind = changeKind(c)}
  {@const open = sel?.path === c.path && sel.staged === staged}
  <li class:open>
    <button class="file" aria-expanded={open} onclick={() => show(c, staged)}>
      <span class="letter {kind}" title={t(kindLabel[kind])}>{kindLetter[kind]}</span>
      <span class="path mono">{#if c.from}<span class="muted">{`${c.from} → `}</span>{/if}{c.path}</span>
      <span class="muted kind">{t(kindLabel[kind])}</span>
    </button>
    {#if staged}
      <button class="icon-btn sm" onclick={() => unstage(c)} disabled={!!busy} title={t("git2Unstage")} aria-label={`${t("git2Unstage")} ${c.path}`}><Minus size={14} aria-hidden="true" /></button>
    {:else}
      <button class="icon-btn sm danger-hover" onclick={() => askDiscard(c, null)} disabled={!!busy} title={t("git2Discard")} aria-label={`${t("git2Discard")} ${c.path}`}><Undo2 size={14} aria-hidden="true" /></button>
      <button class="icon-btn sm" onclick={() => stage(c)} disabled={!!busy} title={t("git2Stage")} aria-label={`${t("git2Stage")} ${c.path}`}><Plus size={14} aria-hidden="true" /></button>
    {/if}
  </li>
  {#if open}
    <li class="diff-row">
      {#if diffError}
        <p class="notice error">{diffError}</p>
      {:else if !diff}
        <p class="muted small" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("git2ReadingDiff")}</p>
      {:else}
        {@const d = diff}
        <div class="diff-tools">
          <span class="stat"><span class="plus">+{d.added}</span> <span class="minus">−{d.removed}</span></span>
          {#if d.partial}
            <span class="muted small hint">{t("git2PickHelp")}</span>
          {:else if d.hunks.length}
            <span class="muted small hint">{t("git2WholeOnly")}</span>
          {/if}
          <span class="tools">
            {#if tracked(c)}
              <button class="btn sm quiet" onclick={() => onfile(c.path, "history")}><History size={14} aria-hidden="true" />{t("git2History")}</button>
              <button class="btn sm quiet" onclick={() => onfile(c.path, "blame")}><ScanText size={14} aria-hidden="true" />{t("git2Blame")}</button>
            {/if}
          </span>
        </div>
        {#if picked.length}
          <div class="picked-bar">
            <span>{t("git2Picked", { count: picked.length })}</span>
            {#if !staged}
              <button class="btn sm quiet danger" onclick={() => askDiscard(c, toPicks(picked))} disabled={!!busy}><Undo2 size={14} aria-hidden="true" />{t("git2DiscardLines")}</button>
            {/if}
            <button class="btn sm primary" onclick={() => stageParts(toPicks(picked))} disabled={!!busy}>
              {#if staged}<Minus size={14} aria-hidden="true" />{t("git2UnstageLines")}{:else}<Plus size={14} aria-hidden="true" />{t("git2StageLines")}{/if}
            </button>
            <button class="btn sm quiet" onclick={() => (picked = [])}>{t("git2Unpick")}</button>
          </div>
        {/if}
        {#if d.partial}
          <GitDiff diff={d} bind:picked>
            {#snippet hunk(hi)}
              {#if !staged}
                <button class="btn sm quiet danger" onclick={() => askDiscard(c, [{ hunk: hi, lines: null }])} disabled={!!busy}><Undo2 size={13} aria-hidden="true" />{t("git2DiscardHunk")}</button>
              {/if}
              <button class="btn sm" onclick={() => stageParts([{ hunk: hi, lines: null }])} disabled={!!busy}>
                {#if staged}<Minus size={13} aria-hidden="true" />{t("git2UnstageHunk")}{:else}<Plus size={13} aria-hidden="true" />{t("git2StageHunk")}{/if}
              </button>
            {/snippet}
          </GitDiff>
        {:else}
          <GitDiff diff={d} />
        {/if}
      {/if}
    </li>
  {/if}
{/snippet}

<div class="changes">
  {#if wt.error}
    <p class="notice error">{wt.error}</p>
  {:else if !wt.changes.length}
    <p class="muted small">{wt.prunable ? t("gitWorktreeGoneHint") : t("gitNoChanges")}</p>
  {/if}

  {#if groups.conflicts.length}
    <h3 class="group danger-text">{t("git2Conflicts")}<span class="n">{groups.conflicts.length}</span></h3>
    <p class="muted small">{t("git2ConflictsHelp", { ours, theirs })}</p>
    <ul class="list">
      {#each groups.conflicts as c (c.path)}
        <li class="conflict">
          <span class="letter conflict">!</span>
          <span class="path mono">{c.path}</span>
          <span class="conflict-actions">
            <button class="btn sm" onclick={() => resolve(c, "ours")} disabled={!!busy} title={t("git2KeepHint", { name: ours })}>{t("git2Keep", { name: ours })}</button>
            <button class="btn sm" onclick={() => resolve(c, "theirs")} disabled={!!busy} title={t("git2KeepHint", { name: theirs })}>{t("git2Keep", { name: theirs })}</button>
            {#if inRepo !== null}
              <button class="btn sm" onclick={() => edit(c)}><FilePen size={14} aria-hidden="true" />{t("git2Edit")}</button>
            {/if}
            <button class="btn sm primary" onclick={() => resolve(c, "edited")} disabled={!!busy} title={t("git2MarkHint")}><Check size={14} aria-hidden="true" />{t("git2Mark")}</button>
          </span>
        </li>
      {/each}
    </ul>
  {/if}

  {#if groups.staged.length}
    <h3 class="group">{t("git2Staged")}<span class="n">{groups.staged.length}</span></h3>
    <ul class="list">
      {#each groups.staged as c (c.index + c.tree + c.path)}{@render fileRow(c, true)}{/each}
    </ul>
  {/if}

  <!-- During an operation, Continue makes its commit. -->
  {#if !wt.prunable && !wt.error && !wt.operation && (wt.head || groups.staged.length)}
    <form class="commit" onsubmit={(e) => (e.preventDefault(), canCommit && commit())}>
      <input class="field" bind:value={subject} placeholder={t("git2Subject")} aria-label={t("git2Subject")} spellcheck="true" />
      <textarea class="field body" bind:value={body} placeholder={t("git2Body")} aria-label={t("git2Body")} rows="3"></textarea>
      {#if amend && amendOf?.remote}
        <p class="notice warn">{t("git2AmendPushed", { remote: amendOf.remote })}</p>
      {/if}
      <div class="commit-row">
        {#if wt.head}
          <label class="check"><input type="checkbox" bind:checked={amend} onchange={toggleAmend} disabled={!!busy} />{t("git2Amend")}</label>
        {/if}
        <span class="muted small">{groups.conflicts.length ? t("git2CommitConflicts") : amend ? t("git2AmendHint") : !groups.staged.length ? t("git2NothingStaged") : ""}</span>
        <button class="btn primary" disabled={!canCommit || !!busy}>
          {#if busy === "commit"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t(amend ? "git2AmendButton" : "git2CommitButton")}
        </button>
      </div>
    </form>
  {/if}

  {#if groups.unstaged.length}
    <h3 class="group">
      {t("git2Unstaged")}<span class="n">{groups.unstaged.length}</span>
      {#if !stashing}
        <button class="btn sm quiet stash" onclick={() => (stashing = { message: "", untracked: false })} disabled={!!busy} title={t("uxfT3StashHint")}><Archive size={14} aria-hidden="true" />{t("git2Stash")}</button>
      {/if}
    </h3>
    {#if stashing}
      {@const s = stashing}
      <form class="inline-form" onsubmit={(e) => (e.preventDefault(), stash())}>
        <label class="labelled grow">
          <span>{t("git2StashMessage")}</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="field" bind:value={s.message} placeholder={t("git2StashPlaceholder")} autocomplete="off" autofocus />
        </label>
        <label class="check"><input type="checkbox" bind:checked={s.untracked} />{t("git2StashUntracked")}</label>
        <div class="buttons">
          <button class="btn sm" type="button" onclick={() => (stashing = null)}>{t("cancel")}</button>
          <button class="btn sm primary" disabled={!!busy}>{t("git2Stash")}</button>
        </div>
      </form>
    {/if}
    <ul class="list">
      {#each groups.unstaged as c (c.index + c.tree + c.path)}{@render fileRow(c, false)}{/each}
    </ul>
  {/if}
  {#if wt.more}<p class="muted small">{t("gitChangesMore", { count: wt.more })}</p>{/if}
</div>

{#if discard}
  {@const d = discard}
  <ConfirmDialog
    bind:open={d.open}
    title={t(d.untracked.length ? "git2DiscardUntrackedTitle" : d.picks ? "git2DiscardLinesTitle" : "git2DiscardTitle", { path: d.path, count: d.lines.length })}
    confirm={t(d.untracked.length ? "gitDelete" : "git2Discard")}
    danger
    onconfirm={confirmDiscard}
  >
    {#if d.untracked.length}
      <p class="lost">{t("git2LostUntracked", { count: d.untracked.length })}</p>
      <ul class="lost-list mono">
        {#each d.untracked.slice(0, 20) as file (file)}<li>{file}</li>{/each}
      </ul>
      {#if d.untracked.length > 20}<p>{t("gitChangesMore", { count: d.untracked.length - 20 })}</p>{/if}
    {:else}
      <p class="lost">{t("git2LostLines", { added: d.lines.filter((l) => l.kind === "+").length, removed: d.lines.filter((l) => l.kind === "-").length })}</p>
      <pre class="lost-list">{#each d.lines.slice(0, 30) as l, i (i)}<span class={l.kind === "+" ? "plus" : "minus"}>{l.kind}{l.text}</span>{"\n"}{/each}</pre>
      {#if d.lines.length > 30}<p>{t("gitChangesMore", { count: d.lines.length - 30 })}</p>{/if}
      {#if d.staged}<p>{t("git2StagedStays")}</p>{/if}
    {/if}
  </ConfirmDialog>
{/if}

<style>
  .changes {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .group {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 12px 0 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .group .n {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }

  .danger-text {
    color: var(--danger);
  }

  .stash {
    margin-left: auto;
  }

  .list {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .list > li {
    display: flex;
    align-items: center;
    gap: 4px;
    border-bottom: 1px solid var(--border);
  }

  .list > li.open {
    background: var(--selected);
  }

  .file {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 5px 4px;
    border: 0;
    background: none;
    color: var(--text);
    text-align: left;
  }

  .file:hover {
    background: var(--hover);
  }

  .list > li.diff-row {
    display: block;
    padding: 8px 0 12px;
    background: none;
  }

  .list > li.conflict {
    flex-wrap: wrap;
    gap: 6px 10px;
    padding: 6px 4px;
  }

  li.conflict .path {
    flex: 1 1 240px;
    color: var(--danger);
    font-weight: 600;
  }

  .conflict-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-left: auto;
  }

  .letter {
    display: inline-grid;
    flex-shrink: 0;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: var(--radius-sm);
    background: var(--hover);
    color: var(--muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .letter.added,
  .letter.untracked {
    background: var(--done-soft);
    color: var(--done);
  }

  .letter.modified,
  .letter.renamed {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .letter.deleted,
  .letter.conflict {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .path {
    flex: 1;
    min-width: 0;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .kind {
    flex-shrink: 0;
    font-size: var(--text-xs);
  }

  .danger-hover:not(:disabled):hover {
    border-color: var(--danger);
    color: var(--danger);
  }

  .diff-tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    margin-bottom: 6px;
  }

  .stat {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .plus {
    color: var(--done);
  }

  .minus {
    color: var(--danger);
  }

  .hint {
    flex: 1;
    min-width: 160px;
  }

  .tools {
    display: flex;
    gap: 4px;
    margin-left: auto;
  }

  .picked-bar {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    padding: 6px 8px;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    background: var(--panel);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .picked-bar span {
    flex: 1;
  }

  .commit {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 8px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .body {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
    font: inherit;
  }

  .commit-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }

  .commit-row .small {
    flex: 1;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .inline-form {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .inline-form .check {
    height: 30px;
  }

  .grow {
    flex: 1;
    min-width: 220px;
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .lost {
    color: var(--danger);
    font-weight: 600;
  }

  .lost-list {
    max-height: 200px;
    margin: 0;
    padding: 8px 10px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    list-style: none;
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
</style>
