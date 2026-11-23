<script lang="ts">
  import ChevronRight from "@lucide/svelte/icons/chevron-right";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import RepoDetection from "$lib/RepoDetection.svelte";
  import ForgeIcon from "$lib/ui/ForgeIcon.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import StreamSteps from "$lib/ui/StreamSteps.svelte";
  import { lang, t } from "$lib/i18n";
  import { analyze, draft, found } from "$lib/newproject.svelte";
  import { analyzing, readText, reads } from "$lib/onboarding.svelte";
  import { codeSpans, ownTeam } from "$lib/onboarding-logic";
  import { tagText, type Card } from "$lib/setup-logic";
  import { approxUsd } from "$lib/usage.svelte";

  /** What TermF understood of each repository in two or three sentences, the details a click away; Claude Code live. */
  let { project, chosen }: { project: string | null; chosen: Card[] } = $props();

  const key = $derived(project ?? "");
  const paths = $derived(chosen.map((c) => c.path));
  const current = $derived(draft.analyzed === paths.join("\n"));
  const running = $derived(!!analyzing[key]);
  const live = $derived(reads[key]);
  const money = new Intl.NumberFormat(lang, { style: "currency", currency: "USD" });

  // Claude Code reads what was chosen as soon as it is shown; another choice since is read again.
  $effect(() => {
    if (!current && !running && paths.length) start();
  });

  function start() {
    draft.failed = "";
    analyze(project, [...paths], lang).catch(() => {});
  }

  function sentences(card: Card): string[] {
    const d = found[card.path];
    if (!d || typeof d === "string") return [];
    const shown = d.tags.slice(0, 4).map(tagText);
    const test = d.scripts.find((s) => s.kind === "test")?.run;
    const f = d.facts;
    const third = f.workflows.length
      ? t("cpSumCi", { list: f.workflows.join(", ") })
      : f.parts.length > 1
        ? t("cpSumParts", { count: f.parts.length, list: f.parts.map((p) => p.path || card.name).join(", ") })
        : f.infra.length
          ? t("cpSumInfra", { list: f.infra.slice(0, 3).join(", ") })
          : "";
    return [
      shown.length ? t("cpSumStack", { list: shown.join(", ") }) : t("cpSumNoStack"),
      test ? t("cpSumTests", { command: test }) : t("cpSumNoTests"),
      third,
    ].filter(Boolean);
  }

  const findings = (card: Card) => (current ? (draft.analysis?.findings ?? []).filter((f) => f.repo === card.name) : []);
  const asked = $derived(current ? (draft.analysis?.questions ?? []).filter((q) => !ownTeam(q)).length : 0);
</script>

{#snippet inline(text: string)}
  {#each codeSpans(text) as sp, i (i)}{#if sp.code}<code>{sp.text}</code>{:else}{sp.text}{/if}{/each}
{/snippet}

<div class="repos">
  {#each chosen as card (card.path)}
    {@const d = found[card.path]}
    {@const own = findings(card)}
    <article class="repo">
      <header>
        <span class="forge"><ForgeIcon host={card.remote?.host} size={16} /></span>
        <h3>{card.name}</h3>
        {#if card.remote}<span class="slug">{card.remote.slug}</span>{/if}
      </header>
      {#if d === undefined}
        <Skeleton rows={2} label={t("setupDetecting", { name: card.name })} />
      {:else if typeof d === "string"}
        <p class="notice error" role="alert">{d}</p>
      {:else}
        <p class="said">{sentences(card).join(" ")}</p>
        <details>
          <summary>
            <ChevronRight size={13} aria-hidden="true" class="caret" />{t("cpDetail")}
            {#if own.length}<span class="found"><Sparkles size={12} aria-hidden="true" />{t("cpFindings", { count: own.length })}</span>{/if}
          </summary>
          <div class="more">
            <RepoDetection {card} found={d} />
            {#if own.length}
              <dl class="claude">
                {#each own as f, i (i)}
                  <dt>{f.label}</dt>
                  <dd>{@render inline(f.value)}</dd>
                {/each}
              </dl>
            {/if}
          </div>
        </details>
      {/if}
    </article>
  {/each}
</div>

<section class="claude-run" aria-live="polite">
  <header>
    <Sparkles size={16} aria-hidden="true" />
    <h3>{t("setupClaudeTitle")}</h3>
    {#if current && !running}
      <button class="btn sm quiet" onclick={start}><RotateCw size={13} aria-hidden="true" />{t("obReanalyze")}</button>
    {/if}
  </header>
  {#if running || !current}
    <p class="muted">{t("cpClaudeReading", { budget: money.format(2) })}</p>
    {#if live?.last.length}
      <StreamSteps live steps={live.last.map((r, i) => ({ verb: readText(r), state: i === live.last.length - 1 ? "now" : "ok" }))} />
    {:else}
      <Skeleton rows={2} label={t("scoutStarting")} />
    {/if}
  {:else if draft.failed}
    <p class="notice warn" role="status">{t("cpClaudeFailed", { error: draft.failed })}</p>
  {:else if draft.analysis}
    {@const a = draft.analysis}
    <p>
      {t("cpClaudeDone", { findings: a.findings.length, questions: asked })}
      {#if a.cost !== null}<span class="muted">· {t("convCost", { cost: approxUsd(money.format(a.cost)) })}</span>{/if}
    </p>
    {#if live?.count}
      <StreamSteps steps={live.last.map((r) => ({ verb: readText(r), state: "ok" }))} summary={t("cpClaudeReads", { count: live.count })} />
    {/if}
  {/if}
</section>

<style>
  .repos {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .repo {
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .repo header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
  }

  .forge {
    display: inline-flex;
    color: var(--soft);
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .slug {
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .said {
    margin: 0;
    color: var(--soft);
    line-height: 1.55;
  }

  details {
    margin-top: 8px;
  }

  summary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent-ink);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  summary :global(.caret) {
    transition: transform var(--dur-fast) var(--ease-out);
  }

  details[open] summary :global(.caret) {
    transform: rotate(90deg);
  }

  .found {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-left: 4px;
    color: var(--muted);
    font-weight: 400;
  }

  .more {
    margin-top: 10px;
  }

  /* The detection's own card, inside this one: its rows only. */
  .more :global(.detect) {
    border: 0;
    background: none;
    animation: none;
  }

  .more :global(.detect > header) {
    display: none;
  }

  .claude {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 12px 0 0;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--well);
    font-size: var(--text-sm);
  }

  .claude dt {
    color: var(--muted);
  }

  .claude dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .claude-run {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 20px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .claude-run header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .claude-run header :global(svg) {
    color: var(--accent-ink);
  }

  .claude-run header .btn {
    margin-left: auto;
  }

  .claude-run p {
    margin: 0;
  }
</style>
