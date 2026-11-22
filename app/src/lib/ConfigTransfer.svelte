<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Info from "@lucide/svelte/icons/info";
  import Link from "@lucide/svelte/icons/link";
  import X from "@lucide/svelte/icons/x";
  import VaultMergeRows from "$lib/VaultMergeRows.svelte";
  import { t } from "$lib/i18n";
  import { app, shortPath } from "$lib/state.svelte";
  import { action, busy, library, type Change, type Imported } from "$lib/library.svelte";
  import { applyImport, settings, type Transfer } from "$lib/settings.svelte";
  import { failed, showVaultAll } from "$lib/vault.svelte";
  import type { Merged } from "$lib/vault-logic";

  /** Claude Code's import in the Library, or a configuration's export or import in Settings. */
  let { panel }: { panel: { kind: "claude" } | Transfer } = $props();

  const project = $derived(app.workspace?.projects.find((p) => p.id === library.scope));
  let source = $state("");
  let overwrite = $state(false);
  let report = $state<Imported[] | null>(null);
  let showUnchanged = $state(false);
  let vaultPassword = $state("");
  let vaultRows = $state<Merged[] | null>(null);
  let vaultError = $state("");
  let vaultLocked = $state(false);
  const projectName = (id: string) => app.workspace?.projects.find((p) => p.id === id)?.name ?? id;

  const kindLabel = {
    skill: "kindSkill",
    command: "kindCommand",
    role: "kindRole",
    preference: "kindPreference",
    rule: "kindRule",
    workflow: "kindWorkflow",
  } as const;
  const outcomeLabel = {
    imported: "outcomeImported",
    replaced: "outcomeReplaced",
    skipped: "outcomeSkipped",
    unsupported: "outcomeUnsupported",
  } as const;
  const statusLabel = {
    added: "statusAddedItem",
    unchanged: "statusUnchanged",
    changed: "statusChanged",
    conflict: "statusConflict",
  } as const;
  const actionLabel = {
    write: "resultWrite",
    replace: "resultReplace",
    merge: "resultMerge",
    keep: "resultKeep",
  } as const;

  function importClaude() {
    return busy(async () => {
      report = await invoke<Imported[]>("library_import_claude", {
        repo: source || null,
        project: library.scope || null,
        overwrite,
      });
      library.version++;
    });
  }

  function choose(change: Change, replace: boolean) {
    if (panel.kind !== "import") return;
    panel.replace = replace
      ? [...panel.replace, change.item]
      : panel.replace.filter((i) => i !== change.item);
  }

  function mergeHint(change: Change): string {
    return change.addedKeys.length
      ? t("mergeAdds", { keys: change.addedKeys.join(", ") })
      : t("mergeKeeps");
  }

  async function showVault(e: SubmitEvent) {
    e.preventDefault();
    if (panel.kind !== "import") return;
    vaultError = "";
    vaultLocked = false;
    try {
      vaultRows = await invoke<Merged[]>("vault_import_preview", { path: panel.path, password: vaultPassword });
    } catch (e) {
      if (/the vault is locked/.test(String(e))) vaultLocked = true;
      else vaultError = failed(e);
    }
  }

  function close() {
    if (panel.kind === "claude") library.panel = null;
    else settings.transfer = null;
    library.version++;
  }
</script>

<section class="transfer">
  <header>
    <h2>
      {#if panel.kind === "claude"}{t("claudeImport")}
      {:else if panel.kind === "exported"}{t("exportDone")}
      {:else}{panel.done ? t("importDone") : t("importTitle")}{/if}
    </h2>
    <button class="icon-btn" onclick={close} aria-label={t("close")} title={t("close")}>
      <X size={16} aria-hidden="true" />
    </button>
  </header>

  {#if panel.kind === "claude"}
    <p class="lead">
      {project ? t("claudeImportHelpProject", { project: project.name }) : t("claudeImportHelpGlobal")}
    </p>
    <div class="form">
      <label class="labelled">
        <span>{t("claudeImportFrom")}</span>
        <select class="field" bind:value={source}>
          <option value="">~/.claude</option>
          {#each project?.repos ?? [] as repo (repo.path)}
            <option value={repo.path}>{repo.name}/.claude</option>
          {/each}
        </select>
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={overwrite} />
        {t("claudeImportOverwrite")}
      </label>
      <div>
        <button class="btn primary" onclick={importClaude} disabled={library.busy}>{t("claudeImportRun")}</button>
      </div>
    </div>
    {#if report}
      {#if report.length}
        <table>
          <thead>
            <tr><th>{t("columnItem")}</th><th>{t("columnKind")}</th><th>{t("columnResult")}</th></tr>
          </thead>
          <tbody>
            {#each report as row (row.kind + row.name)}
              <tr>
                <td class="mono">{row.name}</td>
                <td>{t(kindLabel[row.kind])}</td>
                <td class="outcome {row.outcome}">{t(outcomeLabel[row.outcome])}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else}
        <p class="notice"><Info size={14} aria-hidden="true" />{t("claudeImportNothing")}</p>
      {/if}
    {/if}
  {:else if panel.kind === "exported"}
    <p class="lead">
      <Check size={16} aria-hidden="true" class="ok" />{t("exportSummary", { count: panel.files })}
    </p>
    {#if panel.skippedLinks.length}
      <div class="notice">
        <Link size={14} aria-hidden="true" />
        <div>
          {t("exportLinks")}
          <ul class="paths">
            {#each panel.skippedLinks as path (path)}<li class="mono">{path}</li>{/each}
          </ul>
        </div>
      </div>
    {/if}
  {:else}
    {@const changes = panel.preview.changes}
    {@const pending = changes.filter((c) => c.status !== "unchanged")}
    {@const same = changes.length - pending.length}
    <p class="lead mono">{shortPath(panel.path)}</p>
    {#if !panel.done}
      <p class="muted help">{t("importHelp")}</p>
    {/if}
    {#if pending.length}
      <table>
        <thead>
          <tr>
            <th>{t("columnItem")}</th>
            <th>{t("columnState")}</th>
            <th>{panel.done ? t("columnResult") : t("columnChoice")}</th>
          </tr>
        </thead>
        <tbody>
          {#each showUnchanged ? changes : pending as change (change.item)}
            {@const replaced = panel.replace.includes(change.item)}
            <tr>
              <td class="mono">{change.item}</td>
              <td>
                {#if change.linked}
                  <span class="state linked"><Link size={12} aria-hidden="true" />{t("statusLinked")}</span>
                {:else}
                  <span class="state {change.status}">{t(statusLabel[change.status])}</span>
                {/if}
                {#if change.conflicts.length}
                  <span class="keys">{t("conflictKeys", { keys: change.conflicts.join(", ") })}</span>
                {/if}
                {#if change.addedKeys.length && !change.linked}
                  <span class="keys">{t("addedKeys", { keys: change.addedKeys.join(", ") })}</span>
                {/if}
              </td>
              <td>
                {#if panel.done}
                  {t(actionLabel[action(change, panel.replace)])}
                {:else if change.linked}
                  <span class="muted">{t("linkedKept")}</span>
                {:else if change.status === "added"}
                  <span class="muted">{t("willAdd")}</span>
                {:else if change.status !== "unchanged"}
                  <div class="segmented" role="radiogroup" aria-label={change.item}>
                    <button role="radio" aria-checked={!replaced} onclick={() => choose(change, false)}>
                      {t("merge")}
                    </button>
                    <button role="radio" aria-checked={replaced} onclick={() => choose(change, true)}>
                      {t("replace")}
                    </button>
                  </div>
                  <span class="keys">{replaced ? t("replaceHint") : mergeHint(change)}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="notice"><Info size={14} aria-hidden="true" />{t("importNothing")}</p>
    {/if}
    {#if same && pending.length}
      <button class="btn sm quiet more" onclick={() => (showUnchanged = !showUnchanged)}>
        {showUnchanged ? t("hideUnchanged") : t("showUnchanged", { count: same })}
      </button>
    {/if}

    {#if panel.preview.missingRepos.length}
      <div class="notice missing">
        <FolderGit2 size={14} aria-hidden="true" />
        <div>
          {t("missingRepos")}
          <ul class="paths">
            {#each panel.preview.missingRepos as repo (repo.project + repo.path)}
              <li>
                <span class="mono">{repo.path}</span>
                {#if repo.remote}<span class="mono muted"> · {repo.remote}</span>{/if}
              </li>
            {/each}
          </ul>
        </div>
      </div>
    {/if}

    {#if panel.preview.vault && !panel.done}
      <div class="vault-part">
        <h3>{t("vault3ConfigVault")}</h3>
        <p class="muted help">{t("vault3ConfigVaultText")}</p>
        {#if vaultRows}
          <VaultMergeRows rows={vaultRows} {projectName} />
        {:else}
          <form class="vault-form" onsubmit={showVault}>
            <input
              class="field"
              type="password"
              autocomplete="off"
              aria-label={t("vault3ExportPassword")}
              placeholder={t("vault3ExportPassword")}
              bind:value={vaultPassword}
            />
            <button type="submit" class="btn" disabled={!vaultPassword}>{t("vault3ImportShow")}</button>
          </form>
        {/if}
        {#if vaultLocked}
          <p class="notice">
            <Info size={14} aria-hidden="true" />{t("vault3NotOpen")}
            <button class="btn sm" onclick={showVaultAll}>{t("vault3OpenVault")}</button>
          </p>
        {/if}
        {#if vaultError}<p class="notice error" role="alert">{vaultError}</p>{/if}
      </div>
    {/if}

    {#if !panel.done}
      <footer>
        <button class="btn" onclick={close}>{t("cancel")}</button>
        <button
          class="btn primary"
          onclick={() => applyImport(vaultRows ? vaultPassword : null)}
          disabled={settings.busy || (!pending.length && !vaultRows?.some((r) => r.change !== "unchanged"))}
        >
          {t("importRun")}
        </button>
      </footer>
    {/if}
  {/if}
</section>

<style>
  .transfer {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .transfer > * {
    max-width: 900px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .lead {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 0 16px;
    color: var(--muted);
  }

  .lead :global(.ok) {
    color: var(--done);
  }

  .help {
    margin: -8px 0 16px;
    font-size: var(--text-sm);
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 20px;
  }

  .labelled {
    max-width: 360px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .check input {
    margin: 0;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-align: left;
    text-transform: uppercase;
  }

  td {
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }

  .mono {
    overflow-wrap: anywhere;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 600;
  }

  .state.added,
  .outcome.imported,
  .outcome.replaced {
    color: var(--done);
  }

  .state.conflict {
    color: var(--warning);
  }

  .state.unchanged,
  .state.linked,
  .outcome.skipped,
  .outcome.unsupported {
    color: var(--muted);
  }

  .state.changed {
    color: var(--accent-ink);
  }

  .keys {
    display: block;
    margin-top: 2px;
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .segmented button {
    font-size: var(--text-xs);
  }

  .more {
    margin-top: 8px;
  }

  .notice {
    margin-top: 16px;
  }

  .paths {
    margin: 4px 0 0;
    padding-left: 16px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 20px;
  }

  .vault-part {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 20px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .vault-part h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .vault-part .help {
    margin: 0;
  }

  .vault-form {
    display: flex;
    gap: 8px;
    max-width: 420px;
  }

  .vault-part .notice {
    align-items: center;
  }
</style>
