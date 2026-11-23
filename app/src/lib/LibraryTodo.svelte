<script lang="ts">
  import type { Component } from "svelte";
  import CalendarClock from "@lucide/svelte/icons/calendar-clock";
  import CircleArrowUp from "@lucide/svelte/icons/circle-arrow-up";
  import CircleCheckBig from "@lucide/svelte/icons/circle-check-big";
  import ListChecks from "@lucide/svelte/icons/list-checks";
  import MessageCircleQuestionMark from "@lucide/svelte/icons/message-circle-question-mark";
  import Power from "@lucide/svelte/icons/power";
  import Scale from "@lucide/svelte/icons/scale";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import Telescope from "@lucide/svelte/icons/telescope";
  import Button from "$lib/ui/Button.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { lang, t } from "$lib/i18n";
  import { talkCto } from "$lib/home.svelte";
  import { library } from "$lib/library.svelte";
  import { app } from "$lib/state.svelte";
  import type { Todo } from "$lib/todo-logic";
  import { later, openScout, scheduleWatch, switchOn, todo } from "$lib/todo.svelte";

  const project = $derived(app.workspace?.projects.find((p) => p.id === app.project) ?? null);
  const here = $derived(project?.name ?? t("watchScheduleGlobal"));
  const list = (xs: string[]) => new Intl.ListFormat(lang, { type: "conjunction" }).format(xs);
  let busy = $state("");

  type Line = { icon: Component; title: string; detail?: string; action: string; run: () => unknown; later?: boolean };

  function openConvention(slug: string | null, deduce = false) {
    Object.assign(library, { tab: "conventions", panel: null, global: false, convention: slug, deduce });
  }

  function line(item: Todo): Line {
    switch (item.kind) {
      case "questions":
        return { icon: MessageCircleQuestionMark, title: t("watchTodoQuestions", { name: item.slug, count: item.count }), action: t("watchTodoAnswer"), run: () => openConvention(item.slug) };
      case "decide":
        return { icon: ListChecks, title: t("watchTodoDecide", { name: item.slug, count: item.count }), action: t("watchTodoReview"), run: () => openConvention(item.slug) };
      case "scout":
      case "scoutGlobal": {
        const names = item.names.length > 5 ? `${list(item.names.slice(0, 5))}…` : list(item.names);
        return {
          icon: Telescope,
          title: t(item.kind === "scout" ? "watchTodoScout" : "watchTodoScoutGlobal", { count: item.count }),
          detail: item.outdated ? `${names} · ${t("watchTodoOutdated", { count: item.outdated })}` : names,
          action: t("watchTodoReview"),
          run: () => openScout(item.kind === "scoutGlobal"),
        };
      }
      case "off":
        return { icon: Power, title: t("watchTodoOff", { name: item.item.name }), detail: t("watchOffHint", { project: here }), action: t("watchActivate", { project: here }), run: () => switchOn(item.item), later: true };
      case "claude":
        return { icon: CircleArrowUp, title: t("watchTodoClaude", { now: item.now }), detail: t("watchTodoClaudeDetail", { seen: item.seen }), action: t("watchReviewWithScout"), run: () => openScout(false, true), later: true };
      case "never":
        return { icon: Telescope, title: t("watchTodoNever"), detail: t("watchTodoNeverDetail"), action: t("scoutRun"), run: () => openScout(false, true), later: true };
      case "noConventions":
        return { icon: Scale, title: t("watchTodoNoConventions", { project: here }), detail: t("watchTodoNoConventionsDetail"), action: t("watchDeduce"), run: () => openConvention(null, true), later: true };
      case "noSchedule":
        return { icon: CalendarClock, title: t("watchTodoNoSchedule"), detail: t("watchTodoNoScheduleDetail"), action: t("watchTodoScheduleAction"), run: () => scheduleWatch(null), later: true };
    }
  }

  async function act(item: Todo, run: () => unknown) {
    busy = item.id;
    try {
      await run();
    } finally {
      busy = "";
    }
  }

  function askCto() {
    const lines = (todo.list ?? []).map((item) => `- ${line(item).title}`);
    talkCto(t("watchTodoAskDraft", { project: here, list: lines.join("\n") }));
  }
</script>

<section class="todo" aria-labelledby="library-todo-title">
  <header class="head">
    <div>
      <h2 id="library-todo-title">{t("watchTodo")}</h2>
      <p class="lead">{t("watchTodoLead")}</p>
    </div>
    {#if todo.list?.length && project}
      <Button size="sm" icon={Sparkles} onclick={askCto}>{t("watchTodoAsk")}</Button>
    {/if}
  </header>

  {#if todo.error}<p class="notice error" role="alert">{todo.error}</p>{/if}

  {#if !todo.list}
    <Skeleton shape="list" rows={4} label={t("watchTodoLoading")} />
  {:else if !todo.list.length}
    <EmptyState icon={CircleCheckBig} text={t("watchTodoEmpty")} />
  {:else}
    <ul class="lines">
      {#each todo.list as item (item.id)}
        {@const l = line(item)}
        <li class="line">
          <span class="mark"><l.icon size={16} aria-hidden="true" /></span>
          <div class="text">
            <span class="title">{l.title}</span>
            {#if l.detail}<span class="detail">{l.detail}</span>{/if}
          </div>
          <div class="acts">
            {#if l.later}
              <Button variant="quiet" size="sm" onclick={() => later(item)}>{t("watchTodoLater")}</Button>
            {/if}
            <Button size="sm" variant={item.kind === "questions" || item.kind === "decide" || item.kind === "scout" ? "primary" : "secondary"} loading={busy === item.id} disabled={!!busy} onclick={() => act(item, l.run)}>
              {l.action}
            </Button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .todo {
    flex: 1;
    min-height: 0;
    padding: 20px 24px 40px;
    overflow-y: auto;
  }

  .todo > * {
    max-width: 880px;
  }

  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .lead {
    max-width: 62ch;
    margin: 4px 0 0;
    color: var(--soft);
  }

  .lines {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  .line {
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    min-height: var(--row-lg);
    padding: 12px 14px;
  }

  .line + .line {
    border-top: 1px solid var(--border);
  }

  .mark {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--soft);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .title {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .detail {
    color: var(--soft);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  @media (max-width: 720px) {
    .line {
      grid-template-columns: 32px minmax(0, 1fr);
    }

    .acts {
      grid-column: 2;
    }
  }
</style>
