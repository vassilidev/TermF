<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Bot from "@lucide/svelte/icons/bot";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import Send from "@lucide/svelte/icons/send";
  import CiStatus from "$lib/CiStatus.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { t } from "$lib/i18n";
  import { onScreen, type Session } from "$lib/state.svelte";
  import type { Run } from "$lib/tickets.svelte";
  import { duration, outcome, type AgentMessage, type Job, type RunDetail, type WorkflowRun } from "$lib/ci.svelte";
  import { failedJobs } from "$lib/ci-logic";

  let { run, session }: { run: Run; session: Session | undefined } = $props();

  /** null while they load. */
  let checks = $state<WorkflowRun[] | null>(null);
  let error = $state("");
  /** A failed CI run → its failed jobs. */
  let failed = $state<Record<number, Job[]>>({});
  let message = $state<{ job: Job; message: AgentMessage } | null>(null);
  let busy = $state<number | "send" | null>(null);
  let stepError = $state("");
  let sent = $state(false);

  let request = 0;

  async function load() {
    const r = ++request;
    error = "";
    try {
      const found = await invoke<WorkflowRun[]>("ci_checks", { run: run.id });
      // By the check's own repository: the pull request may not be in the folder's first remote.
      const details = await Promise.all(
        found
          .filter((c) => outcome(c) === "failed")
          .map((c) => invoke<RunDetail>("ci_run", { repo: c.repo, id: c.id })),
      );
      if (r !== request) return;
      [checks, failed] = [found, failedJobs(details)];
    } catch (e) {
      if (r === request) error = String(e);
    }
  }

  $effect(() => {
    void run.prUrl;
    void run.updated;
    untrack(load);
  });

  // While a check runs and the tickets or the run page are on screen.
  $effect(() => {
    if (!(onScreen("tickets") || onScreen("run")) || !checks?.some((c) => c.status !== "completed")) return;
    const timer = setInterval(() => document.visibilityState === "visible" && load(), 30_000);
    return () => clearInterval(timer);
  });

  async function prepare(job: Job) {
    [busy, stepError, sent] = [job.id, "", false];
    try {
      message = { job, message: await invoke<AgentMessage>("ci_failure", { run: run.id, job: job.id }) };
    } catch (e) {
      stepError = String(e);
    } finally {
      busy = null;
    }
  }

  async function send() {
    if (!message || !session) return;
    [busy, stepError] = ["send", ""];
    try {
      await invoke("ci_send", { session: session.id, text: message.message.prompt });
      [message, sent] = [null, true];
    } catch (e) {
      stepError = String(e);
    } finally {
      busy = null;
    }
  }

  const failedStep = (job: Job) => job.steps.find((s) => outcome(s) === "failed")?.name;
</script>

<section class="checks" aria-labelledby="checks-title">
  <div class="head">
    <h3 id="checks-title">{t("ciChecks")}</h3>
    <button class="icon-btn sm" onclick={load} aria-label={t("ciRefresh")} title={t("ciRefresh")}>
      <RefreshCw size={13} aria-hidden="true" />
    </button>
  </div>
  {#if error}
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{error}</p>
  {:else if !checks}
    <Skeleton label={t("ciLoading")} />
  {:else if !checks.length}
    <p class="muted line">{t("ciChecksNone")}</p>
  {:else}
    <ul>
      {#each checks as check (check.id)}
        <li>
          <div class="line">
            <CiStatus item={check} compact />
            <span class="name">{check.workflow}</span>
            <span class="muted">{check.event} · {duration(check.started, check.status === "completed" ? check.updated : null)}</span>
          </div>
          {#each failed[check.id] ?? [] as job (job.id)}
            {@const step = failedStep(job)}
            <div class="job">
              <span>{step ? t("ciFailedAt", { job: job.name, step }) : job.name}</span>
              <button class="btn sm" onclick={() => prepare(job)} disabled={busy !== null}>
                {#if busy === job.id}<LoaderCircle size={13} aria-hidden="true" class="spin" />{:else}<Bot size={13} aria-hidden="true" />{/if}
                {t("ciSendFailure")}
              </button>
            </div>
          {/each}
        </li>
      {/each}
    </ul>
  {/if}
  {#if stepError && !message}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{stepError}</p>{/if}
  {#if message}
    <div class="ask" role="group" aria-labelledby="ci-message-title">
      <h4 id="ci-message-title">{t("ciMessageTitle", { job: message.job.name })}</h4>
      <p class="muted">{t("ciMessageHelp")}</p>
      <pre>{message.message.text}</pre>
      {#if !session}<p class="muted">{t("runNoSession")}</p>{/if}
      {#if stepError}<p class="notice error"><CircleAlert size={14} aria-hidden="true" />{stepError}</p>{/if}
      <div class="buttons">
        <button class="btn sm" onclick={() => (message = null)}>{t("cancel")}</button>
        <button class="btn sm primary" onclick={send} disabled={!session || busy !== null}>
          {#if busy === "send"}<LoaderCircle size={13} aria-hidden="true" class="spin" />{:else}<Send size={13} aria-hidden="true" />{/if}
          {t("ciSend")}
        </button>
      </div>
    </div>
  {/if}
  {#if sent}<p class="ok"><Check size={14} aria-hidden="true" />{t("ciSent")}</p>{/if}
</section>

<style>
  .checks {
    margin: 0 0 20px;
    font-size: var(--text-sm);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
  }

  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  h4 {
    margin: 0;
    font-size: var(--text-sm);
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    padding: 5px 0;
    border-bottom: 1px solid var(--border);
  }

  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }

  .name {
    font-weight: 500;
  }

  .job {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: 6px 0 2px 23px;
    color: var(--danger);
  }

  .ask {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .ask p {
    margin: 0;
  }

  pre {
    max-height: 260px;
    margin: 0;
    padding: 10px;
    overflow: auto;
    border-radius: var(--radius);
    background: var(--chrome);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.5;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .ok {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 10px 0 0;
    color: var(--done);
    font-weight: 600;
  }
</style>
