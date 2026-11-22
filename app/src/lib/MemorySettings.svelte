<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import { currentProject } from "$lib/files.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { app } from "$lib/state.svelte";

  type MemorySource = "claude" | "agents" | "runs" | "library" | "memory" | "docs" | "git" | "artifacts";
  type MemoryStatus = {
    project: string;
    enabled: boolean;
    indexed: number | null;
    sources: { source: MemorySource; included: boolean; docs: number; bytes: number; error: string | null }[];
  };

  const labels: Record<MemorySource, Parameters<typeof t>[0]> = {
    claude: "expertSourceClaude",
    agents: "expertSourceAgents",
    runs: "expertSourceRuns",
    library: "expertSourceLibrary",
    memory: "expertSourceMemory",
    docs: "expertSourceDocs",
    git: "expertSourceGit",
    artifacts: "expertSourceArtifacts",
  };

  let status = $state<MemoryStatus | null>(null);
  let busy = $state(false);
  let error = $state("");

  const project = $derived(currentProject());

  /** Only while `id` is still the current project: another one's panel may be showing by then. */
  function load(id: string) {
    invoke<MemoryStatus>("memory_status", { project: id }).then(
      (s) => {
        if (app.project === id) status = s;
      },
      (e) => {
        if (app.project === id) error = String(e);
      },
    );
  }

  $effect(() => {
    const id = app.project;
    status = null;
    error = "";
    if (id !== null) load(id);
  });

  async function run(command: string, args: Record<string, unknown>) {
    const id = app.project;
    busy = true;
    error = "";
    try {
      const next = await invoke<MemoryStatus | null>(command, args);
      if (app.project === id) {
        if (next) status = next;
      } else if (app.project !== null) {
        // The settings are the same for every project: the one shown now reads them again.
        load(app.project);
      }
    } catch (e) {
      if (app.project === id) error = String(e);
    } finally {
      busy = false;
    }
  }

  function save(enabled: boolean, exclude: MemorySource[]) {
    run("memory_settings_save", { project: app.project, settings: { enabled, exclude } });
  }

  function toggle(source: MemorySource, on: boolean) {
    if (!status) return;
    const off = status.sources.filter((s) => !s.included && s.source !== source).map((s) => s.source);
    save(status.enabled, on ? off : [...off, source]);
  }
</script>

<h2>{t("expertMemoryTitle")}</h2>
<p class="help intro">{t("expertMemoryIntro")}</p>

{#if app.project === null}
  <p class="help">{t("expertMemoryNoProject")}</p>
{:else if status}
  <label class="check">
    <input
      type="checkbox"
      role="switch"
      class="switch"
      checked={status.enabled}
      disabled={busy}
      onchange={(e) =>
        save(
          e.currentTarget.checked,
          status!.sources.filter((s) => !s.included).map((s) => s.source),
        )}
    />
    {t("expertMemoryOn")}
  </label>
  <p class="help">{t("ux3t3AllProjects")}</p>
  {#if !status.enabled}
    <p class="help">{t("expertMemoryOff")}</p>
  {/if}

  <div class="project">
    <div>
      <h3>{t("expertMemoryProject", { project: project?.name ?? app.project })}</h3>
      <p class="help" role="status">
        {#if busy}
          {t("expertMemoryIndexing")}
        {:else if status.indexed}
          {t("expertMemoryIndexed", { ago: ago(status.indexed), count: status.sources.reduce((n, s) => n + s.docs, 0).toLocaleString(lang) })}
        {:else}
          {t("expertMemoryNever")}
        {/if}
      </p>
    </div>
    <button
      class="btn sm"
      disabled={busy || !status.enabled}
      onclick={() => run("memory_reindex", { project: app.project })}
    >
      <RefreshCw size={14} aria-hidden="true" />{t("expertMemoryReindex")}
    </button>
  </div>

  <fieldset disabled={!status.enabled || busy}>
    <legend>{t("expertMemorySources")}</legend>
    {#each status.sources as s (s.source)}
      <label class="source">
        <input type="checkbox" checked={s.included} onchange={(e) => toggle(s.source, e.currentTarget.checked)} />
        <span class="name">{t(labels[s.source])}</span>
        {#if s.included && status.enabled}
          <span class="count">{t("expertMemoryDocs", { count: s.docs.toLocaleString(lang) })}</span>
        {/if}
      </label>
      {#if s.error && s.included}
        <p class="notice">{t("expertMemorySourceError", { error: s.error })}</p>
      {/if}
    {/each}
  </fieldset>
  <p class="help">{t("expertMemoryAuto")}</p>
{/if}

{#if error}
  <p class="notice error" role="alert">{error}</p>
{/if}

<style>
  h2 {
    margin: 0 0 6px;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .intro {
    margin-bottom: 16px;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .project {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 20px 0 12px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }

  .project .help {
    margin-top: 2px;
  }

  fieldset {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: 6px;
    padding: 0;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .source {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    cursor: pointer;
  }

  fieldset:disabled .source {
    cursor: default;
    opacity: 0.6;
  }

  .source input {
    margin: 0;
  }

  .name {
    flex: 1;
    min-width: 0;
  }

  .count {
    color: var(--muted);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }

  .notice {
    margin: 2px 0 6px 24px;
    font-size: var(--text-sm);
  }
</style>
