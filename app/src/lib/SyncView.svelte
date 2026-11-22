<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Copy from "@lucide/svelte/icons/copy";
  import CopyCheck from "@lucide/svelte/icons/copy-check";
  import Download from "@lucide/svelte/icons/download";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import FolderOpen from "@lucide/svelte/icons/folder-open";
  import FolderSync from "@lucide/svelte/icons/folder-sync";
  import GitBranch from "@lucide/svelte/icons/git-branch";
  import GitMergeConflict from "@lucide/svelte/icons/git-merge-conflict";
  import Info from "@lucide/svelte/icons/info";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import Laptop from "@lucide/svelte/icons/laptop";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import Power from "@lucide/svelte/icons/power";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import SyncConflict from "$lib/SyncConflict.svelte";
  import { ago, t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";
  import {
    currentReport,
    keepReport,
    remoteText,
    scopes,
    summary,
    sync,
    syncCall,
    syncNow,
    type Device,
    type Remote,
    type Report,
    type Scope,
  } from "$lib/sync.svelte";

  const status = $derived(app.sync);
  const ready = $derived(!!status?.enabled && status.unlocked);
  const report = $derived(currentReport());
  const info = $derived(summary(status));
  const syncedAt = $derived(status?.lastSync);

  let kind = $state<"git" | "folder">("git");
  let url = $state("");
  let folder = $state("");
  /** The probe's answer: the remote already holds a sync to join. null until probed. */
  let joining = $state<boolean | null>(null);
  // The vault only when asked, as on the command line.
  let chosen = $state<Scope[]>(scopes.map(([scope]) => scope).filter((scope) => scope !== "vault"));
  let deviceName = $state("");

  let mode = $state<"" | "recover" | "password" | "disable">("");
  let password = $state("");
  let confirm = $state("");
  let current = $state("");
  let recoveryKey = $state("");
  let conflict = $state<string | null>(null);
  let devices = $state<Device[] | null>(null);
  let devicesError = $state("");
  let copied = $state(false);
  let stored = $state(false);

  const remote = $derived<Remote>(kind === "git" ? { kind, url: url.trim() } : { kind, path: folder.trim() });
  const length = $derived([...password].length);
  const tooShort = $derived(confirm.length > 0 && length < 8);
  const mismatch = $derived(confirm.length > 0 && !tooShort && confirm !== password);
  const newPasswordOk = $derived(length >= 8 && confirm === password);
  const openConflict = $derived(status?.conflicts.find((c) => c.path === conflict));
  const osLabel: Record<string, Parameters<typeof t>[0]> = { macos: "osMacos", linux: "osLinux", windows: "osWindows" };

  // After each sync, manual or not: the devices' last syncs moved.
  $effect(() => {
    void syncedAt;
    if (ready) loadDevices();
  });

  async function loadDevices() {
    try {
      devices = await invoke<Device[]>("sync_devices");
      devicesError = "";
    } catch (e) {
      devicesError = String(e);
    }
  }

  function setMode(next: typeof mode) {
    mode = next;
    password = confirm = current = recoveryKey = "";
    sync.error = "";
  }

  function setKind(next: typeof kind) {
    kind = next;
    joining = null;
  }

  async function browse() {
    const path = await open({ directory: true, multiple: false });
    if (path) {
      folder = path;
      joining = null;
    }
  }

  async function probe() {
    const found = await syncCall<boolean>("sync_probe", { remote });
    if (found !== undefined) joining = found;
  }

  async function setup() {
    const result = await syncCall<{ recoveryKey: string | null; report: Report; error: string | null }>("sync_setup", {
      remote,
      password,
      scopes: chosen,
      deviceName: deviceName.trim() || null,
    });
    if (!result) return;
    setMode("");
    joining = null;
    // A failed first sync leaves an empty report: the status's lastError tells it instead.
    if (!result.error) keepReport(result.report);
    if (result.recoveryKey) sync.recoveryKey = { key: result.recoveryKey, reason: "setup" };
  }

  async function unlock() {
    if ((await syncCall("sync_unlock", { password })) !== undefined) setMode("");
  }

  async function rekey(command: "sync_recover" | "sync_change_password", reason: "recovered" | "changed") {
    const args = command === "sync_recover" ? { recoveryKey, password } : { current, password };
    const key = await syncCall<string>(command, args);
    if (!key) return;
    setMode("");
    sync.recoveryKey = { key, reason };
    if (ready) loadDevices();
  }

  function choose(scope: Scope, on: boolean) {
    chosen = on ? [...chosen, scope] : chosen.filter((s) => s !== scope);
  }

  async function setScope(scope: Scope, on: boolean) {
    const next = on ? [...(status?.scopes ?? []), scope] : (status?.scopes ?? []).filter((s) => s !== scope);
    await syncCall("sync_scopes", { scopes: next });
  }

  async function disable() {
    if ((await syncCall("sync_disable")) === undefined) return;
    setMode("");
    sync.report = null;
    devices = null;
  }

  async function copy(key: string) {
    try {
      await navigator.clipboard.writeText(key);
      copied = true;
    } catch (e) {
      sync.error = String(e);
    }
  }

  function stash() {
    sync.recoveryKey = null;
    copied = stored = false;
  }

  function submit(run: () => unknown) {
    return (e: SubmitEvent) => {
      e.preventDefault();
      run();
    };
  }

  const focus = (el: HTMLElement) => el.focus();
</script>

{#snippet spinner(command: string, Icon: typeof Lock)}
  {#if sync.busy === command}
    <LoaderCircle size={14} aria-hidden="true" class="spin" />
  {:else}
    <Icon size={14} aria-hidden="true" />
  {/if}
{/snippet}

{#snippet newPassword(label: Parameters<typeof t>[0])}
  <div class="pair">
    <label class="labelled">
      <span>{t(label)}</span>
      <input class="field" type="password" autocomplete="new-password" bind:value={password} aria-invalid={tooShort} />
    </label>
    <label class="labelled">
      <span>{t("syncPasswordConfirm")}</span>
      <input class="field" type="password" autocomplete="new-password" bind:value={confirm} aria-invalid={mismatch} />
    </label>
  </div>
  {#if tooShort || mismatch}
    <p class="invalid" role="alert">{t(tooShort ? "syncPasswordShort" : "syncPasswordMismatch")}</p>
  {:else}
    <p class="help">{t("syncPasswordHelp")}</p>
  {/if}
{/snippet}

{#snippet scopeList(selected: Scope[], onchange: (scope: Scope, on: boolean) => void)}
  <ul class="scopes">
    {#each scopes as [scope, label, help] (scope)}
      {@const on = selected.includes(scope)}
      <li>
        <label class="check">
          <input
            type="checkbox"
            checked={on}
            disabled={!!sync.busy || (on && selected.length === 1)}
            onchange={(e) => onchange(scope, e.currentTarget.checked)}
          />
          <span>{t(label)}</span>
          <span class="muted">{t(help)}</span>
        </label>
      </li>
    {/each}
  </ul>
  <p class="help">{t("syncScopesHelp")}</p>
{/snippet}

{#snippet recoverForm()}
  <form class="form" onsubmit={submit(() => rekey("sync_recover", "recovered"))}>
    <h3>{t("syncRecoverTitle")}</h3>
    <p class="help">{t("syncRecoverHelp")}</p>
    <label class="labelled">
      <span>{t("syncRecoveryKey")}</span>
      <input
        class="field mono"
        bind:value={recoveryKey}
        placeholder="XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX"
        autocomplete="off"
        spellcheck="false"
        {@attach focus}
      />
    </label>
    {@render newPassword("syncPasswordNew")}
    <div class="buttons">
      <button type="button" class="btn" onclick={() => setMode("")}>{t("cancel")}</button>
      <button class="btn primary" disabled={!recoveryKey.trim() || !newPasswordOk || !!sync.busy}>
        {@render spinner("sync_recover", KeyRound)}{t("syncRecover")}
      </button>
    </div>
  </form>
{/snippet}

{#snippet disableBlock()}
  <section class="step">
    {#if mode === "disable"}
      <div class="confirm" role="alert">
        <span>{t("syncDisableConfirm")}</span>
        <button class="btn sm" onclick={() => setMode("")} {@attach focus}>{t("cancel")}</button>
        <button class="btn sm danger" onclick={disable} disabled={!!sync.busy}>{t("syncDisableRun")}</button>
      </div>
    {:else}
      <button class="btn sm quiet danger-text" onclick={() => setMode("disable")}>
        <Power size={14} aria-hidden="true" />{t("syncDisable")}
      </button>
    {/if}
  </section>
{/snippet}

<section class="sync">
  <header class="head">
    <h2><FolderSync size={18} aria-hidden="true" />{t("syncTitle")}</h2>
    <div class="actions">
      {#if ready && !sync.recoveryKey}
        <button class="btn sm" onclick={() => syncNow(false)} disabled={!!sync.busy}>
          {@render spinner("sync_pull", Download)}{t("syncPullOnly")}
        </button>
        <button class="btn sm primary" onclick={() => syncNow()} disabled={!!sync.busy}>
          {@render spinner("sync_push", ArrowDownUp)}{t("syncNow")}
        </button>
      {/if}
    </div>
  </header>

  {#if sync.error}
    <p class="notice error lines" role="alert"><CircleAlert size={14} aria-hidden="true" />{sync.error}</p>
  {/if}

  {#if !status}
    <p class="muted">{t("syncLoading")}</p>
  {:else if sync.recoveryKey}
    {@const { key, reason } = sync.recoveryKey}
    <div class="recovery">
      <h3>{t("syncKeyTitle")}</h3>
      <p class="lead">
        <CircleCheck size={16} aria-hidden="true" class="ok" />
        {t(reason === "setup" ? "syncKeySetup" : reason === "changed" ? "syncKeyChanged" : "syncKeyRecovered")}
      </p>
      <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("syncKeyOnce")}</p>
      <div class="key">
        <code aria-label={t("syncRecoveryKey")}>{key}</code>
        <button class="btn sm" onclick={() => copy(key)}>
          {#if copied}
            <CopyCheck size={14} aria-hidden="true" />{t("syncKeyCopied")}
          {:else}
            <Copy size={14} aria-hidden="true" />{t("syncKeyCopy")}
          {/if}
        </button>
      </div>
      <label class="check stored">
        <input type="checkbox" bind:checked={stored} />
        {t("syncKeyStored")}
      </label>
      <div>
        <button class="btn primary" disabled={!stored} onclick={stash}>{t("syncContinue")}</button>
      </div>
    </div>
  {:else if !status.enabled}
    <p class="lead">{t("syncIntro")}</p>
    <form class="setup" onsubmit={submit(() => (joining === null ? probe() : setup()))}>
      <section class="step">
        <h3>{t("syncWhere")}</h3>
        <div class="segmented" role="radiogroup" aria-label={t("syncWhere")}>
          <button type="button" role="radio" aria-checked={kind === "git"} onclick={() => setKind("git")}>
            <GitBranch size={13} aria-hidden="true" />{t("syncGit")}
          </button>
          <button type="button" role="radio" aria-checked={kind === "folder"} onclick={() => setKind("folder")}>
            <FolderOpen size={13} aria-hidden="true" />{t("syncFolder")}
          </button>
        </div>
        {#if kind === "git"}
          <label class="labelled wide">
            <span>{t("syncGitUrl")}</span>
            <input
              class="field mono"
              bind:value={url}
              oninput={() => (joining = null)}
              placeholder={t("syncGitPlaceholder")}
              autocomplete="off"
              spellcheck="false"
            />
          </label>
          <p class="help">{t("syncGitHelp")}</p>
        {:else}
          <div class="labelled wide">
            <label for="sync-folder">{t("syncFolderPath")}</label>
            <div class="with-button">
              <input
                id="sync-folder"
                class="field mono"
                bind:value={folder}
                oninput={() => (joining = null)}
                autocomplete="off"
                spellcheck="false"
              />
              <button type="button" class="btn" onclick={browse}>{t("syncBrowse")}</button>
            </div>
          </div>
          <p class="help">{t("syncFolderHelp")}</p>
        {/if}
        {#if joining === null}
          <div>
            <button class="btn primary" disabled={!remoteText(remote) || !!sync.busy}>
              {#if sync.busy === "sync_probe"}
                <LoaderCircle size={14} aria-hidden="true" class="spin" />{t("syncChecking")}
              {:else}
                {t("syncCheck")}
              {/if}
            </button>
          </div>
        {:else}
          <p class="notice"><Info size={14} aria-hidden="true" />{joining ? t("syncExisting") : t("syncFresh")}</p>
        {/if}
      </section>

      {#if joining !== null}
        <section class="step">
          <h3>{t("syncPassword")}</h3>
          {#if joining}
            <input
              class="field password"
              type="password"
              autocomplete="current-password"
              bind:value={password}
              aria-label={t("syncPassword")}
              {@attach focus}
            />
          {:else}
            {@render newPassword("syncPasswordLabel")}
          {/if}
        </section>
        <section class="step">
          <h3>{t("syncScopes")}</h3>
          {@render scopeList(chosen, choose)}
        </section>
        <section class="step">
          <h3>{t("syncDevice")}</h3>
          <input
            class="field name"
            bind:value={deviceName}
            placeholder={t("syncDevicePlaceholder")}
            aria-label={t("syncDevice")}
          />
        </section>
        <footer class="buttons">
          <button type="button" class="btn" onclick={() => (setMode(""), (joining = null))}>{t("cancel")}</button>
          <button class="btn primary" disabled={(joining ? !password : !newPasswordOk) || !!sync.busy}>
            {@render spinner("sync_setup", FolderSync)}{joining ? t("syncJoin") : t("syncCreate")}
          </button>
        </footer>
      {/if}
    </form>
  {:else}
    <div class="status tone-{info.tone}" role="status">
      {#if info.tone === "ok"}
        <CircleCheck size={16} aria-hidden="true" />
      {:else if !status.unlocked}
        <Lock size={16} aria-hidden="true" />
      {:else}
        <TriangleAlert size={16} aria-hidden="true" />
      {/if}
      <span>{info.text}</span>
    </div>
    <dl class="meta">
      {#if status.remote}
        <dt>{t("syncRemote")}</dt>
        <dd class="mono">
          {#if status.remote.kind === "git"}<GitBranch size={12} aria-hidden="true" />{:else}<FolderOpen
              size={12}
              aria-hidden="true"
            />{/if}{remoteText(status.remote)}
        </dd>
      {/if}
      <dt>{t("syncDevice")}</dt>
      <dd>{status.deviceName}</dd>
      {#if info.tone !== "ok" && status.lastSync}
        <dt>{t("syncLastSuccess")}</dt>
        <dd>{ago(status.lastSync)}</dd>
      {/if}
      {#if ready}
        <dt>{t("syncKeychain")}</dt>
        <dd class:warning={!status.remembered}>{t(status.remembered ? "syncRemembered" : "syncInMemory")}</dd>
      {/if}
    </dl>

    {#if !ready}
      {#if mode === "recover"}
        <section class="step">{@render recoverForm()}</section>
      {:else}
        <form class="step form" onsubmit={submit(unlock)}>
          <p class="help">{t("syncLockedHelp")}</p>
          <label class="labelled">
            <span>{t("syncPassword")}</span>
            <input class="field" type="password" autocomplete="current-password" bind:value={password} {@attach focus} />
          </label>
          <div class="buttons start">
            <button class="btn primary" disabled={!password || !!sync.busy}>
              {@render spinner("sync_unlock", Lock)}{t("syncUnlock")}
            </button>
            <button type="button" class="btn quiet" onclick={() => setMode("recover")}>{t("syncForgot")}</button>
          </div>
        </form>
      {/if}
      {@render disableBlock()}
    {:else if openConflict}
      <SyncConflict conflict={openConflict} onback={() => (conflict = null)} />
    {:else}
      {#if report}
        <p class="notice">
          <CircleCheck size={14} aria-hidden="true" />
          {report.pulled.length || report.pushed.length
            ? t("syncReport", { pulled: report.pulled.length, pushed: report.pushed.length })
            : t("syncUpToDate")}
        </p>
      {/if}
      {#if status.lastError && status.lastError !== sync.error && !report?.skipped.length}
        <p class="notice error lines"><CircleAlert size={14} aria-hidden="true" />{status.lastError}</p>
      {/if}

      {#if status.conflicts.length}
        <section class="step">
          <h3>{t("syncConflictsTitle")} <span class="count">{status.conflicts.length}</span></h3>
          <p class="help">{t("syncConflictsHelp")}</p>
          <ul class="rows">
            {#each status.conflicts as c (c.path)}
              <li>
                <GitMergeConflict size={14} aria-hidden="true" class="flag" />
                <div class="grow">
                  <span class="mono">{c.path}</span>
                  <span class="sub">
                    {c.keys.length ? t("syncConflictKeys", { keys: c.keys.join(", ") }) : t("syncConflictWhole")}
                  </span>
                </div>
                <button class="btn sm" onclick={() => (conflict = c.path)}>{t("syncResolve")}</button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if report?.skipped.length}
        <section class="step">
          <h3>{t("syncSkippedTitle")} <span class="count">{report.skipped.length}</span></h3>
          <p class="help">{t("syncSkippedHelp")}</p>
          <ul class="rows">
            {#each report.skipped as s (s.path)}
              <li>
                <TriangleAlert size={14} aria-hidden="true" class="flag" />
                <div class="grow">
                  <span class="mono">{s.path}</span>
                  <span class="sub">{s.error}</span>
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if status.missingRepos.length}
        <section class="step">
          <h3>{t("syncMissingTitle")} <span class="count">{status.missingRepos.length}</span></h3>
          <p class="help">{t("syncMissingHelp")}</p>
          <ul class="rows">
            {#each status.missingRepos as repo (repo.project + repo.path)}
              <li>
                <FolderGit2 size={14} aria-hidden="true" class="muted" />
                <div class="grow">
                  <span class="mono">{repo.path}</span>
                  <span class="sub">
                    {app.workspace?.projects.find((p) => p.id === repo.project)?.name ?? repo.project}
                    {#if repo.remote}<span class="mono"> · {repo.remote}</span>{/if}
                  </span>
                </div>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="step">
        <h3>{t("syncScopes")}</h3>
        {@render scopeList(status.scopes, setScope)}
      </section>

      <section class="step">
        <h3>{t("syncDevices")}</h3>
        {#if devicesError}
          <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{devicesError}</p>
        {:else if !devices}
          <p class="muted loading"><LoaderCircle size={14} aria-hidden="true" class="spin" />{t("syncDevicesLoading")}</p>
        {:else if !devices.length}
          <p class="muted">{t("syncDevicesEmpty")}</p>
        {:else}
          <ul class="rows">
            {#each devices as d (d.id)}
              <li>
                <Laptop size={14} aria-hidden="true" class="muted" />
                <div class="grow">
                  <span class="device">
                    {d.name}
                    {#if d.current}<span class="badge current">{t("syncThisDevice")}</span>{/if}
                  </span>
                  <span class="sub">{osLabel[d.os] ? t(osLabel[d.os]) : d.os}</span>
                </div>
                <span class="sub when">{ago(d.lastSync)}</span>
              </li>
            {/each}
          </ul>
        {/if}
        <p class="help">{t("syncDevicesHelp")}</p>
      </section>

      <section class="step">
        <h3>{t("syncSecurity")}</h3>
        {#if mode === "password"}
          <form class="form" onsubmit={submit(() => rekey("sync_change_password", "changed"))}>
            <label class="labelled">
              <span>{t("syncPasswordCurrent")}</span>
              <input
                class="field"
                type="password"
                autocomplete="current-password"
                bind:value={current}
                {@attach focus}
              />
            </label>
            {@render newPassword("syncPasswordNew")}
            <p class="help">{t("syncChangeHelp")}</p>
            <div class="buttons">
              <button type="button" class="btn" onclick={() => setMode("")}>{t("cancel")}</button>
              <button class="btn primary" disabled={!current || !newPasswordOk || !!sync.busy}>
                {@render spinner("sync_change_password", KeyRound)}{t("syncChangePasswordRun")}
              </button>
            </div>
          </form>
        {:else if mode === "recover"}
          {@render recoverForm()}
        {:else}
          <div class="buttons start">
            <button class="btn sm" onclick={() => setMode("password")}>
              <KeyRound size={14} aria-hidden="true" />{t("syncChangePassword")}
            </button>
            <button class="btn sm quiet" onclick={() => setMode("recover")}>{t("syncForgot")}</button>
          </div>
        {/if}
      </section>

      {@render disableBlock()}
    {/if}
  {/if}
</section>

<style>
  .sync {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .sync > * {
    max-width: 820px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h2 :global(svg) {
    color: var(--accent-ink);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .lead {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 8px;
    color: var(--muted);
  }

  .lead :global(.ok) {
    flex-shrink: 0;
    color: var(--done);
  }

  .help {
    max-width: 660px;
    margin: 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .invalid {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }

  .step {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 20px 0;
    border-top: 1px solid var(--border);
  }

  .step:first-child {
    border-top: 0;
  }

  .form {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    width: 100%;
  }

  .labelled {
    width: 420px;
    max-width: 100%;
  }

  .labelled .field {
    font-weight: 400;
  }

  .wide {
    width: 560px;
  }

  .password {
    width: 260px;
  }

  .field[aria-invalid="true"] {
    border-color: var(--danger);
  }

  .pair {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .pair .labelled {
    width: 260px;
  }

  .with-button {
    display: flex;
    gap: 6px;
  }

  .with-button .field {
    flex: 1;
  }

  .name {
    width: 420px;
    max-width: 100%;
  }

  .mono,
  .field.mono {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .segmented button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
  }

  .check {
    display: flex;
    align-items: baseline;
    gap: 8px;
    cursor: pointer;
  }

  .check input {
    margin: 0;
    transform: translateY(2px);
  }

  .scopes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .scopes .muted {
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }

  footer.buttons {
    justify-content: flex-end;
    max-width: 820px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .notice {
    margin-bottom: 12px;
  }

  .step .notice {
    margin-bottom: 0;
  }

  .lines {
    white-space: pre-line;
  }

  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 0 6px;
    font-weight: 600;
  }

  .tone-ok {
    color: var(--done);
  }

  .tone-attention {
    color: var(--warning);
  }

  .tone-error {
    color: var(--danger);
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 16px;
    margin: 0 0 16px;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    overflow-wrap: anywhere;
  }

  dd :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  dd.mono {
    font-size: var(--text-xs);
  }

  .warning {
    color: var(--warning);
  }

  .count {
    min-width: 18px;
    padding: 0 5px;
    border-radius: var(--radius-pill);
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    text-align: center;
  }

  .rows {
    width: 100%;
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--border);
  }

  .rows li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 4px;
    border-bottom: 1px solid var(--border);
  }

  .rows :global(.flag) {
    flex-shrink: 0;
    color: var(--warning);
  }

  .rows :global(.muted) {
    flex-shrink: 0;
  }

  .grow {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .rows .mono {
    font-size: var(--text-xs);
  }

  .sub {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .when {
    flex-shrink: 0;
    font-variant-numeric: tabular-nums;
  }

  .device {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
  }

  .current {
    border-color: transparent;
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .loading {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
  }

  .confirm {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: var(--text-sm);
  }

  .danger-text {
    color: var(--danger);
  }

  .recovery {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 14px;
    max-width: 640px;
    margin-top: 16px;
  }

  .recovery .lead {
    margin: 0;
  }

  .key {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--bg);
  }

  .key code {
    font: 600 var(--text-md) / 1.4 var(--font-mono);
    letter-spacing: 0.04em;
    user-select: all;
  }

  .stored {
    font-weight: 500;
  }
</style>
