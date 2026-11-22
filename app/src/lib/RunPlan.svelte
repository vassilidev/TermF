<script lang="ts">
  import { stepPill } from "$lib/TeamRunDetail.svelte";
  import StatusPill from "$lib/ui/StatusPill.svelte";
  import { t } from "$lib/i18n";
  import { asks, levels, type TeamRun } from "$lib/team-run-logic";

  /** The orchestrator's plan: summary, criteria, what is out of scope, questions left, tasks by level. */
  let { run }: { run: TeamRun } = $props();

  const tasks = $derived(levels(run.plan?.tasks ?? []));
  const asked = $derived(asks(run).some((a) => a.kind === "questions" && a.step.plan));
  const taskStep = (id: string) => run.steps.find((s) => s.kind === "task" && s.task === id);
</script>

{#if run.plan}
  {@const plan = run.plan}
  {#if plan.summary}<p class="summary">{plan.summary}</p>{/if}
  {#if plan.acceptance.length || plan.out_of_scope.length}
    <div class="scope">
      {#if plan.acceptance.length}
        <div>
          <h4>{t("teamRunsCriteria")}</h4>
          <ul class="criteria">
            {#each plan.acceptance as criterion (criterion.id)}
              <li><span class="mono">{criterion.id}</span>{criterion.text}</li>
            {/each}
          </ul>
        </div>
      {/if}
      {#if plan.out_of_scope.length}
        <div>
          <h4>{t("teamRunsOutOfScope")}</h4>
          <ul class="plain">
            {#each plan.out_of_scope as item, i (i)}<li>{item}</li>{/each}
          </ul>
        </div>
      {/if}
    </div>
  {/if}
  {#if plan.questions.length && !asked}
    <h4>{t("teamRunsQuestions")}</h4>
    <ol class="questions">
      {#each plan.questions as question, i (i)}<li>{question.question}</li>{/each}
    </ol>
  {/if}
  {#if plan.tasks.length}
    <h4>{t("teamRunsTasks")} <small class="muted">{t("teamRunsLevelsHelp")}</small></h4>
    <ol class="levels">
      {#each tasks as level, n (n)}
        <li>
          <span class="level">{t("teamRunsLevel", { n: n + 1 })}</span>
          <ul class="tasks">
            {#each level as task (task.id)}
              {@const step = taskStep(task.id)}
              <li class="task">
                <div class="task-head">
                  <span class="mono id">{task.id}</span>
                  <b>{task.goal}</b>
                  {#if step}
                    {@const [status, label] = stepPill(step)}
                    <StatusPill {status} {label} />
                  {/if}
                </div>
                <div class="tags">
                  <span class="badge role">{task.role}</span>
                  {#if task.squad}<span class="badge">{t("teamRunsSquad", { name: task.squad })}</span>{/if}
                  <span class="badge">{task.repo}</span>
                  <span class="badge size">{task.size}</span>
                  {#if task.ui_impact}<span class="badge">{t("teamRunsUiImpact")}</span>{/if}
                  {#if task.covers.length}<span class="muted">{t("teamRunsCovers", { ids: task.covers.join(", ") })}</span>{/if}
                  {#if task.depends_on.length}<span class="muted">{t("teamRunsDependsOn", { ids: task.depends_on.join(", ") })}</span>{/if}
                </div>
                {#if task.files_hint.length}<p class="mono muted hint">{task.files_hint.join("  ")}</p>{/if}
              </li>
            {/each}
          </ul>
        </li>
      {/each}
    </ol>
  {/if}
{:else}
  <p class="muted small">{t("teamRunsNoPlan")}</p>
{/if}

<style>
  p {
    margin: 0;
  }

  .summary {
    line-height: 1.5;
  }

  .small {
    font-size: var(--text-sm);
  }

  h4 {
    margin: 14px 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  h4 small {
    margin-left: 6px;
    font-weight: 400;
  }

  .scope {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 0 24px;
  }

  .criteria,
  .plain {
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    line-height: 1.6;
  }

  .criteria li {
    display: flex;
    gap: 8px;
  }

  .criteria .mono {
    flex-shrink: 0;
    width: 34px;
    color: var(--muted);
    line-height: 1.9;
  }

  .plain li::before {
    content: "–  ";
    color: var(--muted);
  }

  .questions {
    margin: 0;
    padding-left: 20px;
    font-size: var(--text-sm);
    line-height: 1.6;
  }

  .levels {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .level {
    display: block;
    margin-bottom: 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .tasks {
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

  .task {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
  }

  .task + .task {
    border-top: 1px solid var(--border);
  }

  .task-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .task-head b {
    flex: 1;
    min-width: 0;
    font-weight: 600;
  }

  .id {
    color: var(--muted);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    font-size: var(--text-sm);
  }

  .role {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
    font-weight: 600;
  }

  .size {
    color: var(--text);
    font-weight: 700;
  }

  .hint {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
