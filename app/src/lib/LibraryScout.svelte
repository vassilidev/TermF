<script lang="ts">
  import { approxUsd } from "$lib/usage.svelte";
  import { onDestroy, type Component } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import BookOpen from "@lucide/svelte/icons/book-open";
  import Check from "@lucide/svelte/icons/check";
  import Globe from "@lucide/svelte/icons/globe";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import Search from "@lucide/svelte/icons/search";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import SlidersHorizontal from "@lucide/svelte/icons/sliders-horizontal";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import SquareSlash from "@lucide/svelte/icons/square-slash";
  import ShieldCheck from "@lucide/svelte/icons/shield-check";
  import Telescope from "@lucide/svelte/icons/telescope";
  import Terminal from "@lucide/svelte/icons/terminal";
  import UserCog from "@lucide/svelte/icons/user-cog";
  import WandSparkles from "@lucide/svelte/icons/wand-sparkles";
  import X from "@lucide/svelte/icons/x";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { app, shortPath } from "$lib/state.svelte";
  import { library, lineDiff, matches, type Item, type Kind, type Layer } from "$lib/library.svelte";
  import {
    applyScout,
    decide,
    discardScout,
    loadScout,
    reviseEntry,
    runScout,
    scout,
    type Entry,
    type Flaw,
    type ItemRef,
  } from "$lib/scout.svelte";

  type Key = Parameters<typeof t>[0];

  const project = $derived(app.workspace?.projects.find((p) => p.id === library.scope));
  let items = $state<Item[]>([]);
  let mode = $state<"all" | "chosen">("all");
  let chosen = $state<string[]>([]);
  let query = $state("");
  let comments = $state<Record<string, string>>({});
  let confirmed = $state<Record<string, boolean>>({});
  let now = $state(Date.now());

  const kindInfo: Partial<Record<Kind, { icon: Component; label: Key }>> = {
    role: { icon: UserCog, label: "kindRole" },
    preference: { icon: SlidersHorizontal, label: "kindPreference" },
    rule: { icon: ShieldCheck, label: "kindRule" },
    skill: { icon: WandSparkles, label: "kindSkill" },
    command: { icon: SquareSlash, label: "kindCommand" },
  };
  const layerLabel: Record<Layer, Key> = {
    shipped: "layerShipped",
    package: "stackLayer",
    global: "layerGlobal",
    project: "layerProject",
    repo: "layerRepo",
    run: "layerRun",
  };
  const flawLabel: Record<Flaw, Key> = {
    vague: "scoutFlawVague",
    contradiction: "scoutFlawContradiction",
    redundancy: "scoutFlawRedundancy",
    outdated: "scoutFlawOutdated",
    missing: "scoutFlawMissing",
  };
  const actionLabel = { applied: "scoutApplied", noted: "scoutNoted", rejected: "scoutRejected" } as const;
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" });

  const key = (i: ItemRef) => `${i.kind}/${i.name}`;
  /** A confirmation holds for this exact content only, not for a later revision of it. */
  const confirmKey = (e: Entry) => `${key(e)}\n${e.content}\n${e.script}`;
  const ref = (e: Entry): ItemRef => ({ kind: e.kind, name: e.name });
  const proposes = (e: Entry) => !!e.content || !!e.script;

  const examinable = $derived(items.filter((i) => i.kind in kindInfo));
  const listed = $derived(examinable.filter((i) => matches(i, query)));
  const data = $derived(scout.scope === library.scope ? scout.data : null);
  const draft = $derived(data?.draft ?? null);
  const pending = $derived(draft?.entries.filter((e) => e.review === "pending").length ?? 0);
  const decided = $derived((draft?.entries.length ?? 0) - pending);
  /** The slow call of this scope, if any. */
  const running = $derived(scout.runScope === library.scope ? scout.running : "");

  $effect(() => {
    const scope = library.scope;
    void library.version;
    loadScout();
    invoke<Item[]>("library_list", { project: scope || null })
      .then((list) => scope === library.scope && (items = list))
      .catch((e) => (scout.error = String(e)));
  });

  const timer = setInterval(() => (now = Date.now()), 1000);
  onDestroy(() => clearInterval(timer));

  function elapsed(): string {
    const s = Math.max(0, Math.round((now - scout.started) / 1000));
    return t("scoutElapsed", { min: Math.floor(s / 60), s: String(s % 60).padStart(2, "0") });
  }

  function launch() {
    const refs =
      mode === "all" ? [] : examinable.filter((i) => chosen.includes(key(i))).map((i) => ({ kind: i.kind, name: i.name }));
    runScout(refs);
  }

  function pick(item: Item, on: boolean) {
    chosen = on ? [...chosen, key(item)] : chosen.filter((k) => k !== key(item));
  }

  async function revise(e: Entry) {
    if (await reviseEntry(ref(e), comments[key(e)] ?? "")) delete comments[key(e)];
  }

  function target(e: Entry): string {
    if (e.layer === "shipped") return t("scoutToOverride");
    if (e.layer === "global") return t("scoutToGlobal");
    if (e.layer === "project" || project) return t("scoutToProject", { project: project?.name ?? "" });
    return t("scoutToGlobal");
  }

  function host(url: string): string {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }

  function close() {
    library.panel = null;
  }
</script>

{#snippet diffView(before: string, after: string)}
  <pre class="code diff">{#each lineDiff(before, after) as line, i (i)}<span class="l{line.op === '+' ? ' add' : line.op === '-' ? ' del' : ''}">{line.op} {line.text}
</span>{/each}</pre>
{/snippet}

{#snippet kindBadge(kind: Kind)}
  {@const info = kindInfo[kind]}
  {#if info}<span class="badge kind"><info.icon size={12} aria-hidden="true" />{t(info.label)}</span>{/if}
{/snippet}

{#snippet working()}
  <div class="working" role="status">
    <p><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("scoutRunning", { model: data?.model ?? "", elapsed: elapsed() })}</p>
    {#if scout.progress.length}
      <ol class="feed">
        {#each scout.progress.slice(-6) as p, i (i)}
          <li>
            {#if p.kind === "search"}
              <Search size={12} aria-hidden="true" /><span class="what">{t("scoutSearch")}</span><span>{p.text}</span>
            {:else}
              <BookOpen size={12} aria-hidden="true" /><span class="what">{t("scoutFetch")}</span><span class="mono">{p.text}</span>
            {/if}
          </li>
        {/each}
      </ol>
    {:else}
      <p class="muted small">{t("scoutStarting")}</p>
    {/if}
  </div>
{/snippet}

{#snippet sourceLine(url: string)}
  <button class="source" title={url} onclick={() => invoke("open_url", { url }).catch((e) => (scout.error = String(e)))}>
    <Globe size={12} aria-hidden="true" /><span class="host">{host(url)}</span><span class="url">{url}</span>
  </button>
{/snippet}

<section class="scout">
  <header class="head">
    <div>
      <h2><Telescope size={18} aria-hidden="true" />{t("scoutTitle")}</h2>
      <p class="model">
        <Sparkles size={13} aria-hidden="true" />
        {t("scoutModel", { model: data?.model ?? "…", budget: money.format(data?.budget ?? 0) })}
        <span class="muted">{t("scoutModelHint")}</span>
      </p>
    </div>
    <button class="icon-btn" onclick={close} aria-label={t("close")} title={t("close")}>
      <X size={16} aria-hidden="true" />
    </button>
  </header>
  <p class="lead">{t("scoutLead")}</p>

  {#if scout.error}
    <p class="notice error" role="alert">{scout.error}</p>
  {/if}

  <section class="step">
    <div class="step-head">
      <h3>{t("scoutWhat")}</h3>
      <div class="segmented" role="radiogroup" aria-label={t("scoutWhat")}>
        <button role="radio" aria-checked={mode === "all"} onclick={() => (mode = "all")}>
          {t("scoutAll", { count: examinable.length })}
        </button>
        <button role="radio" aria-checked={mode === "chosen"} onclick={() => (mode = "chosen")}>
          {t("scoutChosen", { count: chosen.length })}
        </button>
      </div>
    </div>
    {#if mode === "chosen"}
      <div class="picker">
        <div class="filter">
          <Search size={14} aria-hidden="true" />
          <input class="field" type="search" placeholder={t("scoutFilter")} aria-label={t("scoutFilter")} bind:value={query} />
        </div>
        <ul>
          {#each listed as item (key(item))}
            <li>
              <label>
                <input
                  type="checkbox"
                  checked={chosen.includes(key(item))}
                  onchange={(e) => pick(item, e.currentTarget.checked)}
                />
                {@render kindBadge(item.kind)}
                <span class="item-name">{item.name}</span>
                <span class="badge">{t(layerLabel[item.layer])}</span>
                {#if item.description}<span class="desc">{item.description}</span>{/if}
              </label>
            </li>
          {:else}
            <li class="muted none">{t("filesNoResults")}</li>
          {/each}
        </ul>
      </div>
    {/if}
    <div class="row-end">
      {#if draft?.entries.length}<span class="muted small grow">{t("scoutRerunHint")}</span>{/if}
      <button
        class="btn primary"
        onclick={launch}
        disabled={!!scout.running || !examinable.length || (mode === "chosen" && !chosen.length)}
      >
        <Telescope size={14} aria-hidden="true" />{t("scoutRun")}
      </button>
    </div>
    {#if running === "run"}
      {@render working()}
    {:else if scout.running && !running}
      <p class="muted small right">{t("scoutRunningElsewhere")}</p>
    {/if}
  </section>

  {#if draft}
    <section class="step">
      <div class="step-head">
        <h3>{t("scoutResults")}</h3>
        <span class="muted small">
          {t("scoutResultsMeta", { when: ago(draft.at), pending, total: draft.entries.length })}
          {#if draft.cost !== undefined}· {t("scoutCost", { cost: approxUsd(money.format(draft.cost)) })}{/if}
        </span>
      </div>
      <p class="notice untrusted"><ShieldAlert size={14} aria-hidden="true" />{t("scoutUntrusted")}</p>
      {#if !draft.entries.length}
        <p class="muted">{t("scoutNothing")}</p>
      {/if}

      {#each draft.entries as e (key(e))}
        {@const k = key(e)}
        {@const ck = confirmKey(e)}
        {@const needsConfirm = e.executable && e.review !== "accepted" && !confirmed[ck]}
        <article class="card {e.review}">
          <header>
            {@render kindBadge(e.kind)}
            <span class="item-name">{e.name}</span>
            {#if e.layer}
              <span class="badge layer-{e.layer}">{t(layerLabel[e.layer])}</span>
            {:else}
              <span class="badge new">{t("scoutNew")}</span>
            {/if}
            {#if e.executable}
              <span class="badge flag"><Terminal size={12} aria-hidden="true" />{t("scoutExecutable")}</span>
            {/if}
            {#if e.review !== "pending"}
              <span class="decision"><StatusPill status={e.review} /></span>
            {/if}
          </header>

          {#if e.critique.length}
            <ul class="critique">
              {#each e.critique as f, i (i)}
                <li>
                  <span class="flaw flaw-{f.flaw}">{t(flawLabel[f.flaw])}</span>
                  <div>
                    <p>{f.text}</p>
                    <p class="practice">{t("scoutPractice", { practice: f.practice })}</p>
                    {@render sourceLine(f.source)}
                  </div>
                </li>
              {/each}
            </ul>
          {/if}

          {#if proposes(e)}
            <div class="proposal">
              <p class="summary"><Sparkles size={13} aria-hidden="true" />{e.summary}</p>
              <p class="muted small">{target(e)}</p>
              <div class="sources">
                <span class="muted small">{t("scoutSources")}</span>
                {#each e.sources as url, i (i)}{@render sourceLine(url)}{/each}
              </div>
              {#if e.content}
                {#if e.layer && !e.executable}
                  <details open>
                    <summary>{t("convShowChanges")}</summary>
                    {@render diffView(e.before ?? "", e.content)}
                  </details>
                {:else}
                  <p class="muted small">{e.layer ? t("scoutWholeContent") : t("scoutNewContent")}</p>
                  <pre class="code">{e.content}</pre>
                  {#if e.layer}
                    <details>
                      <summary>{t("convShowChanges")}</summary>
                      {@render diffView(e.before ?? "", e.content)}
                    </details>
                  {/if}
                {/if}
              {/if}
              {#if e.script}
                <p class="file">{e.scriptFile}</p>
                <pre class="code">{e.script}</pre>
                {#if e.beforeScript !== undefined}
                  <details>
                    <summary>{t("convShowChanges")}</summary>
                    {@render diffView(e.beforeScript, e.script)}
                  </details>
                {/if}
              {/if}
            </div>
          {:else}
            <p class="muted small">{t("scoutNoProposal")}</p>
          {/if}

          {#if e.executable && e.review !== "accepted"}
            <label class="confirm">
              <input type="checkbox" bind:checked={confirmed[ck]} />
              {t("scoutConfirm")}
            </label>
          {/if}

          <footer>
            <button
              class="btn sm"
              aria-pressed={e.review === "accepted"}
              disabled={!!scout.running || needsConfirm}
              onclick={() => decide(ref(e), e.review === "accepted" ? "pending" : "accepted", !!confirmed[ck])}
            >
              <Check size={14} aria-hidden="true" />{t("convAccept")}
            </button>
            <button
              class="btn sm"
              aria-pressed={e.review === "rejected"}
              disabled={!!scout.running}
              onclick={() => decide(ref(e), e.review === "rejected" ? "pending" : "rejected")}
            >
              <X size={14} aria-hidden="true" />{t("convReject")}
            </button>
            <button
              class="btn sm quiet"
              aria-expanded={comments[k] !== undefined}
              onclick={() => (comments[k] === undefined ? (comments[k] = "") : delete comments[k])}
            >
              <MessageSquareText size={14} aria-hidden="true" />{t("convRequestChange")}
            </button>
          </footer>
          {#if comments[k] !== undefined}
            <div class="comment">
              <textarea
                class="field"
                rows="2"
                bind:value={comments[k]}
                placeholder={t("convCommentPlaceholder")}
                aria-label={t("convRequestChange")}
              ></textarea>
              <button class="btn sm primary" onclick={() => revise(e)} disabled={!comments[k]?.trim() || !!scout.running}>
                {t("scoutReviseItem")}
              </button>
            </div>
          {/if}
          {#if running === `revise:${k}`}
            {@render working()}
          {/if}
        </article>
      {/each}

      {#if draft.dropped.length}
        <details class="dropped">
          <summary>{t("scoutDropped", { count: draft.dropped.length })}</summary>
          <ul>
            {#each draft.dropped as why, i (i)}<li>{why}</li>{/each}
          </ul>
        </details>
      {/if}

      <footer class="actions">
        <button class="btn quiet" onclick={discardScout} disabled={!!scout.running}>{t("scoutDiscard")}</button>
        <span class="muted small grow">{draft.entries.length ? t("scoutApplyHelp") : ""}</span>
        <button class="btn primary" onclick={applyScout} disabled={!!scout.running || !decided}>
          {t("scoutApply", { count: decided })}
        </button>
      </footer>
    </section>
  {/if}

  {#if data?.journal.length}
    <details class="journal">
      <summary>{t("scoutJournal", { count: data.journal.length })}</summary>
      <ul>
        {#each data.journal as l, i (i)}
          <li>
            <span class="muted">{ago(l.at)}</span>
            <span class="action {l.action}">{t(actionLabel[l.action])}</span>
            {@render kindBadge(l.kind)}
            <span class="item-name">{l.name}</span>
            {#if l.path}<span class="mono muted">{shortPath(l.path)}</span>{/if}
          </li>
        {/each}
      </ul>
    </details>
  {/if}
</section>

<style>
  .scout {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 40px;
    overflow-y: auto;
  }

  .scout > * {
    max-width: 900px;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h2 :global(svg),
  .model :global(svg),
  .summary :global(svg) {
    color: var(--accent-ink);
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .model {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  .lead {
    margin: 10px 0 0;
    color: var(--muted);
    line-height: 1.55;
  }

  .scout > .notice {
    margin-top: 12px;
  }

  .step {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 18px;
    padding-top: 18px;
    border-top: 1px solid var(--border);
  }

  .step-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }

  .grow {
    flex: 1;
  }

  .right {
    text-align: right;
  }

  .row-end {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .picker {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--chrome);
  }

  .filter {
    position: relative;
    padding: 8px;
    color: var(--muted);
  }

  .filter :global(svg) {
    position: absolute;
    top: 16px;
    left: 17px;
    pointer-events: none;
  }

  .filter .field {
    padding-left: 28px;
  }

  .picker ul {
    max-height: 260px;
    margin: 0;
    padding: 0 4px 6px;
    overflow-y: auto;
    list-style: none;
  }

  .picker label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius);
    cursor: pointer;
  }

  .picker label:hover {
    background: var(--hover);
  }

  .picker input {
    margin: 0;
  }

  .picker .item-name {
    flex-shrink: 0;
    max-width: 40%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .desc {
    overflow: hidden;
    color: var(--muted);
    font-size: var(--text-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .none {
    padding: 6px 8px;
  }

  .item-name {
    font: 600 var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .kind {
    color: var(--text);
  }

  .working {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--working-soft);
    color: var(--working);
    font-size: var(--text-sm);
  }

  .working > p {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-weight: 500;
  }

  .feed {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 0;
    padding: 0;
    color: var(--text);
    list-style: none;
  }

  .feed li {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .feed li > span:last-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .feed :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .what {
    flex-shrink: 0;
    width: 72px;
    color: var(--muted);
  }

  .untrusted {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
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

  .layer-project,
  .new {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .new {
    background: var(--done-soft);
    color: var(--done);
  }

  .flag {
    border-color: transparent;
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .decision {
    margin-left: auto;
  }

  .critique {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .critique li {
    display: grid;
    grid-template-columns: 104px minmax(0, 1fr);
    gap: 10px;
  }

  .critique p {
    margin: 0;
  }

  .practice {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .flaw {
    align-self: start;
    justify-self: start;
    padding: 1px 7px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--text);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .flaw-contradiction,
  .flaw-outdated {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .flaw-missing {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .source {
    display: flex;
    align-items: center;
    gap: 5px;
    max-width: 100%;
    min-width: 0;
    margin-top: 2px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--muted);
    font-size: var(--text-xs);
    text-align: left;
    cursor: pointer;
  }

  .source:hover .host {
    text-decoration: underline;
  }

  .source :global(svg) {
    flex-shrink: 0;
  }

  .host {
    flex-shrink: 0;
    color: var(--accent-ink);
    font-weight: 500;
  }

  .url {
    overflow: hidden;
    font-family: var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .proposal {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .critique + .proposal {
    padding-top: 8px;
    border-top: 1px dashed var(--border);
  }

  .summary {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin: 0;
    font-weight: 500;
  }

  .summary :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
  }

  .sources {
    display: flex;
    flex-direction: column;
    gap: 2px;
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
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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

  .confirm {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--waiting-soft);
    color: var(--warning);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .confirm input {
    margin: 0;
  }

  .card footer {
    display: flex;
    gap: 6px;
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
    height: auto;
    padding: 8px 10px;
    resize: vertical;
  }

  .dropped ul,
  .journal ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0 0 0 4px;
    color: var(--muted);
    font-size: var(--text-sm);
    list-style: none;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-top: 14px;
    border-top: 1px solid var(--border);
  }

  .journal {
    margin-top: 18px;
  }

  .journal li {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }

  .action {
    font-weight: 600;
  }

  .action.applied {
    color: var(--done);
  }
</style>
