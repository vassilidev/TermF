<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Copy from "@lucide/svelte/icons/copy";
  import Eye from "@lucide/svelte/icons/eye";
  import EyeOff from "@lucide/svelte/icons/eye-off";
  import FileInput from "@lucide/svelte/icons/file-input";
  import GitCompare from "@lucide/svelte/icons/git-compare";
  import { t } from "$lib/i18n";
  import type { EnvDiff, Ran } from "$lib/ops-logic";
  import { CLEAR_AFTER } from "$lib/ssh.svelte";
  import { app } from "$lib/state.svelte";
  import { copy } from "$lib/vault.svelte";

  type Key = Parameters<typeof t>[0];
  type RunSetup = { run: { id: number; title: string; worktree: string | null } };

  let { result, project, ask }: { result: Ran; project: string; ask: (e: unknown, retry: () => Promise<void>) => boolean } =
    $props();

  const keys = $derived(result.output.keys ?? []);
  const repos = $derived(app.workspace?.projects.find((p) => p.id === project)?.repos ?? []);
  let revealed = $state<Record<number, string>>({});
  let runs = $state<RunSetup[]>([]);
  let run = $state<number | null>(null);
  let repo = $state("");
  let file = $state(".env");
  let diffs = $state<EnvDiff[] | null>(null);
  let notice = $state("");
  let error = $state("");

  onMount(async () => {
    repo = repos[0]?.path ?? "";
    try {
      runs = (await invoke<RunSetup[]>("worktree_runs", { project })).filter((r) => r.run.worktree);
      run = runs[0]?.run.id ?? null;
    } catch {
      runs = [];
    }
  });

  const stateOf = (key: string) => diffs?.find((d) => d.key === key)?.state;
  const local = $derived(diffs?.filter((d) => d.state === "local") ?? []);
  const counts = $derived({
    same: diffs?.filter((d) => d.state === "same").length ?? 0,
    different: diffs?.filter((d) => d.state === "different").length ?? 0,
    missing: diffs?.filter((d) => d.state === "prod").length ?? 0,
    local: local.length,
  });
  const labels: Record<EnvDiff["state"], Key> = {
    same: "opssqlEnvSame",
    different: "opssqlEnvDifferent",
    prod: "opssqlEnvMissing",
    local: "opssqlEnvLocalOnly",
  };

  async function attempt(work: () => Promise<void>) {
    error = "";
    notice = "";
    try {
      await work();
    } catch (e) {
      if (!ask(e, () => attempt(work))) error = String(e);
    }
  }

  const reveal = (i: number) =>
    attempt(async () => {
      revealed[i] = await invoke<string>("ops_reveal", { result: result.id, index: i });
      setTimeout(() => delete revealed[i], CLEAR_AFTER * 1000);
    });

  const copyValue = (i: number) =>
    attempt(async () => {
      await copy(await invoke<string>("ops_env_copy", { result: result.id, index: i }));
      notice = t("opssqlEnvCopied", { key: keys[i], seconds: CLEAR_AFTER });
    });

  const write = (i: number) =>
    attempt(async () => {
      const path = await invoke<string>("ops_env_write", { result: result.id, index: i, run });
      notice = t("opssqlEnvWritten", { key: keys[i], path: `${path}/.env` });
    });

  const compare = () =>
    attempt(async () => {
      diffs = await invoke<EnvDiff[]>("ops_env_compare", { result: result.id, repo, file });
    });
</script>

<section class="env" aria-label={t("opssqlEnvTitle", { count: keys.length })}>
  <header>
    <h3>{t("opssqlEnvTitle", { count: keys.length })}</h3>
    <p class="muted">{t("opssqlEnvHelp")}</p>
  </header>

  <div class="tools">
    <label class="inline">
      <span>{t("opssqlEnvWorktree")}</span>
      <select class="field" bind:value={run} disabled={!runs.length}>
        {#each runs as r (r.run.id)}<option value={r.run.id}>#{r.run.id} · {r.run.title}</option>{:else}<option value={null}
            >{t("opssqlEnvNoWorktree")}</option
          >{/each}
      </select>
    </label>
    <form
      class="compare"
      onsubmit={(e) => {
        e.preventDefault();
        compare();
      }}
    >
      <label class="inline">
        <span>{t("opssqlEnvRepo")}</span>
        <select class="field" bind:value={repo}>
          {#each repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
        </select>
      </label>
      <input class="field mono file" aria-label={t("opssqlEnvFile")} bind:value={file} />
      <button class="btn sm" disabled={!repo || !file.trim()}><GitCompare size={13} aria-hidden="true" />{t("opssqlEnvCompare")}</button>
    </form>
  </div>

  {#if error}<p class="notice error">{error}</p>{/if}
  {#if notice}<p class="notice ok" role="status">{notice}</p>{/if}

  {#if !keys.length}
    <p class="muted">{t("opssqlEnvEmpty")}</p>
  {:else}
    <table>
      <thead>
        <tr>
          <th>{t("opssqlEnvKey")}</th>
          <th>{t("opssqlEnvValue")}</th>
          {#if diffs}<th>{t("opssqlEnvLocal")}</th>{/if}
          <th><span class="sr">{t("opssqlEnvActions")}</span></th>
        </tr>
      </thead>
      <tbody>
        {#each keys as key, i (key)}
          {@const s = stateOf(key)}
          <tr>
            <td class="key">{key}</td>
            <td class="value">
              {#if revealed[i] !== undefined}<mark>{revealed[i]}</mark>{:else}<span class="dots">••••••</span>{/if}
            </td>
            {#if diffs}
              <td><span class="pill {s ?? 'prod'}">{t(labels[s ?? "prod"])}</span></td>
            {/if}
            <td class="actions">
              {#if revealed[i] !== undefined}
                <button class="icon-btn sm" onclick={() => delete revealed[i]} title={t("opssqlEnvHide", { key })} aria-label={t("opssqlEnvHide", { key })}>
                  <EyeOff size={14} aria-hidden="true" />
                </button>
              {:else}
                <button class="icon-btn sm" onclick={() => reveal(i)} title={t("opssqlEnvReveal", { key })} aria-label={t("opssqlEnvReveal", { key })}>
                  <Eye size={14} aria-hidden="true" />
                </button>
              {/if}
              <button class="icon-btn sm" onclick={() => copyValue(i)} title={t("opssqlEnvCopy", { key })} aria-label={t("opssqlEnvCopy", { key })}>
                <Copy size={14} aria-hidden="true" />
              </button>
              <button
                class="icon-btn sm"
                disabled={run === null}
                onclick={() => write(i)}
                title={t("opssqlEnvWrite", { key })}
                aria-label={t("opssqlEnvWrite", { key })}
              >
                <FileInput size={14} aria-hidden="true" />
              </button>
            </td>
          </tr>
        {/each}
        {#each local as d (d.key)}
          <tr class="only-local">
            <td class="key">{d.key}</td>
            <td class="muted">—</td>
            <td><span class="pill local">{t("opssqlEnvLocalOnly")}</span></td>
            <td></td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if diffs}
      <p class="muted small">{t("opssqlEnvSummary", counts)} · {t("opssqlEnvCompareHelp")}</p>
    {/if}
  {/if}
</section>

<style>
  .env {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 20px;
  }

  header h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  header p {
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 20px;
    font-size: var(--text-sm);
  }

  .inline {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 500;
  }

  .inline select {
    max-width: 260px;
  }

  .compare {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .file {
    width: 120px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-strong);
    color: var(--muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-align: left;
  }

  td {
    padding: 5px 10px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }

  .key,
  .value {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .key {
    font-weight: 600;
    white-space: nowrap;
  }

  .value {
    max-width: 380px;
    overflow-wrap: anywhere;
  }

  .dots {
    color: var(--muted);
    letter-spacing: 1px;
  }

  mark {
    padding: 0 3px;
    border-radius: var(--radius-sm);
    background: var(--waiting);
    color: var(--waiting-text);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }

  .pill {
    display: inline-flex;
    padding: 1px 8px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 500;
    white-space: nowrap;
  }

  .pill.same {
    background: var(--done-soft);
    color: var(--done);
  }

  .pill.different {
    background: var(--waiting-soft);
    color: var(--warning);
  }

  .pill.prod {
    background: var(--danger-soft);
    color: var(--danger);
  }

  .pill.local {
    background: var(--hover);
    color: var(--muted);
  }

  .only-local .key {
    color: var(--muted);
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
</style>
