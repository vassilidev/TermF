<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import BrushCleaning from "@lucide/svelte/icons/brush-cleaning";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Button from "$lib/ui/Button.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import SelectionBar from "$lib/ui/SelectionBar.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StreamSteps, { type Step } from "$lib/ui/StreamSteps.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { ago, t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";
  import { refusal } from "$lib/tickets-logic";
  import { tickets } from "$lib/tickets.svelte";
  import { chosen, keyOf, matchPlan, pickable } from "$lib/git-logic";
  import type { BranchCleanup, Cleanup, Plan, Reason, WorktreeCleanup } from "$lib/git.svelte";

  /**
   * The guided cleanup: every branch and worktree that could go, why, and what it would lose; a batch picked, a
   * preview, then one go. Only what loses work asks first, the loss written on the button; the rest goes at once,
   * with Undo. `plan`: a cleanup the git expert proposed, picked here (and applied when it says so).
   */
  let {
    repo,
    cleanup,
    error,
    plan,
    onchanged,
    onask,
    onretry,
  }: {
    repo: string;
    cleanup: Cleanup | null;
    error: string;
    plan: Plan | null;
    onchanged: () => void;
    onask: () => void;
    onretry: () => void;
  } = $props();

  let picked = $state(new Set<string>());
  let confirming = $state(false);
  let steps = $state<Step[] | null>(null);
  let working = $state(false);
  let unknown = $state(0);
  let fromPlan = $state(false);
  /** Why an item could not go, by key, from the last run. */
  let failed = $state<Record<string, string>>({});

  const keys = $derived(cleanup ? pickable(cleanup) : { all: [], suggested: [] });
  const sel = $derived(cleanup ? chosen(cleanup, picked) : null);
  const reasoned = $derived(cleanup?.branches.filter((b) => b.reasons.length || b.blocked) ?? []);
  const others = $derived(cleanup?.branches.filter((b) => !b.reasons.length && !b.blocked) ?? []);

  // The suggestions are picked once per repository read for the first time; a plan from the expert replaces them.
  let seeded = "";
  $effect(() => {
    const [c, r] = [cleanup, repo];
    untrack(() => {
      if (!c || seeded === r) return;
      seeded = r;
      picked = new Set(pickable(c).suggested);
    });
  });

  let planned = 0;
  $effect(() => {
    const [c, p] = [cleanup, plan];
    untrack(() => {
      if (!c || !p || p.id === planned || p.repo !== repo) return;
      planned = p.id;
      const match = matchPlan(c, p.keys);
      [picked, unknown, fromPlan, failed] = [new Set(match.keys), match.unknown, true, {}];
      if (p.apply && match.keys.length) go();
    });
  });

  const count = (n: number, one: Parameters<typeof t>[0], many: Parameters<typeof t>[0]) => (n === 1 ? t(one) : t(many, { count: n }));
  const items = (n: number) => count(n, "gitclItemsOne", "gitclItems");
  function loss(lost: number, changes: number): string {
    const parts = [lost && count(lost, "gitclCommitsOne", "gitclCommits"), changes && count(changes, "gitclChangesOne", "gitclChanges")].filter(
      (p): p is string => !!p,
    );
    return parts.length === 2 ? t("gitclAnd", { a: parts[0], b: parts[1] }) : (parts[0] ?? "");
  }

  const reasonLabel: Record<Reason, Parameters<typeof t>[0]> = {
    merged: "gitclMerged",
    prMerged: "gitclPrMerged",
    gone: "gitclGone",
    stale: "gitclStale",
    run: "gitclRun",
    detached: "gitclDetached",
    prunable: "gitclPrunable",
  };
  const runLabel = (id: number | null) => {
    const run = id === null ? undefined : tickets.runs.find((r) => r.id === id);
    return run ? t("gitRunShort", { id: run.id, key: run.ticket.key }) : id === null ? null : `Run #${id}`;
  };
  const folderName = (w: WorktreeCleanup) => w.path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? w.path;

  function toggle(key: string, on: boolean) {
    const next = new Set(picked);
    if (on) next.add(key);
    else next.delete(key);
    picked = next;
  }

  /** Applies at once what loses nothing; what loses work asks first. */
  function go() {
    if (!sel?.count || working) return;
    if (sel.lost || sel.changes) confirming = true;
    else void apply();
  }

  async function apply() {
    if (!sel) return;
    const { worktrees, branches } = sel;
    const all: (WorktreeCleanup | BranchCleanup)[] = [...worktrees, ...branches];
    steps = all.map((i) => ({ verb: verbOf(i), object: "path" in i ? (runLabel(i.run) ?? shortPath(i.path)) : i.name }));
    [working, failed] = [true, {}];
    // What puts each item back, newest last; a closed run or lost changes cannot come back.
    const undo: (() => Promise<unknown>)[] = [];
    let done = 0;
    for (const [n, item] of all.entries()) {
      steps[n].state = "now";
      try {
        if ("path" in item) await removeWorktree(item, undo);
        else {
          await invoke("git_branch_delete", { repo, name: item.name, tip: item.tip.oid });
          undo.push(() => invoke("git_branch_create", { repo, name: item.name, from: item.tip.oid, checkout: false }));
        }
        steps[n].state = "ok";
        done++;
      } catch (e) {
        steps[n].state = "fail";
        failed[keyOf(item)] = refusal(e).message;
      }
    }
    working = false;
    picked = new Set([...picked].filter((k) => failed[k]));
    onchanged();
    const restore = undo.length ? () => putBack(undo, done) : undefined;
    if (done === all.length) toast(t("gitclDone", { items: items(done) }), { undo: restore });
    else toast(t("gitclDonePart", { done, total: all.length }), { kind: "error", undo: restore });
  }

  async function removeWorktree(w: WorktreeCleanup, undo: (() => Promise<unknown>)[]) {
    if (w.closeRun && w.run !== null) {
      await invoke("tickets_close", { id: w.run, force: false });
      return;
    }
    await invoke("git_worktree_remove", { repo, path: w.path, changes: w.changes });
    if (w.branch && w.head) await invoke("git_branch_delete", { repo, name: w.branch, tip: w.head });
    const { branch, head } = w;
    const back = async () => {
      if (branch && head) await invoke("git_branch_create", { repo, name: branch, from: head, checkout: false });
      // A folder already gone, or one whose changes were lost, is not put back: its branch is.
      if (!w.reasons.includes("prunable") && !w.changes && head) await invoke("git_worktree_restore", { repo, path: w.path, branch, head });
    };
    if (branch || (!w.reasons.includes("prunable") && !w.changes)) undo.push(back);
  }

  async function putBack(undo: (() => Promise<unknown>)[], count: number) {
    try {
      for (const step of [...undo].reverse()) await step();
      toast(t("gitclUndone", { items: items(count) }));
    } catch (e) {
      toast(t("gitclUndoFailed", { error: String(e) }), { kind: "error" });
    } finally {
      onchanged();
    }
  }

  function verbOf(i: WorktreeCleanup | BranchCleanup): string {
    if (!("path" in i)) return t("gitclRemove");
    if (i.closeRun) return t("gitclCloseRun");
    return t(i.reasons.includes("prunable") ? "gitclForget" : "gitclRemove");
  }
</script>

{#snippet reasons(list: Reason[])}
  {#each list as r (r)}<span class="badge" class:accent={r === "merged" || r === "prMerged" || r === "run"}>{t(reasonLabel[r])}</span>{/each}
{/snippet}

{#snippet lossOf(lost: number, changes: number, blocked: boolean)}
  {#if blocked}
    <span class="loss muted">—</span>
  {:else if lost || changes}
    <span class="loss danger"><CircleAlert size={13} aria-hidden="true" />{loss(lost, changes)}</span>
  {:else}
    <span class="loss safe">{t("gitclNothingLost")}</span>
  {/if}
{/snippet}

{#snippet worktreeRow(w: WorktreeCleanup)}
  {@const key = keyOf(w)}
  {@const run = runLabel(w.run)}
  <li class="item" class:blocked={!!w.blocked} class:picked={picked.has(key)}>
    <label>
      <input type="checkbox" checked={picked.has(key)} disabled={!!w.blocked || working} onchange={(e) => toggle(key, e.currentTarget.checked)} />
      <FolderGit2 size={14} aria-hidden="true" class="kind" />
      <span class="what">
        <span class="name">{run ?? folderName(w)}</span>
        <span class="sub">
          <span class="mono">{shortPath(w.path)}</span>
          {#if w.closeRun}· {t("gitclClosesRun")}{:else if w.branch}· {t("gitclBranchGoes", { branch: w.branch })}{/if}
        </span>
      </span>
      <span class="why">
        {#if w.blocked}<span class="badge warn">{t(w.blocked === "locked" ? "gitclLocked" : "gitclRunOpen")}</span>{:else}{@render reasons(w.reasons)}{/if}
      </span>
      {@render lossOf(w.lost, w.changes, !!w.blocked)}
    </label>
    {#if failed[key]}<p class="failed" role="alert">{failed[key]}</p>{/if}
  </li>
{/snippet}

{#snippet branchRow(b: BranchCleanup)}
  {@const key = keyOf(b)}
  <li class="item" class:blocked={!!b.blocked} class:picked={picked.has(key)}>
    <label>
      <input type="checkbox" checked={picked.has(key)} disabled={!!b.blocked || working} onchange={(e) => toggle(key, e.currentTarget.checked)} />
      <GitBranch size={14} aria-hidden="true" class="kind" />
      <span class="what">
        <span class="name mono">{b.name}</span>
        <span class="sub">{b.tip.subject} · {ago(b.tip.date)}{#if b.run !== null} · {runLabel(b.run)}{/if}</span>
      </span>
      <span class="why">
        {#if b.blocked}<span class="badge warn">{t("gitclRunOpen")}</span>{:else}{@render reasons(b.reasons)}{/if}
      </span>
      {@render lossOf(b.lost, 0, !!b.blocked)}
    </label>
    {#if failed[key]}<p class="failed" role="alert">{failed[key]}</p>{/if}
  </li>
{/snippet}

<section class="cleanup" aria-labelledby="git-cleanup-title" data-testid="git-cleanup">
  <header class="head">
    <span class="mark" aria-hidden="true"><BrushCleaning size={18} /></span>
    <div class="titles">
      <h2 id="git-cleanup-title">{t("gitclTitle")}</h2>
      <p>{t("gitclLead")}</p>
    </div>
    <Button size="sm" variant="quiet" icon={Sparkles} onclick={onask}>{t("gitclAsk")}</Button>
  </header>

  {#if error && !cleanup}
    <div class="error">
      <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{error}</span></p>
      <Button size="sm" icon={RefreshCw} onclick={onretry}>{t("gitRetry")}</Button>
    </div>
  {:else if !cleanup}
    <Skeleton shape="list" rows={6} label={t("gitLoading")} />
  {:else if !cleanup.worktrees.length && !cleanup.branches.length}
    <EmptyState icon={BrushCleaning} title={t("gitclEmptyTitle")} text={t("gitclEmptyText")} />
  {:else}
    {#if fromPlan}
      <p class="plan" role="status"><Sparkles size={14} aria-hidden="true" />{t("gitclPlanFound")}{#if unknown} <span class="muted">{t("gitclPlanUnknown", { count: unknown })}</span>{/if}</p>
    {/if}
    <SelectionBar
      count={sel?.count ?? 0}
      total={keys.all.length}
      label={t("gitclSelection")}
      onall={() => (picked = new Set(sel?.count === keys.all.length ? [] : keys.all))}
      onclear={() => (picked = new Set())}
    >
      {#if keys.suggested.length}
        <Button size="sm" variant="quiet" onclick={() => (picked = new Set(keys.suggested))} disabled={working}>{t("gitclSuggested")}</Button>
      {/if}
    </SelectionBar>

    <div class="lists">
      {#if cleanup.worktrees.length}
        <h3>{t("gitWorktrees")}<span class="n num">{cleanup.worktrees.length}</span></h3>
        <ul>{#each cleanup.worktrees as w (w.path)}{@render worktreeRow(w)}{/each}</ul>
      {/if}
      {#if reasoned.length}
        <h3>{t("gitBranches")}<span class="n num">{reasoned.length}</span></h3>
        <ul>{#each reasoned as b (b.name)}{@render branchRow(b)}{/each}</ul>
      {/if}
      {#if others.length}
        <details class="others" open={!reasoned.length}>
          <summary>{t("gitclOthers", { count: others.length })}</summary>
          <ul>{#each others as b (b.name)}{@render branchRow(b)}{/each}</ul>
        </details>
      {/if}
    </div>

    <footer class="foot">
      {#if steps}
        <div class="progress"><StreamSteps {steps} live={working} summary={working ? undefined : t("gitclDone", { items: items(steps.filter((s) => s.state === "ok").length) })} /></div>
      {/if}
      <p class="preview" class:lossy={!!sel && (sel.lost > 0 || sel.changes > 0)} aria-live="polite">
        {#if !sel?.count}
          {t("gitclPreviewNone")}
        {:else if sel.lost || sel.changes}
          {t("gitclPreviewLoss", { items: items(sel.count), loss: loss(sel.lost, sel.changes) })}
        {:else}
          {t("gitclPreviewSafe", { items: items(sel.count) })}
        {/if}
      </p>
      <Button variant="primary" icon={BrushCleaning} loading={working} disabled={!sel?.count} onclick={go} data-testid="git-cleanup-apply">
        {t(sel && (sel.lost || sel.changes) ? "gitclApplyAsk" : "gitclApply", { items: items(sel?.count ?? 0) })}
      </Button>
    </footer>
  {/if}
</section>

{#if sel}
  <ConfirmDialog
    bind:open={confirming}
    title={t("gitclConfirmTitle", { items: items(sel.count) })}
    confirm={t("gitclConfirmLose", { loss: loss(sel.lost, sel.changes) })}
    cancel={t("uiF1DialogKeep")}
    danger
    onconfirm={apply}
  >
    <p>{t("gitclConfirmText")}</p>
    <ul class="lost-list">
      {#each [...sel.worktrees, ...sel.branches].filter((i) => i.lost || ("changes" in i && i.changes)) as i (keyOf(i))}
        <li>
          <span class="mono">{"path" in i ? (runLabel(i.run) ?? shortPath(i.path)) : i.name}</span>
          <span class="lost">{loss(i.lost, "changes" in i ? i.changes : 0)}</span>
          {#if i.reasons.includes("prMerged")}<span class="muted">· {t("gitclPrMerged")}</span>{/if}
        </li>
      {/each}
    </ul>
  </ConfirmDialog>
{/if}

<style>
  .cleanup {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }

  .mark {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .titles p {
    max-width: 62ch;
    margin: 4px 0 0;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .plan {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding: 8px 12px;
    border: 1px solid var(--accent-soft);
    border-radius: var(--radius-md);
    color: var(--accent-ink);
    font-size: var(--text-sm);
  }

  .cleanup :global(.selection) {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 6px;
    margin: 0 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  h3 .n {
    color: var(--muted);
    font-weight: 500;
  }

  .lists {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    border-bottom: 1px solid var(--border);
  }

  .item:last-child {
    border-bottom: 0;
  }

  .item label {
    display: grid;
    grid-template-columns: auto auto minmax(0, 1fr) auto minmax(120px, max-content);
    align-items: center;
    gap: 10px;
    min-height: var(--row-lg);
    padding: 6px 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .item label:hover {
    background: var(--hover);
  }

  .item.picked label {
    background: var(--selected);
  }

  .item.blocked label {
    cursor: default;
    opacity: 0.72;
  }

  .item input {
    margin: 0;
  }

  .item :global(.kind) {
    color: var(--muted);
  }

  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.mono,
  .sub .mono {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .sub {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-sm);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .why {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
  }

  .loss {
    display: inline-flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    font-size: var(--text-sm);
    text-align: right;
  }

  .loss.safe {
    color: var(--done);
  }

  .loss.danger {
    color: var(--danger);
    font-weight: 600;
  }

  .failed {
    margin: 0 8px 8px 58px;
    color: var(--danger);
    font-size: var(--text-sm);
    white-space: pre-line;
  }

  .others summary {
    width: max-content;
    margin-bottom: 6px;
    color: var(--muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .foot {
    position: sticky;
    bottom: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 16px;
    margin-top: auto;
    padding: 12px 0 4px;
    border-top: 1px solid var(--border);
    background: var(--panel);
  }

  .progress {
    flex-basis: 100%;
  }

  .preview {
    flex: 1;
    min-width: 240px;
    margin: 0;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .preview.lossy {
    color: var(--danger);
    font-weight: 500;
  }

  .error {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
  }

  .pre {
    white-space: pre-line;
  }

  .lost-list {
    max-height: 200px;
    padding: 8px 10px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: var(--text-sm);
  }

  .lost-list li {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .lost {
    color: var(--danger);
    font-weight: 600;
  }

  @container git (max-width: 1000px) {
    .item label {
      grid-template-columns: auto auto minmax(0, 1fr) auto;
    }

    .why {
      grid-row: 2;
      grid-column: 3 / -1;
      justify-content: flex-start;
    }
  }
</style>
