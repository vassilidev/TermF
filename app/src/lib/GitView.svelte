<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Archive from "@lucide/svelte/icons/archive";
  import ArchiveRestore from "@lucide/svelte/icons/archive-restore";
  import ArrowBigUpDash from "@lucide/svelte/icons/arrow-big-up-dash";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowDownToLine from "@lucide/svelte/icons/arrow-down-to-line";
  import ArrowRightLeft from "@lucide/svelte/icons/arrow-right-left";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ArrowUpFromLine from "@lucide/svelte/icons/arrow-up-from-line";
  import Ban from "@lucide/svelte/icons/ban";
  import Cherry from "@lucide/svelte/icons/cherry";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Cloud from "@lucide/svelte/icons/cloud";
  import CloudDownload from "@lucide/svelte/icons/cloud-download";
  import Copy from "@lucide/svelte/icons/copy";
  import FileText from "@lucide/svelte/icons/file-text";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import GitBranchPlus from "@lucide/svelte/icons/git-branch-plus";
  import GitCommitHorizontal from "@lucide/svelte/icons/git-commit-horizontal";
  import GitCompareArrows from "@lucide/svelte/icons/git-compare-arrows";
  import GitMerge from "@lucide/svelte/icons/git-merge";
  import GitMergeConflict from "@lucide/svelte/icons/git-merge-conflict";
  import GitPullRequest from "@lucide/svelte/icons/git-pull-request";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Rewind from "@lucide/svelte/icons/rewind";
  import SkipForward from "@lucide/svelte/icons/skip-forward";
  import StepForward from "@lucide/svelte/icons/step-forward";
  import Tag from "@lucide/svelte/icons/tag";
  import TagPlus from "@lucide/svelte/icons/tag-plus";
  import Trash from "@lucide/svelte/icons/trash";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import Banner from "$lib/ui/Banner.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import GitChanges from "$lib/GitChanges.svelte";
  import GitDiff from "$lib/GitDiff.svelte";
  import GitFile from "$lib/GitFile.svelte";
  import { showRun } from "$lib/commands";
  import { ago, t } from "$lib/i18n";
  import { currentProject, scopeRepos } from "$lib/files.svelte";
  import { editProject } from "$lib/project.svelte";
  import { onScreen, shortPath } from "$lib/state.svelte";
  import { tickets } from "$lib/tickets.svelte";
  import {
    git,
    type Branch,
    type Commit,
    type CommitDetail,
    type FileDiff,
    type ForcePlan,
    type GitState,
    type GitTag,
    type Graph,
    type Operation,
    type Outcome,
    type Pr,
    type Pull,
    type Push,
    type RebasePlan,
    type ResetPlan,
    type Stash,
    type Unique,
    type Worktree,
  } from "$lib/git.svelte";
  import { lanes, localFor, onRemote, path, prFor, samePath, sorted, splitMessage, tally } from "$lib/git-logic";

  /** Commits read at first, then each time more are asked for. */
  const STEP = 200;
  /** Lane spacing and row height of the graph, in pixels; lanes past `MAX_LANES` are cut off. */
  const LANE = 14;
  const ROW = 28;
  const MAX_LANES = 16;

  type Selection =
    | { kind: "branch"; name: string }
    | { kind: "worktree"; path: string }
    | { kind: "commit"; oid: string }
    | { kind: "tag"; name: string }
    | { kind: "stash"; oid: string }
    | { kind: "file"; dir: string; path: string; tab: "history" | "blame" };
  type Notice = { kind: "ok" | "warn"; text: string; output?: string };
  type Mode = "soft" | "mixed" | "hard";

  let snap = $state<GitState | null>(null);
  let graph = $state<Graph | null>(null);
  let prs = $state<Pr[]>([]);
  let limit = $state(STEP);
  let loading = $state(false);
  let loadError = $state("");
  let error = $state("");
  let notice = $state<Notice | null>(null);
  let busy = $state("");
  let selected = $state<Selection | null>(null);
  let form = $state<
    | { kind: "create"; from: string; name: string; checkout: boolean }
    | { kind: "rename"; from: string; to: string }
    | { kind: "tag"; target: string; name: string; message: string }
    | null
  >(null);
  let removal = $state<{ branch: Branch; unique: Unique | null; typed: string; open: boolean } | null>(null);
  let remoteRemoval = $state<{ branch: Branch; open: boolean } | null>(null);
  /** What git said last about an operation that stopped, shown with it. */
  let opOutput = $state("");
  let detail = $state<CommitDetail | null>(null);
  let detailError = $state("");
  let stashFiles = $state<{ oid: string; files: FileDiff[] } | null>(null);
  let rebasing = $state<{ onto: string; plan: RebasePlan | null; open: boolean } | null>(null);
  let resetting = $state<{ target: Commit; plan: ResetPlan | null; mode: Mode; typed: string; open: boolean } | null>(null);
  let forcing = $state<{ branch: Branch; plan: ForcePlan | null; typed: string; open: boolean } | null>(null);
  let tagRemoval = $state<{ tag: GitTag; unique: Unique | null; typed: string; open: boolean } | null>(null);
  let stashRemoval = $state<{ stash: Stash; open: boolean } | null>(null);
  let aborting = $state<{ dir: string; op: Operation; open: boolean } | null>(null);
  let graphEl = $state<HTMLElement>();

  const repos = $derived(scopeRepos());
  const rows = $derived(graph ? lanes(graph.commits) : []);
  const graphWidth = $derived(Math.min(MAX_LANES, Math.max(1, ...rows.map((r) => r.width))) * LANE);
  const groups = $derived(snap ? sorted(snap.branches) : { local: [], remote: [] });
  const current = $derived(snap?.branches.find((b) => b.current && !b.remote));
  const here = $derived(snap?.worktrees.find((w) => samePath(w.path, git.repo)) ?? snap?.worktrees.find((w) => w.main));
  const defaultName = $derived.by(() => {
    const name = snap?.defaultBranch;
    const remote = name && snap?.remotes.find((r) => name.startsWith(`${r}/`));
    return name && remote ? name.slice(remote.length + 1) : name;
  });
  const shown = $derived.by(() => {
    if (!snap) return null;
    const s = selected;
    if (s?.kind === "branch") {
      const branch = snap.branches.find((b) => b.name === s.name);
      if (branch) return { kind: "branch", branch } as const;
    }
    if (s?.kind === "worktree") {
      const worktree = snap.worktrees.find((w) => w.path === s.path);
      if (worktree) return { kind: "worktree", worktree } as const;
    }
    if (s?.kind === "commit") {
      const commit = graph?.commits.find((c) => c.oid === s.oid) ?? (detail?.commit.oid === s.oid ? detail.commit : null);
      return { kind: "commit", oid: s.oid, commit } as const;
    }
    if (s?.kind === "tag") {
      const tag = snap.tags.find((t) => t.name === s.name);
      if (tag) return { kind: "tag", tag } as const;
    }
    if (s?.kind === "stash") {
      const stash = snap.stashes.find((x) => x.oid === s.oid);
      if (stash) return { kind: "stash", stash } as const;
    }
    if (s?.kind === "file") return { kind: "file", file: s } as const;
    const branch = current ?? groups.local[0];
    if (branch) return { kind: "branch", branch } as const;
    return here ? ({ kind: "worktree", worktree: here } as const) : null;
  });

  /** The commit the graph marks for what is shown. */
  const pickedOid = $derived.by(() => {
    if (shown?.kind === "commit") return shown.oid;
    if (shown?.kind === "branch") return shown.branch.tip.oid;
    if (shown?.kind === "worktree") return shown.worktree.head;
    return shown?.kind === "tag" ? shown.tag.target : null;
  });

  // A repository out of the current project falls back to the project's first one.
  $effect(() => {
    if (!repos.some((r) => r.path === git.repo)) git.repo = repos[0]?.path ?? "";
  });

  /** The repository last read while on screen: coming back to it refreshes quietly. */
  let read = "";
  $effect(() => {
    const repo = git.repo;
    const visible = onScreen("git");
    untrack(() => {
      if (!visible || !repo) return;
      load(repo === read);
      read = repo;
    });
  });

  let request = 0;

  /** Reads the repository; `quiet` keeps what is shown, and an error then goes above it. */
  async function load(quiet: boolean) {
    const repo = git.repo;
    if (!repo) return;
    const r = ++request;
    loading = true;
    if (!quiet) [snap, graph, prs, loadError, selected, limit, form] = [null, null, [], "", null, STEP, null];
    try {
      const [s, g] = await Promise.all([
        invoke<GitState>("git_state", { repo }),
        invoke<Graph>("git_graph", { repo, limit }),
      ]);
      if (r !== request) return;
      [snap, graph, loadError] = [s, g, ""];
    } catch (e) {
      if (r !== request) return;
      if (snap) error = String(e);
      else loadError = String(e);
    } finally {
      if (r === request) loading = false;
    }
    // gh is optional and slow: its answer comes after, and nothing when it fails.
    invoke<Pr[]>("git_prs", { repo }).then((found) => repo === git.repo && (prs = found), () => {});
  }

  // Through `load`, so a refresh meanwhile cannot bring back fewer commits than asked for.
  function more() {
    limit += STEP;
    return load(true);
  }

  async function act(name: string, run: (repo: string) => Promise<{ kind: "ok" | "warn"; text: string } | void>) {
    [busy, error, notice] = [name, "", null];
    try {
      notice = (await run(git.repo)) ?? null;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
      await load(true);
    }
  }

  const ok = (text: string) => ({ kind: "ok" as const, text });
  const warn = (text: string) => ({ kind: "warn" as const, text });

  /** What an operation that may stop on conflicts did: done, with git's output, or stopped, the output kept with it. */
  function after(o: Outcome, done: string): Notice {
    opOutput = o.pending ? o.output : "";
    return o.pending ? { kind: "warn", text: t("git2Stopped"), output: o.output } : { kind: "ok", text: done, output: o.output };
  }

  let detailSeq = 0;
  $effect(() => {
    const [s, repo] = [selected, git.repo];
    untrack(() => {
      if (s?.kind !== "commit" || detail?.commit.oid === s.oid) return;
      const n = ++detailSeq;
      detailError = "";
      invoke<CommitDetail>("git_show", { repo, rev: s.oid }).then(
        (d) => n === detailSeq && (detail = d),
        (e) => n === detailSeq && (detailError = String(e)),
      );
    });
  });

  let stashSeq = 0;
  $effect(() => {
    const [s, stashes, repo] = [shown, snap?.stashes, git.repo];
    untrack(() => {
      if (s?.kind !== "stash" || stashFiles?.oid === s.stash.oid || !stashes) return;
      const { index, oid } = s.stash;
      const n = ++stashSeq;
      invoke<FileDiff[]>("git_stash_show", { repo, index, oid }).then(
        (files) => n === stashSeq && (stashFiles = { oid, files }),
        (e) => n === stashSeq && (error = String(e)),
      );
    });
  });

  /** Shows a commit, in the graph too when it is read. */
  function pickCommit(oid: string) {
    [selected, form] = [{ kind: "commit", oid }, null];
    graphEl?.querySelector(`[data-oid="${oid}"]`)?.scrollIntoView({ block: "nearest" });
  }

  const merge = (b: Branch) =>
    act("merge", async (repo) =>
      after(await invoke<Outcome>("git_merge", { repo, branch: b.name }), t("git2Merged", { branch: b.name, current: snap?.current ?? "" })),
    );

  async function askRebase(b: Branch) {
    rebasing = { onto: b.name, plan: null, open: true };
    const asked = rebasing;
    try {
      const plan = await invoke<RebasePlan>("git_rebase_plan", { repo: git.repo, onto: b.name });
      if (rebasing === asked) asked.plan = plan;
    } catch (e) {
      if (rebasing === asked) [asked.open, error] = [false, String(e)];
    }
  }

  function rebase() {
    const [onto, head, branch] = [rebasing?.onto ?? "", snap?.head ?? "", snap?.current ?? ""];
    act("rebase", async (repo) => after(await invoke<Outcome>("git_rebase", { repo, onto, head }), t("git2Rebased", { branch, onto })));
  }

  const cherryPick = (c: Commit) =>
    act("cherry-pick", async (repo) => after(await invoke<Outcome>("git_cherry_pick", { repo, oid: c.oid }), t("git2CherryPicked", { sha: short(c.oid) })));

  const revert = (c: Commit) =>
    act("revert", async (repo) => after(await invoke<Outcome>("git_revert", { repo, oid: c.oid }), t("git2Reverted", { sha: short(c.oid) })));

  const proceed = (dir: string, step: "continue" | "skip" | "abort") =>
    act(`op:${step}`, async () => after(await invoke<Outcome>("git_proceed", { repo: dir, step }), t(step === "abort" ? "git2Aborted" : "git2Finished")));

  async function askReset(c: Commit) {
    resetting = { target: c, plan: null, mode: "mixed", typed: "", open: true };
    const asked = resetting;
    try {
      const plan = await invoke<ResetPlan>("git_reset_plan", { repo: git.repo, target: c.oid });
      if (resetting === asked) asked.plan = plan;
    } catch (e) {
      if (resetting === asked) [asked.open, error] = [false, String(e)];
    }
  }

  /** A reset that loses something asks for the branch's name. */
  const resetLoses = (r: NonNullable<typeof resetting>) =>
    !!r.plan && (r.plan.lost.total > 0 || (r.mode === "hard" && r.plan.changes.length > 0));

  function reset() {
    const r = resetting;
    if (!r?.plan) return;
    const [target, mode, head, branch] = [r.target.oid, r.mode, snap?.head ?? "", r.plan.branch];
    act("reset", async (repo) => {
      await invoke("git_reset", { repo, target, mode, head });
      return ok(t("git2ResetDone", { branch, sha: short(target) }));
    });
  }

  async function askForce(b: Branch) {
    forcing = { branch: b, plan: null, typed: "", open: true };
    const asked = forcing;
    try {
      const plan = await invoke<ForcePlan>("git_force_plan", { repo: git.repo, branch: b.name });
      if (forcing === asked) asked.plan = plan;
    } catch (e) {
      if (forcing === asked) [asked.open, error] = [false, String(e)];
    }
  }

  function force() {
    const f = forcing;
    if (!f?.plan) return;
    const [branch, theirs] = [f.branch.name, f.plan.theirs];
    act("force", async (repo) => {
      const remote = await invoke<string>("git_force_push", { repo, branch, theirs });
      return ok(t("git2Forced", { branch, remote }));
    });
  }

  function createTag() {
    if (form?.kind !== "tag") return;
    const { target, message } = form;
    const name = form.name.trim();
    return act("tag", async (repo) => {
      await invoke("git_tag_create", { repo, name, target, message: message.trim() || null });
      [form, selected] = [null, { kind: "tag", name }];
      return ok(t("git2TagCreated", { name }));
    });
  }

  async function askTagRemoval(tag: GitTag) {
    tagRemoval = { tag, unique: null, typed: "", open: true };
    const asked = tagRemoval;
    try {
      const unique = await invoke<Unique>("git_tag_unique", { repo: git.repo, name: tag.name });
      if (tagRemoval === asked) asked.unique = unique;
    } catch (e) {
      if (tagRemoval === asked) [asked.open, error] = [false, String(e)];
    }
  }

  function removeTag() {
    const tag = tagRemoval?.tag;
    if (!tag) return;
    act("tag-delete", async (repo) => {
      await invoke("git_tag_delete", { repo, name: tag.name, oid: tag.oid });
      selected = null;
      return ok(t("git2TagDeleted", { name: tag.name }));
    });
  }

  const pushTag = (tag: GitTag) =>
    act("tag-push", async (repo) => ok(t("git2TagPushed", { name: tag.name, remote: await invoke<string>("git_tag_push", { repo, name: tag.name }) })));

  const applyStash = (st: Stash, pop: boolean) =>
    act(pop ? "stash-pop" : "stash-apply", async (repo) => {
      const o = await invoke<Outcome>("git_stash_apply", { repo, index: st.index, oid: st.oid, pop });
      if (pop && o.ok) selected = null;
      return after(o, t(pop ? "git2StashPopped" : "git2StashApplied"));
    });

  function dropStash() {
    const st = stashRemoval?.stash;
    if (!st) return;
    act("stash-drop", async (repo) => {
      await invoke("git_stash_drop", { repo, index: st.index, oid: st.oid });
      selected = null;
      return ok(t("git2StashDropped"));
    });
  }

  const fetchAll = () =>
    act("fetch", async (repo) => {
      await invoke("git_fetch", { repo });
      return ok(t("gitFetched"));
    });

  const pull = (branch: Branch) =>
    act("pull", async (repo) => {
      const p = await invoke<Pull>("git_pull", { repo });
      const vars = { branch: branch.name, upstream: branch.upstream ?? "", ahead: p.ahead, behind: p.behind };
      if (p.outcome === "updated") return ok(t("gitPulled", vars));
      if (p.outcome === "upToDate") return ok(t("gitUpToDate", vars));
      if (p.outcome === "diverged") return warn(t("gitDiverged", vars));
      return warn(t(p.outcome === "gone" ? "gitPullGone" : "gitPullNoUpstream", vars));
    });

  const push = (branch: Branch) =>
    act(`push:${branch.name}`, async (repo) => {
      const p = await invoke<Push>("git_push", { repo, branch: branch.name });
      return ok(t(p.upstreamSet ? "gitPushedUpstream" : "gitPushed", { branch: branch.name, remote: p.remote }));
    });

  function switchTo(branch: Branch) {
    // A remote branch: its local twin when there is one, else git makes a local branch of its name that tracks it.
    const local = branch.remote ? localFor(branch, snap?.branches ?? []) : branch;
    const target = local?.name ?? branch.name;
    const name = local?.name ?? onRemote(branch);
    return act("switch", async (repo) => {
      await invoke("git_switch", { repo, branch: target });
      selected = { kind: "branch", name };
      return ok(t("gitSwitched", { branch: name }));
    });
  }

  function create() {
    if (form?.kind !== "create") return;
    const { from, checkout } = form;
    const name = form.name.trim();
    return act("create", async (repo) => {
      await invoke("git_branch_create", { repo, name, from, checkout });
      [form, selected] = [null, { kind: "branch", name }];
      return ok(t("gitCreated", { name }));
    });
  }

  function rename() {
    if (form?.kind !== "rename") return;
    const { from } = form;
    const to = form.to.trim();
    return act("rename", async (repo) => {
      await invoke("git_branch_rename", { repo, from, to });
      [form, selected] = [null, { kind: "branch", name: to }];
      return ok(t("gitRenamed", { name: to }));
    });
  }

  async function askRemoval(branch: Branch) {
    removal = { branch, unique: null, typed: "", open: true };
    const asked = removal;
    try {
      const unique = await invoke<Unique>("git_unique", { repo: git.repo, branch: branch.name });
      if (removal === asked) asked.unique = unique;
    } catch (e) {
      if (removal === asked) [asked.open, error] = [false, String(e)];
    }
  }

  function remove() {
    const branch = removal?.branch;
    if (!branch) return;
    act("delete", async (repo) => {
      await invoke("git_branch_delete", { repo, name: branch.name, tip: branch.tip.oid });
      selected = null;
      return ok(t("gitDeleted", { name: branch.name }));
    });
  }

  function removeRemote() {
    const branch = remoteRemoval?.branch;
    if (!branch?.remote) return;
    const remote = branch.remote;
    act("delete-remote", async (repo) => {
      await invoke("git_remote_branch_delete", { repo, remote, branch: onRemote(branch) });
      selected = null;
      return ok(t("gitRemoteDeleted", { name: onRemote(branch), remote }));
    });
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      notice = ok(t("gitShaCopied"));
    } catch (e) {
      error = String(e);
    }
  }

  function openPr(pr: Pr) {
    invoke("open_url", { url: pr.url }).catch((e) => (error = String(e)));
  }

  /** Shows a branch, its tip in the graph too when it is read. */
  function pick(branch: Branch) {
    [selected, form] = [{ kind: "branch", name: branch.name }, null];
    graphEl?.querySelector(`[data-oid="${branch.tip.oid}"]`)?.scrollIntoView({ block: "nearest" });
  }

  const short = (oid: string) => oid.slice(0, 7);
  /** A commit by the branch or tag at it, else its short SHA. */
  const refName = (oid: string | null) =>
    !oid
      ? "?"
      : (snap?.branches.find((b) => b.tip.oid === oid && !b.remote)?.name ??
        snap?.branches.find((b) => b.tip.oid === oid)?.name ??
        snap?.tags.find((x) => x.target === oid)?.name ??
        short(oid));
  /** What the two sides of a conflict are; in a rebase, ours is the branch rebased onto. */
  function sidesOf(wt: Worktree) {
    const op = wt.operation;
    if (op?.kind === "rebase") return { ours: refName(op.onto), theirs: t("git2YourCommit", { sha: short(op.theirs ?? "") }) };
    const ours = wt.branch ?? "HEAD";
    return { ours, theirs: op ? refName(op.theirs) : t("git2TheStash") };
  }
  function opTitle(op: Operation) {
    return t(({ merge: "git2OpMerge", rebase: "git2OpRebase", cherryPick: "git2OpPick", revert: "git2OpRevert" } as const)[op.kind]);
  }
  function opDetail(wt: Worktree, op: Operation) {
    if (op.kind === "rebase")
      return t("git2OpRebaseOf", { branch: op.branch ?? "HEAD", onto: refName(op.onto), step: op.step, total: op.total });
    if (op.kind === "merge") return t("git2OpMergeOf", { name: refName(op.theirs), branch: wt.branch ?? "HEAD" });
    const subject = graph?.commits.find((c) => c.oid === op.theirs)?.subject;
    return t("git2OpCommit", { sha: short(op.theirs ?? ""), subject: subject ?? "" });
  }
  const modeLabel = { soft: "git2Soft", mixed: "git2Mixed", hard: "git2Hard" } as const;
  const modeHelp = { soft: "git2SoftHelp", mixed: "git2MixedHelp", hard: "git2HardHelp" } as const;
  const runOf = (wt: Worktree) => tickets.runs.find((r) => r.worktree && samePath(r.worktree, wt.path));
  /** A worktree's folder from the repository's when it is inside it (a run's), else from the home folder. */
  const where = (wt: Worktree) => {
    const base = git.repo.replace(/[\\/]+$/, "");
    const inside = /^[\\/]/.test(wt.path.slice(base.length)) && samePath(wt.path.slice(0, base.length), base);
    return inside ? `.${wt.path.slice(base.length)}` : shortPath(wt.path);
  };
  const isDefault = (b: Branch) => onRemote(b) === defaultName;
  const prState = (pr: Pr) => (pr.draft && pr.state === "OPEN" ? "draft" : pr.state.toLowerCase());
  const prLabel = (pr: Pr) =>
    t(({ open: "gitPrOpen", draft: "gitPrDraft", merged: "gitPrMerged", closed: "gitPrClosed" } as const)[prState(pr)] ?? "gitPrOpen", { number: pr.number });

  function treeSummary(wt: Worktree) {
    const { changed, conflicts } = tally(wt.changes, wt.more);
    if (!changed && !conflicts) return t("gitClean");
    return [changed && t("gitChanged", { count: changed }), conflicts && t("gitConflicts", { count: conflicts })]
      .filter(Boolean)
      .join(" · ");
  }

  /** The labels of a commit: its branches and tags, HEAD when it is detached there. */
  function labels(commit: Commit) {
    const list = commit.refs.map((r) => ({ ...r, head: r.kind === "local" && r.name === snap?.current }));
    if (snap && snap.current === null && snap.head === commit.oid) list.unshift({ name: "HEAD", kind: "local", head: true });
    return list.sort((a, b) => Number(b.head) - Number(a.head));
  }
</script>

<svelte:window onfocus={() => onScreen("git") && git.repo && load(true)} />

{#snippet aheadBehind(b: Branch)}
  {#if b.ahead}<span class="count" title={t("gitAheadHint", { count: b.ahead })}><ArrowUp size={11} aria-hidden="true" />{b.ahead}</span>{/if}
  {#if b.behind}<span class="count" title={t("gitBehindHint", { count: b.behind })}><ArrowDown size={11} aria-hidden="true" />{b.behind}</span>{/if}
{/snippet}

{#snippet opBox(wt: Worktree, op: Operation)}
  {@const conflicts = tally(wt.changes).conflicts}
  <div class="op" class:conflicted={conflicts > 0} role="status">
    <GitMergeConflict size={16} aria-hidden="true" class="op-icon" />
    <div class="op-text">
      <strong>{opTitle(op)}</strong>
      <span>{opDetail(wt, op)}</span>
      <span class="op-state">{conflicts ? t("gitConflicts", { count: conflicts }) : t("git2NoConflictLeft")}</span>
    </div>
    <div class="op-actions">
      {#if !(shown?.kind === "worktree" && shown.worktree.path === wt.path)}
        <button class="btn sm" onclick={() => ([selected, form] = [{ kind: "worktree", path: wt.path }, null])}>{t("git2SeeFiles")}</button>
      {/if}
      <button class="btn sm quiet-danger" onclick={() => (aborting = { dir: wt.path, op, open: true })} disabled={!!busy}>
        <Ban size={14} aria-hidden="true" />{t("git2Abort")}
      </button>
      {#if op.kind !== "merge"}
        <button class="btn sm" onclick={() => proceed(wt.path, "skip")} disabled={!!busy} title={t("git2SkipHint")}>
          <SkipForward size={14} aria-hidden="true" />{t("git2Skip")}
        </button>
      {/if}
      <button class="btn sm primary" onclick={() => proceed(wt.path, "continue")} disabled={!!busy || conflicts > 0} title={conflicts ? t("git2ContinueHint") : undefined}>
        {#if busy === "op:continue"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<StepForward size={14} aria-hidden="true" />{/if}{t("git2Continue")}
      </button>
    </div>
    {#if opOutput}
      <details class="out">
        <summary>{t("git2Output")}</summary>
        <pre>{opOutput}</pre>
      </details>
    {/if}
  </div>
{/snippet}

{#snippet branchRow(b: Branch)}
  {@const pr = prFor(b, prs, snap?.remotes ?? [])}
  {@const picked = shown?.kind === "branch" && shown.branch.name === b.name}
  <li class="row" class:current={picked}>
    <button class="pick" aria-current={picked} onclick={() => pick(b)}>
      <span class="line">
        {#if b.current}
          <span class="dot" aria-hidden="true"></span>
        {:else if b.remote}
          <Cloud size={13} aria-hidden="true" class="muted-icon" />
        {:else}
          <GitBranch size={13} aria-hidden="true" class="muted-icon" />
        {/if}
        <span class="bname" class:strong={b.current}>{b.name}</span>
        {#if b.current}<span class="sr-only">{t("gitCurrent")}</span>{/if}
        <span class="badges">
          {@render aheadBehind(b)}
          {#if b.gone}<span class="count warn" title={t("gitGoneHint")}>{t("gitGone")}</span>{/if}
          {#if pr}<span class="count pr {prState(pr)}" title={prLabel(pr)}><GitPullRequest size={11} aria-hidden="true" />{pr.number}</span>{/if}
          {#if b.worktree}<span class="count" title={t("gitInWorktree", { path: shortPath(b.worktree) })}><FolderGit2 size={11} aria-hidden="true" /></span>{/if}
        </span>
      </span>
      <span class="desc">{b.tip.subject} · {ago(b.tip.date)}</span>
    </button>
  </li>
{/snippet}

<div class="git">
  <ViewHeader title={t("gitView")} context={currentProject()?.name ?? t("ticketsAll")}>
    {#if repos.length > 1}
      <label class="labelled inline">
        <span>{t("gitRepo")}</span>
        <select class="field" bind:value={git.repo}>
          {#each repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
        </select>
      </label>
    {/if}
    {#snippet actions()}
      <button class="btn sm" onclick={fetchAll} disabled={!snap || !!busy} title={t("gitFetchHint")}>
        {#if busy === "fetch"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<CloudDownload size={14} aria-hidden="true" />{/if}{t("gitFetch")}
      </button>
      <button class="btn sm" onclick={() => current && pull(current)} disabled={!current?.upstream || !!busy} title={t("gitPullHint")}>
        {#if busy === "pull"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<ArrowDownToLine size={14} aria-hidden="true" />{/if}{t("gitPull")}
      </button>
      <button class="btn sm" onclick={() => current && push(current)} disabled={!current || !!busy} title={t("gitPushHint")}>
        {#if busy === `push:${current?.name}`}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<ArrowUpFromLine size={14} aria-hidden="true" />{/if}{t("gitPush")}
      </button>
      <button class="btn sm" onclick={() => load(true)} disabled={!git.repo || loading}>
        <RefreshCw size={14} aria-hidden="true" class={loading ? "spin" : ""} />{t("gitRefresh")}
      </button>
    {/snippet}
  </ViewHeader>

  {#if snap && here}
    <div class="strip">
      <span class="on">
        <GitBranch size={14} aria-hidden="true" />
        {#if snap.current}<strong class="bname">{snap.current}</strong>{:else}<strong>{t("gitDetached")}</strong>{#if snap.head}<span class="mono muted">{short(snap.head)}</span>{/if}{/if}
      </span>
      {#if current?.upstream}
        <span class="muted">{t("gitTracks", { upstream: current.upstream })}</span>
        {@render aheadBehind(current)}
        {#if !current.ahead && !current.behind && !current.gone}<span class="muted">{t("gitLevel")}</span>{/if}
      {:else if current}
        <span class="muted">{t("gitNoUpstream")}</span>
      {/if}
      <button class="tree" class:dirty={tally(here.changes, here.more).changed} class:conflict={tally(here.changes).conflicts} onclick={() => (selected = { kind: "worktree", path: here.path })}>
        {#if tally(here.changes).conflicts}<CircleAlert size={13} aria-hidden="true" />{/if}{treeSummary(here)}
      </button>
    </div>
  {/if}

  {#if snap && here?.operation}{@render opBox(here, here.operation)}{/if}

  {#if error || notice}
    <div class="banners">
      {#if error}<Banner kind="error" ondismiss={() => (error = "")}><span class="pre">{error}</span></Banner>{/if}
      {#if notice}
        <Banner kind={notice.kind} ondismiss={() => (notice = null)}>
          {notice.text}
          {#if notice.output && !(notice.kind === "warn" && here?.operation)}
            <details class="out">
              <summary>{t("git2Output")}</summary>
              <pre>{notice.output}</pre>
            </details>
          {/if}
        </Banner>
      {/if}
    </div>
  {/if}

  {#if !repos.length}
    {@const project = currentProject()}
    <EmptyState icon={GitBranch} title={t("gitNoRepo")} text={t("gitNoRepoHelp")}>
      <button class="btn" onclick={() => editProject(project?.id ?? "")}>
        <Pencil size={14} aria-hidden="true" />{t(project ? "ticketsEditProject" : "newProject")}
      </button>
    </EmptyState>
  {:else if loadError}
    <EmptyState>
      <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{loadError}</span></p>
      <button class="btn" onclick={() => load(false)}><RefreshCw size={14} aria-hidden="true" />{t("gitRetry")}</button>
    </EmptyState>
  {:else if !snap || !graph}
    <p class="loading muted" role="status"><LoaderCircle size={16} aria-hidden="true" class="spin" />{t("gitLoading")}</p>
  {:else}
    <div class="split">
      <nav class="list" aria-label={t("gitBranches")}>
        <h2 class="section">{t("gitLocal")}<span class="n">{groups.local.length}</span></h2>
        <ul>
          {#each groups.local as b (b.name)}{@render branchRow(b)}{:else}<li class="none">{t("gitNoBranch")}</li>{/each}
        </ul>
        {#if groups.remote.length}
          <h2 class="section">{t("gitRemote")}<span class="n">{groups.remote.length}</span></h2>
          <ul>
            {#each groups.remote as b (b.name)}{@render branchRow(b)}{/each}
          </ul>
        {/if}
        <h2 class="section">{t("gitWorktrees")}<span class="n">{snap.worktrees.length}</span></h2>
        <ul>
          {#each snap.worktrees as wt (wt.path)}
            {@const run = runOf(wt)}
            {@const picked = shown?.kind === "worktree" && shown.worktree.path === wt.path}
            <li class="row" class:current={picked}>
              <button class="pick" aria-current={picked} onclick={() => ([selected, form] = [{ kind: "worktree", path: wt.path }, null])}>
                <span class="line">
                  <FolderGit2 size={13} aria-hidden="true" class="muted-icon" />
                  {#if run}
                    <span class="label">{t("gitRunShort", { id: run.id, key: run.ticket.key })}</span>
                  {:else if wt.main}
                    <span class="label">{t("gitRepoFolder")}</span>
                  {:else}
                    <span class="bname">{where(wt).split("/").pop()}</span>
                  {/if}
                  <span class="badges">
                    {#if tally(wt.changes).conflicts}<span class="count danger">{t("gitConflicts", { count: tally(wt.changes).conflicts })}</span>{/if}
                    {#if wt.prunable}<span class="count warn">{t("gitWorktreeGone")}</span>{/if}
                  </span>
                </span>
                <span class="desc">{wt.branch ?? t("gitDetached")} · {wt.prunable ? where(wt) : treeSummary(wt)}</span>
              </button>
            </li>
          {/each}
        </ul>
        {#if snap.stashes.length}
          <h2 class="section">{t("git2Stashes")}<span class="n">{snap.stashes.length}</span></h2>
          <ul>
            {#each snap.stashes as st (st.oid)}
              {@const picked = shown?.kind === "stash" && shown.stash.oid === st.oid}
              <li class="row" class:current={picked}>
                <button class="pick" aria-current={picked} onclick={() => ([selected, form] = [{ kind: "stash", oid: st.oid }, null])}>
                  <span class="line"><Archive size={13} aria-hidden="true" class="muted-icon" /><span class="label">{st.message}</span></span>
                  <span class="desc">{`stash@{${st.index}}`} · {ago(st.date)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
        {#if snap.tags.length}
          <h2 class="section">{t("git2Tags")}<span class="n">{snap.tags.length}</span></h2>
          <ul>
            {#each snap.tags as tg (tg.name)}
              {@const picked = shown?.kind === "tag" && shown.tag.name === tg.name}
              <li class="row" class:current={picked}>
                <button class="pick" aria-current={picked} onclick={() => ([selected, form] = [{ kind: "tag", name: tg.name }, null])}>
                  <span class="line"><Tag size={13} aria-hidden="true" class="muted-icon" /><span class="bname">{tg.name}</span></span>
                  <span class="desc">{tg.message || short(tg.target)} · {ago(tg.date)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </nav>

      <section class="detail" aria-live="polite">
        {#if shown?.kind === "branch"}
          {@const b = shown.branch}
          {@const pr = prFor(b, prs, snap.remotes)}
          <header class="card-head">
            {#if b.remote}<Cloud size={18} aria-hidden="true" />{:else}<GitBranch size={18} aria-hidden="true" />{/if}
            <h2 class="bname">{b.name}</h2>
            {#if b.current}<span class="tag accent">{t("gitCurrent")}</span>{/if}
            {#if b.merged !== null && !isDefault(b)}
              <span class="tag" class:done={b.merged}>{b.merged ? t("gitMerged", { branch: snap.defaultBranch ?? "" }) : t("gitNotMerged", { branch: snap.defaultBranch ?? "" })}</span>
            {/if}
            {#if pr}
              <button class="tag pr {prState(pr)}" onclick={() => openPr(pr)} title={pr.title}>
                <GitPullRequest size={12} aria-hidden="true" />{prLabel(pr)}
              </button>
            {/if}
          </header>
          <dl class="meta">
            {#if !b.remote}
              <dt>{t("gitUpstream")}</dt>
              <dd>
                {#if b.upstream}
                  <span class="bname">{b.upstream}</span>
                  {#if b.gone}<span class="warn-text">{t("gitGoneHint")}</span>{:else if b.ahead || b.behind}<span class="ab">{@render aheadBehind(b)}</span>{:else}<span class="muted">{t("gitLevel")}</span>{/if}
                {:else}
                  <span class="muted">{t("gitNoUpstreamHint")}</span>
                {/if}
              </dd>
            {/if}
            <dt>{t("gitLastCommit")}</dt>
            <dd>{b.tip.subject} <span class="muted">· {b.tip.author} · {ago(b.tip.date)} ·</span> <span class="mono muted">{short(b.tip.oid)}</span></dd>
            {#if b.worktree}
              <dt>{t("gitWorktree")}</dt>
              <dd class="mono">{shortPath(b.worktree)}</dd>
            {/if}
          </dl>
          <div class="actions">
            {#if !b.current}
              <button class="btn sm primary" onclick={() => switchTo(b)} disabled={!!busy || !!b.worktree} title={b.worktree ? t("gitInWorktree", { path: shortPath(b.worktree) }) : undefined}>
                {#if busy === "switch"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<ArrowRightLeft size={14} aria-hidden="true" />{/if}{t("gitSwitch")}
              </button>
            {/if}
            {#if b.current && b.upstream}
              <button class="btn sm" onclick={() => pull(b)} disabled={!!busy} title={t("gitPullHint")}>
                <ArrowDownToLine size={14} aria-hidden="true" />{t("gitPull")}
              </button>
            {/if}
            {#if !b.remote}
              <button class="btn sm" onclick={() => push(b)} disabled={!!busy} title={t("gitPushHint")}>
                {#if busy === `push:${b.name}`}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<ArrowUpFromLine size={14} aria-hidden="true" />{/if}{t("gitPush")}
              </button>
            {/if}
            {#if !b.current && snap.current}
              <button class="btn sm" onclick={() => merge(b)} disabled={!!busy || !!here?.operation} title={t("git2MergeHint", { branch: b.name, current: snap.current })}>
                {#if busy === "merge"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<GitMerge size={14} aria-hidden="true" />{/if}{t("git2MergeInto", { current: snap.current })}
              </button>
              <button class="btn sm" onclick={() => askRebase(b)} disabled={!!busy || !!here?.operation} title={t("git2RebaseHint", { branch: b.name, current: snap.current })}>
                <GitCompareArrows size={14} aria-hidden="true" />{t("git2RebaseOnto", { current: snap.current })}
              </button>
            {/if}
            <button class="btn sm" aria-expanded={form?.kind === "create"} onclick={() => (form = { kind: "create", from: b.name, name: "", checkout: true })} disabled={!!busy}>
              <GitBranchPlus size={14} aria-hidden="true" />{t("gitNewBranch")}
            </button>
            {#if !b.remote}
              <button class="btn sm" aria-expanded={form?.kind === "rename"} onclick={() => (form = { kind: "rename", from: b.name, to: b.name })} disabled={!!busy}>
                <Pencil size={14} aria-hidden="true" />{t("gitRename")}
              </button>
              {#if b.upstream && b.ahead && b.behind && !isDefault(b)}
                <button class="btn sm quiet-danger" onclick={() => askForce(b)} disabled={!!busy} title={t("git2ForceHint")}>
                  <ArrowBigUpDash size={14} aria-hidden="true" />{t("git2ForcePush")}
                </button>
              {/if}
              {#if !b.current && !b.worktree}
                <button class="btn sm quiet-danger" onclick={() => askRemoval(b)} disabled={!!busy}>
                  <Trash size={14} aria-hidden="true" />{t("gitDelete")}
                </button>
              {/if}
            {:else if !isDefault(b)}
              <button class="btn sm quiet-danger" onclick={() => (remoteRemoval = { branch: b, open: true })} disabled={!!busy}>
                <Trash size={14} aria-hidden="true" />{t("gitDeleteRemote", { remote: b.remote })}
              </button>
            {/if}
          </div>
        {:else if shown?.kind === "worktree"}
          {@const wt = shown.worktree}
          {@const run = runOf(wt)}
          <header class="card-head">
            <FolderGit2 size={18} aria-hidden="true" />
            <h2 class:bname={!wt.main}>{wt.main ? t("gitRepoFolder") : where(wt)}</h2>
            {#if wt.locked}<span class="tag">{t("gitLocked")}</span>{/if}
            {#if wt.prunable}<span class="tag warn">{t("gitWorktreeGone")}</span>{/if}
          </header>
          <dl class="meta">
            <dt>{t("gitFolder")}</dt>
            <dd class="mono">{shortPath(wt.path)}</dd>
            <dt>{t("gitBranch")}</dt>
            <dd>{#if wt.branch}<span class="bname">{wt.branch}</span>{:else}{t("gitDetached")} <span class="mono muted">{short(wt.head ?? "")}</span>{/if}</dd>
            {#if run}
              <dt>{t("gitRun")}</dt>
              <dd><button class="link" onclick={() => showRun(run.id)}>{t("gitRunShort", { id: run.id, key: run.ticket.key })} · {run.title}</button></dd>
            {/if}
          </dl>
          {#if wt.operation && wt.path !== here?.path}{@render opBox(wt, wt.operation)}{/if}
          <h3>{t("gitChanges")}</h3>
          <GitChanges
            {wt}
            repo={git.repo}
            {busy}
            {act}
            ours={sidesOf(wt).ours}
            theirs={sidesOf(wt).theirs}
            onfile={(path, tab) => ([selected, form] = [{ kind: "file", dir: wt.path, path, tab }, null])}
          />
        {:else if shown?.kind === "commit" && !shown.commit}
          {#if detailError}
            <p class="notice error"><CircleAlert size={14} aria-hidden="true" /><span class="pre">{detailError}</span></p>
          {:else}
            <p class="muted small" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitLoading")}</p>
          {/if}
        {:else if shown?.kind === "commit" && shown.commit}
          {@const c = shown.commit}
          {@const d = detail?.commit.oid === c.oid ? detail : null}
          <header class="card-head">
            <GitCommitHorizontal size={18} aria-hidden="true" />
            <h2>{c.subject}</h2>
          </header>
          <dl class="meta">
            <dt>{t("gitCommit")}</dt>
            <dd class="mono">{c.oid}</dd>
            <dt>{t("gitAuthor")}</dt>
            <dd>{c.author} <span class="muted">· {ago(c.date)}</span></dd>
            {#if c.parents.length}
              <dt>{t(c.parents.length > 1 ? "gitMergeOf" : "gitParent")}</dt>
              <dd class="mono">{c.parents.map(short).join(" + ")}</dd>
            {/if}
            {#if d?.remote}
              <dt>{t("git2OnRemote")}</dt>
              <dd class="bname">{d.remote}</dd>
            {/if}
          </dl>
          {#if d && splitMessage(d.message).body}
            <pre class="body-text">{splitMessage(d.message).body}</pre>
          {/if}
          <div class="actions">
            <button class="btn sm" aria-expanded={form?.kind === "create"} onclick={() => (form = { kind: "create", from: c.oid, name: "", checkout: true })} disabled={!!busy}>
              <GitBranchPlus size={14} aria-hidden="true" />{t("gitBranchHere")}
            </button>
            <button class="btn sm" onclick={() => copy(c.oid)}><Copy size={14} aria-hidden="true" />{t("gitCopySha")}</button>
            <button class="btn sm" aria-expanded={form?.kind === "tag"} onclick={() => (form = { kind: "tag", target: c.oid, name: "", message: "" })} disabled={!!busy}>
              <TagPlus size={14} aria-hidden="true" />{t("git2TagHere")}
            </button>
            {#if snap.current && c.parents.length < 2}
              <button class="btn sm" onclick={() => cherryPick(c)} disabled={!!busy || !!here?.operation} title={t("git2CherryPickHint", { current: snap.current })}>
                <Cherry size={14} aria-hidden="true" />{t("git2CherryPick")}
              </button>
              <button class="btn sm" onclick={() => revert(c)} disabled={!!busy || !!here?.operation} title={t("git2RevertHint", { current: snap.current })}>
                <Undo2 size={14} aria-hidden="true" />{t("git2Revert")}
              </button>
            {/if}
            {#if snap.current && c.oid !== snap.head}
              <button class="btn sm quiet-danger" onclick={() => askReset(c)} disabled={!!busy || !!here?.operation}>
                <Rewind size={14} aria-hidden="true" />{t("git2ResetHere", { branch: snap.current })}
              </button>
            {/if}
          </div>
          {#if d}
            <h3>{t("gitChanges")}<span class="n">{d.files.length}</span></h3>
            <div class="files">
              {#each d.files as f, i (i)}
                <details class="file">
                  <summary>
                    <span class="path mono">{#if f.from}<span class="muted">{`${f.from} → `}</span>{/if}{f.path}</span>
                    <span class="stat mono"><span class="plus">+{f.added}</span> <span class="minus">−{f.removed}</span></span>
                  </summary>
                  <GitDiff diff={f} />
                </details>
              {/each}
            </div>
          {/if}
        {:else if shown?.kind === "tag"}
          {@const tg = shown.tag}
          {@const target = graph.commits.find((c) => c.oid === tg.target)}
          <header class="card-head">
            <Tag size={18} aria-hidden="true" />
            <h2 class="bname">{tg.name}</h2>
            <span class="tag">{t(tg.annotated ? "git2Annotated" : "git2Lightweight")}</span>
          </header>
          <dl class="meta">
            <dt>{t("gitCommit")}</dt>
            <dd><button class="link mono" onclick={() => pickCommit(tg.target)}>{short(tg.target)}</button>{#if target}<span>{target.subject}</span>{/if}</dd>
            {#if tg.annotated}
              <dt>{t("git2Message")}</dt>
              <dd>{tg.message}</dd>
              <dt>{t("gitAuthor")}</dt>
              <dd>{tg.tagger} <span class="muted">· {ago(tg.date)}</span></dd>
            {/if}
          </dl>
          <div class="actions">
            <button class="btn sm" onclick={() => pushTag(tg)} disabled={!!busy || !snap.remotes.length} title={t("git2TagPushHint")}>
              {#if busy === "tag-push"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<ArrowUpFromLine size={14} aria-hidden="true" />{/if}{t("gitPush")}
            </button>
            <button class="btn sm quiet-danger" onclick={() => askTagRemoval(tg)} disabled={!!busy}>
              <Trash size={14} aria-hidden="true" />{t("gitDelete")}
            </button>
          </div>
        {:else if shown?.kind === "stash"}
          {@const st = shown.stash}
          {@const files = stashFiles?.oid === st.oid ? stashFiles.files : null}
          <header class="card-head">
            <Archive size={18} aria-hidden="true" />
            <h2>{st.message}</h2>
          </header>
          <dl class="meta">
            <dt>{t("git2StashRef")}</dt>
            <dd class="mono">{`stash@{${st.index}}`}</dd>
            <dt>{t("git2Date")}</dt>
            <dd>{ago(st.date)}</dd>
          </dl>
          <div class="actions">
            <button class="btn sm primary" onclick={() => applyStash(st, true)} disabled={!!busy || !!here?.operation} title={t("git2PopHint")}>
              <ArchiveRestore size={14} aria-hidden="true" />{t("git2Pop")}
            </button>
            <button class="btn sm" onclick={() => applyStash(st, false)} disabled={!!busy || !!here?.operation} title={t("git2ApplyHint")}>{t("git2Apply")}</button>
            <button class="btn sm quiet-danger" onclick={() => (stashRemoval = { stash: st, open: true })} disabled={!!busy}>
              <Trash size={14} aria-hidden="true" />{t("gitDelete")}
            </button>
          </div>
          <h3>{t("gitChanges")}{#if files}<span class="n">{files.length}</span>{/if}</h3>
          {#if !files}
            <p class="muted small" role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitLoading")}</p>
          {:else}
            <div class="files">
              {#each files as f, i (i)}
                <details class="file">
                  <summary>
                    <span class="path mono">{f.path}</span>
                    <span class="stat mono"><span class="plus">+{f.added}</span> <span class="minus">−{f.removed}</span></span>
                  </summary>
                  <GitDiff diff={f} />
                </details>
              {/each}
            </div>
          {/if}
        {:else if shown?.kind === "file"}
          {@const f = shown.file}
          <header class="card-head">
            <FileText size={18} aria-hidden="true" />
            <h2 class="bname">{f.path}</h2>
          </header>
          {#key `${f.dir}\n${f.path}\n${f.tab}`}
            <GitFile dir={f.dir} path={f.path} tab={f.tab} onpick={pickCommit} />
          {/key}
        {/if}

        {#if form}
          {@const f = form}
          <form class="inline-form" onsubmit={(e) => (e.preventDefault(), f.kind === "create" ? create() : f.kind === "tag" ? createTag() : rename())}>
            {#if f.kind === "create"}
              <label class="labelled grow">
                <span>{t("gitNewBranchFrom", { from: f.from.length === 40 ? short(f.from) : f.from })}</span>
                <!-- svelte-ignore a11y_autofocus -->
                <input class="field mono" bind:value={f.name} placeholder={t("gitBranchName")} spellcheck="false" autocomplete="off" autofocus />
              </label>
              <label class="check"><input type="checkbox" bind:checked={f.checkout} />{t("gitCheckoutAfter")}</label>
            {:else if f.kind === "tag"}
              <label class="labelled grow">
                <span>{t("git2TagOn", { sha: short(f.target) })}</span>
                <!-- svelte-ignore a11y_autofocus -->
                <input class="field mono" bind:value={f.name} placeholder={t("git2TagPlaceholder")} spellcheck="false" autocomplete="off" autofocus />
              </label>
              <label class="labelled full">
                <span>{t("git2TagMessage")}</span>
                <textarea class="field area" bind:value={f.message} rows="2"></textarea>
              </label>
            {:else}
              <label class="labelled grow">
                <span>{t("gitRenameTo", { name: f.from })}</span>
                <!-- svelte-ignore a11y_autofocus -->
                <input class="field mono" bind:value={f.to} spellcheck="false" autocomplete="off" autofocus />
              </label>
            {/if}
            <div class="buttons">
              <button class="btn sm" type="button" onclick={() => (form = null)}>{t("cancel")}</button>
              <button class="btn sm primary" disabled={!!busy || !(f.kind === "rename" ? f.to.trim() && f.to.trim() !== f.from : f.name.trim())}>
                {t(f.kind === "rename" ? "gitRename" : "gitCreate")}
              </button>
            </div>
          </form>
        {/if}

        <h3 class="history">{t("gitHistory")}</h3>
        {#if !graph.commits.length}
          <p class="muted small">{t("gitNoCommit")}</p>
        {:else}
          <ol class="graph" bind:this={graphEl} aria-label={t("gitHistory")} style:--graph={`${graphWidth}px`}>
            {#each graph.commits as commit, i (commit.oid)}
              {@const row = rows[i]}
              {@const picked = pickedOid === commit.oid}
              <li data-oid={commit.oid}>
                <button class="commit" class:picked aria-current={picked} onclick={() => ([selected, form] = [{ kind: "commit", oid: commit.oid }, null])}>
                  <svg width={graphWidth} height={ROW} aria-hidden="true">
                    {#each row.lines as line, n (n)}<path d={path(line, LANE, ROW)} class="lane c{line.color}" />{/each}
                    {#if row.lane < MAX_LANES}
                      <circle
                        cx={LANE / 2 + row.lane * LANE}
                        cy={ROW / 2}
                        r={commit.oid === snap.head ? 5 : 4}
                        class="node c{row.color}"
                        class:merge={commit.parents.length > 1}
                        class:head={commit.oid === snap.head}
                      />
                    {/if}
                  </svg>
                  <span class="message">
                    {#each labels(commit) as ref (ref.kind + ref.name)}
                      <span class="ref {ref.kind}" class:head={ref.head}>
                        {#if ref.kind === "tag"}<Tag size={11} aria-hidden="true" />{:else if ref.kind === "remote"}<Cloud size={11} aria-hidden="true" />{:else}<GitBranch size={11} aria-hidden="true" />{/if}<span>{ref.name}</span>
                      </span>
                    {/each}
                    <span class="subject" class:merge={commit.parents.length > 1}>{commit.subject}</span>
                  </span>
                  <span class="author">{commit.author}</span>
                  <span class="when">{ago(commit.date)}</span>
                  <span class="sha mono">{short(commit.oid)}</span>
                </button>
              </li>
            {/each}
          </ol>
          {#if graph.more}
            <button class="btn sm more" onclick={more}>{t("gitMore")}</button>
          {/if}
        {/if}
      </section>
    </div>
  {/if}
</div>

{#if removal}
  {@const r = removal}
  <ConfirmDialog
    bind:open={r.open}
    title={t("gitDeleteTitle", { name: r.branch.name })}
    confirm={t("gitDelete")}
    danger={!r.unique || r.unique.total > 0}
    disabled={!r.unique || (r.unique.total > 0 && r.typed !== r.branch.name)}
    onconfirm={remove}
  >
    {#if !r.unique}
      <p role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitDeleteReading")}</p>
    {:else if !r.unique.total}
      <p>{t("gitDeleteSafe")}</p>
    {:else}
      <p class="lost">{t("gitDeleteLost", { count: r.unique.total })}</p>
      <ul class="lost-list">
        {#each r.unique.commits as c (c.oid)}<li><span class="mono">{short(c.oid)}</span> {c.subject}</li>{/each}
      </ul>
      {#if r.unique.total > r.unique.commits.length}<p>{t("gitChangesMore", { count: r.unique.total - r.unique.commits.length })}</p>{/if}
      <label class="labelled">
        <span>{t("gitDeleteTypeName", { name: r.branch.name })}</span>
        <input class="field mono" bind:value={r.typed} spellcheck="false" autocomplete="off" />
      </label>
    {/if}
  </ConfirmDialog>
{/if}

{#if remoteRemoval?.branch.remote}
  {@const r = remoteRemoval}
  <ConfirmDialog
    bind:open={r.open}
    title={t("gitDeleteRemoteTitle", { name: onRemote(r.branch), remote: r.branch.remote ?? "" })}
    confirm={t("gitDeleteRemote", { remote: r.branch.remote ?? "" })}
    danger
    onconfirm={removeRemote}
  >
    <p>{t("gitDeleteRemoteText", { remote: r.branch.remote ?? "" })}</p>
  </ConfirmDialog>
{/if}

{#if rebasing}
  {@const r = rebasing}
  <ConfirmDialog
    bind:open={r.open}
    title={t("git2RebaseTitle", { branch: snap?.current ?? "", onto: r.onto })}
    confirm={t("git2Rebase")}
    danger={!!r.plan?.pushed}
    disabled={!r.plan}
    onconfirm={rebase}
  >
    {#if !r.plan}
      <p role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitDeleteReading")}</p>
    {:else if !r.plan.total}
      <p>{t("git2RebaseNothing", { branch: snap?.current ?? "", onto: r.onto })}</p>
    {:else}
      <p>{t("git2RebaseRewrites", { count: r.plan.total })}</p>
      <ul class="lost-list">
        {#each r.plan.commits as c (c.oid)}<li><span class="mono">{short(c.oid)}</span> {c.subject}</li>{/each}
      </ul>
      {#if r.plan.total > r.plan.commits.length}<p>{t("gitChangesMore", { count: r.plan.total - r.plan.commits.length })}</p>{/if}
      {#if r.plan.pushed}<p class="lost">{t("git2RebasePushed", { count: r.plan.pushed })}</p>{/if}
      <p>{t("git2RebaseConflicts")}</p>
    {/if}
  </ConfirmDialog>
{/if}

{#if resetting}
  {@const r = resetting}
  <ConfirmDialog
    bind:open={r.open}
    title={t("git2ResetTitle", { branch: r.plan?.branch ?? snap?.current ?? "", sha: short(r.target.oid) })}
    confirm={t("git2Reset")}
    danger={r.mode === "hard" || !!r.plan?.lost.total}
    disabled={!r.plan || (resetLoses(r) && r.typed !== r.plan.branch)}
    onconfirm={reset}
  >
    <div class="segmented" role="radiogroup" aria-label={t("git2ResetMode")}>
      {#each ["soft", "mixed", "hard"] as const as m (m)}
        <button role="radio" aria-checked={r.mode === m} onclick={() => (r.mode = m)}>{t(modeLabel[m])}</button>
      {/each}
    </div>
    <p>{t(modeHelp[r.mode])}</p>
    {#if !r.plan}
      <p role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitDeleteReading")}</p>
    {:else}
      {#if r.plan.leaving}<p>{t("git2ResetLeaving", { branch: r.plan.branch, count: r.plan.leaving })}</p>{/if}
      {#if r.plan.lost.total}
        <p class="lost">{t("git2ResetLost", { count: r.plan.lost.total })}</p>
        <ul class="lost-list">
          {#each r.plan.lost.commits as c (c.oid)}<li><span class="mono">{short(c.oid)}</span> {c.subject}</li>{/each}
        </ul>
        {#if r.plan.lost.total > r.plan.lost.commits.length}<p>{t("gitChangesMore", { count: r.plan.lost.total - r.plan.lost.commits.length })}</p>{/if}
      {/if}
      {#if r.mode === "hard" && r.plan.changes.length}
        <p class="lost">{t("git2ResetChanges", { count: r.plan.changes.length })}</p>
        <ul class="lost-list mono">
          {#each r.plan.changes as c (c.index + c.tree + c.path)}<li>{c.path}</li>{/each}
        </ul>
      {/if}
      {#if !resetLoses(r)}<p>{t("git2ResetSafe")}</p>{/if}
      {#if resetLoses(r)}
        <label class="labelled">
          <span>{t("gitDeleteTypeName", { name: r.plan.branch })}</span>
          <input class="field mono" bind:value={r.typed} spellcheck="false" autocomplete="off" />
        </label>
      {/if}
    {/if}
  </ConfirmDialog>
{/if}

{#if forcing}
  {@const f = forcing}
  <ConfirmDialog
    bind:open={f.open}
    title={t("git2ForceTitle", { branch: f.branch.name, remote: f.plan?.remote ?? "" })}
    confirm={t("git2ForcePush")}
    danger
    disabled={!f.plan || f.typed !== f.branch.name}
    onconfirm={force}
  >
    {#if !f.plan}
      <p role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitDeleteReading")}</p>
    {:else}
      <p>{t("git2ForceText", { remote: f.plan.remote, branch: f.branch.name })}</p>
      {#if f.plan.lost.total}
        <p class="lost">{t("git2ForceLost", { count: f.plan.lost.total, remote: f.plan.remote, branch: f.branch.name })}</p>
        <ul class="lost-list">
          {#each f.plan.lost.commits as c (c.oid)}<li><span class="mono">{short(c.oid)}</span> {c.subject} <span class="muted">· {c.author}</span></li>{/each}
        </ul>
        {#if f.plan.lost.total > f.plan.lost.commits.length}<p>{t("gitChangesMore", { count: f.plan.lost.total - f.plan.lost.commits.length })}</p>{/if}
      {:else}
        <p>{t("git2ForceSafe", { remote: f.plan.remote, branch: f.branch.name })}</p>
      {/if}
      <label class="labelled">
        <span>{t("gitDeleteTypeName", { name: f.branch.name })}</span>
        <input class="field mono" bind:value={f.typed} spellcheck="false" autocomplete="off" />
      </label>
    {/if}
  </ConfirmDialog>
{/if}

{#if tagRemoval}
  {@const r = tagRemoval}
  <ConfirmDialog
    bind:open={r.open}
    title={t("git2TagDeleteTitle", { name: r.tag.name })}
    confirm={t("gitDelete")}
    danger={!r.unique || r.unique.total > 0}
    disabled={!r.unique || (r.unique.total > 0 && r.typed !== r.tag.name)}
    onconfirm={removeTag}
  >
    {#if !r.unique}
      <p role="status"><LoaderCircle size={14} aria-hidden="true" class="spin" /> {t("gitDeleteReading")}</p>
    {:else if !r.unique.total}
      <p>{t("git2TagDeleteSafe")}</p>
    {:else}
      <p class="lost">{t("git2TagDeleteLost", { count: r.unique.total })}</p>
      <ul class="lost-list">
        {#each r.unique.commits as c (c.oid)}<li><span class="mono">{short(c.oid)}</span> {c.subject}</li>{/each}
      </ul>
      <label class="labelled">
        <span>{t("gitDeleteTypeName", { name: r.tag.name })}</span>
        <input class="field mono" bind:value={r.typed} spellcheck="false" autocomplete="off" />
      </label>
    {/if}
  </ConfirmDialog>
{/if}

{#if stashRemoval}
  {@const r = stashRemoval}
  {@const files = stashFiles?.oid === r.stash.oid ? stashFiles.files : null}
  <ConfirmDialog bind:open={r.open} title={t("git2StashDropTitle", { name: `stash@{${r.stash.index}}` })} confirm={t("gitDelete")} danger onconfirm={dropStash}>
    <p class="lost">{t("git2StashDropLost", { message: r.stash.message })}</p>
    {#if files}
      <ul class="lost-list">
        {#each files as f, i (i)}<li><span class="mono">{f.path}</span> <span class="muted">+{f.added} −{f.removed}</span></li>{/each}
      </ul>
    {/if}
  </ConfirmDialog>
{/if}

{#if aborting}
  {@const a = aborting}
  <ConfirmDialog bind:open={a.open} title={t("git2AbortTitle")} confirm={t("git2Abort")} danger onconfirm={() => proceed(a.dir, "abort")}>
    <p class="lost">{t(a.op.kind === "rebase" ? "git2AbortRebase" : a.op.kind === "merge" ? "git2AbortMerge" : "git2AbortCommit", { branch: a.op.branch ?? "" })}</p>
  </ConfirmDialog>
{/if}

<style>
  .git {
    --lane-0: #4a4ad9;
    --lane-1: #0f8a6a;
    --lane-2: #d1495b;
    --lane-3: #b8740a;
    --lane-4: #7c3aed;
    --lane-5: #0e7490;
    --lane-6: #b4469b;
    --lane-7: #5b7a1e;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
    container: git / inline-size;
  }

  @media (prefers-color-scheme: dark) {
    .git {
      --lane-0: #8a8cff;
      --lane-1: #3cc59b;
      --lane-2: #f07a8a;
      --lane-3: #f2b03d;
      --lane-4: #b18cff;
      --lane-5: #4cc3dc;
      --lane-6: #e07fcd;
      --lane-7: #a3c95a;
    }
  }

  .c0 { --c: var(--lane-0); }
  .c1 { --c: var(--lane-1); }
  .c2 { --c: var(--lane-2); }
  .c3 { --c: var(--lane-3); }
  .c4 { --c: var(--lane-4); }
  .c5 { --c: var(--lane-5); }
  .c6 { --c: var(--lane-6); }
  .c7 { --c: var(--lane-7); }

  .inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }

  .inline select {
    width: 200px;
  }

  .strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    min-height: 36px;
    padding: 6px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--chrome);
    font-size: var(--text-sm);
  }

  .on {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .tree {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-left: auto;
    padding: 2px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
    background: var(--panel);
    color: var(--done);
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .tree.dirty {
    color: var(--warning);
  }

  .tree.conflict {
    border-color: var(--danger);
    color: var(--danger);
  }

  .tree:hover {
    background: var(--hover);
  }

  .bname {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .strong {
    font-weight: 600;
  }

  .pre {
    white-space: pre-line;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 40px auto;
  }

  .list {
    display: flex;
    flex-direction: column;
  }

  .list ul {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
  }

  .section {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 6px 8px 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .section .n {
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }

  .none {
    padding: 4px 8px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .line .bname,
  .line .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pick .desc {
    padding-left: 19px;
  }

  .line :global(.muted-icon) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .dot {
    flex-shrink: 0;
    width: 9px;
    height: 9px;
    margin: 0 2px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }

  .badges {
    display: inline-flex;
    flex-shrink: 0;
    gap: 4px;
    margin-left: auto;
  }

  .count {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    font-weight: 500;
    line-height: 17px;
  }

  .count.warn {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .count.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .count.pr.open,
  .tag.pr.open {
    background: var(--done-soft);
    color: var(--done);
  }

  .count.pr.merged,
  .tag.pr.merged {
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .count.pr.closed,
  .tag.pr.closed {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .detail {
    display: flex;
    flex-direction: column;
  }

  .detail > :global(*) {
    max-width: none;
  }

  .card-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .card-head h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  .card-head h2.bname {
    font-size: var(--text-lg);
  }

  .tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 8px;
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 18px;
  }

  .tag.accent {
    background: var(--accent);
    color: var(--accent-text);
  }

  .tag.done {
    background: var(--done-soft);
    color: var(--done);
  }

  .tag.warn {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  button.tag:hover {
    text-decoration: underline;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: baseline;
    gap: 6px 16px;
    margin: 16px 0 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 8px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  .ab {
    display: inline-flex;
    gap: 4px;
  }

  .warn-text {
    color: var(--warning);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-ink);
    text-align: left;
  }

  .link:hover {
    text-decoration: underline;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 14px;
  }

  .quiet-danger:not(:disabled):hover {
    border-color: var(--danger);
    color: var(--danger);
  }

  .inline-form {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 10px 14px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .grow {
    flex: 1;
    min-width: 240px;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  h3 {
    margin: 22px 0 8px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .history {
    margin-top: 26px;
  }

  .graph {
    margin: 0 -12px;
    padding: 0;
    list-style: none;
  }

  .commit {
    display: grid;
    grid-template-columns: var(--graph) minmax(0, 1fr) minmax(0, 110px) 92px 60px;
    align-items: center;
    gap: 12px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-size: var(--text-sm);
    text-align: left;
  }

  .commit:hover {
    background: var(--hover);
  }

  .commit.picked {
    background: var(--selected);
  }

  svg {
    display: block;
    overflow: hidden;
  }

  .lane {
    fill: none;
    stroke: var(--c);
    stroke-width: 2;
    stroke-linecap: round;
  }

  .node {
    fill: var(--c);
    stroke: var(--panel);
    stroke-width: 1.5;
  }

  .node.merge {
    fill: var(--panel);
    stroke: var(--c);
    stroke-width: 2;
  }

  .node.head {
    stroke: var(--c);
    stroke-width: 2;
    fill: var(--panel);
    outline: none;
  }

  .message {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    overflow: hidden;
  }

  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subject.merge {
    color: var(--muted);
  }

  .ref {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    gap: 3px;
    max-width: 220px;
    padding: 0 6px;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 17px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ref :global(svg) {
    flex-shrink: 0;
  }

  .ref span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ref.local {
    border-color: var(--accent);
    color: var(--accent-ink);
  }

  .ref.head {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-text);
    font-weight: 600;
  }

  .ref.remote {
    color: var(--muted);
  }

  .ref.tag {
    border-color: var(--waiting-ring);
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .author,
  .when {
    overflow: hidden;
    color: var(--muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .when {
    font-variant-numeric: tabular-nums;
  }

  .sha {
    color: var(--muted);
  }

  .more {
    display: flex;
    margin: 10px auto 0;
  }

  .lost {
    color: var(--danger);
    font-weight: 600;
  }

  .lost-list {
    max-height: 180px;
    margin: 0;
    padding: 8px 10px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    list-style: none;
    color: var(--text);
    font-size: var(--text-sm);
  }

  /* A narrow window or a panel beside another: fewer graph columns, then the list above the detail. */
  @container git (max-width: 1000px) {
    .split {
      grid-template-columns: 280px minmax(0, 1fr);
    }

    .commit {
      grid-template-columns: var(--graph) minmax(0, 1fr) 84px;
    }

    .author,
    .sha {
      display: none;
    }
  }

  @container git (max-width: 700px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 2fr) minmax(0, 3fr);
    }

    .split > .list {
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }

    .split > .detail {
      padding: 16px 16px 24px;
    }
  }

  .lost-list li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .op {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--waiting-ring);
    background: var(--waiting-soft);
    font-size: var(--text-sm);
  }

  .detail .op {
    margin: 14px 0 0;
    border: 1px solid var(--waiting-ring);
    border-radius: var(--radius-lg);
  }

  .op.conflicted {
    border-color: var(--danger);
    background: var(--danger-soft);
  }

  .op :global(.op-icon) {
    flex-shrink: 0;
    color: var(--warning);
  }

  .op.conflicted :global(.op-icon),
  .op.conflicted .op-state {
    color: var(--danger);
  }

  .op-text {
    display: flex;
    flex: 1;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 2px 10px;
    min-width: 240px;
  }

  .op-state {
    font-weight: 600;
  }

  .op-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .op .out {
    flex-basis: 100%;
  }

  .out summary {
    width: max-content;
    color: var(--muted);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .out pre {
    max-height: 220px;
    margin: 6px 0 0;
    padding: 8px 10px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
  }

  h3 .n {
    margin-left: 6px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    font-weight: 500;
  }

  .body-text {
    margin: 12px 0 0;
    padding: 10px 12px;
    border-left: 3px solid var(--border-strong);
    color: var(--text);
    font-family: var(--font-ui);
    font-size: var(--text-sm);
    white-space: pre-wrap;
  }

  .files {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .file summary {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 4px;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
    list-style: none;
    cursor: pointer;
  }

  .file summary::-webkit-details-marker {
    display: none;
  }

  .file summary::before {
    content: "";
    flex-shrink: 0;
    width: 6px;
    height: 6px;
    margin: 0 2px;
    border-right: 1.5px solid var(--muted);
    border-bottom: 1.5px solid var(--muted);
    transform: rotate(-45deg);
    transition: transform var(--dur-fast) ease-out;
  }

  .file[open] summary::before {
    transform: rotate(45deg);
  }

  .file summary:hover {
    background: var(--hover);
  }

  .file[open] summary {
    margin-bottom: 6px;
  }

  .file .path {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .stat {
    flex-shrink: 0;
    font-weight: 600;
  }

  .plus {
    color: var(--done);
  }

  .minus {
    color: var(--danger);
  }

  .full {
    flex-basis: 100%;
  }

  .area {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
    font: inherit;
  }
</style>
