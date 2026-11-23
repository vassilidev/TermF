<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import Send from "@lucide/svelte/icons/send";
  import Button from "$lib/ui/Button.svelte";
  import Card from "$lib/ui/Card.svelte";
  import Field from "$lib/ui/Field.svelte";
  import { stepName, ticketKey } from "$lib/TeamRunDetail.svelte";
  import { type AgentSession, ctoOf, showConversation, upsert } from "$lib/agents.svelte";
  import type { StoredEvent } from "$lib/conversation-logic";
  import { t } from "$lib/i18n";
  import { hear, type Reply, type TeamRun, tellable } from "$lib/team-run-logic";

  /**
   * "Tell the team something": a message to the CTO, who leads the run, or to the agent of one of its steps. `open`:
   * the run's worktree is still there, where a finished step's conversation would take up again.
   */
  let { run, open = true }: { run: TeamRun; open?: boolean } = $props();

  const LEAD = "lead";
  const agents = $derived(tellable(run, open));
  let to = $state(LEAD);
  let draft = $state("");
  let busy = $state(false);
  let error = $state("");
  let reply = $state<Reply | null>(null);

  $effect(() => {
    if (to !== LEAD && !agents.some((s) => s.key === to)) to = LEAD;
  });

  const stop = listen<StoredEvent>("agent-event", ({ payload: e }) => {
    if (reply) hear(reply, e);
  });
  onDestroy(() => void stop.then((f) => f()));

  async function send(e: SubmitEvent) {
    e.preventDefault();
    const text = draft.trim();
    if (!text || busy) return;
    busy = true;
    error = "";
    try {
      let session: number;
      let message = text;
      let who: string;
      if (to === LEAD) {
        const cto = ctoOf(run.target.project) ?? (await invoke<AgentSession>("cto_open", { project: run.target.project }));
        upsert(cto);
        session = cto.id;
        who = t("ux3t1Cto");
        message = t("practicalTeamTellAbout", { run: run.run, ticket: ticketKey(run.target.ticket), title: run.target.title, text });
      } else {
        const step = agents.find((s) => s.key === to);
        if (!step || step.session === null) return;
        session = step.session;
        who = `${step.role ?? ""} · ${stepName(step)}`;
      }
      reply = { session, who, text: "", heard: false, done: false, error: null };
      await invoke<boolean>("agent_send_or_queue", { id: session, text: message });
      draft = "";
    } catch (err) {
      reply = null;
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<Card title={t("practicalTeamTell")}>
  <p class="help">{t("practicalTeamTellHelp")}</p>
  <form onsubmit={send}>
    <Field label={t("practicalTeamTellTo")}>
      {#snippet control(attrs)}
        <select class="field" {...attrs} bind:value={to} disabled={busy}>
          <option value={LEAD}>{t("practicalTeamTellLead")}</option>
          {#each agents as step (step.key)}
            <option value={step.key}>{step.role ?? ""} · {stepName(step)}</option>
          {/each}
        </select>
      {/snippet}
    </Field>
    <Field label={t("practicalTeamTellMessage")} placeholder={t("practicalTeamTellPlaceholder")} bind:value={draft} disabled={busy} error={error || undefined} />
    <Button variant="primary" size="sm" type="submit" icon={Send} loading={busy} disabled={!draft.trim()}>{t("practicalTeamTellSend")}</Button>
  </form>
  {#if reply}
    <div class="reply" aria-live="polite">
      <div class="who">
        <b>{reply.who}</b>
        {#if !reply.done}<LoaderCircle size={14} class="spin" aria-label={t("practicalTeamTellWaiting")} />{/if}
        <Button variant="link" size="sm" icon={MessagesSquare} onclick={() => showConversation(reply!.session)}>{t("practicalTeamTellOpen")}</Button>
      </div>
      {#if reply.text}<p class="said">{reply.text}</p>{:else if !reply.done}<p class="muted">{t("practicalTeamTellWaiting")}</p>{/if}
      {#if reply.error}<p class="failed" role="alert">{reply.error}</p>{/if}
    </div>
  {/if}
</Card>

<style>
  .help {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  form {
    display: grid;
    grid-template-columns: minmax(10rem, 16rem) minmax(0, 1fr) auto;
    align-items: end;
    gap: 8px 10px;
  }

  @media (max-width: 720px) {
    form {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .reply {
    margin-top: 12px;
    padding: 10px 12px;
    border-left: 3px solid var(--primary);
    border-radius: var(--radius-md);
    background: var(--bg);
  }

  .who {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .who :global(.btn) {
    margin-left: auto;
  }

  .said {
    margin: 6px 0 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .muted {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .failed {
    margin: 6px 0 0;
    color: var(--danger);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
</style>
