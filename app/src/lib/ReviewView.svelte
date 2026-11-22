<script module lang="ts">
  import type { Mode } from "$lib/visual-logic";

  /** By review, the version and the comparison shown; by version, the layout and the screen: kept when the page goes. */
  const pages = $state<Record<number, { version?: number; mode: Mode }>>({});
  const layouts = $state<Record<string, { layout: "swipe" | "detail"; screen?: string }>>({});
</script>

<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MapPin from "@lucide/svelte/icons/map-pin";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import X from "@lucide/svelte/icons/x";
  import ReviewContext from "$lib/ReviewContext.svelte";
  import ReviewStage from "$lib/ReviewStage.svelte";
  import ReviewSwipe from "$lib/ReviewSwipe.svelte";
  import { ticketKey } from "$lib/TeamRunDetail.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import StatusPill, { type Status } from "$lib/ui/StatusPill.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { closeReview } from "$lib/commands";
  import { t } from "$lib/i18n";
  import { decide, decisionLabel, outcomeText, screenMeta, screenTitle, upsertReview, visual } from "$lib/visual.svelte";
  import { pending, serial, settles, swipeFirst, type Decision, type Pin, type Review, type Screen } from "$lib/visual-logic";

  /** A visual review, one version at a time (ARCHITECTURE §8.2): the quick mode when several screens wait, else screen by screen. */
  let { id }: { id: number } = $props();

  const review = $derived(visual.reviews.find((r) => r.id === id));
  const page = $derived(pages[id] ?? { mode: "side" as Mode });
  const latest = $derived(review?.versions.at(-1));
  const version = $derived(review?.versions.find((v) => v.n === page.version) ?? latest);
  const older = $derived(!!version && version !== latest);
  const vkey = $derived(version ? `${id}/${version.n}` : "");
  const layout = $derived(layouts[vkey]?.layout ?? (swipeFirst(version) ? "swipe" : "detail"));
  const waiting = $derived(version ? pending(version) : []);
  const screen = $derived<Screen | undefined>(
    version && (version.screens.find((s) => s.id === layouts[vkey]?.screen) ?? waiting[0] ?? version.screens.find((s) => s.changed) ?? version.screens[0]),
  );
  const decidable = $derived(version?.state === "ready" && !older);
  /** Every screen answered, and still ready: the team did not take the outcome (paused or stopped). */
  const unsent = $derived(decidable && !!version?.screens.length && !waiting.length);
  const progress = $derived(version?.state === "capturing" ? (visual.progress[id]?.version === version.n ? visual.progress[id] : version.progress) : null);

  // The layout holds once chosen: answering cards one by one must not switch to the detail under the user.
  $effect(() => {
    if (vkey && !layouts[vkey] && version?.state !== "capturing") layouts[vkey] = { layout: swipeFirst(version) ? "swipe" : "detail" };
  });

  function setPage(change: Partial<{ version: number; mode: Mode }>) {
    pages[id] = { ...page, ...change };
  }

  function setLayout(change: Partial<{ layout: "swipe" | "detail"; screen: string }>) {
    layouts[vkey] = { ...layouts[vkey], layout: change.layout ?? layout, ...(change.screen ? { screen: change.screen } : {}) };
  }

  let comment = $state("");
  let pins = $state<Pin[]>([]);
  let picked = $state<number | null>(null);
  let draftOf = "";
  $effect(() => {
    const k = `${vkey}/${screen?.id}`;
    if (!screen || k === draftOf) return;
    draftOf = k;
    comment = screen.comment;
    pins = $state.snapshot(screen.pins);
    picked = null;
  });

  let busy = $state("");
  let error = $state("");
  let needWhat = $state(false);
  let confirming = $state(false);
  let stage = $state<ReturnType<typeof ReviewStage>>();
  let commentField = $state<HTMLTextAreaElement>();

  const modes = $derived([
    { id: "side", label: t("review5bSide") },
    { id: "slider", label: t("review5bSlider") },
    { id: "overlay", label: t("review5bOverlay") },
    { id: "heatmap", label: t("review5bHeatmap") },
  ]);

  const pill = $derived.by((): { status: Status; label: string } | null => {
    if (!version) return null;
    if (version.state === "capturing") return { status: "working", label: t("review5bCapturingPill") };
    if (version.state === "failed") return { status: "failed", label: t("review5bFailedPill") };
    if (version.state === "ready") return { status: "waiting", label: t("review5bPending") };
    if (version.outcome === "approved") return { status: "accepted", label: t("review5bApproved") };
    if (version.outcome === "rejected") return { status: "rejected", label: t("review5bRejected") };
    return { status: "idle", label: t("review5bChanges") };
  });

  async function run(name: string, work: () => Promise<unknown>) {
    busy = name;
    error = "";
    try {
      await work();
      return true;
    } catch (e) {
      error = String(e);
      return false;
    } finally {
      busy = "";
    }
  }

  /** Every write of the page, one at a time: an older answer never lands after a newer one. */
  const write = serial();

  /** Pins and comment go with the review at once, whatever the screen's decision, in the background: the buttons stay. */
  function save() {
    if (!decidable || !screen) return;
    const [sid, text, list] = [screen.id, comment.trim(), $state.snapshot(pins)];
    // The decision as it is when this write goes: a decision sent meanwhile is kept.
    const now = () => version?.screens.find((x) => x.id === sid)?.decision ?? "pending";
    write(() => decide(id, sid, now(), text, list)).catch((e) => (error = String(e)));
  }

  function pin(x: number, y: number) {
    if (!decidable) return;
    pins.push({ x, y, comment: "" });
    picked = pins.length - 1;
    save();
    queueMicrotask(() => document.getElementById(`review-pin-${picked}`)?.focus());
  }

  function unpin(i: number) {
    pins.splice(i, 1);
    picked = null;
    save();
  }

  function choose(decision: Exclude<Decision, "pending">) {
    if (!screen || !version) return;
    needWhat = decision === "changes" && !comment.trim() && !pins.length;
    if (needWhat) return commentField?.focus();
    if (decision === "rejected" && review?.run !== null && settles(version, screen.id, decision) === "rejected") return void (confirming = true);
    send(decision);
  }

  async function send(decision: Decision) {
    const s = screen;
    if (!s) return;
    const [text, kept] = [comment.trim(), $state.snapshot(pins)];
    if (!(await run(decision, () => write(() => decide(id, s.id, decision, text, kept))))) return;
    if (decision === "pending" || !version) return;
    // On to the next screen still to decide.
    const list = version.screens;
    const at = list.findIndex((x) => x.id === s.id);
    const next = [...list.slice(at + 1), ...list.slice(0, at)].find((x) => x.decision === "pending");
    if (next) setLayout({ screen: next.id });
  }

  /** The last answer again: the engine settles the version and hands it to the team once more. */
  function resend() {
    const last = version?.screens.filter((s) => s.decided !== null).sort((a, b) => (b.decided ?? 0) - (a.decided ?? 0))[0];
    if (last) run("resend", () => write(() => decide(id, last.id, last.decision, last.comment, last.pins)));
  }

  function retry(r: Review) {
    const request = r.run !== null ? { run: r.run } : { repo: r.repoPath, base: r.base, head: r.head };
    run("retry", async () => upsertReview(await invoke<Review>("visual_start", { request })));
  }

  const phases = {
    prepare: "review5bPhasePrepare",
    before: "review5bPhaseBefore",
    after: "review5bPhaseAfter",
    diff: "review5bPhaseDiff",
    done: "review5bPhaseDiff",
  } as const;

  const changeOf = (s: Screen) =>
    s.error
      ? t("review5bScreenFailed")
      : !s.changed
        ? t("review5bSame")
        : s.rects.length
          ? s.rects.length === 1
            ? t("review5bZone")
            : t("review5bZones", { count: s.rects.length })
          : t("review5bTextsOnly");
</script>

<div class="review">
  <ViewHeader title={review?.title ?? t("review5bTitle")} context={review?.ticket ? ticketKey(review.ticket) : t("review5bTitle")} mono={!!review?.ticket}>
    {#snippet lead()}
      <button class="icon-btn back" onclick={closeReview} aria-label={t("ux3t2Back")} title={t("ux3t2Back")}>
        <ArrowLeft size={16} aria-hidden="true" />
      </button>
    {/snippet}
    {#if pill}<StatusPill status={pill.status} label={pill.label} />{/if}
    {#snippet actions()}
      {#if version && !older && (version.state === "ready" || version.state === "done") && version.screens.length}
        <div class="segmented" role="group" aria-label={t("review5bLayout")}>
          <button aria-pressed={layout === "swipe"} onclick={() => setLayout({ layout: "swipe" })}>{t("review5bSwipe")}</button>
          <button aria-pressed={layout === "detail"} onclick={() => setLayout({ layout: "detail" })}>{t("review5bDetail")}</button>
        </div>
      {/if}
      {#if review && review.versions.length > 1 && version}
        <select class="field versions" aria-label={t("review5bVersion")} value={version.n} onchange={(e) => setPage({ version: Number(e.currentTarget.value) })}>
          {#each [...review.versions].reverse() as v (v.n)}
            <option value={v.n}>v{v.n} · {v.state === "done" && v.outcome ? t(decisionLabel[v.outcome]) : t(v.state === "ready" ? "review5bPending" : v.state === "capturing" ? "review5bCapturingPill" : "review5bFailedPill")}</option>
          {/each}
        </select>
      {/if}
    {/snippet}
  </ViewHeader>

  {#if !review || !version}
    <EmptyState title={t("review5bNone")} />
  {:else}
    {#if error}<p class="notice error top" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    {#if older}
      <p class="notice top"><Info size={14} aria-hidden="true" />{t("review5bOlder", { n: version.n })}</p>
    {/if}
    {#if version.state === "done" && version.outcome && layout === "detail"}
      <p class="notice top outcome {version.outcome}"><Check size={14} aria-hidden="true" />{outcomeText(review, version)}</p>
    {/if}
    {#if version.state === "done" && version.error}
      <p class="notice warn top"><TriangleAlert size={14} aria-hidden="true" />{version.error}</p>
    {/if}
    {#if unsent}
      <p class="notice warn top">
        <TriangleAlert size={14} aria-hidden="true" /><span>{t("review5bUnsent")}</span>
        <button class="btn sm" onclick={resend} disabled={!!busy}>
          {#if busy === "resend"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("review5bResend")}
        </button>
      </p>
    {/if}

    {#if version.state === "capturing" && progress}
      <section class="state" aria-live="polite">
        <h2><LoaderCircle size={16} aria-hidden="true" class="spin" />{t("review5bCapturing", { n: version.n })}</h2>
        <p>{t(phases[progress.phase])}</p>
        {#if progress.total}
          <progress max={progress.total} value={progress.done}></progress>
          <p class="muted">{t("review5bProgress", { done: progress.done, total: progress.total })}</p>
        {/if}
      </section>
    {:else if version.state === "failed"}
      <section class="state">
        <h2><CircleAlert size={16} aria-hidden="true" />{t("review5bFailed", { n: version.n })}</h2>
        {#if version.error}<p class="mono error-text">{version.error}</p>{/if}
        {#if !older}
          <div>
            <button class="btn primary" onclick={() => retry(review)} disabled={!!busy}>
              {#if busy === "retry"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<RotateCcw size={14} aria-hidden="true" />{/if}
              {t("review5bRetry")}
            </button>
          </div>
        {/if}
      </section>
    {:else if !version.screens.length || !screen}
      <EmptyState title={t("review5bNoScreens")} />
    {:else if layout === "swipe" && !older}
      <ReviewSwipe {review} {version} ondetail={() => setLayout({ layout: "detail" })} />
    {:else}
      <div class="body">
        <nav class="screens" aria-labelledby="review-screens">
          <h3 id="review-screens">{t("review5bScreens", { count: version.screens.length })}</h3>
          <ul>
            {#each version.screens as s (s.id)}
              <li>
                <button class="pick" aria-current={s.id === screen.id ? "true" : undefined} onclick={() => setLayout({ screen: s.id })}>
                  <span class="dot {s.decision}" aria-hidden="true"></span>
                  <span class="name mono">{screenTitle(s)}</span>
                  <span class="meta">{screenMeta(s)}</span>
                  <span class="meta" class:changed={s.changed} class:issue={!!s.error}>{changeOf(s)}</span>
                  <span class="decision {s.decision}">{t(decisionLabel[s.decision])}</span>
                </button>
              </li>
            {/each}
          </ul>
          {#if version.notes.length}
            <div class="notes">
              <h3>{t("review5bNotes")}</h3>
              <ul>
                {#each version.notes as note, i (i)}<li>{note}</li>{/each}
              </ul>
            </div>
          {/if}
        </nav>

        <section class="viewer" aria-label={screenTitle(screen)}>
          <Tabs id="review-mode" label={t("review5bModes")} tabs={modes} selected={page.mode} onselect={(m) => setPage({ mode: m as Mode })}>
            <div class="stage">
              <ReviewStage bind:this={stage} {review} {version} {screen} mode={page.mode} {pins} {picked} onpin={decidable ? pin : undefined} />
            </div>
          </Tabs>
          {#if decidable}
            <div class="decide">
              <label class="labelled comment">
                <span>{t("review5bComment")}</span>
                <textarea
                  class="field"
                  rows="2"
                  placeholder={t("review5bCommentHint")}
                  bind:this={commentField}
                  bind:value={comment}
                  onblur={() => comment.trim() !== screen?.comment && save()}
                ></textarea>
                {#if needWhat}<span class="need" role="alert">{t("review5bNeedWhat")}</span>{/if}
              </label>
              <div class="buttons">
                <span class="where">{t("review5bLeft", { count: waiting.length })}</span>
                {#if screen.decision !== "pending"}
                  <button class="icon-btn" onclick={() => send("pending")} disabled={!!busy} aria-label={t("review5bRetake")} title={t("review5bRetake")}>
                    <Undo2 size={15} aria-hidden="true" />
                  </button>
                {/if}
                <button class="btn reject" aria-pressed={screen.decision === "rejected"} onclick={() => choose("rejected")} disabled={!!busy}>
                  {#if busy === "rejected"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<X size={14} aria-hidden="true" />{/if}
                  {t("teamRunsRefuse")}
                </button>
                <button class="btn" aria-pressed={screen.decision === "changes"} onclick={() => choose("changes")} disabled={!!busy}>
                  {#if busy === "changes"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<MessageSquareText size={14} aria-hidden="true" />{/if}
                  {t("teamRunsAskChanges")}
                </button>
                <button class="btn primary" aria-pressed={screen.decision === "approved"} onclick={() => choose("approved")} disabled={!!busy}>
                  {#if busy === "approved"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Check size={14} aria-hidden="true" />{/if}
                  {t("teamRunsValidate")}
                </button>
              </div>
            </div>
          {/if}
        </section>

        <aside class="side">
          {#if !decidable && screen.comment}
            <section class="pins">
              <h4>{t("review5bComment")}</h4>
              <p class="said">{t("review5bQuote", { text: screen.comment })}</p>
            </section>
          {/if}
          {#if decidable || pins.length}
          <section class="pins" aria-labelledby="review-pins">
            <h4 id="review-pins">{t("review5bPins", { count: pins.length })}</h4>
            {#if pins.length}
              <ol>
                {#each pins as p, i (i)}
                  <li class:picked={picked === i}>
                    <span class="n" aria-hidden="true">{i + 1}</span>
                    {#if decidable}
                      <input
                        id="review-pin-{i}"
                        class="field"
                        aria-label={t("review5bPinComment", { n: i + 1 })}
                        placeholder={t("review5bPinHint")}
                        bind:value={p.comment}
                        onfocus={() => (picked = i)}
                        onblur={save}
                      />
                      <button class="icon-btn sm" aria-label={t("review5bPinRemove", { n: i + 1 })} title={t("review5bPinRemove", { n: i + 1 })} onclick={() => unpin(i)}>
                        <X size={14} aria-hidden="true" />
                      </button>
                    {:else}
                      <span>{p.comment || t("review5bPinLabel", { n: i + 1 })}</span>
                    {/if}
                  </li>
                {/each}
              </ol>
            {:else if decidable}
              <p class="muted">{t("review5bPinsNone")}</p>
            {/if}
            {#if decidable}
              <button
                class="btn sm quiet add"
                onclick={() => {
                  const at = stage?.center();
                  if (at) pin(at.x, at.y);
                }}
              >
                <MapPin size={14} aria-hidden="true" />{t("review5bPinAdd")}
              </button>
            {/if}
          </section>
          {/if}
          <ReviewContext {review} {version} {screen} files />
        </aside>
      </div>
    {/if}
  {/if}
</div>

<ConfirmDialog bind:open={confirming} title={t("teamRunsRefuseTitle")} confirm={t("teamRunsRefuse")} danger onconfirm={() => send("rejected")}>
  <p>{t("teamRunsRefuseText")}</p>
</ConfirmDialog>

<style>
  .review {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg);
  }

  .versions {
    width: auto;
  }

  .top {
    margin: 12px 20px 0;
  }

  .notice.top span {
    flex: 1;
  }

  .outcome.approved {
    border-color: var(--done);
  }

  .state {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: min(640px, calc(100% - 40px));
    margin: 32px auto;
    padding: 20px 24px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .state h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
  }

  .state p {
    margin: 0;
    line-height: 1.5;
  }

  .state progress {
    width: 100%;
    accent-color: var(--accent);
  }

  .error-text {
    color: var(--danger);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .muted {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .body {
    display: grid;
    flex: 1;
    grid-template-columns: 240px minmax(0, 1fr) 300px;
    min-height: 0;
  }

  .screens,
  .side {
    min-height: 0;
    overflow-y: auto;
    background: var(--chrome);
  }

  .screens {
    padding: 12px 8px;
    border-right: 1px solid var(--border);
  }

  .side {
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 14px 16px;
    border-left: 1px solid var(--border);
  }

  h3 {
    margin: 0 8px 8px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .screens ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pick {
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr);
    column-gap: 8px;
    width: 100%;
    padding: 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    text-align: left;
  }

  .pick:hover {
    background: var(--hover);
  }

  .pick[aria-current="true"] {
    background: var(--selected);
  }

  .pick > :not(.dot) {
    grid-column: 2;
  }

  .dot {
    width: 8px;
    height: 8px;
    margin-top: 5px;
    border-radius: 50%;
    background: var(--waiting);
  }

  .dot.approved {
    background: var(--done);
  }

  .dot.rejected {
    background: var(--danger);
  }

  .dot.changes {
    background: var(--waiting-ring);
  }

  .name {
    overflow: hidden;
    font-size: var(--text-sm);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .meta.changed {
    color: var(--accent-ink);
  }

  .meta.issue {
    color: var(--danger);
  }

  .decision {
    margin-top: 2px;
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .decision.pending {
    color: var(--warning);
  }

  .decision.approved {
    color: var(--done);
  }

  .decision.rejected {
    color: var(--danger);
  }

  .decision.changes {
    color: var(--warning);
  }

  .notes {
    margin-top: 16px;
  }

  .notes ul {
    gap: 6px;
    padding: 0 8px;
    color: var(--muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .viewer {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .stage {
    flex: 1;
    min-height: 0;
    padding: 12px 16px;
  }

  .decide {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px 14px;
    border-top: 1px solid var(--border);
    background: var(--chrome);
  }

  .comment textarea {
    height: auto;
    padding: 6px 8px;
    resize: vertical;
  }

  .need {
    color: var(--danger);
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
  }

  .where {
    margin-right: auto;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .btn.reject {
    color: var(--danger);
  }

  .buttons .btn[aria-pressed="true"]:not(.primary) {
    background: var(--selected);
  }

  .pins {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  h4 {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .pins ol {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .pins li {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
  }

  .pins .field {
    flex: 1;
    min-width: 0;
  }

  .n {
    display: inline-grid;
    flex-shrink: 0;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--waiting-ring);
    color: #fff;
    font-size: var(--text-xs);
    font-weight: 700;
  }

  .picked .n {
    outline: 2px solid var(--focus);
    outline-offset: 1px;
  }

  .add {
    align-self: flex-start;
    margin-left: -8px;
  }

  .said {
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
