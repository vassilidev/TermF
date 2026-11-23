<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import UsersRound from "@lucide/svelte/icons/users-round";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Wizard from "$lib/ui/Wizard.svelte";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { keys } from "$lib/keymap.svelte";
  import { talkCto } from "$lib/home.svelte";
  import { newProject } from "$lib/newproject.svelte";
  import { installs, osOf, type Tools } from "$lib/release-logic";
  import { app } from "$lib/state.svelte";
  import { endTour, pauseTour, tour } from "$lib/tour.svelte";
  import { billingText } from "$lib/usage.svelte";
  import type { Billing } from "$lib/usage-logic";

  type Key = Parameters<typeof t>[0];
  type Line = { state: "now" | "ok" | "warn" | "fail"; title: string; text: string };

  let dialog: HTMLDialogElement;
  const projects = $derived(app.workspace?.projects ?? []);
  const os = osOf(navigator.platform);

  /** Claude Code, checked live, one line after the other: installed, signed in, its agents followed. */
  let checks = $state<(Line | null)[]>([null, null, null]);
  let checking = $state(false);
  let copied = $state(false);
  let version = $state("");
  const answers = $derived([t("wsIntroWelcomeDone"), version, projects[0]?.name ?? "", "", ""]);
  const steps = $derived(
    (
      [
        ["welcome", "wsIntroWelcome"],
        ["claude", "wsIntroClaude"],
        ["project", "wsIntroProject"],
        ["cto", "wsIntroCto"],
        ["help", "wsIntroHelp"],
      ] as [string, Key][]
    ).map(([id, label], i) => ({ id, label: t(label), answer: answers[i] })),
  );
  const step = $derived(tour.step ?? 0);

  async function check() {
    checking = true;
    checks = [{ state: "now", title: t("wsCheckLooking"), text: "" }, null, null];
    let tools: Tools | null = null;
    try {
      tools = await invoke<Tools>("tools_check");
    } catch {
      tools = null;
    }
    const install = tools?.claude.install;
    checks[0] = !install
      ? { state: "fail", title: t("wsCheckMissing"), text: t("wsCheckMissingText") }
      : tools!.claude.tooOld
        ? { state: "warn", title: t("wsCheckOld"), text: t("releaseTooOld", { version: install.version ?? "", minimum: tools!.claude.minimum }) }
        : { state: "ok", title: t("wsCheckInstalled"), text: t("wsCheckInstalledText", { version: install.version ?? "", path: install.path }) };
    version = install?.version ? t("wsCheckVersion", { version: install.version }) : "";
    checks[1] = { state: "now", title: t("wsCheckAccount"), text: "" };
    let billing: Billing = { mode: "unknown", plan: null };
    if (install) billing = await invoke<Billing>("claude_billing", { refresh: true }).catch(() => billing);
    checks[1] =
      billing.mode === "none"
        ? { state: "fail", title: t("wsCheckSignedOut"), text: t("wsCheckSignedOutText") }
        : billing.mode === "unknown"
          ? { state: "warn", title: t("wsCheckAccountUnknown"), text: billingText(billing) }
          : { state: "ok", title: t("wsCheckSignedIn"), text: billingText(billing) };
    const api = app.workspace?.apiError;
    checks[2] = api
      ? { state: "warn", title: t("wsCheckTrackingOff"), text: api }
      : { state: "ok", title: t("wsCheckTracking"), text: t("wsCheckTrackingText") };
    checking = false;
  }

  onMount(() => {
    dialog.showModal();
    return () => dialog.open && dialog.close();
  });

  $effect(() => {
    if (step === 1 && checks[0] === null && !checking) check();
  });

  // Enter continues: the keyboard waits on Continue, or on the page while the checks run.
  $effect(() => {
    void step;
    const busy = checking;
    tick().then(() => (dialog.querySelector<HTMLElement>("footer .primary:not(:disabled)") ?? (busy ? dialog : null))?.focus());
  });

  function go(to: number) {
    if (to >= steps.length) endTour();
    else if (to >= 0) tour.step = to;
  }

  function createProject() {
    pauseTour();
    newProject();
  }

  function cto() {
    endTour();
    talkCto();
  }

  async function copyInstall(command: string) {
    await navigator.clipboard.writeText(command).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  const continueLabel = $derived(
    step === 4 ? t("wsIntroStart") : step === 2 && !projects.length ? t("wsIntroLater") : undefined,
  );
</script>

<!-- Escape skips it, as the button does: the introduction is remembered as seen either way. -->
<dialog bind:this={dialog} class="intro" tabindex="-1" aria-labelledby="intro-title" oncancel={(e) => (e.preventDefault(), endTour())}>
  <Wizard
    title={t("wsIntroTitle")}
    {steps}
    current={step}
    onstep={go}
    onback={() => go(step - 1)}
    oncontinue={() => go(step + 1)}
    canContinue={step !== 1 || !checking}
    busy={step === 1 && checking}
    {continueLabel}
  >
    {#snippet head()}
      <div class="brand">
        <svg viewBox="64 64 896 896" aria-hidden="true">
          <defs>
            <linearGradient id="intro-mark" x1="100" y1="100" x2="924" y2="924" gradientUnits="userSpaceOnUse">
              <stop offset="0" stop-color="#5d5fea" /><stop offset="1" stop-color="#3f3fc6" />
            </linearGradient>
          </defs>
          <rect x="100" y="100" width="824" height="824" rx="230" fill="url(#intro-mark)" />
          <g fill="none" stroke-linecap="round" stroke-linejoin="round">
            <path d="M541 388 691 512 541 636" stroke="#fff" stroke-width="88" />
            <path d="M329 244 435 332 329 420" stroke="#62cf8e" stroke-width="80" />
            <path d="M329 604 435 692 329 780" stroke="#f2b03d" stroke-width="80" />
          </g>
        </svg>
        TermF
      </div>
    {/snippet}
    {#snippet tail()}{t("wsIntroTail")}{/snippet}
    {#snippet skip()}
      <button class="btn quiet skip" onclick={endTour}>{t("wsIntroSkip")}</button>
    {/snippet}

    {#if step === 0}
      <h1 id="intro-title" class="h-display">{t("wsIntroWelcomeTitle")}</h1>
      <p class="lead">{t("wsIntroWelcomeLead")}</p>
      <ul class="points">
        <li><span class="ic"><SquareTerminal size={16} aria-hidden="true" /></span><div><b>{t("wsIntroPoint1")}</b><p>{t("wsIntroPoint1Text")}</p></div></li>
        <li><span class="ic"><UsersRound size={16} aria-hidden="true" /></span><div><b>{t("wsIntroPoint2")}</b><p>{t("wsIntroPoint2Text")}</p></div></li>
        <li><span class="ic"><Lock size={16} aria-hidden="true" /></span><div><b>{t("wsIntroPoint3")}</b><p>{t("wsIntroPoint3Text")}</p></div></li>
      </ul>
    {:else if step === 1}
      <h1 id="intro-title" class="h-display">{t("wsIntroClaudeTitle")}</h1>
      <p class="lead">{t("wsIntroClaudeLead")}</p>
      <ul class="checks" aria-live="polite">
        {#each checks as c, i (i)}
          <li class="ck {c?.state ?? 'todo'}">
            <span class="ic" aria-hidden="true">
              {#if c?.state === "ok"}<Check size={14} />{:else if c?.state === "now"}<LoaderCircle class="spin" size={14} />{:else if c}<CircleAlert size={14} />{/if}
            </span>
            {#if c}
              <div class="main-t">
                <div class="t" class:shimmer={c.state === "now"}>{c.title}</div>
                {#if c.text}<div class="s">{c.text}</div>{/if}
                {#if i === 0 && c.state === "fail" && installs.claude.command[os]}
                  {@const command = installs.claude.command[os]!}
                  <div class="command">
                    <code>{command}</code>
                    <button class="btn sm quiet" onclick={() => copyInstall(command)}>
                      {#if copied}<Check size={14} aria-hidden="true" />{t("releaseCopied")}{:else}<Copy size={14} aria-hidden="true" />{t("releaseCopy")}{/if}
                    </button>
                  </div>
                {/if}
              </div>
            {:else}
              <div class="main-t"><Skeleton rows={2} delay={0} /></div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if !checking && checks.some((c) => c && c.state !== "ok")}
        <button class="btn sm again" onclick={check}><RefreshCw size={14} aria-hidden="true" />{t("releaseRecheck")}</button>
      {/if}
      <p class="note"><Lock size={14} aria-hidden="true" /><span>{t("wsIntroNothingWritten")}</span></p>
    {:else if step === 2}
      <h1 id="intro-title" class="h-display">{t("wsIntroProjectTitle")}</h1>
      <p class="lead">{t("wsIntroProjectLead")}</p>
      {#if projects.length}
        <p class="have">{t("wsIntroHaveProjects", { count: projects.length, names: projects.slice(0, 3).map((p) => p.name).join(", ") })}</p>
        <button class="btn" onclick={createProject}><FolderPlus size={14} aria-hidden="true" />{t("wsIntroAnotherProject")}</button>
      {:else}
        <button class="btn primary lg" onclick={createProject}><FolderPlus size={16} aria-hidden="true" />{t("wsIntroCreateProject")}</button>
      {/if}
    {:else if step === 3}
      <h1 id="intro-title" class="h-display">{t("wsIntroCtoTitle")}</h1>
      <p class="lead">{t("wsIntroCtoLead")}</p>
      <p class="keys"><kbd>{keys.cto}</kbd><span>{t("wsIntroCtoKey")}</span></p>
      <button class="btn" onclick={cto} disabled={app.project === null} title={app.project === null ? t("wsIntroCtoNeedsProject") : undefined}>
        <MessageSquareText size={14} aria-hidden="true" />{t("wsIntroCtoNow")}
      </button>
      {#if app.project === null}<p class="hint">{t("wsIntroCtoNeedsProject")}</p>{/if}
    {:else}
      <h1 id="intro-title" class="h-display">{t("wsIntroHelpTitle")}</h1>
      <p class="lead">{t("wsIntroHelpLead")}</p>
      <dl class="where">
        <dt><kbd>{keys.palette}</kbd></dt>
        <dd><b>{t("palette")}</b>{t("wsIntroHelpPalette")}</dd>
        <dt><kbd>{isMac ? "⌘I" : "Ctrl+I"}</kbd></dt>
        <dd><b>{t("wsIntroHelpAi")}</b>{t("wsIntroHelpAiText")}</dd>
        <dt><kbd>{keys.help}</kbd></dt>
        <dd><b>{t("t6bHelp")}</b>{t("wsIntroHelpSheet")}</dd>
      </dl>
    {/if}
  </Wizard>
</dialog>

<style>
  .intro {
    width: min(960px, calc(100vw - 48px));
    max-width: none;
    height: min(620px, calc(100vh - 48px));
    max-height: none;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-3);
  }

  .intro[open] {
    animation: enter var(--dur-slow) var(--ease-out);
  }

  .intro::backdrop {
    background: rgb(10 10 20 / 0.45);
  }

  @keyframes enter {
    from {
      opacity: 0;
      transform: scale(0.97);
    }
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px;
    font-size: var(--text-lg);
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .brand svg {
    width: 28px;
    height: 28px;
  }

  .points,
  .checks {
    display: flex;
    flex-direction: column;
    margin: 24px 0 0;
    padding: 0;
    list-style: none;
  }

  .points {
    gap: 18px;
  }

  .points li {
    display: flex;
    gap: 14px;
  }

  .points p {
    margin: 2px 0 0;
    color: var(--soft);
  }

  .points .ic {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .checks {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }

  .ck {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    border-top: 1px solid var(--border);
  }

  .ck:first-child {
    border-top: 0;
  }

  .ck .ic {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
  }

  .ck.ok .ic {
    background: var(--done-soft);
    color: var(--done);
  }

  .ck.now .ic {
    background: var(--working-soft);
    color: var(--working);
  }

  .ck.warn .ic {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .ck.fail .ic {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .ck.todo .ic {
    border: 1.5px dashed var(--border-strong);
  }

  .main-t {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .t {
    font-weight: 600;
    line-height: 24px;
  }

  .s {
    color: var(--muted);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .command {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }

  .command code {
    flex: 1;
    min-width: 0;
    padding: 4px 8px;
    border-radius: var(--radius);
    background: var(--well);
    font: var(--text-sm) var(--font-mono);
    overflow-wrap: anywhere;
  }

  .again {
    margin-top: 12px;
  }

  .note {
    display: flex;
    gap: 10px;
    margin: 16px 0 0;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    background: var(--well);
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .note :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--muted);
  }

  .have {
    margin: 20px 0 12px;
    color: var(--soft);
  }

  .lead + .btn,
  .lead + .keys {
    margin-top: 24px;
  }

  .keys {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
    color: var(--soft);
  }

  .hint {
    margin: 8px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .where {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 16px 16px;
    align-items: baseline;
    margin: 24px 0 0;
  }

  .where dt {
    justify-self: start;
  }

  .where dd {
    display: flex;
    flex-direction: column;
    margin: 0;
    color: var(--soft);
  }

  .where dd b {
    color: var(--text);
  }

  .skip {
    margin-right: 8px;
  }
</style>
