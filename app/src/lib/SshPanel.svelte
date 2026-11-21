<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Minus from "@lucide/svelte/icons/minus";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import X from "@lucide/svelte/icons/x";
  import { t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";
  import { act, nameError, ssh, type HostEntry, type KeyType, type Rotation } from "$lib/ssh.svelte";

  // Keyed on the panel by SshView: a new panel is a new instance.
  const panel = ssh.panel as NonNullable<typeof ssh.panel>;
  const old = panel.kind === "rotate" ? panel.key : null;
  const host = panel.kind === "host" ? panel.host : null;

  const keys = $derived(ssh.inventory?.keys ?? []);
  const dir = $derived(ssh.inventory ? shortPath(ssh.inventory.dir) : "~/.ssh");
  const config = $derived(`${dir}/config`);

  let kind = $state<KeyType>("ed25519");
  let name = $state(
    old ? `${old.name.replace(/\.pub$/, "").replace(/[-_]\d{4}$/, "")}-${new Date().getFullYear()}` : "id_ed25519",
  );
  let named = !!old;
  let comment = $state(old?.comment ?? "");
  let passphrase = $state("");
  let confirm = $state("");
  let upload = $state(true);
  let rotation = $state<Rotation | null>(null);
  let confirmTrash = $state(false);

  const nameProblem = $derived(nameError(name, keys));
  const mismatch = $derived(!!confirm && passphrase !== confirm);
  const keyReady = $derived(!nameProblem && passphrase === confirm);

  const types: [KeyType, string][] = [
    ["ed25519", "Ed25519"],
    ["ecdsa", "ECDSA P-256"],
    ["rsa", "RSA 4096"],
  ];

  function pick(next: KeyType) {
    kind = next;
    if (!named) name = `id_${next}`;
  }

  function keyArgs() {
    return { name, kind, comment: comment.trim(), passphrase: passphrase || null };
  }

  async function generate() {
    let path = "";
    const done = await act(async () => {
      path = await invoke<string>("ssh_generate", keyArgs());
      return t("sshGenerated", { path: shortPath(path) });
    });
    if (done) {
      ssh.selected = path;
      ssh.tab = "keys";
      ssh.panel = null;
    }
  }

  async function rotate() {
    if (!old) return;
    await act(async () => {
      rotation = await invoke<Rotation>("ssh_rotate", { path: old.path, ...keyArgs(), upload });
    });
    passphrase = confirm = "";
  }

  async function trashOld() {
    if (!old || !rotation) return;
    const next = rotation.key;
    const done = await act(async () => {
      const to = await invoke<string>("ssh_trash", { path: old.path });
      return t("sshTrashed", { name: old.name, dir: shortPath(to) });
    });
    if (done) close(next);
  }

  function close(select = "") {
    if (select) {
      ssh.selected = select;
      ssh.tab = "keys";
    }
    ssh.panel = null;
  }

  // The host form, prefilled with the block being edited: an option left empty is removed from it.
  let alias = $state(host?.patterns[0] ?? "");
  let hostName = $state(host?.hostName ?? "");
  let user = $state(host?.user ?? "");
  let port = $state(host?.port ?? "");
  let identityFile = $state(host?.identityFiles[0] ?? "");
  let identitiesOnly = $state(host ? !!host.identitiesOnly : true);
  let preview = $state<{ diff: string; warnings: string[]; for: string } | null>(null);

  const portOk = $derived(!port.trim() || (/^\d+$/.test(port.trim()) && +port >= 1 && +port <= 65535));
  const entry = $derived<HostEntry>({
    alias: alias.trim(),
    hostName: hostName.trim() || null,
    user: user.trim() || null,
    port: port.trim() ? Number(port) : null,
    identityFile: identityFile || null,
    // Without a key of its own, IdentitiesOnly would also keep ssh from offering the agent's keys.
    identitiesOnly: (!!identityFile && identitiesOnly) || null,
  });
  const fresh = $derived(!!preview && preview.for === JSON.stringify(entry));
  const known = $derived(!identityFile || keys.some((k) => k.path === identityFile));

  async function showPreview() {
    const asked = JSON.stringify(entry);
    ssh.error = "";
    try {
      const result = await invoke<{ diff: string; warnings: string[] }>("ssh_host_preview", { host: entry });
      preview = { ...result, for: asked };
    } catch (e) {
      ssh.error = String(e);
    }
  }

  async function save() {
    const saved = entry.alias;
    const done = await act(async () => {
      await invoke("ssh_host_save", { host: entry });
      return t("sshHostSaved", { alias: saved });
    });
    if (done) {
      ssh.tab = "hosts";
      ssh.panel = null;
    }
  }

  function diffLines(diff: string) {
    return diff.replace(/\n$/, "").split("\n");
  }
</script>

{#snippet keyForm()}
  <div class="form">
    <div class="labelled">
      <span id="ssh-type">{t("sshKeyType")}</span>
      <div class="segmented" role="radiogroup" aria-labelledby="ssh-type">
        {#each types as [value, label] (value)}
          <button type="button" role="radio" aria-checked={kind === value} onclick={() => pick(value)}>{label}</button>
        {/each}
      </div>
      <span class="hint">{t("sshTypeHelp")}</span>
    </div>
    <label class="labelled">
      <span>{t("sshKeyName")}</span>
      <input
        class="field mono-field"
        bind:value={name}
        oninput={() => (named = true)}
        aria-invalid={!!nameProblem}
        autocomplete="off"
        spellcheck="false"
      />
      {#if nameProblem}
        <span class="hint bad">{t(nameProblem)}</span>
      {:else}
        <span class="hint">{t("sshKeyFiles", { path: `${dir}/${name}` })}</span>
      {/if}
    </label>
    <label class="labelled">
      <span>{t("sshComment")}</span>
      <input class="field" bind:value={comment} placeholder={t("sshCommentPlaceholder")} autocomplete="off" spellcheck="false" />
    </label>
    <div class="pair">
      <label class="labelled">
        <span>{t("sshPassphrase")}</span>
        <input class="field" type="password" bind:value={passphrase} autocomplete="new-password" />
      </label>
      <label class="labelled">
        <span>{t("sshPassphraseConfirm")}</span>
        <input class="field" type="password" bind:value={confirm} autocomplete="new-password" aria-invalid={mismatch} />
      </label>
    </div>
    <span class="hint" class:bad={mismatch}>{mismatch ? t("sshPassMismatch") : t("sshPassphraseAdvice")}</span>
  </div>
{/snippet}

<section class="panel">
  <header>
    <h2>
      {#if panel.kind === "generate"}{t("sshGenerate")}
      {:else if panel.kind === "rotate"}{t("sshRotateTitle", { name: old?.name ?? "" })}
      {:else if host}{t("sshHostEdit", { alias: host.patterns[0] })}
      {:else}{t("sshHostNew")}{/if}
    </h2>
    <button class="icon-btn" onclick={() => close(rotation?.key)} aria-label={t("close")} title={t("close")}>
      <X size={16} aria-hidden="true" />
    </button>
  </header>

  {#if panel.kind === "generate"}
    <p class="lead">{t("sshGenerateHelp", { dir })}</p>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        generate();
      }}
    >
      {@render keyForm()}
      <footer>
        <button type="button" class="btn" onclick={() => close()}>{t("cancel")}</button>
        <button type="submit" class="btn primary" disabled={!keyReady || ssh.busy}>
          {#if ssh.busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("sshGenerating")}
          {:else}{t("sshGenerateRun")}{/if}
        </button>
      </footer>
    </form>
  {:else if panel.kind === "rotate" && old}
    {#if !rotation}
      <p class="lead">{t("sshRotateHelp")}</p>
      <p class="uses">
        {#if old.hosts.length || old.repos.length}
          {t("sshRotateUses", { uses: [...old.hosts, ...old.repos.map(shortPath)].join(", ") })}
        {:else}
          {t("sshRotateUnused")}
        {/if}
      </p>
      <form
        onsubmit={(e) => {
          e.preventDefault();
          rotate();
        }}
      >
        {@render keyForm()}
        <label class="check">
          <input type="checkbox" bind:checked={upload} />
          {t("sshRotateUpload")}
        </label>
        {#if !upload}
          <p class="hint">{t("sshRotateUploadHint")}</p>
        {/if}
        <footer>
          <button type="button" class="btn" onclick={() => close()}>{t("cancel")}</button>
          <button type="submit" class="btn primary" disabled={!keyReady || ssh.busy}>
            {#if ssh.busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("sshRotating")}
            {:else}{t("sshRotate")}{/if}
          </button>
        </footer>
      </form>
    {:else}
      <h3 class="report-title">{t("sshRotationDone")}</h3>
      <ul class="report">
        <li class="good"><Check size={14} aria-hidden="true" />{t("sshRotationKey", { path: shortPath(rotation.key) })}</li>
        <li class:good={rotation.uploaded}>
          {#if rotation.uploaded}<Check size={14} aria-hidden="true" />{:else}<Minus size={14} aria-hidden="true" />{/if}
          {rotation.uploaded ? t("sshRotationUploaded") : t("sshRotationNotUploaded")}
        </li>
        <li class:good={rotation.hosts.length > 0}>
          {#if rotation.hosts.length}<Check size={14} aria-hidden="true" />{:else}<Minus size={14} aria-hidden="true" />{/if}
          {t("sshRotationHosts", { list: rotation.hosts.join(", ") || t("sshNone") })}
        </li>
        <li class:good={rotation.repos.length > 0}>
          {#if rotation.repos.length}<Check size={14} aria-hidden="true" />{:else}<Minus size={14} aria-hidden="true" />{/if}
          {t("sshRotationRepos", { list: rotation.repos.map(shortPath).join(", ") || t("sshNone") })}
        </li>
      </ul>
      {#if rotation.warning}
        <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("sshRotationWarning")}</p>
      {/if}
      {#if rotation.errors.length}
        <div class="notice error" role="alert">
          <TriangleAlert size={14} aria-hidden="true" />
          <div>
            {t("sshRotationErrors")}
            <ul class="paths">
              {#each rotation.errors as error, i (i)}<li>{error}</li>{/each}
            </ul>
          </div>
        </div>
      {/if}
      <div class="old">
        {#if confirmTrash}
          <span>{t("sshTrashConfirm", { name: old.name, dir: `${dir}/.termf-trash` })}</span>
          <button class="btn sm" onclick={() => (confirmTrash = false)}>{t("cancel")}</button>
          <button class="btn sm danger" onclick={trashOld} disabled={ssh.busy}>{t("sshTrash")}</button>
        {:else}
          <span>{t("sshRotationOld", { name: old.name })}</span>
          <button class="btn sm quiet danger-text" onclick={() => (confirmTrash = true)}>{t("sshTrashOld")}</button>
        {/if}
      </div>
      <footer>
        <button class="btn primary" onclick={() => close(rotation?.key)}>{t("sshShowNewKey")}</button>
      </footer>
    {/if}
  {:else}
    <p class="lead">{t("sshHostHelp", { file: config })}</p>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        if (fresh && preview?.diff) save();
        else showPreview();
      }}
    >
      <div class="form grid">
        <label class="labelled">
          <span>{t("sshAlias")}</span>
          <input
            class="field mono-field"
            bind:value={alias}
            readonly={!!host}
            placeholder="github-work"
            autocomplete="off"
            spellcheck="false"
          />
        </label>
        <label class="labelled">
          <span>HostName</span>
          <input class="field mono-field" bind:value={hostName} placeholder="github.com" autocomplete="off" spellcheck="false" />
        </label>
        <label class="labelled">
          <span>User</span>
          <input class="field mono-field" bind:value={user} placeholder="git" autocomplete="off" spellcheck="false" />
        </label>
        <label class="labelled">
          <span>Port</span>
          <input class="field mono-field" bind:value={port} inputmode="numeric" placeholder="22" aria-invalid={!portOk} />
          {#if !portOk}<span class="hint bad">{t("sshPortInvalid")}</span>{/if}
        </label>
        <label class="labelled wide">
          <span>{t("sshIdentityFile")}</span>
          <select class="field" bind:value={identityFile}>
            <option value="">{t("sshIdentityNone")}</option>
            {#if !known}<option value={identityFile}>{shortPath(identityFile)}</option>{/if}
            {#each keys as key (key.path)}
              <option value={key.path}>{key.name}</option>
            {/each}
          </select>
        </label>
        <label class="check wide">
          <input type="checkbox" bind:checked={identitiesOnly} disabled={!identityFile} />
          {t("sshIdentitiesOnly")}
        </label>
      </div>

      <div class="preview-head">
        <button type="button" class="btn" onclick={showPreview} disabled={!entry.alias || !portOk}>{t("sshPreview")}</button>
        {#if !fresh}<span class="muted hint">{t("sshPreviewNeeded")}</span>{/if}
      </div>

      {#if preview && fresh}
        {#each preview.warnings as warning, i (i)}
          <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{warning}</p>
        {/each}
        {#if preview.diff}
          <p class="file">{t("sshPreviewTitle", { file: config })}</p>
          <pre class="code diff">{#each diffLines(preview.diff) as line, i (i)}<span
                class="l{line[0] === '+' ? ' add' : line[0] === '-' ? ' del' : ''}">{line}
</span>{/each}</pre>
        {:else}
          <p class="notice"><Check size={14} aria-hidden="true" />{t("sshPreviewSame")}</p>
        {/if}
      {/if}

      <footer>
        <button type="button" class="btn" onclick={() => close()}>{t("cancel")}</button>
        <button type="button" class="btn primary" onclick={save} disabled={!fresh || !preview?.diff || ssh.busy}>
          {t("save")}
        </button>
      </footer>
    </form>
  {/if}
</section>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .panel > * {
    max-width: 760px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .lead {
    margin: 6px 0 16px;
    color: var(--muted);
  }

  .uses {
    margin: -6px 0 16px;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px 16px;
  }

  .wide {
    grid-column: 1 / -1;
  }

  .labelled {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--text-sm);
    font-weight: 500;
  }

  .form > .labelled {
    max-width: 420px;
  }

  .labelled .segmented {
    align-self: flex-start;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
    max-width: 560px;
  }

  .mono-field {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .mono-field[readonly] {
    background: var(--hover);
  }

  .hint {
    margin: 0;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 400;
  }

  .hint.bad {
    color: var(--danger);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    cursor: pointer;
  }

  .grid .check {
    margin-top: 0;
  }

  .check input {
    margin: 0;
  }

  .check + .hint {
    margin-top: 4px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }

  .preview-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 20px;
  }

  .notice {
    margin-top: 12px;
  }

  .notice.warn {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .file {
    margin: 14px 0 6px;
    color: var(--muted);
    font: var(--text-xs) var(--font-mono);
  }

  .code {
    max-height: 360px;
    margin: 0;
    padding: 10px 12px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre;
  }

  .diff .l {
    display: block;
  }

  .diff .add {
    background: var(--done-soft);
    color: var(--done);
  }

  .diff .del {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .report-title {
    margin: 16px 0 8px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .report {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .report li {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }

  .report li.good {
    color: var(--text);
  }

  .report :global(svg) {
    flex-shrink: 0;
    margin-top: 3px;
  }

  .report li.good :global(svg) {
    color: var(--done);
  }

  .paths {
    margin: 4px 0 0;
    padding-left: 16px;
  }

  .old {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 20px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .old span {
    flex: 1 1 280px;
  }

  .danger-text {
    color: var(--danger);
  }
</style>
