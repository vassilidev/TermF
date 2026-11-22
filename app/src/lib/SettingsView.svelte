<script lang="ts">
  import type { Component } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Bot from "@lucide/svelte/icons/bot";
  import BrainCircuit from "@lucide/svelte/icons/brain-circuit";
  import Coffee from "@lucide/svelte/icons/coffee";
  import FileDown from "@lucide/svelte/icons/file-down";
  import FileUp from "@lucide/svelte/icons/file-up";
  import Folder from "@lucide/svelte/icons/folder";
  import FolderSync from "@lucide/svelte/icons/folder-sync";
  import Keyboard from "@lucide/svelte/icons/keyboard";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";
  import Search from "@lucide/svelte/icons/search";
  import Workflow from "@lucide/svelte/icons/workflow";
  import ConfigTransfer from "$lib/ConfigTransfer.svelte";
  import ContextReset from "$lib/ContextReset.svelte";
  import KeepAwake from "$lib/KeepAwake.svelte";
  import MemorySettings from "$lib/MemorySettings.svelte";
  import ShortcutEditor from "$lib/ShortcutEditor.svelte";
  import SyncView from "$lib/SyncView.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import SectionNav from "$lib/ui/SectionNav.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { ci, type CiNotify } from "$lib/ci.svelte";
  import { claudeView, loadArtifacts, type Artifacts } from "$lib/claude.svelte";
  import { t } from "$lib/i18n";
  import { actions } from "$lib/keymap-logic";
  import { exportConfig, importConfig, settings, type Section } from "$lib/settings.svelte";
  import { app, load } from "$lib/state.svelte";
  import { summary } from "$lib/sync.svelte";

  type Key = Parameters<typeof t>[0];

  /** Each section, with the texts the search looks into. */
  const sections: { id: Section; label: Key; icon: Component; words: Key[] }[] = [
    {
      id: "awake",
      label: "keepAwake",
      icon: Coffee,
      words: ["keepAwakeAuto", "keepAwakeAlways", "keepAwakeOff", "keepDisplayOn", "keepAwakeHelp"],
    },
    {
      id: "context",
      label: "contextSettings",
      icon: RotateCcw,
      words: ["contextAuto", "contextResetAt", "contextWarningAt", "contextSettingsHelp"],
    },
    {
      id: "keys",
      label: "shortcuts",
      icon: Keyboard,
      words: ["settingsKeysHelp", ...actions.flatMap((a) => (a.help ? [a.label, a.help] : [a.label]))],
    },
    { id: "ci", label: "viewCi", icon: Workflow, words: ["ciNotify", "ciNotifyHelp", "ciNotifyMine", "ciNotifyAll"] },
    { id: "claude", label: "claudeViewTitle", icon: Bot, words: ["claudeViewTabArtifacts", "claudeViewAuto", "claudeRedesignSkipConfirm"] },
    {
      id: "memory",
      label: "expertMemoryTitle",
      icon: BrainCircuit,
      words: ["expertMemoryIntro", "expertMemorySources", "expertSourceClaude", "expertSourceGit", "expertSourceDocs"],
    },
    {
      id: "config",
      label: "settingsConfig",
      icon: Folder,
      words: ["configFolder", "reload", "configExportLong", "configImportLong"],
    },
    { id: "sync", label: "syncTitle", icon: FolderSync, words: ["syncIntro", "syncDevice", "syncRemote", "syncPassword"] },
  ];

  let query = $state("");
  let error = $state("");

  const fold = (s: string) =>
    s
      .normalize("NFD")
      .replace(/\p{Diacritic}/gu, "")
      .toLowerCase();
  const found = $derived.by(() => {
    const words = fold(query).split(/\s+/).filter(Boolean);
    return sections.filter((s) => {
      const text = fold([s.label, ...s.words].map((k) => t(k)).join(" "));
      return words.every((w) => text.includes(w));
    });
  });

  // A search that hides the open section opens the first one it found.
  $effect(() => {
    if (found.length && !found.some((s) => s.id === settings.section)) settings.section = found[0].id;
  });

  $effect(() => {
    void settings.section;
    error = "";
  });

  $effect(() => {
    if (settings.section === "claude" && !claudeView.artifacts) loadArtifacts();
  });

  const tone = $derived(summary(app.sync).tone);

  function setNotify(e: Event) {
    const select = e.currentTarget as HTMLSelectElement;
    invoke<CiNotify>("ci_notify_set", { notify: select.value as CiNotify }).then(
      (n) => (ci.notify = n),
      (e) => {
        select.value = ci.notify ?? "mine";
        error = String(e);
      },
    );
  }

  function setArtifacts(e: Event, command: "artifacts_auto" | "artifacts_confirm", arg: "on" | "skip") {
    const input = e.currentTarget as HTMLInputElement;
    invoke<Artifacts>(command, { [arg]: input.checked }).then(
      (a) => (claudeView.artifacts = a),
      (e) => {
        input.checked = !input.checked;
        error = String(e);
      },
    );
  }
</script>

<div class="settings-view">
  <ViewHeader title={t("settings")} context={t("settingsContext")} />
  <div class="body">
    {#snippet search()}
      <div class="search">
        <Search size={14} aria-hidden="true" />
        <input
          class="field"
          type="search"
          placeholder={t("navSearch")}
          aria-label={t("settingsSearch")}
          bind:value={query}
        />
      </div>
    {/snippet}
    <SectionNav
      items={found.map((s) => ({
        id: s.id,
        label: t(s.label),
        icon: s.icon,
        dot: s.id !== "sync" ? null : tone === "error" ? "error" : tone === "attention" ? "warn" : null,
        dotLabel: t("syncNeedsYou"),
      }))}
      current={settings.section}
      label={t("settings")}
      onselect={(id) => (settings.section = id)}
      top={search}
      empty={t("filesNoResults")}
    />

    <div class="content">
      {#if error || (settings.section === "config" && settings.error)}
        <!-- Above the import preview too, whose own errors land here. -->
        <div class="banners">
          {#if error}<Banner kind="error" ondismiss={() => (error = "")}>{error}</Banner>{/if}
          {#if settings.section === "config" && settings.error}
            <Banner kind="error" ondismiss={() => (settings.error = "")}>{settings.error}</Banner>
          {/if}
        </div>
      {/if}
      {#if settings.section === "sync"}
        <SyncView />
      {:else if settings.section === "config" && settings.transfer}
        <ConfigTransfer panel={settings.transfer} />
      {:else}
        <div class="page">
          {#if settings.section === "awake"}
            <KeepAwake />
          {:else if settings.section === "context"}
            <ContextReset />
          {:else if settings.section === "keys"}
            <ShortcutEditor filter={query} />
          {:else if settings.section === "ci"}
            <h2>{t("viewCi")}</h2>
            <label class="labelled">
              <span>{t("ciNotify")}</span>
              <select class="field" value={ci.notify ?? "mine"} onchange={setNotify}>
                <option value="mine">{t("ciNotifyMine")}</option>
                <option value="all">{t("ciNotifyAll")}</option>
                <option value="off">{t("ciNotifyOff")}</option>
              </select>
            </label>
            <p class="help">{t("ciNotifyHelp")}</p>
          {:else if settings.section === "claude"}
            <h2>{t("claudeViewTitle")}</h2>
            <h3>{t("claudeViewTabArtifacts")}</h3>
            <label class="check">
              <input
                type="checkbox"
                role="switch"
                class="switch"
                checked={(claudeView.artifacts?.refreshHours ?? 0) > 0}
                disabled={!claudeView.artifacts}
                onchange={(e) => setArtifacts(e, "artifacts_auto", "on")}
              />
              {t("claudeViewAuto", { hours: claudeView.artifacts?.refreshHours || 6 })}
            </label>
            <label class="check">
              <input
                type="checkbox"
                role="switch"
                class="switch"
                checked={claudeView.artifacts?.deleteWithoutConfirmation ?? false}
                disabled={!claudeView.artifacts}
                onchange={(e) => setArtifacts(e, "artifacts_confirm", "skip")}
              />
              {t("claudeRedesignSkipConfirm")}
            </label>
            <p class="help">{t("claudeRedesignSkipConfirmHelp")}</p>
          {:else if settings.section === "memory"}
            <MemorySettings />
          {:else if settings.section === "config"}
            <h2>{t("settingsConfig")}</h2>
            {#if app.workspace}
              <h3>{t("configFolder")}</h3>
              <code title={app.workspace.configDir}>{app.workspace.configDir}</code>
              <div class="buttons">
                <button class="btn sm" onclick={load}>
                  <RefreshCw size={14} aria-hidden="true" />{t("reload")}
                </button>
                <button class="btn sm" onclick={exportConfig} disabled={settings.busy}>
                  <FileDown size={14} aria-hidden="true" />{t("configExportLong")}
                </button>
                <button class="btn sm" onclick={importConfig} disabled={settings.busy}>
                  <FileUp size={14} aria-hidden="true" />{t("configImportLong")}
                </button>
              </div>
            {/if}
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .settings-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .body {
    display: grid;
    flex: 1;
    grid-template: minmax(0, 1fr) / 224px minmax(0, 1fr);
    min-height: 0;
  }

  .search {
    position: relative;
    color: var(--muted);
  }

  .search :global(svg) {
    position: absolute;
    top: 8px;
    left: 10px;
    pointer-events: none;
  }

  .search .field {
    padding-left: 30px;
  }

  .content {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  /* Same frame as the sync and transfer panels, which bring their own. */
  .page {
    flex: 1;
    min-height: 0;
    padding: 20px 28px 32px;
    overflow-y: auto;
  }

  .page > :global(*) {
    max-width: 760px;
  }

  h2 {
    margin: 0 0 12px;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-base);
    font-weight: 600;
  }

  .labelled {
    max-width: 320px;
  }

  .help {
    margin: 6px 0 0;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .check + .check {
    margin-top: 12px;
  }

  code {
    display: block;
    margin-bottom: 10px;
    overflow: hidden;
    color: var(--muted);
    font: var(--text-sm) var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
