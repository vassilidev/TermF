<script lang="ts">
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import Conversation from "$lib/Conversation.svelte";
  import Button from "$lib/ui/Button.svelte";
  import Card from "$lib/ui/Card.svelte";
  import Field from "$lib/ui/Field.svelte";
  import { stepName, ticketKey } from "$lib/TeamRunDetail.svelte";
  import { agents, ctoOf, showConversation } from "$lib/agents.svelte";
  import { ensure } from "$lib/home.svelte";
  import { t } from "$lib/i18n";
  import { type TeamRun, tellable } from "$lib/team-run-logic";

  /**
   * "Tell the team something": the conversation of the CTO, who leads the run, or of the agent of one of its steps,
   * drawn as everywhere else. `open`: the run's worktree is still there, where a finished step's conversation would take
   * up again.
   */
  let { run, open = true }: { run: TeamRun; open?: boolean } = $props();

  const LEAD = "lead";
  const steps = $derived(tellable(run, open));
  let to = $state(LEAD);

  $effect(() => {
    if (to !== LEAD && !steps.some((s) => s.key === to)) to = LEAD;
  });

  // The CTO's conversation is made, without a process, the first time it is needed.
  $effect(() => {
    if (to === LEAD) void ensure(run.target.project, "cto");
  });

  const session = $derived.by(() => {
    if (to === LEAD) return ctoOf(run.target.project);
    const id = steps.find((s) => s.key === to)?.session;
    return agents.sessions.find((s) => s.id === id);
  });

  /** To the CTO, a message says which run it is about. */
  const about = (text: string) =>
    Promise.resolve(t("practicalTeamTellAbout", { run: run.run, ticket: ticketKey(run.target.ticket), title: run.target.title, text }));
</script>

<Card title={t("practicalTeamTell")}>
  <p class="help">{t("practicalTeamTellHelp")}</p>
  <div class="to">
    <Field label={t("practicalTeamTellTo")}>
      {#snippet control(attrs)}
        <select class="field" {...attrs} bind:value={to}>
          <option value={LEAD}>{t("practicalTeamTellLead")}</option>
          {#each steps as step (step.key)}
            <option value={step.key}>{step.role ?? ""} · {stepName(step)}</option>
          {/each}
        </select>
      {/snippet}
    </Field>
    {#if session}
      {@const id = session.id}
      <Button variant="link" size="sm" icon={MessagesSquare} onclick={() => showConversation(id)}>{t("practicalTeamTellOpen")}</Button>
    {/if}
  </div>
  <div class="talk">
    {#if session}
      {#key session.id}
        <Conversation {session} bare placeholder={t("practicalTeamTellPlaceholder")} outgoing={to === LEAD ? about : undefined} />
      {/key}
    {/if}
  </div>
</Card>

<style>
  .help {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .to {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px 12px;
  }

  .to :global(.field) {
    max-width: 22rem;
  }

  .talk {
    display: flex;
    flex-direction: column;
    height: 28rem;
    margin-top: 12px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }
</style>
