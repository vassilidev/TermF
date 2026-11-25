<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CloudUpload from "@lucide/svelte/icons/cloud-upload";
  import ArrowRightLeft from "@lucide/svelte/icons/arrow-right-left";
  import Copy from "@lucide/svelte/icons/copy";
  import Eye from "@lucide/svelte/icons/eye";
  import FlaskConical from "@lucide/svelte/icons/flask-conical";
  import FolderGit2 from "@lucide/svelte/icons/folder-git-2";
  import Info from "@lucide/svelte/icons/info";
  import KeyRound from "@lucide/svelte/icons/key-round";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Lock from "@lucide/svelte/icons/lock";
  import Plug from "@lucide/svelte/icons/plug";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import Server from "@lucide/svelte/icons/server";
  import ShieldAlert from "@lucide/svelte/icons/shield-alert";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import Unplug from "@lucide/svelte/icons/unplug";
  import X from "@lucide/svelte/icons/x";
  import KeyTest from "$lib/KeyTest.svelte";
  import KeyTools from "$lib/KeyTools.svelte";
  import SecretView from "$lib/SecretView.svelte";
  import SshPanel from "$lib/SshPanel.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { askAI, provide } from "$lib/ask.svelte";
  import { ago, t } from "$lib/i18n";
  import { shortPath } from "$lib/state.svelte";
  import { act, algorithm, readable, refresh, ssh, type Host, type HostTest, type RepoKeys, type SshKey } from "$lib/ssh.svelte";

  const inventory = $derived(ssh.inventory);
  const keys = $derived(inventory?.keys ?? []);
  const current = $derived(keys.find((k) => k.path === ssh.selected) ?? keys[0]);
  const tooOpen = $derived(keys.filter((k) => k.tooOpen));
  const trash = $derived(inventory ? `${shortPath(inventory.dir)}/.termf-trash` : "");
  const emptyHere = $derived(
    !!inventory &&
      !ssh.panel &&
      ((ssh.tab === "keys" && !keys.length) || (ssh.tab === "hosts" && !inventory.hosts.length) || (ssh.tab === "agent" && !inventory.agent.error && !inventory.agent.keys.length)),
  );

  // The key shown goes with a question: its public side only, never the private file's content.
  $effect(() => {
    const key = current;
    return provide("ssh", {
      chips: key ? [{ id: "ssh-key", kind: "object", label: key.name, text: [key.algorithm, key.fingerprint, shortPath(key.path)].filter(Boolean).join(" · ") }] : [],
    });
  });

  const askDevops = (draft: string) => void askAI({ role: "devops", draft });

  /** The inline step open under the current key's actions. */
  let asking = $state<"" | "agent" | "upload" | "trash" | "reveal" | "test">("");
  let passphrase = $state("");
  let title = $state("");
  /** By host: the last test, or null while it runs. */
  let tests = $state<Record<string, HostTest | null>>({});

  const tabs = [
    ["keys", "sshTabKeys"],
    ["agent", "sshTabAgent"],
    ["hosts", "sshTabHosts"],
    ["repos", "sshTabRepos"],
  ] as const;

  function count(tab: (typeof tabs)[number][0]): number {
    if (!inventory) return 0;
    return tab === "agent" ? inventory.agent.keys.length : inventory[tab].length;
  }

  function select(path: string) {
    ssh.selected = path;
    asking = "";
    passphrase = "";
  }

  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

  /** A key file as the user knows it: its name when it is one of the listed keys. */
  function keyName(file: string): string {
    return keys.find((k) => k.path === file || k.publicPath === file)?.name ?? shortPath(file);
  }

  async function addToAgent(key: SshKey) {
    const done = await act(async () => {
      await invoke("ssh_agent_add", { path: key.path, passphrase: passphrase || null });
      return t("sshAgentAdded", { name: key.name });
    });
    if (done) asking = "";
    passphrase = "";
  }

  function removeFromAgent(key: SshKey) {
    return act(async () => {
      await invoke("ssh_agent_remove", { path: key.path });
      return t("sshAgentRemoved", { name: key.name });
    });
  }

  async function copy(key: SshKey) {
    ssh.error = "";
    try {
      await navigator.clipboard.writeText(key.publicKey ?? "");
      ssh.done = t("sshCopied", { name: key.name });
    } catch (e) {
      ssh.error = String(e);
    }
  }

  async function upload(key: SshKey) {
    const done = await act(async () => {
      await invoke("ssh_github_upload", { path: key.path, title });
      return t("sshUploaded", { name: key.name });
    });
    if (done) asking = "";
  }

  async function moveToTrash(key: SshKey) {
    await act(async () => {
      const dir = await invoke<string>("ssh_trash", { path: key.path });
      return t("sshTrashed", { name: key.name, dir: shortPath(dir) });
    });
    asking = "";
  }

  function fix() {
    return act(async () => {
      const fixed = await invoke<string[]>("ssh_fix_permissions");
      return fixed.length ? t("sshFixed", { count: fixed.length }) : t("sshFixedNone");
    });
  }

  async function test(host: string) {
    tests[host] = null;
    try {
      tests[host] = await invoke<HostTest>("ssh_test", { host });
    } catch (e) {
      tests[host] = { ok: false, message: String(e) };
    }
  }

  async function assign(repo: RepoKeys, path: string, list: HTMLSelectElement, was: string) {
    const name = fileName(repo.path);
    const done = await act(async () => {
      await invoke("ssh_assign", { repo: repo.path, path: path || null });
      return path ? t("sshAssigned", { repo: name, name: keyName(path) }) : t("sshUnassigned", { repo: name });
    });
    // A refused change puts the list back.
    if (!done) list.value = was;
  }

  /** A single named host of a block: what ssh can connect to, and what the form can edit. */
  const single = (h: Host) => h.line > 0 && h.patterns.length === 1 && !/[*?!]/.test(h.patterns[0]);
  // The form only writes ~/.ssh/config, not the files it includes.
  const inMainConfig = (h: Host) =>
    !!inventory && h.file.startsWith(inventory.dir) && h.file.slice(inventory.dir.length + 1) === "config";
  const inSshDir = (file: string) =>
    !!inventory && file.startsWith(inventory.dir) && /[\\/]/.test(file.charAt(inventory.dir.length));

  /** Why saving this block from the form would lose part of it: the form sets one key, inside ~/.ssh. */
  function byHand(h: Host): "sshEditManyKeys" | "sshEditOutside" | null {
    if (h.identityFiles.length > 1) return "sshEditManyKeys";
    return h.identityFiles.every(inSshDir) ? null : "sshEditOutside";
  }

  function target(h: Host): string {
    return `${h.user ? `${h.user}@` : ""}${h.hostName ?? h.patterns.join(" ")}${h.port ? `:${h.port}` : ""}`;
  }

  function mode(m: number | null): string {
    return m === null ? "—" : m.toString(8).padStart(3, "0");
  }
</script>

{#snippet testButton(host: string)}
  <button
    class="btn sm"
    onclick={() => test(host)}
    disabled={tests[host] === null}
    title={t("sshTestTitle", { host })}
    aria-label={t("sshTestTitle", { host })}
  >
    {#if tests[host] === null}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
    {t("sshTest")}
  </button>
{/snippet}

{#snippet testResult(host: string)}
  {@const result = tests[host]}
  {#if result}
    <p class="test" class:ok={result.ok} class:failed={!result.ok} role="status">
      {#if result.ok}<Check size={13} aria-hidden="true" />{:else}<X size={13} aria-hidden="true" />{/if}
      <span class="mono">{result.message}</span>
    </p>
  {/if}
{/snippet}

<div class="ssh">
  <ViewHeader title={t("viewSsh")} context={inventory ? shortPath(inventory.dir) : ""} mono>
    {#snippet actions()}
      <button class="btn sm" onclick={refresh}>
        <RefreshCw size={14} aria-hidden="true" />{t("sshRefresh")}
      </button>
      <button class="btn sm" onclick={() => { ssh.panel = { kind: "host", host: null }; }}>
        <Server size={14} aria-hidden="true" />{t("sshHostNew")}
      </button>
      <!-- An empty tab holds the screen's one main action. -->
      <button class="btn sm" class:primary={!emptyHere} onclick={() => { ssh.panel = { kind: "generate" }; }}>
        <Plus size={14} aria-hidden="true" />{t("sshGenerate")}
      </button>
    {/snippet}
  </ViewHeader>

  <Tabs
    id="ssh"
    label={t("viewSsh")}
    tabs={tabs.map(([id, label]) => ({ id, label: t(label), count: inventory ? count(id) : null }))}
    selected={ssh.panel ? null : ssh.tab}
    onselect={(tab) => { ssh.tab = tab as typeof ssh.tab; ssh.panel = null; ssh.error = ""; ssh.done = ""; }}
  >
    {#if ssh.error || ssh.done || (!ssh.panel && (tooOpen.length || inventory?.problems.length))}
      <div class="banners">
        {#if ssh.error}
          <Banner kind="error" ondismiss={() => (ssh.error = "")}>{ssh.error}</Banner>
        {/if}
        {#if ssh.done}
          <Banner kind="ok" ondismiss={() => (ssh.done = "")}>{ssh.done}</Banner>
        {/if}
        {#if tooOpen.length && !ssh.panel}
          <Banner kind="warn" icon={ShieldAlert}>
            {t("sshTooOpenBanner", { keys: tooOpen.map((k) => k.name).join(", ") })}
            {#snippet action()}
              <button class="btn sm" onclick={fix} disabled={ssh.busy}>{t("sshFix")}</button>
            {/snippet}
          </Banner>
        {/if}
        {#if inventory?.problems.length && !ssh.panel}
          <div class="notice">
            <Info size={14} aria-hidden="true" />
            <div>
              {t("sshProblems")}
              <ul class="paths">
                {#each inventory.problems as problem, i (i)}
                  <li><span class="mono">{shortPath(problem.path)}</span> · {problem.code === "outside" ? t("uxfT4SshOutside") : problem.error}</li>
                {/each}
              </ul>
            </div>
          </div>
        {/if}
      </div>
    {/if}

    {#if ssh.panel}
      {#key ssh.panel}
        {#if ssh.panel.kind === "generate" || ssh.panel.kind === "convert"}<KeyTools />{:else}<SshPanel />{/if}
      {/key}
    {:else if !inventory}
      {#if !ssh.error}
        <div class={ssh.tab === "keys" ? "split" : "table-pane"}>
          <div class:list={ssh.tab === "keys"}><Skeleton shape="list" rows={6} label={t("sshLoading")} /></div>
        </div>
      {/if}
    {:else if ssh.tab === "keys"}
      {#if !keys.length}
        <EmptyState
          icon={KeyRound}
          title={t("sshNoKeys")}
          text={t("sshNoKeysHelp")}
          action={{ label: t("sshGenerate"), icon: Plus, run: () => { ssh.panel = { kind: "generate", usage: "ssh" }; } }}
          onask={() => askDevops(t("restSshAskKeys"))}
          askLabel={t("aigAskRole", { role: "devops" })}
        />
      {:else}
        <div class="split">
          <ul class="list">
            {#each keys as key (key.path)}
              <li class="row" class:current={key === current}>
                <button class="pick" aria-current={key === current} onclick={() => select(key.path)}>
                  <span class="name">{key.name}</span>
                  <span class="desc">{algorithm(key.algorithm)}{key.comment ? ` · ${key.comment}` : ""}</span>
                </button>
                {#if key.tooOpen}
                  <span class="flag warn-icon" role="img" title={t("sshTooOpenTitle")} aria-label={t("sshTooOpenTitle")}>
                    <TriangleAlert size={14} aria-hidden="true" />
                  </span>
                {/if}
                {#if key.encrypted}
                  <span class="flag" role="img" title={t("sshProtectedTitle")} aria-label={t("sshProtectedTitle")}>
                    <Lock size={13} aria-hidden="true" />
                  </span>
                {/if}
                {#if key.orphan}<span class="badge">{t("sshOrphanBadge")}</span>{/if}
                {#if key.inAgent}<span class="badge agent">{t("sshAgentBadge")}</span>{/if}
              </li>
            {/each}
          </ul>

          <section class="detail" aria-live="polite">
            {#if current}
              {@const key = current}
              <header>
                <div>
                  <div class="title">
                    <h2>{key.name}</h2>
                    <span class="badge">{algorithm(key.algorithm)}</span>
                    {#if key.inAgent}<span class="badge agent">{t("sshAgentBadge")}</span>{/if}
                  </div>
                  <p class="description">{key.comment || t("sshNoComment")}</p>
                </div>
                <button class="btn sm quiet danger" aria-expanded={asking === "trash"} onclick={() => (asking = "trash")}>
                  <Trash2 size={14} aria-hidden="true" />{t("sshTrash")}
                </button>
              </header>

              <div class="actions">
                {#if key.inAgent}
                  <button class="btn sm" onclick={() => removeFromAgent(key)} disabled={ssh.busy}>
                    <Unplug size={14} aria-hidden="true" />{t("sshAgentRemove")}
                  </button>
                {:else if !key.orphan}
                  <button
                    class="btn sm"
                    aria-expanded={asking === "agent"}
                    disabled={ssh.busy}
                    onclick={() => (key.encrypted ? (asking = "agent") : addToAgent(key))}
                  >
                    <Plug size={14} aria-hidden="true" />{t("sshAgentAdd")}
                  </button>
                {/if}
                {#if key.publicKey}
                  <button class="btn sm" onclick={() => copy(key)}>
                    <Copy size={14} aria-hidden="true" />{t("sshCopy")}
                  </button>
                  <button
                    class="btn sm"
                    aria-expanded={asking === "upload"}
                    onclick={() => ((asking = "upload"), (title = key.comment || key.name))}
                  >
                    <CloudUpload size={14} aria-hidden="true" />{t("sshUpload")}
                  </button>
                {/if}
                <button class="btn sm" onclick={() => { ssh.panel = { kind: "rotate", key }; }}>
                  <RotateCw size={14} aria-hidden="true" />{t("sshRotate")}
                </button>
                {#if !key.orphan}
                  <button class="btn sm" aria-expanded={asking === "reveal"} onclick={() => (asking = asking === "reveal" ? "" : "reveal")}>
                    <Eye size={14} aria-hidden="true" />{t("keysReveal")}
                  </button>
                  {#if readable(key.algorithm)}
                    <button class="btn sm" aria-expanded={asking === "test"} onclick={() => (asking = asking === "test" ? "" : "test")}>
                      <FlaskConical size={14} aria-hidden="true" />{t("keysTest")}
                    </button>
                    <button class="btn sm" onclick={() => { ssh.panel = { kind: "convert", path: key.path }; }}>
                      <ArrowRightLeft size={14} aria-hidden="true" />{t("keysConvert")}
                    </button>
                  {/if}
                {/if}
              </div>

              {#if asking === "agent"}
                <form
                  class="ask"
                  onsubmit={(e) => {
                    e.preventDefault();
                    addToAgent(key);
                  }}
                >
                  <label class="labelled">
                    <span>{t("sshPassphraseFor", { name: key.name })}</span>
                    <input class="field" type="password" autocomplete="off" bind:value={passphrase} {@attach (el) => el.focus()} />
                  </label>
                  <button type="button" class="btn sm" onclick={() => ((asking = ""), (passphrase = ""))}>{t("cancel")}</button>
                  <button type="submit" class="btn sm primary" disabled={ssh.busy}>{t("sshAdd")}</button>
                  <p class="muted hint">{t("sshPassphraseHint")}</p>
                </form>
              {:else if asking === "upload"}
                <form
                  class="ask"
                  onsubmit={(e) => {
                    e.preventDefault();
                    upload(key);
                  }}
                >
                  <label class="labelled">
                    <span>{t("sshUploadTitle")}</span>
                    <input class="field" bind:value={title} {@attach (el) => el.focus()} />
                  </label>
                  <button type="button" class="btn sm" onclick={() => (asking = "")}>{t("cancel")}</button>
                  <button type="submit" class="btn sm primary" disabled={ssh.busy}>
                    {#if ssh.busy}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
                    {t("sshUploadRun")}
                  </button>
                </form>
              {:else if asking === "reveal"}
                <div class="inline">
                  <SecretView
                    name={key.name}
                    encrypted={!!key.encrypted}
                    load={(passphrase) => invoke<string>("keys_reveal", { path: key.path, passphrase })}
                  />
                </div>
              {:else if asking === "test"}
                <div class="inline">
                  <KeyTest path={key.path} name={key.name} encrypted={!!key.encrypted} {key} />
                </div>
              {:else if asking === "trash"}
                <div class="ask confirm" role="alert">
                  <span>{t("sshTrashConfirm", { name: key.name, dir: trash })}</span>
                  <button class="btn sm" onclick={() => (asking = "")}>{t("cancel")}</button>
                  <button class="btn sm danger" onclick={() => moveToTrash(key)} disabled={ssh.busy}>{t("sshTrash")}</button>
                </div>
              {/if}

              {#if key.orphan}
                <p class="notice"><Info size={14} aria-hidden="true" />{t("sshOrphanHelp")}</p>
              {/if}

              <dl class="meta">
                <dt>{t("sshMetaType")}</dt>
                <dd>{algorithm(key.algorithm)}{key.bits ? ` · ${t("sshBits", { bits: key.bits })}` : ""}</dd>
                <dt>{t("sshMetaFingerprint")}</dt>
                <dd class="mono">{key.fingerprint ?? "—"}</dd>
                {#if !key.orphan}
                  <dt>{t("sshMetaPassphrase")}</dt>
                  <dd>
                    {#if key.encrypted}
                      <span class="yes"><Lock size={12} aria-hidden="true" />{t("sshProtected")}</span>
                    {:else}
                      {t("sshUnprotected")}
                    {/if}
                  </dd>
                {/if}
                <dt>{t("sshMetaPermissions")}</dt>
                <dd>
                  {#if key.tooOpen}
                    <span class="warn-text">{t("sshPermsTooOpen", { mode: mode(key.mode) })}</span>
                  {:else}
                    <span class="mono">{mode(key.mode)}</span>
                  {/if}
                </dd>
                <dt>{t("sshMetaAgent")}</dt>
                <dd>{key.inAgent ? t("sshLoaded") : t("sshNotLoaded")}</dd>
                <dt>{t("sshMetaHosts")}</dt>
                <dd>{key.hosts.length ? key.hosts.join(", ") : t("sshNone")}</dd>
                <dt>{t("sshMetaRepos")}</dt>
                <dd>
                  {#if key.repos.length}
                    {#each key.repos as repo (repo)}<span class="mono block">{shortPath(repo)}</span>{/each}
                  {:else}
                    {t("sshNone")}
                  {/if}
                </dd>
                <dt>{t("sshMetaFiles")}</dt>
                <dd class="mono">
                  <span class="block">{shortPath(key.path)}</span>
                  {#if key.publicPath && !key.orphan}<span class="block">{shortPath(key.publicPath)}</span>{/if}
                </dd>
                {#if key.modified}
                  <dt>{t("sshMetaModified")}</dt>
                  <dd>{ago(key.modified)}</dd>
                {/if}
              </dl>

              {#if key.publicKey}
                <h3>{t("sshPublicKey")}</h3>
                <pre class="public">{key.publicKey}</pre>
              {/if}
            {/if}
          </section>
        </div>
      {/if}
    {:else if ssh.tab === "agent"}
      <div class="table-pane">
        {#if inventory.agent.error}
          <p class="notice"><Info size={14} aria-hidden="true" />{t("sshAgentError", { error: inventory.agent.error })}</p>
        {:else if !inventory.agent.keys.length}
          <EmptyState
            icon={Plug}
            title={t("restSshAgentEmpty")}
            text={t("restSshAgentEmptyText")}
            action={keys.length
              ? { label: t("restSshAgentPick"), icon: KeyRound, run: () => (ssh.tab = "keys") }
              : { label: t("sshGenerate"), icon: Plus, run: () => { ssh.panel = { kind: "generate", usage: "ssh" }; } }}
          />
        {:else}
          <table>
            <thead>
              <tr>
                <th>{t("sshMetaType")}</th>
                <th>{t("sshMetaFingerprint")}</th>
                <th>{t("sshMetaComment")}</th>
                <th>{t("sshColFile")}</th>
                <th><span class="sr-only">{t("sshColActions")}</span></th>
              </tr>
            </thead>
            <tbody>
              {#each inventory.agent.keys as agentKey, i (i)}
                {@const file = keys.find((k) => k.fingerprint === agentKey.fingerprint)}
                <tr>
                  <td class="nowrap">{agentKey.kind}{agentKey.bits ? ` · ${agentKey.bits}` : ""}</td>
                  <td class="mono">{agentKey.fingerprint}</td>
                  <td>{agentKey.comment}</td>
                  <td>
                    {#if file}
                      <button class="btn link" onclick={() => (select(file.path), (ssh.tab = "keys"))}>{file.name}</button>
                    {:else}
                      <span class="badge" title={t("sshAgentOnlyTitle")}>{t("sshAgentOnly")}</span>
                    {/if}
                  </td>
                  <td class="end">
                    {#if file}
                      <button class="btn sm" onclick={() => removeFromAgent(file)} disabled={ssh.busy}>
                        <Unplug size={14} aria-hidden="true" />{t("sshAgentRemove")}
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {:else if ssh.tab === "hosts"}
      <div class="table-pane">
        {#if !inventory.hosts.length}
          <EmptyState
            icon={Server}
            title={t("restSshHostsEmpty")}
            text={t("restSshHostsEmptyText")}
            action={{ label: t("sshHostNew"), icon: Plus, run: () => { ssh.panel = { kind: "host", host: null }; } }}
            onask={() => askDevops(t("restSshAskHosts"))}
            askLabel={t("aigAskRole", { role: "devops" })}
          />
        {:else}
          <table>
            <thead>
              <tr>
                <th>{t("sshColHost")}</th>
                <th>{t("sshColTarget")}</th>
                <th>{t("sshColKey")}</th>
                <th>{t("sshColDeclared")}</th>
                <th><span class="sr-only">{t("sshColActions")}</span></th>
              </tr>
            </thead>
            <tbody>
              <!-- Index keys: a file included twice lists its blocks twice. -->
              {#each inventory.hosts as host, i (i)}
                {@const tested = single(host) && !!tests[host.patterns[0]]}
                {@const editable = single(host) && inMainConfig(host)}
                {@const reason = editable ? byHand(host) : null}
                <tr class:tested>
                  <td>
                    {#if host.line === 0}
                      <span class="muted">{t("sshGlobalOptions")}</span>
                    {:else}
                      <span class="strong mono">{host.patterns.join(" ")}</span>
                    {/if}
                  </td>
                  <td class="mono">{host.hostName || host.user || host.port ? target(host) : "—"}</td>
                  <td>
                    {#each host.identityFiles as file, j (j)}<span class="block">{keyName(file)}</span>{:else}—{/each}
                    {#if host.identitiesOnly}<span class="badge">IdentitiesOnly</span>{/if}
                    {#if reason}<span class="muted small block">{t(reason)}</span>{/if}
                  </td>
                  <td class="mono muted file">{shortPath(host.file)}{host.line ? `:${host.line}` : ""}</td>
                  <td class="end">
                    {#if single(host)}
                      <div class="row-actions">
                        {@render testButton(host.patterns[0])}
                        {#if editable && !reason}
                          <button class="btn sm" onclick={() => { ssh.panel = { kind: "host", host }; }}>{t("sshEdit")}</button>
                        {/if}
                      </div>
                    {/if}
                  </td>
                </tr>
                {#if tested}
                  <tr class="result">
                    <td colspan="5">{@render testResult(host.patterns[0])}</td>
                  </tr>
                {/if}
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {:else}
      <div class="table-pane">
        {#if !inventory.repos.length}
          <EmptyState
            icon={FolderGit2}
            title={t("restSshReposEmpty")}
            text={t("restSshReposEmptyText")}
            onask={() => askDevops(t("restSshAskRepos"))}
            askLabel={t("aigAskRole", { role: "devops" })}
          />
        {:else}
          <table>
            <thead>
              <tr>
                <th>{t("sshColRepo")}</th>
                <th>{t("sshColRemotes")}</th>
                <th>{t("sshColKey")}</th>
                <th>{t("sshColAssign")}</th>
              </tr>
            </thead>
            <tbody>
              {#each inventory.repos as repo (repo.path)}
                {@const assigned = repo.sshCommand
                  ? (keys.find((k) => !k.orphan && repo.identityFiles.includes(k.path))?.path ?? "?")
                  : ""}
                <tr>
                  <td>
                    <span class="strong block">{fileName(repo.path)}</span>
                    <span class="mono muted">{shortPath(repo.path)}</span>
                  </td>
                  <td>
                    {#each repo.remotes as remote (remote.name)}
                      <div class="remote">
                        <span class="mono"><span class="muted">{remote.name}</span> {remote.url}</span>
                        {#if !repo.sshCommand}{@render testButton(remote.host)}{/if}
                      </div>
                      {#if !repo.sshCommand}{@render testResult(remote.host)}{/if}
                    {/each}
                  </td>
                  <td>
                    {#each repo.identityFiles as file, i (i)}<span class="block">{keyName(file)}</span>{:else}{t("sshDefaultKeys")}{/each}
                    <span class="muted small block">
                      {repo.sshCommand ? t("sshViaCommand") : repo.identityFiles.length ? t("sshViaConfig") : ""}
                    </span>
                  </td>
                  <td>
                    <select
                      class="field"
                      value={assigned}
                      disabled={ssh.busy}
                      aria-label={t("sshAssignFor", { repo: fileName(repo.path) })}
                      onchange={(e) => assign(repo, e.currentTarget.value, e.currentTarget, assigned)}
                    >
                      <option value="">{t("sshAssignNone")}</option>
                      {#if assigned === "?"}
                        <option value="?" disabled>{repo.identityFiles[0] ? shortPath(repo.identityFiles[0]) : repo.sshCommand}</option>
                      {/if}
                      {#each keys.filter((k) => !k.orphan) as key (key.path)}
                        <option value={key.path}>{key.name}</option>
                      {/each}
                    </select>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </div>
    {/if}
  </Tabs>
</div>

<style>
  /* A key's long file name wraps instead of running out of its column. */
  .btn.link {
    white-space: normal;
    overflow-wrap: anywhere;
    text-align: left;
  }

  .ssh {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .banners .notice > div {
    flex: 1;
  }

  .paths {
    margin: 4px 0 0;
    padding-left: 16px;
  }

  .flag {
    display: inline-grid;
    place-items: center;
    color: var(--muted);
  }

  .warn-icon,
  .warn-text {
    color: var(--warning);
  }

  .warn-text {
    font-weight: 500;
  }

  /* No tinted fill: on a selected row it would bring the text under 4.5:1. */
  .agent {
    color: var(--done);
  }

  .detail header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }

  h3 {
    margin: 0 0 6px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .description {
    margin: 4px 0 0;
    color: var(--muted);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 16px 0 0;
  }

  .ask {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px;
    margin-top: 10px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }

  .inline {
    margin-top: 10px;
  }

  .ask.confirm {
    align-items: center;
    font-size: var(--text-sm);
  }

  .ask.confirm span {
    flex: 1 1 260px;
  }

  .labelled {
    flex: 1 1 260px;
    max-width: 360px;
  }

  .hint {
    flex-basis: 100%;
    margin: 0;
    font-size: var(--text-xs);
  }

  .detail .notice {
    margin-top: 16px;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 20px 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .yes {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--done);
    font-weight: 500;
  }

  .mono {
    overflow-wrap: anywhere;
  }

  .block {
    display: block;
  }

  .public {
    margin: 0;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    font: var(--text-xs) / 1.6 var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .table-pane {
    flex: 1;
    min-height: 0;
    padding: 12px 20px 32px;
    overflow-y: auto;
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

  td .badge {
    margin-top: 2px;
  }

  tr.tested td {
    border-bottom: 0;
  }

  tr.result td {
    padding-top: 0;
  }

  tr.result .test {
    margin: 0;
  }

  .nowrap {
    white-space: nowrap;
  }

  td.file {
    overflow-wrap: anywhere;
  }

  .strong {
    font-weight: 600;
  }

  .small {
    font-size: var(--text-xs);
  }

  .end {
    text-align: right;
  }

  .row-actions {
    display: inline-flex;
    gap: 6px;
  }

  .remote {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .remote + .remote {
    margin-top: 4px;
  }

  .test {
    display: flex;
    align-items: flex-start;
    gap: 4px;
    margin: 4px 0 0;
  }

  .test :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .test.ok {
    color: var(--done);
  }

  .test.failed {
    color: var(--danger);
  }

  td select {
    min-width: 200px;
  }
</style>
