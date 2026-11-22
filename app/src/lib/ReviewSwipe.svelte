<script module lang="ts">
  import type { Swipe } from "$lib/visual-logic";

  /** By review and version: the cards passed over and the answers to take back, kept when the page goes. */
  const decks = $state<Record<string, { skipped: string[]; history: { id: string; kind: Swipe }[] }>>({});
</script>

<script lang="ts">
  import { fly, scale } from "svelte/transition";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Eye from "@lucide/svelte/icons/eye";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Undo2 from "@lucide/svelte/icons/undo-2";
  import X from "@lucide/svelte/icons/x";
  import ZoomIn from "@lucide/svelte/icons/zoom-in";
  import ReviewContext from "$lib/ReviewContext.svelte";
  import ReviewStage from "$lib/ReviewStage.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";
  import { ui } from "$lib/state.svelte";
  import { decide, decisionLabel, image, outcomeText, screenMeta, screenTitle } from "$lib/visual.svelte";
  import { deck, pending, settles, swipeKind, type Pin, type Review, type Screen, type Version } from "$lib/visual-logic";

  /**
   * The quick review (ARCHITECTURE §8.2): one card per screen still to decide, one gesture each — right approves, left
   * rejects, up asks for a change (a comment, pins), down passes — by drag, trackpad, touch or the arrow keys. Each
   * answer is saved at once, so the deck starts again where it was left; the last one settles the version and the
   * recap says where it went.
   */
  let { review, version, ondetail }: { review: Review; version: Version; ondetail: () => void } = $props();

  const key = $derived(`${review.id}/${version.n}`);
  const memory = () => (decks[key] ??= { skipped: [], history: [] });
  const cards = $derived(deck(version, decks[key]?.skipped ?? []));
  const card = $derived<Screen | undefined>(cards[0]);
  const history = $derived(decks[key]?.history ?? []);
  const total = $derived(version.screens.length);
  // The card's place in the version, so passing over a card moves on; the bar, what is answered.
  const position = $derived(version.screens.findIndex((s) => s.id === card?.id) + 1);
  const answered = $derived(total - pending(version).length);

  // By card: a card going away clears its own binding, never the next card's.
  const stages: Record<string, ReturnType<typeof ReviewStage> | null> = $state({});
  const stage = () => (card ? stages[card.id] : null);
  let busy = $state(false);
  let error = $state("");
  let composing = $state(false);
  let draft = $state<{ comment: string; pins: Pin[] }>({ comment: "", pins: [] });
  let before = $state(false);
  let zoomed = $state(false);
  /** The card moved by a gesture, and the size of its picture, which sets how far a gesture must go. */
  let drag = $state<{ dx: number; dy: number; w: number; h: number } | null>(null);
  let leaving = $state<Swipe>("skip");
  let confirming = $state(false);
  let said = $state("");

  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  const duration = reduced ? 0 : 220;
  const away: Record<Swipe, [number, number]> = { approved: [1, 0], rejected: [-1, 0], changes: [0, -1], skip: [0, 1] };
  const stamps: Record<Swipe, Parameters<typeof t>[0]> = {
    approved: "review5bApproved",
    rejected: "review5bRejected",
    changes: "review5bStampChange",
    skip: "review5bSkipped",
  };

  // The next cards' images are read before they show: no wait between two cards.
  $effect(() => {
    for (const s of cards.slice(0, 3))
      for (const kind of ["before", "after"] as const) if (s[kind]) image(review.id, version.n, s.id, kind).catch(() => {});
  });

  // A new card starts on its "after" image, whole; an update of the same card keeps the view.
  const cardId = $derived(card?.id);
  $effect(() => {
    void cardId;
    zoomed = false;
    before = false;
  });

  const stamp = $derived.by(() => {
    if (!drag) return null;
    const kind = Math.abs(drag.dx) >= Math.abs(drag.dy) ? (drag.dx > 0 ? "approved" : "rejected") : drag.dy < 0 ? "changes" : "skip";
    const reach = Math.abs(drag.dx) >= Math.abs(drag.dy) ? Math.abs(drag.dx) / Math.min(170, drag.w * 0.25) : Math.abs(drag.dy) / Math.min(130, drag.h * 0.2);
    return { kind: kind as Swipe, opacity: reach < 0.2 ? 0 : Math.min(1, reach) };
  });

  function openComposer(pin?: Pin) {
    if (!card) return;
    if (!composing) draft = { comment: card.comment, pins: $state.snapshot(card.pins) };
    composing = true;
    if (pin) draft.pins.push(pin);
  }

  function closeComposer() {
    composing = false;
  }

  async function act(kind: Swipe) {
    const s = card;
    if (!s || busy) return;
    error = "";
    if (kind === "skip") {
      const m = memory();
      m.skipped = [...m.skipped.filter((id) => id !== s.id), s.id];
      remember(s.id, kind);
      leaving = kind;
      closeComposer();
      said = `${t("review5bSkip")} · ${screenTitle(s)}`;
      return;
    }
    if (kind === "changes" && (!composing || (!draft.comment.trim() && !draft.pins.length))) return openComposer();
    if (kind === "rejected" && review.run !== null && settles(version, s.id, kind) === "rejected") {
      confirming = true;
      return;
    }
    await send(s, kind);
  }

  /** One entry by screen, its last answer: U takes back what was said last. */
  function remember(id: string, kind: Swipe) {
    const m = memory();
    m.history = [...m.history.filter((h) => h.id !== id), { id, kind }];
  }

  async function send(s: Screen, kind: Exclude<Swipe, "skip">) {
    busy = true;
    leaving = kind;
    // Before the answer: a paused team keeps the decision yet answers with an error, and U must take it back.
    remember(s.id, kind);
    try {
      const changes = kind === "changes";
      await decide(review.id, s.id, kind, changes ? draft.comment.trim() : undefined, changes ? $state.snapshot(draft.pins) : undefined);
      closeComposer();
      before = false;
      said = `${t(decisionLabel[kind])} · ${screenTitle(s)}`;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /** Takes the last answer back while the version is not settled; a card passed over comes back first. */
  async function undo() {
    const m = memory();
    const last = m.history.at(-1);
    if (!last || busy || version.state !== "ready") return;
    error = "";
    // A decision that never landed has nothing to take back.
    if (last.kind !== "skip" && version.screens.find((s) => s.id === last.id)?.decision !== "pending") {
      busy = true;
      try {
        await decide(review.id, last.id, "pending");
      } catch (e) {
        error = String(e);
        return;
      } finally {
        busy = false;
      }
    }
    m.history.pop();
    m.skipped = m.skipped.filter((id) => id !== last.id);
    leaving = "skip";
    said = t("review5bUndone");
  }

  /** A drag, a flick or a touch moves the card; a click that did not move pins a comment there. */
  function down(e: PointerEvent) {
    if (e.button > 0 || busy || !card) return;
    const el = e.currentTarget as HTMLElement;
    const [sx, sy, w, h] = [e.clientX, e.clientY, el.clientWidth, el.clientHeight];
    let [last, lx, ly, vx, vy, moved] = [e.timeStamp, sx, sy, 0, 0, false];
    el.setPointerCapture(e.pointerId);
    const move = (m: PointerEvent) => {
      const dt = m.timeStamp - last;
      if (dt > 0) [vx, vy, last, lx, ly] = [(m.clientX - lx) / dt, (m.clientY - ly) / dt, m.timeStamp, m.clientX, m.clientY];
      const [dx, dy] = [m.clientX - sx, m.clientY - sy];
      if (!moved && Math.hypot(dx, dy) > 8) moved = true;
      // Writing a change, the card stays: a drag neither swipes nor pins.
      if (moved && !composing) drag = { dx, dy, w, h };
    };
    const up = (u: PointerEvent) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      if (!moved) {
        const at = u.type === "pointerup" ? stage()?.pointAt(u.clientX, u.clientY) : null;
        if (at) openComposer({ x: at.x, y: at.y, comment: "" });
        return;
      }
      if (u.timeStamp - last > 90) vx = vy = 0;
      const kind = u.type === "pointerup" && drag ? swipeKind(drag.dx, drag.dy, vx, vy, w, h) : null;
      drag = null;
      if (kind) act(kind);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  }

  /** Two fingers on a trackpad: the card follows the page's scroll, and goes once the fingers rest. */
  function trackpad(node: HTMLElement) {
    let sum = { dx: 0, dy: 0 };
    let timer = 0;
    const on = (e: WheelEvent) => {
      if (e.ctrlKey || busy || composing || !card) return;
      e.preventDefault();
      sum = { dx: sum.dx - e.deltaX, dy: sum.dy - e.deltaY };
      drag = { ...sum, w: node.clientWidth, h: node.clientHeight };
      clearTimeout(timer);
      timer = window.setTimeout(() => {
        const kind = swipeKind(sum.dx, sum.dy, 0, 0, node.clientWidth, node.clientHeight);
        sum = { dx: 0, dy: 0 };
        drag = null;
        if (kind) act(kind);
      }, 160);
    };
    node.addEventListener("wheel", on, { passive: false });
    return () => {
      clearTimeout(timer);
      node.removeEventListener("wheel", on);
    };
  }

  function toggleZoom() {
    stage()?.toggleZoom();
    zoomed = !!stage()?.zoomed();
  }

  function keydown(e: KeyboardEvent) {
    if (ui.view !== "review" || ui.palette || ui.help || ui.switcher || ui.launcher || document.querySelector("dialog[open]")) return;
    const target = e.target as HTMLElement;
    if (target.closest?.("textarea, input")) {
      if (e.key === "Escape") closeComposer();
      else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) act("changes");
      else return;
      return void e.preventDefault();
    }
    if (target.closest?.("select, [role=tablist]") || e.metaKey || e.ctrlKey || e.altKey) return;
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (k === "u") return void (e.preventDefault(), undo());
    if (!card || (composing && !["Escape", " ", "b", "z"].includes(k))) return;
    const keys: Record<string, () => void> = {
      ArrowRight: () => act("approved"),
      ArrowLeft: () => act("rejected"),
      ArrowUp: () => act("changes"),
      ArrowDown: () => act("skip"),
      " ": () => !e.repeat && (before = true),
      b: () => (before = !before),
      z: toggleZoom,
      Escape: () => (composing ? closeComposer() : zoomed && toggleZoom()),
    };
    if (!keys[k]) return;
    e.preventDefault();
    keys[k]();
  }

  const focus = (node: HTMLElement) => node.focus();
</script>

<svelte:window onkeydown={keydown} onkeyup={(e) => e.key === " " && (before = false)} onblur={() => (before = false)} />

<div class="swipe">
  <p class="sr-only" aria-live="polite">{said}</p>
  {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}

  {#if card}
    <div class="top">
      <span class="count">{t("review5bCard", { at: position, count: total })}</span>
      <span class="track" aria-hidden="true"><span style:width="{(answered / total) * 100}%"></span></span>
      <button class="btn sm quiet" onclick={undo} disabled={!history.length || busy}>
        <Undo2 size={14} aria-hidden="true" />{t("review5bUndo")}<kbd>U</kbd>
      </button>
    </div>

    <div class="pile">
      <!-- One card at a time; each keeps its own screen while it flies out. -->
      {#each cards.slice(0, 1) as c (c.id)}
        <article
          class="card"
          class:dragging={!!drag}
          aria-label={t("review5bCard", { at: position, count: total })}
          style:transform={drag ? `translate(${drag.dx}px, ${drag.dy}px) rotate(${drag.dx * 0.03}deg)` : undefined}
          in:scale={{ start: 0.96, duration }}
          out:fly={{ x: away[leaving][0] * 640, y: away[leaving][1] * 520, duration, opacity: 0 }}
        >
          <header>
            <div class="title">
              <b class="mono">{screenTitle(c)}</b>
              <span class="muted">{screenMeta(c)}</span>
              <span class="side" class:before>{t(before ? "review5bBefore" : "review5bAfter")}</span>
            </div>
          </header>
          <div class="body">
            <!-- svelte-ignore a11y_no_static_element_interactions (every gesture has its button and its key) -->
            <div class="pic" onpointerdown={down} {@attach trackpad}>
              <ReviewStage
                bind:this={stages[c.id]}
                {review}
                {version}
                screen={c}
                mode="single"
                {before}
                contain
                free={false}
                controls={false}
                pins={composing ? draft.pins : c.pins}
                onpin={(x, y) => openComposer({ x, y, comment: "" })}
              />
              {#if stamp && stamp.opacity}
                <span class="stamp {stamp.kind}" style:opacity={stamp.opacity}>{t(stamps[stamp.kind])}</span>
              {/if}
            </div>
            <aside>
              <ReviewContext {review} {version} screen={c} />
            </aside>
          </div>
          {#if composing}
            <form class="composer" onsubmit={(e) => (e.preventDefault(), act("changes"))}>
              <label class="labelled">
                <span>{t("review5bComposerTitle")}</span>
                <textarea class="field" rows="2" placeholder={t("review5bCommentHint")} bind:value={draft.comment} {@attach focus}></textarea>
              </label>
              <p class="help">{t("review5bComposerHint")}</p>
              {#if draft.pins.length}
                <ol class="pins">
                  {#each draft.pins as pin, i (i)}
                    <li>
                      <span class="n" aria-hidden="true">{i + 1}</span>
                      <input class="field" aria-label={t("review5bPinComment", { n: i + 1 })} bind:value={pin.comment} />
                      <button
                        type="button"
                        class="icon-btn sm"
                        aria-label={t("review5bPinRemove", { n: i + 1 })}
                        title={t("review5bPinRemove", { n: i + 1 })}
                        onclick={() => draft.pins.splice(i, 1)}
                      >
                        <X size={14} aria-hidden="true" />
                      </button>
                    </li>
                  {/each}
                </ol>
              {/if}
              <div class="acts">
                <button type="button" class="btn sm" onclick={closeComposer}>{t("cancel")}</button>
                <button class="btn sm primary" disabled={busy || (!draft.comment.trim() && !draft.pins.length)}>
                  {#if busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}{t("review5bSendChanges")}
                </button>
              </div>
            </form>
          {/if}
        </article>
      {/each}
    </div>

    <div class="gestures">
      <button class="btn reject" onclick={() => act("rejected")} disabled={busy}>
        <ArrowLeft size={15} aria-hidden="true" />{t("teamRunsRefuse")}
      </button>
      <button class="btn" onclick={() => act("changes")} disabled={busy} aria-pressed={composing}>
        <ArrowUp size={15} aria-hidden="true" />{t("review5bChange")}
      </button>
      <span class="tools">
        <button
          class="btn sm quiet"
          aria-pressed={before}
          onpointerdown={(e) => (e.preventDefault(), (before = true))}
          onpointerup={() => (before = false)}
          onpointerleave={() => (before = false)}
          onclick={(e) => e.detail === 0 && (before = !before)}
        >
          <Eye size={14} aria-hidden="true" />{t("review5bHoldBefore")}<kbd>␣</kbd>
        </button>
        <button class="btn sm quiet" aria-pressed={zoomed} onclick={toggleZoom}>
          <ZoomIn size={14} aria-hidden="true" />{t("review5bZoomToggle")}<kbd>Z</kbd>
        </button>
      </span>
      <button class="btn" onclick={() => act("skip")} disabled={busy}>
        <ArrowDown size={15} aria-hidden="true" />{t("review5bSkip")}
      </button>
      <button class="btn primary" onclick={() => act("approved")} disabled={busy}>
        {t("teamRunsValidate")}<ArrowRight size={15} aria-hidden="true" />
      </button>
    </div>
  {:else}
    <section class="recap" aria-labelledby="recap-title">
      <h3 id="recap-title">{t("review5bRecap")}</h3>
      {#if version.outcome}<p class="outcome {version.outcome}">{outcomeText(review, version)}</p>{/if}
      <ul>
        {#each version.screens as s (s.id)}
          <li>
            <span class="decision {s.decision}">{t(decisionLabel[s.decision])}</span>
            <span class="what">
              <b class="mono">{screenTitle(s)}</b>
              <span class="muted">{screenMeta(s)}</span>
              {#if s.comment}<span class="comment">{t("review5bQuote", { text: s.comment })}</span>{/if}
              {#each s.pins as pin, i (i)}
                <span class="comment"><span class="n" aria-hidden="true">{i + 1}</span>{pin.comment || t("review5bPinLabel", { n: i + 1 })}</span>
              {/each}
            </span>
          </li>
        {/each}
      </ul>
      <div class="acts">
        <button class="btn sm" onclick={ondetail}>{t("review5bDetail")}</button>
      </div>
    </section>
  {/if}
</div>

<ConfirmDialog
  bind:open={confirming}
  title={t("teamRunsRefuseTitle")}
  confirm={t("teamRunsRefuse")}
  danger
  onconfirm={() => card && send(card, "rejected")}
>
  <p>{t("teamRunsRefuseText")}</p>
</ConfirmDialog>

<style>
  .swipe {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    min-height: 0;
    padding: 16px 20px 20px;
  }

  .top {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: var(--text-sm);
  }

  .count {
    font-weight: 600;
    white-space: nowrap;
  }

  .track {
    flex: 1;
    height: 4px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--border);
  }

  .track span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width var(--dur) var(--ease-out);
  }

  kbd {
    margin-left: 4px;
    padding: 0 4px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
  }

  .pile {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / minmax(0, 1fr);
    min-height: 0;
  }

  .card {
    display: flex;
    flex-direction: column;
    grid-area: 1 / 1;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-2);
    transition: transform var(--dur-slow) var(--ease-out);
  }

  .card.dragging {
    transition: none;
  }

  header {
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 12px;
  }

  .side {
    margin-left: auto;
    padding: 1px 10px;
    border-radius: var(--radius-pill);
    background: var(--selected);
    font-size: var(--text-xs);
    font-weight: 650;
  }

  .side.before {
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .title b {
    font-size: var(--text-md);
    font-weight: 600;
  }

  .muted {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .body {
    display: grid;
    flex: 1;
    grid-template-columns: minmax(0, 1fr) 280px;
    min-height: 0;
  }

  .pic {
    position: relative;
    min-height: 0;
    padding: 12px;
    touch-action: none;
    user-select: none;
  }

  aside {
    min-height: 0;
    padding: 12px 16px;
    overflow-y: auto;
    border-left: 1px solid var(--border);
  }

  .stamp {
    position: absolute;
    top: 28px;
    left: 50%;
    padding: 6px 14px;
    border: 3px solid currentColor;
    border-radius: var(--radius);
    background: var(--panel);
    font-size: var(--text-lg);
    font-weight: 750;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    transform: translateX(-50%) rotate(-6deg);
    pointer-events: none;
  }

  .stamp.approved {
    color: var(--done);
  }

  .stamp.rejected {
    color: var(--danger);
  }

  .stamp.changes {
    color: var(--waiting-ring);
  }

  .stamp.skip {
    color: var(--muted);
  }

  .composer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
    background: var(--waiting-soft);
  }

  .composer textarea {
    height: auto;
    padding: 6px 8px;
    background: var(--panel);
    resize: vertical;
  }

  .help {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .pins {
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
    gap: 8px;
  }

  .pins .field {
    flex: 1;
    background: var(--panel);
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

  .acts {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }

  .gestures {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .tools {
    display: flex;
    gap: 4px;
    margin: 0 8px;
  }

  .btn.reject {
    color: var(--danger);
  }

  .recap {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(720px, 100%);
    margin: 0 auto;
    padding: 20px 24px;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .recap h3 {
    margin: 0;
    font-size: var(--text-xl);
  }

  .outcome {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--hover);
    line-height: 1.5;
  }

  .outcome.approved {
    background: var(--done-soft);
  }

  .outcome.changes {
    background: var(--waiting-soft);
  }

  .outcome.rejected {
    background: var(--danger-soft);
  }

  .recap ul {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .recap li {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
    font-size: var(--text-sm);
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .comment {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .decision {
    font-weight: 600;
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

  .decision.pending {
    color: var(--muted);
  }

  @media (prefers-reduced-motion: reduce) {
    .card,
    .track span {
      transition: none;
    }
  }
</style>
