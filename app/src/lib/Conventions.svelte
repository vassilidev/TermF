<script lang="ts">
  import { onMount, type Component } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import FileCog from "@lucide/svelte/icons/file-cog";
  import Gauge from "@lucide/svelte/icons/gauge";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import Plus from "@lucide/svelte/icons/plus";
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
  import type { Project } from "$lib/state.svelte";

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
  type Impact = { name: string; repo: string } & (
    | { status: "done"; count: number; findings: string[] }
    | { status: "unsupported" }
    | { status: "failed"; message: string }
  );

  let { project }: { project: Project } = $props();

  let model = $state("");
  let effort = $state("");
  let list = $state<Convention[]>([]);
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

  const current = $derived(list.find((c) => c.slug === slug));
  const draft = $derived(current?.draft ?? null);
  const pack = $derived(current?.pack ?? null);
  const dirty = $derived(!!current && text !== current.source);
  const pending = $derived(draft?.items.filter((i) => i.review === "pending").length ?? 0);
  const accepted = $derived(draft?.items.filter((i) => i.review === "accepted").length ?? 0);

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
      const data = await invoke<{ model: string; effort: string; list: Convention[] }>("conventions_list", {
        project: project.id,
      });
      ({ model, effort, list } = data);
      select(list[0]?.slug ?? "");
      creating = !list.length;
    } catch (e) {
      error = String(e);
    }
  });

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
            {#if r.status === "done"}
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

<div class="conventions">
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

    {#if creating || !current}
      <div class="intro">
        <Sparkles size={20} aria-hidden="true" />
        <h2>{t("convIntroTitle")}</h2>
        <p>{t("convIntro")}</p>
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
            {t("convModel", { model, effort })}
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
          <button class="btn sm quiet danger-text" onclick={() => (confirmDelete = true)}>{t("delete")}</button>
        {/if}
      </header>

      <section class="step">
        <h3>{t("convWrite")}</h3>
        <textarea
          class="field text"
          bind:value={text}
          rows="7"
          placeholder={t("convPlaceholder")}
          aria-label={t("convWrite")}
          disabled={running === "propose"}
        ></textarea>
        <div class="row-end">
          <span class="muted saved">{dirty ? t("convUnsaved") : ""}</span>
          <button class="btn" onclick={save} disabled={!dirty || !!running}>{t("save")}</button>
          <button class="btn primary" onclick={() => propose()} disabled={!text.trim() || !!running}>
            <Sparkles size={14} aria-hidden="true" />{pack || draft ? t("convRegenerate") : t("convPropose")}
          </button>
        </div>
        {#if running === "propose"}
          <p class="notice working" role="status">
            <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("convRunning", { model })}
          </p>
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
              {#if draft.cost !== undefined}· {t("convCost", { cost: money.format(draft.cost) })}{/if}
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
              <p class="sentence">« {sentence} »</p>
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
                      <span class="decision {item.review}">
                        {item.review === "accepted" ? t("convAccepted") : t("convRejected")}
                      </span>
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
            <button class="btn quiet" onclick={() => update("conventions_discard")} disabled={!!running}>
              {t("convDiscard")}
            </button>
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
                    onchange={(e) =>
                      update("conventions_switch", { sentence, enabled: e.currentTarget.checked, level: null })}
                  />
                {:else}
                  <span class="switch-gap"></span>
                {/if}
                <div class="convention-main">
                  <p class="sentence">« {sentence} »</p>
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

  h2 {
    margin: 0;
    font-size: 17px;
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

  .danger-text {
    color: var(--danger);
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

  :global(.spin) {
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(.spin) {
      animation: none;
    }
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
    border-left: 3px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
  }

  .card.accepted {
    border-left-color: var(--done);
  }

  .card.rejected {
    border-left-color: var(--border);
    opacity: 0.65;
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
    margin-left: auto;
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .decision.accepted {
    color: var(--done);
  }

  .decision.rejected {
    color: var(--muted);
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
    opacity: 0.6;
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
