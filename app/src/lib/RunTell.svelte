<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy } from "svelte";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import MessagesSquare from "@lucide/svelte/icons/messages-square";
  import Send from "@lucide/svelte/icons/send";
  import { stepName, ticketKey } from "$lib/TeamRunDetail.svelte";
  import { type AgentSession, ctoOf, showConversation, upsert } from "$lib/agents.svelte";
  import type { StoredEvent } from "$lib/conversation-logic";
  import { t } from "$lib/i18n";
  import type { TeamRun } from "$lib/team-run-logic";

  /** "Tell the team something": a message to the CTO, who leads the run, or to the agent of one of its steps. */
  let { run }: { run: TeamRun } = $props();

  const LEAD = "lead";
  const agents = $derived(
    run.steps
      .filter((s) => s.session !== null && s.kind !== "foreach")
      .sort((a, b) => Number(b.state === "running") - Number(a.state === "running")),
  );
  let to = $state(LEAD);
  let draft = $state("");
  let busy = $state(false);
  let error = $state("");
  /** The conversation that answers, what it said since the message, and whether its turn ended. */
  let reply = $state<{ session: number; who: string; text: string; heard: boolean; done: boolean } | null>(null);

  const stop = listen<StoredEvent>("agent-event", ({ payload: e }) => {
    if (!reply || e.session !== reply.session) return;
    if (e.kind === "user") reply.heard = true;
    else if (!reply.heard) return;
    else if (e.kind === "delta") reply.text += e.text;
    else if (e.kind === "text" && !e.parent) reply.text = e.text;
    else if (e.kind === "turn_end") reply.done = true;
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
        who = "CTO";
        message = t("practicalTeamTellAbout", { run: run.run, ticket: ticketKey(run.target.ticket), title: run.target.title, text });
      } else {
        const step = agents.find((s) => s.key === to);
        if (!step || step.session === null) return;
        session = step.session;
        who = `${step.role ?? ""} · ${stepName(step)}`;
      }
      reply = { session, who, text: "", heard: false, done: false };
      await invoke("agent_send", { id: session, text: message });
      draft = "";
    } catch (err) {
      reply = null;
      error = String(err);
    } finally {
      busy = false;
    }
  }
</script>

<section class="panel" aria-labelledby="run-tell">
  <h3 id="run-tell">{t("practicalTeamTell")}</h3>
  <p class="help">{t("practicalTeamTellHelp")}</p>
  <form onsubmit={send}>
    <label class="to">
      <span>{t("practicalTeamTellTo")}</span>
      <select bind:value={to} disabled={busy}>
        <option value={LEAD}>{t("practicalTeamTellLead")}</option>
        {#each agents as step (step.key)}
          <option value={step.key}>{step.role ?? ""} · {stepName(step)}</option>
        {/each}
      </select>
    </label>
    <input
      type="text"
      bind:value={draft}
      placeholder={t("practicalTeamTellPlaceholder")}
      aria-label={t("practicalTeamTell")}
      disabled={busy}
    />
    <button class="btn primary sm" type="submit" disabled={busy || !draft.trim()}>
      <Send size={14} aria-hidden="true" />{t("practicalTeamTellSend")}
    </button>
  </form>
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  {#if reply}
    <div class="reply" aria-live="polite">
      <div class="who">
        <b>{reply.who}</b>
        {#if !reply.done}<LoaderCircle size={14} class="spin" aria-label={t("practicalTeamTellWaiting")} />{/if}
        <button class="btn link sm" type="button" onclick={() => showConversation(reply!.session)}>
          <MessagesSquare size={14} aria-hidden="true" />{t("practicalTeamTellOpen")}
        </button>
      </div>
      {#if reply.text}<p class="said">{reply.text}</p>{:else if !reply.done}<p class="muted">{t("practicalTeamTellWaiting")}</p>{/if}
    </div>
  {/if}
</section>

<style>
  .panel {
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  h3 {
    margin: 0 0 4px;
    font-size: var(--text-base);
    font-weight: 650;
  }

  .help {
    margin: 0 0 10px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  form {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
  }

  .to {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--text-sm);
    color: var(--muted);
  }

  .to select {
    max-width: 16rem;
  }

  input {
    min-width: 0;
  }

  @container (max-width: 520px) {
    form {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .reply {
    margin-top: 12px;
    padding: 10px 12px;
    border-left: 3px solid var(--primary);
    border-radius: var(--radius);
    background: var(--bg);
  }

  .who {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .who .btn {
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
</style>
