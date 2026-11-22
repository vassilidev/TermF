<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Banner from "$lib/ui/Banner.svelte";
  import { showRun } from "$lib/commands";
  import { ago, t } from "$lib/i18n";
  import type { Decision } from "$lib/team-run-logic";
  import { team } from "$lib/team.svelte";

  /** The project's decision journal, newest first; mounted by its Configuration page. */
  let { project }: { project: string } = $props();

  let decisions = $state<Decision[] | null>(null);
  let error = $state("");

  /** The last request: an answer to an earlier one, arriving late, is dropped. */
  let asked = 0;

  // Again whenever a run of the project changes: a gate or the CTO may have decided.
  $effect(() => {
    void team.runs.filter((r) => r.target.project === project).map((r) => r.updated);
    const n = ++asked;
    invoke<Decision[]>("team_decisions", { project }).then(
      (list) => {
        if (n !== asked) return;
        decisions = list;
        error = "";
      },
      (e) => {
        if (n === asked) error = String(e);
      },
    );
  });
</script>

<section aria-labelledby="config-decisions">
  <h3 id="config-decisions">{t("disciplineDecisions")} {#if decisions?.length}<small>{decisions.length}</small>{/if}</h3>
  <p class="help">{t("disciplineDecisionsHelp")}</p>
  {#if error}
    <Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>
  {/if}
  {#if decisions?.length}
    <ol class="journal">
      {#each decisions as d (d.id)}
        <li class:replaced={d.replacedBy !== null}>
          <span class="id mono">D{d.id}</span>
          <div class="body">
            <p class="text">{d.text}</p>
            <p class="meta">
              <span class="badge">{t(d.by === "cto" ? "disciplineByCto" : "disciplineByYou")}</span>
              <button class="link" onclick={() => showRun(d.run)}>{t("disciplineRun", { run: d.run })}</button>
              <span>{ago(d.at)}</span>
              {#if d.replaces !== null}<span>{t("disciplineReplaces", { id: d.replaces })}</span>{/if}
              {#if d.replacedBy !== null}<span class="gone">{t("disciplineReplacedBy", { id: d.replacedBy })}</span>{/if}
            </p>
          </div>
        </li>
      {/each}
    </ol>
  {:else if decisions}
    <p class="muted">{t("disciplineDecisionsEmpty")}</p>
  {/if}
</section>

<style>
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  h3 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 650;
  }

  h3 small {
    color: var(--muted);
    font-size: var(--text-sm);
    font-weight: 400;
  }

  p {
    margin: 0;
  }

  .help {
    color: var(--muted);
    line-height: 1.5;
  }

  .journal {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    list-style: none;
  }

  li {
    display: flex;
    gap: 12px;
    padding: 10px 12px;
  }

  li + li {
    border-top: 1px solid var(--border);
  }

  .id {
    flex-shrink: 0;
    min-width: 36px;
    color: var(--muted);
    font-size: var(--text-sm);
    line-height: 1.6;
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .text {
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }

  .link:hover {
    text-decoration: underline;
  }

  .replaced .text {
    color: var(--muted);
    text-decoration: line-through;
  }

  .gone {
    color: var(--warning);
  }
</style>
