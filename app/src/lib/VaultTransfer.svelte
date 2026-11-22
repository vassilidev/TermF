<script lang="ts">
  import ArrowDownUp from "@lucide/svelte/icons/arrow-down-up";
  import FileDown from "@lucide/svelte/icons/file-down";
  import FileUp from "@lucide/svelte/icons/file-up";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import StickyNote from "@lucide/svelte/icons/sticky-note";
  import Timer from "@lucide/svelte/icons/timer";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import X from "@lucide/svelte/icons/x";
  import { invoke } from "@tauri-apps/api/core";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import VaultMergeRows from "$lib/VaultMergeRows.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import { t } from "$lib/i18n";
  import { app, shortPath } from "$lib/state.svelte";
  import { failed, mergeText, needsConfirm, refreshVault, say, setStatus, vault, type VaultStatus } from "$lib/vault.svelte";
  import { folderProject, host, leftOut, type ForeignPreview, type Merged } from "$lib/vault-logic";

  /** Export (sealed, or in clear on request) and import (a TermF export, or another manager's file). */
  let { project, projectName, onclose }: { project: string | null; projectName: (id: string) => string; onclose: () => void } = $props();

  const projects = $derived(app.workspace?.projects ?? []);
  let what = $state<string | null>(null);
  let password = $state("");
  let again = $state("");
  let busy = $state("");
  let error = $state("");
  let confirming = $state(false);
  let typed = $state("");
  let plain = $state(false);

  let file = $state("");
  let kind = $state<"termf" | "foreign" | null>(null);
  let exportPassword = $state("");
  let rows = $state<Merged[] | null>(null);
  let preview = $state<ForeignPreview | null>(null);
  let skip = $state<number[]>([]);
  let into = $state<Record<string, string | null>>({});
  let done = $state<{ path: string; count: number } | null>(null);
  let deleting = $state(false);

  const sources: Record<string, Parameters<typeof t>[0]> = {
    "bitwarden-json": "vault3SourceBitwardenJson",
    "bitwarden-csv": "vault3SourceBitwardenCsv",
    "1password-1pux": "vault3Source1passwordPux",
    "1password-csv": "vault3Source1passwordCsv",
    "chrome-csv": "vault3SourceChromeCsv",
    "firefox-csv": "vault3SourceFirefoxCsv",
    "safari-csv": "vault3SourceSafariCsv",
    "keepassxc-csv": "vault3SourceKeepassxcCsv",
    "keepass-xml": "vault3SourceKeepassXml",
  };
  const taken = $derived(preview ? preview.rows.length - skip.length : 0);
  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

  async function run(what: string, action: () => Promise<void>) {
    busy = what;
    error = "";
    try {
      await action();
    } catch (e) {
      if (needsConfirm(e)) {
        confirming = true;
        setStatus(await invoke<VaultStatus>("vault_status"));
      } else error = failed(e);
    } finally {
      busy = "";
    }
  }

  function confirm(e: SubmitEvent) {
    e.preventDefault();
    run("confirm", async () => {
      try {
        setStatus(await invoke<VaultStatus>("vault_reauth", { password: typed }));
        confirming = false;
      } finally {
        typed = "";
      }
    });
  }

  function exportSealed(e: SubmitEvent) {
    e.preventDefault();
    run("export", async () => {
      if (password !== again) throw t("vaultPasswordsDiffer");
      const path = await save({ defaultPath: "termf-vault.termfvault", filters: [{ name: "TermF", extensions: ["termfvault"] }] });
      if (!path) return;
      await invoke("vault_export", { path, project: what, password });
      password = again = "";
      say(t("vault3Exported", { path: shortPath(path) }));
    });
  }

  function exportPlain() {
    run("plain", async () => {
      const path = await save({ defaultPath: "termf-vault.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
      if (!path) return;
      await invoke("vault_export_csv", { path, project: what });
      say(t("vault3PlainDone", { path: shortPath(path) }));
    });
  }

  function pick() {
    run("pick", async () => {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: t("vault3ImportTitle"), extensions: ["termfvault", "json", "csv", "1pux", "xml", "gz", "tgz"] }],
      });
      if (!path) return;
      const read = await invoke<{ kind: "termf" } | { kind: "foreign"; preview: ForeignPreview }>("vault_import_read", { path });
      file = path;
      done = null;
      rows = null;
      kind = read.kind;
      if (read.kind === "foreign") {
        preview = read.preview;
        skip = leftOut(read.preview.rows);
        into = Object.fromEntries(read.preview.folders.map((f) => [f, folderProject(f, projects) ?? project]));
      } else preview = null;
    });
  }

  function show(e: SubmitEvent) {
    e.preventDefault();
    run("show", async () => {
      rows = await invoke<Merged[]>("vault_import_preview", { path: file, password: exportPassword });
    });
  }

  function importTermf() {
    run("import", async () => {
      const merged = await invoke<Merged[]>("vault_import_apply", { path: file, password: exportPassword });
      exportPassword = "";
      kind = null;
      rows = null;
      say(t("vault3Imported", { summary: mergeText(merged) }));
      await refreshVault();
    });
  }

  function importForeign() {
    const shown = preview;
    if (!shown) return;
    run("import", async () => {
      const count = await invoke<number>("vault_import_foreign", {
        path: file,
        choice: { digest: shown.digest, skip, projects: into },
      });
      done = { path: file, count };
      kind = null;
      preview = null;
      await refreshVault();
    });
  }

  function remove() {
    const imported = done;
    if (!imported) return;
    run("delete", async () => {
      await invoke("vault_import_delete", { path: imported.path });
      done = null;
      say(t("vault3FileDeleted"));
    });
  }

  function toggle(index: number, on: boolean) {
    skip = on ? skip.filter((i) => i !== index) : [...skip, index];
  }

  // Locked meanwhile: what was typed goes.
  $effect(() => {
    if (!vault.status?.unlocked) password = again = exportPassword = typed = "";
  });
</script>

{#snippet spinner(name: string)}
  {#if busy === name}<LoaderCircle size={14} class="spin" aria-hidden="true" />{/if}
{/snippet}

<section class="transfer" aria-labelledby="vault-transfer-title">
  <header class="head">
    <span class="avatar" aria-hidden="true"><ArrowDownUp size={18} /></span>
    <div class="titles"><h2 id="vault-transfer-title">{t("vault3TransferTitle")}</h2></div>
    <button class="icon-btn" aria-label={t("close")} title={t("close")} onclick={onclose}><X size={16} aria-hidden="true" /></button>
  </header>

  {#if error}<p class="notice error" role="alert">{error}</p>{/if}

  {#if confirming}
    <form class="confirm" onsubmit={confirm}>
      <label class="labelled">
        {t("vault3ConfirmPassword")}
        <!-- svelte-ignore a11y_autofocus (asked for by the export just tried) -->
        <input class="field" type="password" autocomplete="current-password" bind:value={typed} required autofocus />
      </label>
      <button type="submit" class="btn primary" disabled={!!busy || !typed}>{@render spinner("confirm")}{t("vault3Confirm")}</button>
    </form>
  {/if}

  {#if done}
    <div class="card warn" role="status">
      <h3><TriangleAlert size={15} aria-hidden="true" />{t("vault3DeleteFileTitle")}</h3>
      <p>{t("vault3ImportedCount", { n: done.count })} {t("vault3DeleteFileText", { path: shortPath(done.path) })}</p>
      <div><button class="btn sm danger" disabled={!!busy} onclick={() => (deleting = true)}><Trash2 size={14} aria-hidden="true" />{t("vault3DeleteFile")}</button></div>
    </div>
  {/if}

  <div class="card">
    <h3><FileUp size={15} aria-hidden="true" />{t("vault3ImportTitle")}</h3>
    <p class="muted">{t("vault3ImportText")}</p>
    <div class="pick">
      <button class="btn sm" disabled={!!busy} onclick={pick}>{@render spinner("pick")}{t("vault3ImportPick")}</button>
      {#if kind}<span class="mono muted">{shortPath(file)}</span>{/if}
    </div>

    {#if kind === "termf"}
      <form class="inline" onsubmit={show}>
        <label class="labelled">
          {t("vault3ImportTermf")}
          <!-- svelte-ignore a11y_autofocus (the file was just chosen) -->
          <input class="field" type="password" autocomplete="off" bind:value={exportPassword} required autofocus />
        </label>
        <div class="buttons">
          <button type="submit" class="btn" disabled={!!busy || !exportPassword}>{@render spinner("show")}{t("vault3ImportShow")}</button>
        </div>
      </form>
      {#if rows}
        <p class="muted fine">{t("vault3ImportMerge")}</p>
        <VaultMergeRows {rows} {projectName} />
        <div class="buttons">
          <button class="btn primary" disabled={!!busy || rows.every((r) => r.change === "unchanged")} onclick={importTermf}>
            {@render spinner("import")}{t("vault3ImportRun")}
          </button>
        </div>
      {/if}
    {:else if kind === "foreign" && preview}
      <p class="source">{t("vault3From", { source: sources[preview.source] ? t(sources[preview.source]) : preview.source, n: preview.rows.length })}</p>
      {#if preview.unsupported}<p class="notice">{t("vault3Unsupported", { n: preview.unsupported })}</p>{/if}
      {#if preview.folders.some((f) => f)}
        <div class="folders">
          <h4>{t("vault3Folders")}</h4>
          {#each preview.folders as folder (folder)}
            <label class="folder">
              <span class="name">{folder || t("vault3NoFolder")}</span>
              <select class="field" value={into[folder] ?? ""} onchange={(e) => (into[folder] = e.currentTarget.value || null)}>
                <option value="">{t("vaultGlobal")}</option>
                {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
              </select>
            </label>
          {/each}
        </div>
      {/if}
      <table class="rows">
        <thead>
          <tr>
            <th class="check"><span class="sr-only">{t("vault3ImportRun")}</span></th>
            <th>{t("vault3ColTitle")}</th>
            <th>{t("vault3ColUser")}</th>
            <th>{t("vault3ColFolder")}</th>
            <th>{t("vault3ColState")}</th>
          </tr>
        </thead>
        <tbody>
          {#each preview.rows as row (row.index)}
            {@const on = !skip.includes(row.index)}
            <tr class:off={!on}>
              <td class="check">
                <input type="checkbox" checked={on} aria-label={row.title} onchange={(e) => toggle(row.index, e.currentTarget.checked)} />
              </td>
              <td>
                <span class="title">
                  {#if row.kind === "note"}<StickyNote size={13} aria-hidden="true" />{:else}<KeyRound size={13} aria-hidden="true" />{/if}
                  {row.title}
                </span>
                <span class="sub">
                  {#if row.urls[0]}{host(row.urls[0])}{/if}
                  {#if row.hasPassword}<span title={t("vault3HasPassword")}><Lock size={11} aria-label={t("vault3HasPassword")} /></span>{/if}
                  {#if row.hasTotp}<span title={t("vault3Has2fa")}><Timer size={11} aria-label={t("vault3Has2fa")} /></span>{/if}
                  {#if row.hasNotes}<span title={t("vault3HasNotes")}><StickyNote size={11} aria-label={t("vault3HasNotes")} /></span>{/if}
                </span>
              </td>
              <td class="muted">{row.username}</td>
              <td class="muted">{row.folder}</td>
              <td>
                {#if row.problem === "trash"}<span class="state warn">{t("vault3Trash")}</span>
                {:else if row.duplicate === ""}<span class="state warn">{t("vault3DuplicateFile")}</span>
                {:else if row.duplicate}<span class="state warn">{t("vault3Duplicate", { title: row.duplicate })}</span>
                {:else}<span class="state new">{t("vault3New")}</span>{/if}
                {#if row.problem === "totp"}<span class="hint">{t("vault3TotpNotes")}</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
      <div class="buttons">
        <button class="btn" disabled={!!busy} onclick={() => ((kind = null), (preview = null))}>{t("cancel")}</button>
        <button class="btn primary" disabled={!!busy || !taken} onclick={importForeign}>
          {@render spinner("import")}{t("vault3ImportCount", { n: taken })}
        </button>
      </div>
    {/if}
  </div>

  <div class="card">
    <h3><FileDown size={15} aria-hidden="true" />{t("vault3ExportTitle")}</h3>
    <p class="muted">{t("vault3ExportText")}</p>
    <form class="inline" onsubmit={exportSealed}>
      <label class="labelled">
        {t("vault3ExportWhat")}
        <select class="field" bind:value={what}>
          <option value={null}>{t("vault3ExportAll")}</option>
          {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
      </label>
      <div class="row">
        <label class="labelled">
          {t("vault3ExportPassword")}
          <input class="field" type="password" autocomplete="new-password" minlength="8" bind:value={password} required />
        </label>
        <label class="labelled">
          {t("vaultPasswordAgain")}
          <input class="field" type="password" autocomplete="new-password" bind:value={again} required />
        </label>
      </div>
      <p class="muted fine">{t("vault3ExportPasswordHelp")}</p>
      <div class="buttons">
        <button type="button" class="btn quiet" disabled={!!busy} onclick={() => (plain = true)}>{t("vault3PlainLink")}</button>
        <button type="submit" class="btn primary" disabled={!!busy || password.length < 8 || !again}>{@render spinner("export")}{t("vault3ExportRun")}</button>
      </div>
    </form>
  </div>
</section>

<ConfirmDialog bind:open={plain} title={t("vault3PlainTitle")} confirm={t("vault3PlainRun")} danger onconfirm={exportPlain}>
  <p>{t("vault3PlainText")}</p>
</ConfirmDialog>

<ConfirmDialog
  bind:open={deleting}
  title={t("vault3DeleteFileConfirm", { name: done ? fileName(done.path) : "" })}
  confirm={t("vault3DeleteFile")}
  danger
  onconfirm={remove}
>
  <p>{t("vault3DeleteFileConfirmText")}</p>
</ConfirmDialog>

<style>
  .transfer {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .avatar {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
    color: var(--accent-ink);
  }

  .titles {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .confirm {
    display: flex;
    align-items: flex-end;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    background: var(--accent-soft);
  }

  .confirm .labelled {
    flex: 1;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .card.warn {
    border-color: var(--warning);
    background: var(--waiting-soft);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  h3 :global(svg) {
    color: var(--muted);
  }

  .card.warn h3 :global(svg) {
    color: var(--warning);
  }

  h4 {
    margin: 0 0 4px;
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .card p {
    margin: 0;
  }

  .fine {
    font-size: var(--text-sm);
  }

  .pick {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .pick .mono {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .source {
    font-weight: 600;
  }

  .inline {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .row .labelled {
    flex: 1 1 200px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .folders {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .folder {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .folder .name {
    flex: 1;
    min-width: 0;
    font-weight: 500;
    overflow-wrap: anywhere;
  }

  .folder .field {
    width: 220px;
    height: 28px;
  }

  .rows {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-align: left;
    text-transform: uppercase;
  }

  td {
    padding: 7px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
    overflow-wrap: anywhere;
  }

  .check {
    width: 28px;
  }

  tr.off td:not(.check) {
    color: var(--muted);
  }

  .title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }

  .title :global(svg) {
    flex-shrink: 0;
    color: var(--muted);
  }

  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 19px;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .sub span {
    display: inline-flex;
  }

  .state {
    font-weight: 600;
  }

  .state.new {
    color: var(--done);
  }

  .state.warn {
    color: var(--warning);
  }

  .hint {
    display: block;
    margin-top: 2px;
    color: var(--muted);
    font-size: var(--text-xs);
  }
</style>
