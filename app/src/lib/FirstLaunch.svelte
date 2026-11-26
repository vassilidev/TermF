<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import FolderPlus from "@lucide/svelte/icons/folder-plus";
  import FolderSync from "@lucide/svelte/icons/folder-sync";
  import Laptop from "@lucide/svelte/icons/laptop";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import MessageSquareText from "@lucide/svelte/icons/message-square-text";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import SquareTerminal from "@lucide/svelte/icons/square-terminal";
  import UsersRound from "@lucide/svelte/icons/users-round";
  import EnvView from "$lib/EnvView.svelte";
  import SyncView from "$lib/SyncView.svelte";
  import VaultRecoveryKey from "$lib/VaultRecoveryKey.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Wizard from "$lib/ui/Wizard.svelte";
  import { loadEnv } from "$lib/devenv.svelte";
  import { isMac } from "$lib/files.svelte";
  import { t } from "$lib/i18n";
  import { introSteps, isSetup, nextStep, putOff, type Done, type IntroStep, type Machine } from "$lib/intro-logic";
  import { keys } from "$lib/keymap.svelte";
  import { talkCto } from "$lib/home.svelte";
  import { newProject } from "$lib/newproject.svelte";
  import { installs, osOf, type Tools } from "$lib/release-logic";
  import { app } from "$lib/state.svelte";
  import { remoteText, sync } from "$lib/sync.svelte";
  import { SETUP_LABEL, chooseMachine, endTour, loadSetup, pauseTour, setup, setupDone, tour } from "$lib/tour.svelte";
  import { billingText } from "$lib/usage.svelte";
  import type { Billing } from "$lib/usage-logic";
  import { failed, setStatus, vault, type VaultStatus } from "$lib/vault.svelte";

  type Key = Parameters<typeof t>[0];
  type Line = { state: "now" | "ok" | "warn" | "fail"; title: string; text: string };

  let dialog: HTMLDialogElement;
  const projects = $derived(app.workspace?.projects ?? []);
  const os = osOf(navigator.platform);
  const none: Done = { env: false, vault: false, sync: false };

  /** What was set up when the introduction opened, left out of it; null until read. */
  let opened = $state<Done | null>(null);
  let loaded = $state(false);
  const done = $derived(setupDone() ?? none);
  const ids = $derived(introSteps(setup.machine, opened ?? none, tour.only));
  const id = $derived(tour.step ?? ids[0]);
  const current = $derived(Math.max(0, ids.indexOf(id)));

  /** Claude Code, checked live, one line after the other: installed, signed in, its agents followed. */
  let checks = $state<(Line | null)[]>([null, null, null]);
  let checking = $state(false);
  let copied = $state(false);
  let version = $state("");
  /** Entries of the environment kept from the detection, here. */
  let kept = $state(0);
  let password = $state("");
  let again = $state("");
  let creating = $state(false);
  let created = $state(false);
  let vaultError = $state("");

  const label: Record<IntroStep, Key> = {
    welcome: "wsIntroWelcome",
    claude: "wsIntroClaude",
    ...SETUP_LABEL,
    project: "wsIntroProject",
    cto: "wsIntroCto",
    help: "wsIntroHelp",
  };
  function answer(step: IntroStep): string {
    if (isSetup(step) && !done[step]) return t("obmSkipped");
    if (step === "welcome") return t("wsIntroWelcomeDone");
    if (step === "claude") return version;
    if (step === "env") return kept ? t("myenvKept", { n: kept }) : "";
    if (step === "vault") return t(created ? "obmVaultCreated" : "obmVaultReady");
    if (step === "sync") return app.sync?.remote ? remoteText(app.sync.remote) : "";
    if (step === "project") return projects[0]?.name ?? "";
    return "";
  }
  const steps = $derived(ids.map((s) => ({ id: s, label: t(label[s]), answer: answer(s), skipped: isSetup(s) && !done[s] })));

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

  /** The vault rides with the sync once there is one, and on a second machine it is what comes over. */
  function suggest() {
    sync.suggested = setup.machine === "other" || done.vault ? "vault" : null;
  }
  if (tour.step === "sync") suggest();

  onMount(() => {
    dialog.showModal();
    loadSetup(true).then(() => (loaded = true));
    return () => dialog.open && dialog.close();
  });

  // Read once: the facts may come in one after the other.
  $effect(() => {
    if (loaded && !opened) opened = setupDone();
  });

  // Resumed for a step done meanwhile: the next one, or nothing left to do.
  $effect(() => {
    if (!opened || ids.includes(id)) return;
    untrack(() => {
      if (ids.length) tour.step = ids[0];
      else close();
    });
  });

  $effect(() => {
    if (id === "claude" && checks[0] === null && !checking) check();
  });

  // Enter continues: the keyboard waits on Continue, on the page while the checks run, or on the machine's choice;
  // the vault's password is typed at once.
  $effect(() => {
    void id;
    const busy = checking;
    tick().then(() =>
      (
        dialog.querySelector<HTMLElement>(".vault-form input") ??
        dialog.querySelector<HTMLElement>("footer .primary:not(:disabled)") ??
        (busy ? dialog : dialog.querySelector<HTMLElement>(".machine input"))
      )?.focus(),
    );
  });

  /** The vault opened here locks once left, as from its own view, unless the user turned that off. */
  function leave() {
    if (id === "vault" && vault.status?.unlocked) invoke<VaultStatus>("vault_leave").then(setStatus, () => {});
  }

  function show(to: IntroStep) {
    leave();
    if (to === "sync") suggest();
    // The sync may have brought the list.
    if (to === "env") loadEnv(false);
    tour.step = to;
  }

  /** Finished, skipped or left for the CTO: what is not set up waits to be resumed. */
  function close() {
    leave();
    endTour(putOff(setup.passed, ids, done));
  }

  async function next() {
    // Joining the sync may have brought the vault.
    if (id === "sync") await invoke<VaultStatus>("vault_status").then(setStatus, () => {});
    const to = nextStep(ids, id, setupDone() ?? none);
    if (to) show(to);
    else close();
  }

  function createProject() {
    pauseTour();
    newProject();
  }

  function cto() {
    close();
    talkCto();
  }

  async function copyInstall(command: string) {
    await navigator.clipboard.writeText(command).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 1500);
  }

  /** The recovery key made here is on screen. */
  let keyShown = $state(false);

  async function newKey() {
    vault.recoveryKey = { key: await invoke<string>("vault_recovery_new"), reason: "created" };
    keyShown = true;
    setStatus(await invoke<VaultStatus>("vault_status"));
  }

  // The key's own Continue, once it is stored, goes on to the next step.
  $effect(() => {
    if (!keyShown || vault.recoveryKey) return;
    keyShown = false;
    if (untrack(() => id) === "vault") untrack(next);
  });

  async function createVault(e: SubmitEvent) {
    e.preventDefault();
    vaultError = "";
    if (password !== again) {
      vaultError = t("vaultPasswordsDiffer");
      return;
    }
    creating = true;
    try {
      setStatus(await invoke<VaultStatus>("vault_init", { password, remember: false }));
      created = true;
      await newKey();
    } catch (e) {
      vaultError = failed(e);
    } finally {
      password = again = "";
      creating = false;
    }
  }

  async function retryKey() {
    vaultError = "";
    creating = true;
    await newKey().catch((e) => (vaultError = failed(e)));
    creating = false;
  }

  const last = $derived(current === ids.length - 1);
  const continueLabel = $derived(
    (isSetup(id) && !done[id]) || (id === "project" && !projects.length)
      ? t("wsIntroLater")
      : last
        ? t(tour.only ? "obmFinish" : "wsIntroStart")
        : undefined,
  );
  const canContinue = $derived(
    id === "welcome"
      ? !!setup.machine || !!opened?.sync
      : id === "claude"
        ? !checking
        : id === "vault"
          ? !creating && !vault.recoveryKey
          : id === "sync"
            ? !sync.recoveryKey && !sync.busy
            : true,
  );
  const machines: [Machine, typeof Laptop, Key, Key][] = [
    ["first", Laptop, "obmMachineFirst", "obmMachineFirstText"],
    ["other", FolderSync, "obmMachineOther", "obmMachineOtherText"],
  ];
</script>

<!-- Escape skips it, as the button does: the introduction is remembered as seen either way. -->
<dialog bind:this={dialog} class="intro" tabindex="-1" aria-labelledby="intro-title" oncancel={(e) => (e.preventDefault(), close())}>
  <Wizard
    title={t("wsIntroTitle")}
    {steps}
    {current}
    onstep={(i) => show(ids[i])}
    onback={() => current > 0 && show(ids[current - 1])}
    oncontinue={next}
    {canContinue}
    busy={id === "claude" && checking}
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
      <button class="btn quiet skip" onclick={close}>{t("wsIntroSkip")}</button>
    {/snippet}

    {#if id === "welcome"}
      <h1 id="intro-title" class="h-display">{t("wsIntroWelcomeTitle")}</h1>
      <p class="lead">{t("wsIntroWelcomeLead")}</p>
      <ul class="points">
        <li><span class="ic"><SquareTerminal size={16} aria-hidden="true" /></span><div><b>{t("wsIntroPoint1")}</b><p>{t("wsIntroPoint1Text")}</p></div></li>
        <li><span class="ic"><UsersRound size={16} aria-hidden="true" /></span><div><b>{t("wsIntroPoint2")}</b><p>{t("wsIntroPoint2Text")}</p></div></li>
        <li><span class="ic"><Lock size={16} aria-hidden="true" /></span><div><b>{t("wsIntroPoint3")}</b><p>{t("wsIntroPoint3Text")}</p></div></li>
      </ul>
      {#if !opened?.sync}
        <fieldset class="machine">
          <legend>{t("obmMachine")}</legend>
          {#each machines as [m, Icon, title, text] (m)}
            <label class="choice" class:on={setup.machine === m}>
              <input type="radio" name="intro-machine" value={m} checked={setup.machine === m} onchange={() => chooseMachine(m)} />
              <span class="ic"><Icon size={16} aria-hidden="true" /></span>
              <span class="what"><b>{t(title)}</b><span>{t(text)}</span></span>
            </label>
          {/each}
        </fieldset>
      {/if}
    {:else if id === "claude"}
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
    {:else if id === "env"}
      <h1 id="intro-title" class="h-display">{t("obmEnvTitle")}</h1>
      <p class="lead">{t("myenvEmptyHelp")}</p>
      {#if kept}<p class="done-line" role="status"><Check size={16} aria-hidden="true" />{t("myenvKept", { n: kept })}</p>{/if}
      <div class="embed"><EnvView only onkept={(n) => (kept += n)} /></div>
    {:else if id === "vault"}
      <h1 id="intro-title" class="h-display">{t("obmVaultTitle")}</h1>
      {#if vault.recoveryKey}
        <div class="embed key"><VaultRecoveryKey /></div>
      {:else if done.vault}
        <p class="lead">{t("vaultCreateText")}</p>
        <p class="done-line" role="status"><Check size={16} aria-hidden="true" />{t(created ? "obmVaultCreated" : "obmVaultReady")}</p>
        {#if created && !vault.status?.recovery}
          <button class="btn" onclick={retryKey} disabled={creating}>
            {#if creating}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}{t("vault2RecoveryCreate")}
          </button>
          {#if vaultError}<p class="notice error" role="alert">{vaultError}</p>{/if}
        {/if}
      {:else if setup.machine === "other" && !done.sync && ids.includes("sync")}
        <p class="lead">{t("vaultCreateText")}</p>
        <p class="note"><FolderSync size={14} aria-hidden="true" /><span>{t("obmVaultFromSync")}</span></p>
        <button class="btn primary from-sync" onclick={() => show("sync")}><FolderSync size={14} aria-hidden="true" />{t("obmVaultJoinSync")}</button>
      {:else}
        <p class="lead">{t("vaultCreateText")}</p>
        <form class="vault-form" onsubmit={createVault}>
          <label class="labelled">
            {t("vaultNewMasterPassword")}
            <input class="field" type="password" autocomplete="new-password" minlength={8} bind:value={password} required />
          </label>
          <label class="labelled">
            {t("vaultPasswordAgain")}
            <input class="field" type="password" autocomplete="new-password" bind:value={again} required />
          </label>
          {#if vaultError}<p class="notice error" role="alert">{vaultError}</p>{/if}
          <div>
            <button type="submit" class="btn primary" disabled={creating || !password}>
              {#if creating}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}{t("vaultCreate")}
            </button>
          </div>
          <p class="hint">{t("vaultCreateWarning")}</p>
        </form>
      {/if}
    {:else if id === "sync"}
      <h1 id="intro-title" class="h-display">{t(setup.machine === "other" ? "obmSyncJoinTitle" : "obmSyncTitle")}</h1>
      {#if done.sync && !sync.recoveryKey}
        <p class="lead">{t("syncIntro")}</p>
        <p class="done-line" role="status">
          <Check size={16} aria-hidden="true" /><span>{t("obmSyncDone", { remote: app.sync?.remote ? remoteText(app.sync.remote) : "" })}</span>
        </p>
        {#if app.sync?.missingRepos.length}
          <p class="note"><FolderSync size={14} aria-hidden="true" /><span>{t("obmSyncRepos", { n: app.sync.missingRepos.length })}</span></p>
        {/if}
      {:else}
        <div class="embed"><SyncView embedded /></div>
      {/if}
    {:else if id === "project"}
      <h1 id="intro-title" class="h-display">{t("wsIntroProjectTitle")}</h1>
      <p class="lead">{t("wsIntroProjectLead")}</p>
      {#if projects.length}
        <p class="have">{t("wsIntroHaveProjects", { count: projects.length, names: projects.slice(0, 3).map((p) => p.name).join(", ") })}</p>
        <button class="btn" onclick={createProject}><FolderPlus size={14} aria-hidden="true" />{t("wsIntroAnotherProject")}</button>
      {:else}
        <button class="btn primary lg" onclick={createProject}><FolderPlus size={16} aria-hidden="true" />{t("wsIntroCreateProject")}</button>
      {/if}
    {:else if id === "cto"}
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

  .points .ic,
  .choice .ic {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .machine {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin: 28px 0 0;
    padding: 0;
    border: 0;
  }

  .machine legend {
    margin-bottom: 10px;
    padding: 0;
    font-weight: 600;
  }

  .choice {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: border-color var(--dur-fast) ease, background-color var(--dur-fast) ease;
  }

  .choice:hover {
    background: var(--hover);
  }

  .choice.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .choice:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* The card is the control; the radio stays for the keyboard and screen readers. */
  .choice input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .what {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .what b {
    color: var(--text);
    font-size: var(--text-md);
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

  .done-line {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 20px 0 0;
    font-weight: 500;
  }

  .done-line :global(svg) {
    flex-shrink: 0;
    color: var(--done);
  }

  .embed {
    margin-top: 20px;
  }

  /* The key's own screen, without the margins it takes as a page of the vault. */
  .key :global(.recovery) {
    max-width: none;
    margin: 0;
    padding: 0;
  }

  .vault-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 420px;
    margin-top: 24px;
  }

  .from-sync {
    margin-top: 16px;
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
