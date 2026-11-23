<script lang="ts">
  import { approxUsd } from "$lib/usage.svelte";
  import { onMount, type Component } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Check from "@lucide/svelte/icons/check";
  import FileCog from "@lucide/svelte/icons/file-cog";
  import Gauge from "@lucide/svelte/icons/gauge";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import Plus from "@lucide/svelte/icons/plus";
  import ScanSearch from "@lucide/svelte/icons/scan-search";
  import Users from "@lucide/svelte/icons/users";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import SquareSlash from "@lucide/svelte/icons/square-slash";
  import Terminal from "@lucide/svelte/icons/terminal";
  import UserCog from "@lucide/svelte/icons/user-cog";
  import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
  import X from "@lucide/svelte/icons/x";
  import Zap from "@lucide/svelte/icons/zap";
  import { lang, t } from "$lib/i18n";
  import { library, lineDiff, slugify } from "$lib/library.svelte";
  import { readText, type Read } from "$lib/onboarding.svelte";
  import type { Project } from "$lib/state.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { upsertRun } from "$lib/team.svelte";
  import type { TeamRun } from "$lib/team-run-logic";
  import StatusPill from "$lib/ui/StatusPill.svelte";

  type ItemKind = "rule" | "hook" | "preference" | "skill" | "command" | "role" | "repo";
  type Level = "advise" | "warn" | "block";
  type Review = "pending" | "accepted" | "rejected";
  type Proposal = {
    kind: ItemKind;
    name: string;
    sentence: string;
    description: string;
    how: string;
    level: Level;
    roles: string[];
    body: string;
    script: string;
    event: string;
    matcher: string;
    repo: string;
    path: string;
    content: string;
    before?: string;
    review: Review;
  };
  type Pack = {
    model: string;
    effort: string;
    cost?: number;
    questions: string[];
    dropped: string[];
    answers: { question: string; answer: string }[];
    items: Proposal[];
    /** The text it was proposed from (absent before TermF recorded it). */
    from?: string;
  };
  type Diff = { name: string; change: "added" | "changed" | "unchanged" | "removed"; previous: Proposal | null };
  type Convention = {
    slug: string;
    source: string;
    pack: Pack | null;
    draft: Pack | null;
    diff: Diff[];
    active: string[];
  };
  type Impact = { name: string; repo: string; changes: boolean } & (
    | { status: "done"; count: number; findings: string[] }
    | { status: "unsupported" }
    | { status: "failed"; message: string }
  );

  /** `only`: that convention alone, its text written elsewhere (the onboarding assistant); proposed on arrival. */
  let { project, only }: { project: Project; only?: string } = $props();

  let model = $state("");
  let effort = $state("");
  let budget = $state(0);
  let list = $state<Convention[]>([]);
  let loaded = $state(false);
  let slug = $state("");
  let text = $state("");
  let newName = $state("");
  let creating = $state(false);
  let confirmDelete = $state(false);
  /** The slow call in progress: "propose", "impact", "revise:<name>", or "" when idle. */
  let running = $state("");
  let error = $state("");
  let answers = $state<Record<string, string>>({});
  let comments = $state<Record<string, string>>({});
  let impact = $state<Impact[] | null>(null);
  /** What Claude Code reads while it proposes, the latest last. */
  let steps = $state<Read[]>([]);
  $effect(() => {
    const off = listen<Read & { project: string; slug: string }>("conventions-progress", (e) => {
      if (e.payload.project === project.id && e.payload.slug === slug) steps = [...steps, e.payload].slice(-6);
    });
    return () => void off.then((f) => f());
  });

  const current = $derived(list.find((c) => c.slug === slug));
  const draft = $derived(current?.draft ?? null);
  const pack = $derived(current?.pack ?? null);
  const dirty = $derived(!!current && text !== current.source);
  const pending = $derived(draft?.items.filter((i) => i.review === "pending").length ?? 0);
  const accepted = $derived(draft?.items.filter((i) => i.review === "accepted").length ?? 0);
  /** Accepted repository files, which the team may deliver instead. */
  const toTeam = $derived(draft?.items.filter((i) => i.review === "accepted" && i.kind === "repo").length ?? 0);

  const kindInfo: Record<ItemKind, { icon: Component; label: Parameters<typeof t>[0] }> = {
    rule: { icon: ShieldCheck, label: "convRule" },
    hook: { icon: Zap, label: "convHook" },
    preference: { icon: SlidersHorizontal, label: "convPreference" },
    skill: { icon: WandSparkles, label: "convSkill" },
    command: { icon: SquareSlash, label: "convCommand" },
    role: { icon: UserCog, label: "convRole" },
    repo: { icon: FileCog, label: "convRepo" },
  };
  const levels: [Level, Parameters<typeof t>[0]][] = [
    ["advise", "levelAdvise"],
    ["warn", "levelWarn"],
    ["block", "levelBlock"],
  ];
  const changeLabel = {
    added: "changeAdded",
    changed: "changeChanged",
    unchanged: "changeUnchanged",
    removed: "changeRemoved",
  } as const;
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" });

  onMount(async () => {
    try {
      const data = await invoke<{ model: string; effort: string; budget: number; list: Convention[] }>("conventions_list", {
        project: project.id,
      });
      ({ model, effort, budget, list } = data);
      loaded = true;
      if (only) {
        select(only);
        // Nothing proposed yet, or proposed from a text the assistant has rewritten since.
        const last = current?.draft ?? current?.pack;
        if (current?.source.trim() && (!last || (last.from && last.from !== current.source))) await propose();
        return;
      }
      // The to-do may name the convention to open, or ask for one drawn from the code.
      const asked = library.convention;
      const deducing = library.deduce;
      Object.assign(library, { convention: null, deduce: false });
      select(list.some((c) => c.slug === asked) ? (asked as string) : (list[0]?.slug ?? ""));
      creating = !list.length;
      if (deducing) await deduce();
    } catch (e) {
      error = String(e);
    }
  });

  /** A convention whose text says to follow what the code already does, proposed at once. */
  async function deduce() {
    const base = t("watchDeduceSlug");
    let next = base;
    for (let n = 2; list.some((c) => c.slug === next); n++) next = `${base}-${n}`;
    slug = next;
    if (!(await update("conventions_save", { source: t("watchDeduceText") }))) return;
    select(next);
    await propose();
  }

  async function discard() {
    if (!(await update("conventions_discard"))) return;
    const at = slug;
    toast(t("watchConvDiscarded"), {
      undo: async () => {
        const back = await invoke<Convention>("conventions_restore", { project: project.id, slug: at }).catch((e) => {
          toast(String(e), { kind: "error" });
          return null;
        });
        if (back) {
          put(back);
          toast(t("watchConvRestored"), { kind: "info" });
        }
      },
    });
  }

  /** The accepted repository files go to the team (one run per repository, delivered as a pull request); the
   * rest applies here. */
  async function handOff() {
    if (!draft) return;
    const files = draft.items.filter((i) => i.review === "accepted" && i.kind === "repo");
    running = "team";
    error = "";
    try {
      const repos = [...new Set(files.map((f) => f.repo))];
      for (const repo of repos) {
        const text = t("watchTeamConvRequest", {
          repo,
          slug,
          files: files
            .filter((f) => f.repo === repo)
            .map((f) => t("watchTeamConvFile", { path: f.path, why: f.description, sentence: f.sentence, content: f.content.trimEnd() }))
            .join("\n\n"),
        });
        const run = await invoke<TeamRun>("team_start", { run: null, request: { text, project: project.id, repo }, options: {} });
        upsertRun(run);
        toast(t("watchTeamStarted", { run: run.run }));
      }
      const convention = await invoke<Convention>("conventions_apply", { project: project.id, slug, repos: false });
      put(convention);
      library.version++;
    } catch (e) {
      error = String(e);
    } finally {
      running = "";
    }
  }

  function select(next: string) {
    slug = next;
    text = list.find((c) => c.slug === next)?.source ?? "";
    impact = null;
    confirmDelete = false;
    creating = false;
  }

  function put(convention: Convention) {
    const i = list.findIndex((c) => c.slug === convention.slug);
    if (i < 0) list.push(convention);
    else list[i] = convention;
  }

  async function call<T>(command: string, args: Record<string, unknown> = {}, what = "busy"): Promise<T | undefined> {
    running = what;
    error = "";
    try {
      return await invoke<T>(command, { project: project.id, slug, ...args });
    } catch (e) {
      error = String(e);
    } finally {
      running = "";
    }
  }

  async function update(command: string, args: Record<string, unknown> = {}, what = "busy") {
    const convention = await call<Convention>(command, args, what);
    if (convention) put(convention);
    if (command !== "conventions_review") library.version++;
    return convention;
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const next = slugify(newName);
    if (!next) return;
    if (list.some((c) => c.slug === next)) return select(next);
    slug = next;
    if (await update("conventions_save", { source: "" })) {
      newName = "";
      select(next);
    }
  }

  async function save() {
    return update("conventions_save", { source: text });
  }

  async function propose(withAnswers = false) {
    if (dirty && !(await save())) return;
    impact = null;
    steps = [];
    const given = withAnswers
      ? (draft?.questions ?? []).map((question) => ({ question, answer: answers[question] ?? "" }))
      : [];
    if (await update("conventions_propose", { answers: given }, "propose")) answers = {};
  }

  async function revise(name: string) {
    if (await update("conventions_revise", { name, comment: comments[name] ?? "" }, `revise:${name}`)) {
      delete comments[name];
    }
  }

  function decide(name: string, review: Review | null, level: Level | null = null) {
    return update("conventions_review", { name, review, level });
  }

  async function measure() {
    impact = (await call<Impact[]>("conventions_impact", {}, "impact")) ?? impact;
  }

  async function remove() {
    await call("conventions_delete");
    if (error) return;
    list = list.filter((c) => c.slug !== slug);
    select(list[0]?.slug ?? "");
    creating = !list.length;
    library.version++;
  }

  function groups(items: Proposal[]): [string, Proposal[]][] {
    const map = new Map<string, Proposal[]>();
    for (const item of items) map.set(item.sentence, [...(map.get(item.sentence) ?? []), item]);
    return [...map];
  }

  const checked = (item: Proposal) => item.kind === "rule" && !!item.script;
  const executable = (item: Proposal) => item.kind === "hook" || checked(item);
  const content = (item: Proposal) => [item.body, item.script, item.content].filter(Boolean).join("\n\n");

  function measured(name: string) {
    return impact?.filter((i) => i.name === name) ?? [];
  }

  function status(c: Convention): string {
    if (c.draft?.questions.length) return t("convStatusQuestions");
    if (c.draft) return t("convStatusReview");
    if (c.pack) return t("convStatusApplied", { count: c.pack.items.length });
    return t("convStatusNew");
  }
</script>

{#snippet kindBadge(item: Proposal)}
  {@const info = kindInfo[item.kind]}
  <span class="badge kind"><info.icon size={12} aria-hidden="true" />{t(info.label)}</span>
{/snippet}

{#snippet impactLine(item: Proposal)}
  {@const results = measured(item.name)}
  <div class="impact">
    <Gauge size={14} aria-hidden="true" />
    {#if !results.length}
      <span class="muted">{t("impactNotMeasured")}</span>
    {:else}
      <ul>
        {#each results as r (r.repo)}
          <li>
            <span class="repo">{r.repo}</span>
            {#if r.status === "done" && r.changes}
              <span class:clean={!r.count} class:found={r.count} title={t("watchImpactChanges")}>
                {r.count ? t("watchImpactRecent", { count: r.count }) : t("watchImpactRecentClean")}
              </span>
              {#if r.findings.length}
                <pre class="findings">{r.findings.join("\n")}</pre>
              {/if}
            {:else if r.status === "done"}
              <span class:clean={!r.count} class:found={r.count}>
                {r.count ? t("impactCount", { count: r.count }) : t("impactClean")}
              </span>
              {#if r.findings.length}
                <pre class="findings">{r.findings.join("\n")}</pre>
              {/if}
            {:else if r.status === "unsupported"}
              <span class="muted">{t("impactUnsupported")}</span>
            {:else}
              <span class="failed">{t("impactFailed", { message: r.message })}</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
{/snippet}

{#snippet diffView(before: string, after: string)}
  <pre class="code diff">{#each lineDiff(before, after) as line, i (i)}<span class="l{line.op === '+' ? ' add' : line.op === '-' ? ' del' : ''}">{line.op} {line.text}
</span>{/each}</pre>
{/snippet}

<div class="conventions" class:only>
  <aside class="docs">
    <ul>
      {#each list as c (c.slug)}
        <li>
          <button class="doc" aria-current={c.slug === slug && !creating} onclick={() => select(c.slug)}>
            <span class="doc-name">{c.slug}</span>
            <span class="doc-status">{status(c)}</span>
          </button>
        </li>
      {/each}
    </ul>
    <button class="btn sm quiet add" onclick={() => (creating = true)} disabled={creating}>
      <Plus size={14} aria-hidden="true" />{t("convNew")}
    </button>
  </aside>

  <section class="doc-pane">
    {#if error}
      <p class="notice error" role="alert">{error}</p>
    {/if}

    {#if !loaded && !error}
      <Skeleton rows={6} />
    {:else if creating || !current}
      <div class="intro">
        <Sparkles size={20} aria-hidden="true" />
        <h2>{t("convIntroTitle")}</h2>
        <p>{t("convIntro")}</p>
        <div class="deduce">
          <div>
            <h3>{t("watchDeduceTitle")}</h3>
            <p>{t("watchDeduceHint")}</p>
            <p class="model">{t("convModel", { model, effort })} · {t("watchConvBudget", { budget: money.format(budget) })}</p>
          </div>
          <button class="btn primary" onclick={deduce} disabled={!!running}>
            <ScanSearch size={14} aria-hidden="true" />{t("watchDeduce")}
          </button>
        </div>
        <p class="or">{t("watchOrWrite")}</p>
        <form class="new" onsubmit={create}>
          <input
            class="field"
            bind:value={newName}
            placeholder={t("convNamePlaceholder")}
            aria-label={t("convName")}
            {@attach (el) => el.focus()}
          />
          {#if list.length}
            <button type="button" class="btn" onclick={() => (creating = false)}>{t("cancel")}</button>
          {/if}
          <button type="submit" class="btn primary" disabled={!slugify(newName)}>{t("convCreate")}</button>
        </form>
      </div>
    {:else}
      <header class="doc-head">
        <div>
          <h2>{current.slug}</h2>
          <p class="model">
            <Sparkles size={13} aria-hidden="true" />
            {t("convModel", { model, effort })} · {t("watchConvBudget", { budget: money.format(budget) })}
            <span class="hint">{t("convModelHint")}</span>
          </p>
        </div>
        {#if confirmDelete}
          <div class="confirm" role="alert">
            <span>{t("convDeleteConfirm", { name: current.slug })}</span>
            <button class="btn sm" onclick={() => (confirmDelete = false)}>{t("cancel")}</button>
            <button class="btn sm danger" onclick={remove}>{t("delete")}</button>
          </div>
        {:else}
          <button class="btn sm quiet danger" onclick={() => (confirmDelete = true)}>{t("delete")}</button>
        {/if}
      </header>

      <section class="step">
        {#if !only}
          <h3>{t("convWrite")}</h3>
          <textarea
            class="field text"
            bind:value={text}
            rows="7"
            placeholder={t("convPlaceholder")}
            aria-label={t("convWrite")}
            disabled={running === "propose"}
          ></textarea>
        {/if}
        <div class="row-end">
          {#if only}
            <p class="model grow">
              <Sparkles size={13} aria-hidden="true" />
              {t("convModel", { model, effort })}
            </p>
          {:else}
            <span class="muted saved">{dirty ? t("convUnsaved") : ""}</span>
            <button class="btn" onclick={save} disabled={!dirty || !!running}>{t("save")}</button>
          {/if}
          <button class="btn" class:primary={!only || !(pack || draft)} onclick={() => propose()} disabled={!text.trim() || !!running}>
            <Sparkles size={14} aria-hidden="true" />{pack || draft ? t("convRegenerate") : t("convPropose")}
          </button>
        </div>
        {#if running === "propose"}
          <p class="notice working" role="status">
            <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("convRunning", { model })}
          </p>
          {#if steps.length}
            <ol class="feed">
              {#each steps as step, i (i)}<li class="mono">{readText(step)}</li>{/each}
            </ol>
          {/if}
        {/if}
      </section>

      {#if draft?.questions.length}
        <section class="step">
          <h3>{t("convQuestions")}</h3>
          <p class="muted lead">{t("convQuestionsHelp")}</p>
          {#each draft.questions as question (question)}
            <label class="question">
              <span>{question}</span>
              <textarea class="field" rows="2" bind:value={answers[question]}></textarea>
            </label>
          {/each}
          <div class="row-end">
            <button
              class="btn primary"
              onclick={() => propose(true)}
              disabled={!!running || draft.questions.every((q) => !answers[q]?.trim())}
            >
              {t("convAnswer")}
            </button>
          </div>
        </section>
      {/if}

      {#if draft && !draft.questions.length && (draft.items.length || draft.dropped.length)}
        {@const removed = current.diff.filter((d) => d.change === "removed")}
        <section class="step">
          <div class="step-head">
            <h3>{t("convReview")}</h3>
            <span class="muted">
              {t("convReviewCounts", { total: draft.items.length, accepted, pending })}
              {#if draft.cost !== undefined}· {t("convCost", { cost: approxUsd(money.format(draft.cost)) })}{/if}
            </span>
          </div>
          <p class="muted lead">{t("convReviewHelp")}</p>
          {#if pack}
            <p class="notice">
              {t("convReplaces", {
                added: current.diff.filter((d) => d.change === "added").length,
                changed: current.diff.filter((d) => d.change === "changed").length,
                removed: removed.length,
              })}
            </p>
          {/if}

          {#each groups(draft.items) as [sentence, items] (sentence)}
            <div class="group">
              <p class="sentence">{t("quoted", { text: sentence })}</p>
              {#each items as item (item.name)}
                {@const change = current.diff.find((d) => d.name === item.name)}
                <article class="card {item.review}">
                  <header>
                    {@render kindBadge(item)}
                    <span class="item-name">{item.name}</span>
                    {#if change && pack}
                      <span class="badge change-{change.change}">{t(changeLabel[change.change])}</span>
                    {/if}
                    {#if executable(item)}
                      <span class="badge flag"><Terminal size={12} aria-hidden="true" />{t("convExecutable")}</span>
                    {/if}
                    {#if item.kind === "repo"}
                      <span class="badge flag"><FileCog size={12} aria-hidden="true" />{t("convWritesRepo", { repo: item.repo })}</span>
                    {/if}
                    {#if item.review !== "pending"}
                      <span class="decision"><StatusPill status={item.review} /></span>
                    {/if}
                  </header>
                  <p class="description">{item.description}</p>
                  <p class="how"><ShieldCheck size={13} aria-hidden="true" />{item.how}</p>
                  {#if item.roles.length}
                    <p class="muted small">{t("convRoles", { roles: item.roles.join(", ") })}</p>
                  {/if}
                  {#if item.kind === "hook"}
                    <p class="muted small">{t("convHookOn", { event: item.event, matcher: item.matcher || "*" })}</p>
                  {/if}

                  {#if item.kind === "repo"}
                    <p class="file">
                      {item.repo}/{item.path}
                      {#if item.before === undefined}<span class="muted">· {t("convNewFile")}</span>{/if}
                    </p>
                    {@render diffView(item.before ?? "", item.content)}
                  {:else if executable(item)}
                    {#if item.body.trim()}<p class="body">{item.body}</p>{/if}
                    <p class="file">{item.kind === "hook" ? "hook.sh" : "check.sh"}</p>
                    <pre class="code">{item.script}</pre>
                  {:else}
                    <details>
                      <summary>{t("convShowContent")}</summary>
                      <pre class="code">{item.body}</pre>
                    </details>
                  {/if}
                  {#if change?.change === "changed" && change.previous}
                    <details>
                      <summary>{t("convShowChanges")}</summary>
                      {@render diffView(content(change.previous), content(item))}
                    </details>
                  {/if}

                  {#if checked(item)}
                    <div class="level-row">
                      <span class="muted small">{t("convLevel")}</span>
                      <div class="segmented" role="radiogroup" aria-label={t("convLevel")}>
                        {#each levels as [value, label] (value)}
                          <button
                            role="radio"
                            aria-checked={item.level === value}
                            onclick={() => decide(item.name, null, value)}
                          >
                            {t(label)}
                          </button>
                        {/each}
                      </div>
                    </div>
                    {@render impactLine(item)}
                  {/if}

                  <footer>
                    <button
                      class="btn sm"
                      aria-pressed={item.review === "accepted"}
                      onclick={() => decide(item.name, item.review === "accepted" ? "pending" : "accepted")}
                    >
                      <Check size={14} aria-hidden="true" />{t("convAccept")}
                    </button>
                    <button
                      class="btn sm"
                      aria-pressed={item.review === "rejected"}
                      onclick={() => decide(item.name, item.review === "rejected" ? "pending" : "rejected")}
                    >
                      <X size={14} aria-hidden="true" />{t("convReject")}
                    </button>
                    <button
                      class="btn sm quiet"
                      aria-expanded={comments[item.name] !== undefined}
                      onclick={() =>
                        comments[item.name] === undefined ? (comments[item.name] = "") : delete comments[item.name]}
                    >
                      <MessageSquareText size={14} aria-hidden="true" />{t("convRequestChange")}
                    </button>
                  </footer>
                  {#if comments[item.name] !== undefined}
                    <div class="comment">
                      <textarea
                        class="field"
                        rows="2"
                        bind:value={comments[item.name]}
                        placeholder={t("convCommentPlaceholder")}
                        aria-label={t("convRequestChange")}
                        {@attach (el) => el.focus()}
                      ></textarea>
                      <button
                        class="btn sm primary"
                        onclick={() => revise(item.name)}
                        disabled={!comments[item.name]?.trim() || !!running}
                      >
                        {#if running === `revise:${item.name}`}
                          <LoaderCircle size={14} aria-hidden="true" class="spin" />
                        {/if}
                        {t("convRegenerateItem")}
                      </button>
                    </div>
                  {/if}
                </article>
              {/each}
            </div>
          {/each}

          {#if removed.length}
            <div class="removed">
              <h4>{t("convRemoved")}</h4>
              <ul>
                {#each removed as d (d.name)}
                  <li>
                    {#if d.previous}{@render kindBadge(d.previous)}{/if}
                    <span class="item-name">{d.name}</span>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}

          {#if draft.dropped.length}
            <details class="dropped">
              <summary>{t("convDropped", { count: draft.dropped.length })}</summary>
              <ul>
                {#each draft.dropped as why, i (i)}<li>{why}</li>{/each}
              </ul>
            </details>
          {/if}

          <footer class="actions">
            <button class="btn" onclick={measure} disabled={!!running || !draft.items.some((i) => checked(i) && i.review === "accepted")}>
              {#if running === "impact"}
                <LoaderCircle size={14} aria-hidden="true" class="spin" />
              {:else}
                <Gauge size={14} aria-hidden="true" />
              {/if}
              {t("convMeasure")}
            </button>
            <span class="muted small grow">{t("convMeasureHelp")}</span>
            <button class="btn quiet" onclick={discard} disabled={!!running}>
              {t("cancel")}
            </button>
            {#if toTeam}
              <button
                class="btn"
                onclick={handOff}
                disabled={!!running || pending > 0}
                title={t("watchTeamConvHelp", { count: toTeam })}
              >
                {#if running === "team"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Users size={14} aria-hidden="true" />{/if}
                {t("watchTeam")}
              </button>
            {/if}
            <button
              class="btn primary"
              onclick={() => update("conventions_apply")}
              disabled={!!running || pending > 0 || !draft.items.length}
              title={pending ? t("convDecideFirst") : undefined}
            >
              {t("convApply", { count: accepted })}
            </button>
          </footer>
          {#if pending}
            <p class="muted small right">{t("convDecideFirst")}</p>
          {/if}
        </section>
      {/if}

      {#if pack?.items.length}
        <section class="step">
          <div class="step-head">
            <h3>{t("convApplied")}</h3>
            {#if !draft}
              <button class="btn sm" onclick={measure} disabled={!!running || !pack.items.some(checked)}>
                {#if running === "impact"}
                  <LoaderCircle size={14} aria-hidden="true" class="spin" />
                {:else}
                  <Gauge size={14} aria-hidden="true" />
                {/if}
                {t("convMeasure")}
              </button>
            {/if}
          </div>
          <p class="muted lead">{t("convAppliedHelp")}</p>
          <ul class="applied">
            {#each groups(pack.items) as [sentence, items] (sentence)}
              {@const switchable = items.filter((i) => i.kind !== "repo")}
              {@const on = switchable.length > 0 && switchable.every((i) => current.active.includes(i.name))}
              {@const rules = items.filter(checked)}
              <li class="convention" class:off={switchable.length && !on}>
                {#if switchable.length}
                  <input
                    class="switch"
                    type="checkbox"
                    role="switch"
                    checked={on}
                    disabled={!!running}
                    aria-label={sentence}
                    onchange={async (e) => {
                      const input = e.currentTarget;
                      const done = await update("conventions_switch", { sentence, enabled: input.checked, level: null });
                      // A refused switch goes back to the state on disk.
                      if (!done) input.checked = on;
                    }}
                  />
                {:else}
                  <span class="switch-gap"></span>
                {/if}
                <div class="convention-main">
                  <p class="sentence">{t("quoted", { text: sentence })}</p>
                  <div class="chips">
                    {#each items as item (item.name)}
                      {@const Icon = kindInfo[item.kind].icon}
                      <span class="badge" title={item.description}>
                        <Icon size={12} aria-hidden="true" />{item.name}
                      </span>
                    {/each}
                  </div>
                  {#each rules as rule (rule.name)}
                    {@render impactLine(rule)}
                  {/each}
                </div>
                {#if rules.length}
                  <div class="segmented" role="radiogroup" aria-label={t("convLevel")}>
                    {#each levels as [value, label] (value)}
                      <button
                        role="radio"
                        aria-checked={rules[0].level === value}
                        disabled={!!running}
                        onclick={() => update("conventions_switch", { sentence, enabled: null, level: value })}
                      >
                        {t(label)}
                      </button>
                    {/each}
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}
  </section>
</div>

<style>
  .conventions {
    display: grid;
    flex: 1;
    grid-template-columns: 240px minmax(0, 1fr);
    min-height: 0;
  }

  .only {
    grid-template-columns: minmax(0, 1fr);
  }

  .only .docs,
  .only .doc-head {
    display: none;
  }

  .only .doc-pane {
    padding: 0;
    overflow: visible;
  }

  .only .model {
    margin: 0;
  }

  .docs {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
    padding: 10px 6px;
    overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--chrome);
  }

  .docs ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .doc {
    display: flex;
    flex-direction: column;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    text-align: left;
  }

  .doc:hover {
    background: var(--hover);
  }

  .doc[aria-current="true"] {
    background: var(--selected);
  }

  .doc-name {
    font-weight: 500;
  }

  .doc-status {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .add {
    align-self: flex-start;
    margin-left: 4px;
  }

  .new {
    display: flex;
    gap: 8px;
    margin-top: 20px;
  }

  .new .field {
    flex: 1;
  }

  .row-end {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .doc-pane {
    min-width: 0;
    min-height: 0;
    padding: 20px 28px 40px;
    overflow-y: auto;
  }

  .doc-pane > * {
    max-width: 860px;
  }

  .intro {
    max-width: 520px;
    margin: 48px auto 0;
    color: var(--muted);
    text-align: center;
  }

  .intro :global(svg) {
    color: var(--accent-ink);
  }

  .intro h2 {
    margin: 8px 0 6px;
    color: var(--text);
  }

  .deduce {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-top: 24px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
    text-align: left;
  }

  .deduce h3 {
    color: var(--text);
  }

  .deduce p {
    margin: 4px 0 0;
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .deduce .model {
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .deduce .btn {
    flex-shrink: 0;
  }

  .intro .deduce :global(svg) {
    color: inherit;
  }

  .or {
    margin: 20px 0 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .or + .new {
    margin-top: 8px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h4 {
    margin: 0 0 6px;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .doc-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 8px;
  }

  .model {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  .model :global(svg) {
    color: var(--accent-ink);
  }

  .hint {
    color: var(--muted);
  }

  .confirm {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
    max-width: 420px;
    font-size: var(--text-sm);
  }

  .step {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 0;
    border-top: 1px solid var(--border);
  }

  .doc-head + .step {
    border-top: 0;
  }

  .step-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    font-size: var(--text-sm);
  }

  .lead {
    margin: -4px 0 0;
    font-size: var(--text-sm);
  }

  .text {
    height: auto;
    min-height: 150px;
    padding: 10px 12px;
    line-height: 1.6;
    resize: vertical;
  }

  textarea.field {
    height: auto;
    padding: 8px 10px;
    resize: vertical;
  }

  .saved {
    margin-right: auto;
    font-size: var(--text-sm);
  }

  .working {
    align-items: center;
    background: var(--working-soft);
    color: var(--working);
  }

  .feed {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 8px 0 0;
    padding: 0 0 0 10px;
    color: var(--muted);
    font-size: var(--text-sm);
    list-style: none;
  }

  .feed li {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .question {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-weight: 500;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 6px;
  }

  .sentence {
    margin: 0;
    color: var(--text);
    font-weight: 600;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
  }

  .card.rejected {
    background: none;
    box-shadow: none;
    color: var(--muted);
  }

  .card header {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }

  .item-name {
    font: 600 var(--text-sm) var(--font-mono);
  }

  .kind {
    color: var(--text);
  }

  .flag {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .change-added {
    border-color: transparent;
    background: var(--done-soft);
    color: var(--done);
  }

  .change-changed {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .decision {
    display: inline-flex;
    margin-left: auto;
  }

  .description,
  .how,
  .body {
    margin: 0;
  }

  .how {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .how :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
  }

  .body {
    font-size: var(--text-sm);
    white-space: pre-wrap;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .file {
    margin: 4px 0 0;
    font: var(--text-xs) var(--font-mono);
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

  details summary {
    color: var(--accent-ink);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  details[open] summary {
    margin-bottom: 6px;
  }

  .level-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .impact {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .impact :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
    color: var(--muted);
  }

  .impact ul {
    flex: 1;
    min-width: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .repo {
    margin-right: 6px;
    font-weight: 500;
  }

  .found {
    color: var(--warning);
    font-weight: 600;
  }

  .clean {
    color: var(--done);
    font-weight: 600;
  }

  .failed {
    color: var(--danger);
  }

  .findings {
    margin: 4px 0 2px;
    color: var(--muted);
    font: var(--text-xs) / 1.5 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .card footer {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }

  .card footer .btn[aria-pressed="true"] {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .comment {
    display: flex;
    align-items: flex-end;
    gap: 8px;
  }

  .comment textarea {
    flex: 1;
  }

  .removed ul,
  .dropped ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .removed li {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
    text-decoration: line-through;
  }

  .dropped ul {
    padding-left: 4px;
    color: var(--muted);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .grow {
    flex: 1;
  }

  .right {
    text-align: right;
  }

  .applied {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .convention {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 0;
    border-bottom: 1px solid var(--border);
  }

  .convention .switch {
    margin-top: 2px;
  }

  .switch-gap {
    width: 28px;
    flex-shrink: 0;
  }

  .convention.off .convention-main {
    color: var(--muted);
  }

  .convention-main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chips .badge {
    font-family: var(--font-mono);
  }
</style>
