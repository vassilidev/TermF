<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import History from "@lucide/svelte/icons/history";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Play from "@lucide/svelte/icons/play";
  import Plus from "@lucide/svelte/icons/plus";
  import Server from "@lucide/svelte/icons/server";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import OpsChecks from "$lib/OpsChecks.svelte";
  import OpsForge from "$lib/OpsForge.svelte";
  import OpsRedis from "$lib/OpsRedis.svelte";
  import OpsServer from "$lib/OpsServer.svelte";
  import OpsEnv from "$lib/OpsEnv.svelte";
  import OpsFollow from "$lib/OpsFollow.svelte";
  import OpsForm from "$lib/OpsForm.svelte";
  import OpsParams from "$lib/OpsParams.svelte";
  import OpsRequest from "$lib/OpsRequest.svelte";
  import OpsSql from "$lib/OpsSql.svelte";
  import Banner from "$lib/ui/Banner.svelte";
  import Card from "$lib/ui/Card.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import NextStep from "$lib/ui/NextStep.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import Tabs from "$lib/ui/Tabs.svelte";
  import ViewHeader from "$lib/ui/ViewHeader.svelte";
  import { ago, t } from "$lib/i18n";
  import { bounded, isSql, runsOn, target, type Check, type Connection, type Kind, type Plan, type Ran } from "$lib/ops-logic";
  import { loadHistory, loadOps, ops, said, stopFollow, type OpsTab } from "$lib/ops.svelte";
  import { app, onScreen } from "$lib/state.svelte";
  import type { VaultStatus } from "$lib/vault.svelte";
  import { askAI, provide } from "$lib/ask.svelte";

  type Key = Parameters<typeof t>[0];

  const project = $derived(app.workspace?.projects.find((p) => p.id === app.project));

  // The ⌘I panel: the DBA on the SQL console, devops elsewhere; a connection goes by its name, never its secrets.
  $effect(() => {
    const sql = ops.tab === "sql";
    const name = sql ? ops.sql.connection : ops.connection;
    const query = sql ? ops.sql.query.trim() : "";
    return provide("ops", {
      role: sql ? "dba" : "devops",
      chips: [
        ...(name ? [{ id: "ops-connection", kind: "object" as const, label: name, secret: true }] : []),
        ...(query ? [{ id: "ops-query", kind: "selection" as const, label: t("opssqlTab"), text: query }] : []),
      ],
    });
  });
  const visible = $derived(onScreen("ops"));

  $effect(() => {
    const id = project?.id;
    if (!visible || !id) return;
    untrack(() => {
      if (ops.project !== id) {
        ops.connection = null;
        ops.editing = null;
        ops.recipe = null;
        ops.result = null;
        ops.proposal = null;
        ops.sql.result = null;
        stopFollow();
        ops.live.lines = [];
        ops.live.ended = null;
      }
      loadOps(id);
      loadHistory(id);
    });
  });

  const requests = $derived(ops.requests.filter((r) => r.project === project?.id));
  const kindLabel = (k: Kind) => t(`opsKind${k[0].toUpperCase()}${k.slice(1)}` as Key);
  const access = (c: Connection) =>
    t(c.ai_access === "full" ? "opsAccessFull" : c.ai_access === "redacted" ? "opsAccessRedacted" : "opsAccessNone");

  // Connections.
  const current = $derived(ops.connections.find((c) => c.name === ops.connection) ?? (ops.editing === null ? ops.connections[0] : undefined));
  let checks = $state<Check[] | null>(null);
  let testing = $state(false);
  let deleting = $state(false);
  let busyError = $state("");

  function pick(name: string) {
    ops.connection = name;
    ops.editing = null;
    checks = null;
  }

  function blank(): Connection {
    return { name: "", kind: "ssh", env: "prod", ai_access: "none" };
  }

  /** The connections whose last test failed, with what failed (codes only: never a host, a user or a secret). */
  let failures = $state<Record<string, string>>({});
  /** By project and connection: the same name in another project is another server. */
  const failKey = (name: string) => `${project?.id ?? ""}\n${name}`;
  const failed = $derived(ops.connections.find((c) => failures[failKey(c.name)] !== undefined));

  async function test(c: Connection) {
    testing = true;
    checks = null;
    busyError = "";
    const key = failKey(c.name);
    try {
      checks = await invoke<Check[]>("ops_test", { project: project?.id, connection: c, previous: c.name });
      const bad = checks.filter((x) => !x.ok && !x.warn);
      if (bad.length) failures[key] = bad.map((x) => `${x.code}: failed${x.hint ? ` (${x.hint})` : ""}`).join("\n");
      else delete failures[key];
    } catch (e) {
      busyError = String(e);
      failures[key] = "the test did not run";
    } finally {
      testing = false;
    }
  }

  function whyFailed(name: string) {
    askAI({
      role: "devops",
      chips: [{ id: "ops-test", kind: "selection", label: t("restOpsTestFailedBadge"), text: `${kindLabel(ops.connections.find((c) => c.name === name)?.kind ?? "ssh")}\n${failures[failKey(name)]}` }],
      draft: t("restOpsWhyDraft", { name }),
    });
  }

  async function remove(name: string) {
    try {
      ops.connections = await invoke<Connection[]>("ops_connection_delete", { project: project?.id, name });
      ops.connection = null;
      checks = null;
    } catch (e) {
      busyError = String(e);
    }
  }

  function openRecipe(name: string, connection?: string) {
    const r = ops.recipes.find((x) => x.name === name);
    if (r && isSql(r)) {
      ops.tab = "sql";
      if (connection) ops.sql.connection = connection;
      return;
    }
    if (r?.follow) {
      ops.tab = "live";
      ops.live.recipe = name;
      if (connection && ops.live.id === null) ops.live.connection = connection;
      return;
    }
    ops.tab = "recipes";
    ops.recipe = name;
    ops.proposal = null;
    ops.result = null;
    ops.form = { connection: connection ?? "", params: {} };
  }

  // Recipes: a query goes to the SQL console, a stream to the live logs.
  const listed = $derived(ops.recipes.filter(bounded));
  const recipe = $derived(listed.find((r) => r.name === ops.recipe));
  // The recipes that can run here first; the last run of each from the history, events apart.
  const usable = (r: (typeof listed)[number]) => ops.connections.some((c) => runsOn(r, c));
  const cards = $derived([...listed].sort((a, b) => Number(usable(b)) - Number(usable(a))));
  const lastRun = (name: string) => ops.history.find((h) => h.recipe === name && h.event === null);
  const fitting = $derived(recipe ? ops.connections.filter((c) => runsOn(recipe, c)) : []);
  $effect(() => {
    if (fitting.length && !fitting.some((c) => c.name === ops.form.connection)) {
      untrack(() => { ops.form.connection = fitting[0].name; });
    }
  });
  const given = $derived(Object.fromEntries(Object.entries(ops.form.params).filter(([, v]) => String(v).trim() !== "")));
  let plan = $state<Plan | null>(null);
  let planError = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const ask = { project: project?.id, recipe: recipe?.name, connection: ops.form.connection, params: given };
    clearTimeout(timer);
    if (!ask.project || !ask.recipe || !ask.connection) {
      plan = null;
      return;
    }
    timer = setTimeout(() => {
      invoke<Plan>("ops_plan", ask).then(
        (p) => {
          plan = p;
          planError = "";
        },
        (e) => {
          plan = null;
          planError = String(e);
        },
      );
    }, 250);
  });

  let running = $state(false);
  let confirming = $state(false);
  let typed = $state("");
  /** The vault's password the run waits for: to unlock it, or to confirm prod. */
  let needs = $state<"unlock" | "reauth" | null>(null);
  let password = $state("");
  let after: (() => Promise<void>) | null = null;
  let revealed = $state<Record<number, string>>({});
  let copied = $state(false);

  function askPassword(e: unknown, retry: () => Promise<void>): boolean {
    const text = String(e);
    const kind = /the vault is locked/.test(text) ? "unlock" : /needs the master password again/.test(text) ? "reauth" : null;
    if (!kind) return false;
    needs = kind;
    after = retry;
    return true;
  }

  async function givePassword() {
    busyError = "";
    try {
      if (needs === "unlock") await invoke<VaultStatus>("vault_unlock", { password });
      else await invoke<VaultStatus>("vault_reauth", { password });
      needs = null;
      password = "";
      const retry = after;
      after = null;
      await retry?.();
    } catch (e) {
      busyError = /wrong master password/.test(String(e)) ? t("vaultWrongPassword") : String(e);
    }
  }

  async function start() {
    if (recipe?.mode === "write") {
      typed = "";
      confirming = true;
      return;
    }
    await run(null);
  }

  async function run(confirm: string | null) {
    if (!recipe || !project) return;
    running = true;
    busyError = "";
    revealed = {};
    try {
      ops.result = await invoke<Ran>("ops_run", {
        project: project.id,
        recipe: recipe.name,
        connection: ops.form.connection,
        params: given,
        confirm,
        proposal: ops.proposal,
      });
      ops.proposal = null;
      loadHistory(project.id);
    } catch (e) {
      if (!askPassword(e, () => run(confirm))) busyError = String(e);
    } finally {
      running = false;
    }
  }

  async function reveal(index: number) {
    const id = ops.result?.id;
    if (id === undefined) return;
    try {
      revealed[index] = await invoke<string>("ops_reveal", { result: id, index });
      setTimeout(() => delete revealed[index], 30_000);
    } catch (e) {
      if (!askPassword(e, () => reveal(index))) busyError = String(e);
    }
  }

  async function copyOutput() {
    if (!ops.result) return;
    await navigator.clipboard.writeText(ops.result.output.text).catch(() => {});
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  const proposer = $derived(ops.proposal === null ? null : ops.requests.find((r) => r.id === ops.proposal));
  const approvalLabels: Record<string, Key> = {
    user: "opsApprovalUser",
    approved: "opsApprovalApproved",
    refused: "opsApprovalRefused",
    expired: "opsApprovalExpired",
    proposed: "opsApprovalProposed",
    "not asked": "opsApprovalNotAsked",
  };
  const eventLabels: Record<string, Key> = {
    follow: "opssqlEventFollow",
    reveal: "opssqlEventReveal",
    copy: "opssqlEventCopy",
    write: "opssqlEventWrite",
    schema: "opsdbEventSchema",
    export: "opsdbEventExport",
    dba: "opsdbEventDba",
  };
</script>

{#snippet passwordForm()}
  {#if needs}
    <form
      class="password"
      onsubmit={(e) => {
        e.preventDefault();
        givePassword();
      }}
    >
      <span>{t(needs === "unlock" ? "opsPasswordUnlock" : "opsPasswordReauth")}</span>
      <input class="field" type="password" autocomplete="current-password" placeholder={t("opsPassword")} bind:value={password} />
      <button class="btn sm primary" disabled={!password}>{t("opsContinue")}</button>
      <button type="button" class="btn sm" onclick={() => ((needs = null), (after = null))}>{t("cancel")}</button>
    </form>
  {/if}
{/snippet}

<div class="ops-view">
  <ViewHeader title={t("opsView")} context={project?.name ?? ""}>
    {#snippet actions()}
      {#if project && ops.tab === "connections" && ops.connections.length && ops.editing === null}
        <button class="btn sm primary" onclick={() => ((ops.editing = ""), (checks = null))}>
          <Plus size={14} aria-hidden="true" />{t("opsAdd")}
        </button>
      {/if}
    {/snippet}
  </ViewHeader>

  {#if !project}
    <div class="pane"><EmptyState icon={Server} title={t("opsView")} text={t("opsNoProject")} /></div>
  {:else}
    <p class="intro muted">{t("opsIntro")}</p>
    {#if requests.length}
      <div class="requests">
        {#each requests as r (r.id)}<OpsRequest request={r} />{/each}
      </div>
    {/if}
    {#if failed}
      <div class="next">
        <NextStep text={t("restOpsTestFailed", { name: failed.name })} action={{ label: t("aigAskWhy"), run: () => whyFailed(failed.name) }} />
      </div>
    {/if}
    <Tabs
      id="ops"
      label={t("opsView")}
      tabs={[
        { id: "connections", label: t("opsTabConnections"), count: ops.connections.length },
        { id: "recipes", label: t("opsTabRecipes"), count: listed.length },
        { id: "sql", label: t("opssqlTab") },
        { id: "live", label: t("opssqlLive") },
        { id: "forge", label: t("opsforgeTab") },
        { id: "history", label: t("opsTabHistory") },
      ]}
      selected={ops.tab}
      onselect={(tab) => (ops.tab = tab as OpsTab)}
    >
      {#if ops.error || busyError || ops.errors.length || ops.queryErrors.length}
        <div class="banners">
          {#if ops.error}<Banner kind="error" ondismiss={() => (ops.error = "")}>{ops.error}</Banner>{/if}
          {#if busyError}<Banner kind="error" ondismiss={() => (busyError = "")}>{busyError}</Banner>{/if}
          {#if ops.errors.length}<Banner kind="warn">{t("opsRecipeErrors", { list: ops.errors.join(" · ") })}</Banner>{/if}
          {#if ops.queryErrors.length}<Banner kind="warn">{t("opsdbQueryErrors", { list: ops.queryErrors.join(" · ") })}</Banner>{/if}
        </div>
      {/if}

      {#if ops.project !== project.id && ops.tab === "recipes"}
        <div class="pane cards-pane"><Skeleton shape="cards" rows={6} /></div>
      {:else if ops.project !== project.id && ops.tab === "connections"}
        <div class="split"><div class="list"><Skeleton shape="list" rows={4} /></div></div>
      {:else if ops.tab === "connections"}
        {#if !ops.connections.length && ops.editing === null}
          <div class="pane">
            <EmptyState
              icon={Server}
              title={t("opsEmpty")}
              text={t("opsEmptyHelp")}
              onask={() => askAI({ role: "devops", draft: t("aigOpsDraft") })}
              askLabel={t("aigAskRole", { role: "devops" })}
            >
              <button class="btn primary" onclick={() => (ops.editing = "")}><Plus size={14} aria-hidden="true" />{t("opsAdd")}</button>
            </EmptyState>
          </div>
        {:else}
          <div class="split">
            <ul class="list" aria-label={t("opsTabConnections")}>
              {#each ops.connections as c (c.name)}
                <li class="row" class:current={c === current && ops.editing === null}>
                  <button class="pick" aria-current={c === current} onclick={() => pick(c.name)}>
                    <span class="name mono">{c.name}</span>
                    <span class="desc">{kindLabel(c.kind)}</span>
                  </button>
                  {#if failures[failKey(c.name)] !== undefined}
                    <span class="failed" title={t("restOpsTestFailedBadge")}><CircleAlert size={14} aria-hidden="true" /><span class="sr-only">{t("restOpsTestFailedBadge")}</span></span>
                  {/if}
                  <span class="badge env" class:prod={c.env === "prod"}>{c.env}</span>
                </li>
              {/each}
            </ul>
            <section class="detail" aria-live="polite">
              {#if ops.editing !== null}
                {@render passwordForm()}
                {#key ops.editing}
                  <OpsForm
                    project={project.id}
                    ask={askPassword}
                    start={ops.editing ? (ops.connections.find((c) => c.name === ops.editing) ?? blank()) : blank()}
                    previous={ops.editing || null}
                    onclose={(saved) => {
                      ops.editing = null;
                      if (saved) ops.connection = saved;
                    }}
                  />
                {/key}
              {:else if current}
                <header>
                  <h2 class="mono">{current.name}</h2>
                  <p class="muted">{kindLabel(current.kind)} · <span class="badge env" class:prod={current.env === "prod"}>{current.env}</span></p>
                </header>
                {#if current.description}<p>{current.description}</p>{/if}
                <dl class="meta">
                  <dt>{t("opsTarget")}</dt>
                  <dd class="mono">{target(current, ops.connections)}</dd>
                  <dt>{t("opsAccess")}</dt>
                  <dd>{access(current)}</dd>
                  <dt>{t("opsSecrets")}</dt>
                  <dd class="mono">{Object.values(current.secrets ?? {}).join(", ") || t("opsNoSecrets")}</dd>
                </dl>
                <div class="actions">
                  <button class="btn sm primary" disabled={testing} onclick={() => test(current)}>
                    {#if testing}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("opsTesting")}{:else}{t("opsTest")}{/if}
                  </button>
                  <button class="btn sm" onclick={() => { ops.editing = current.name; checks = null; }}>
                    <Pencil size={14} aria-hidden="true" />{t("opsEdit")}
                  </button>
                  <button class="btn sm" onclick={() => (deleting = true)}><Trash2 size={14} aria-hidden="true" />{t("opsDelete")}</button>
                </div>
                {@render passwordForm()}
                {#if checks}<div class="gap"><OpsChecks {checks} /></div>{/if}
                {#key current.name}
                  {#if current.kind === "ssh"}
                    <OpsServer project={project.id} conn={current} ask={askPassword} />
                  {:else if current.kind === "redis"}
                    <OpsRedis project={project.id} conn={current} ask={askPassword} />
                  {/if}
                {/key}
                <h3>{t("opsRecipesHere")}</h3>
                <ul class="recipes">
                  {#each ops.recipes.filter((r) => runsOn(r, current)) as r (r.name)}
                    <li>
                      <button class="btn sm" onclick={() => openRecipe(r.name, current.name)}>
                        <Play size={13} aria-hidden="true" /><span class="mono">{r.name}</span>
                      </button>
                      <span class="muted">{said(r.description)}</span>
                    </li>
                  {/each}
                </ul>
                <ConfirmDialog
                  bind:open={deleting}
                  title={t("opsDeleteTitle", { name: current.name })}
                  confirm={t("opsDelete")}
                  danger
                  onconfirm={() => remove(current.name)}
                >
                  <p>{t("opsDeleteBody")}</p>
                </ConfirmDialog>
              {/if}
            </section>
          </div>
        {/if}
      {:else if ops.tab === "recipes"}
        {#if !recipe}
          <div class="pane cards-pane">
            <ul class="cards" aria-label={t("opsTabRecipes")}>
              {#each cards as r (r.name)}
                {@const on = ops.connections.filter((c) => runsOn(r, c))}
                {@const required = r.params.filter((p) => p.required).map((p) => p.name)}
                {@const last = lastRun(r.name)}
                <li data-recipe={r.name}>
                  <Card title={r.name}>
                    {#snippet actions()}{#if r.mode === "write"}<span class="badge danger">{t("opsWrite")}</span>{/if}{/snippet}
                    <p class="what">{said(r.description)}</p>
                    <ul class="recipe-facts">
                      <li class:muted={!on.length}>{on.length ? t("restOpsOn", { list: on.map((c) => c.name).join(", ") }) : t("opsNoConnectionFor")}</li>
                      {#if required.length}<li>{t("restOpsNeeds", { list: required.join(", ") })}</li>{/if}
                      <li class="last">
                        {#if last}
                          <span>{t("restOpsLast", { when: ago(last.at) })}</span>
                          {#if last.exit !== null}
                            <span class="badge" class:ok={last.exit === 0} class:danger={last.exit !== 0}>{t("opsExit", { code: last.exit })}</span>
                          {:else if last.approval !== "user" && approvalLabels[last.approval]}
                            <span class="badge" class:danger={last.approval === "refused" || last.approval === "expired"}>{t(approvalLabels[last.approval])}</span>
                          {/if}
                        {:else}
                          <span class="muted">{t("restOpsNever")}</span>
                        {/if}
                      </li>
                    </ul>
                    <button class="btn sm" disabled={!on.length} onclick={() => openRecipe(r.name)}><Play size={13} aria-hidden="true" />{t("opsRun")}</button>
                  </Card>
                </li>
              {/each}
            </ul>
          </div>
        {:else}
        <div class="pane run-pane">
          <button class="btn sm quiet back" onclick={() => ((ops.recipe = null), (ops.proposal = null))}><ArrowLeft size={14} aria-hidden="true" />{t("restOpsAll")}</button>
          <section class="detail" aria-live="polite">
            {#if recipe}
              <header>
                <h2 class="mono">{recipe.name}</h2>
                <p class="muted">{said(recipe.description)}</p>
              </header>
              {#if recipe.mode === "write"}<p class="notice warn">{t("opsWriteHelp")}</p>{/if}
              {#if proposer}<p class="notice">{t("opsProposedBy", { who: proposer.agent.role === "cto" ? t("ux3t1Cto") : (proposer.agent.role ?? proposer.agent.title) })}</p>{/if}
              {#if !fitting.length}
                <p class="notice">{t("opsNoConnectionFor")}</p>
              {:else}
                <form
                  class="run"
                  onsubmit={(e) => {
                    e.preventDefault();
                    start();
                  }}
                >
                  <label class="labelled">
                    <span>{t("opsConnection")}</span>
                    <select class="field" bind:value={ops.form.connection}>
                      {#each fitting as c (c.name)}<option value={c.name}>{c.name} · {c.env}</option>{/each}
                    </select>
                  </label>
                  <OpsParams params={recipe.params} bind:values={ops.form.params} />

                  <div class="command">
                    <span class="label">{plan?.remote ? t("opsRemote", { target: plan.target }) : t("opsCommand")}</span>
                    {#if plan}
                      <pre>{plan.remote ?? plan.command}</pre>
                      {#if plan.secrets.length}<p class="muted small">{t("opsVaultSecrets", { names: plan.secrets.join(", ") })}</p>{/if}
                    {:else if planError}
                      <p class="notice error">{planError}</p>
                    {/if}
                  </div>
                  <div class="buttons">
                    <button class="btn primary" disabled={running || !plan}>
                      {#if running}<LoaderCircle size={14} aria-hidden="true" class="spin" />{t("opsRunning")}{:else}<Play size={14} aria-hidden="true" />{t("opsRun")}{/if}
                    </button>
                  </div>
                </form>
                {@render passwordForm()}
              {/if}

              {#if ops.result && ops.result.plan.recipe === recipe.name}
                {@const out = ops.result.output}
                <section class="result" aria-label={t("opsCommand")}>
                  <div class="result-head">
                    <span class="badge" class:ok={out.exit === 0} class:danger={out.exit !== 0}>
                      {out.exit === null ? (out.stopped ?? "—") : t("opsExit", { code: out.exit })}
                    </span>
                    <span class="muted">{t("opsDuration", { seconds: (out.millis / 1000).toFixed(1) })}</span>
                    <span class="muted">{t("opsMasked", { count: out.masks })}</span>
                    {#if out.truncated}<span class="muted">{t("opsTruncated")}</span>{/if}
                    {#if out.stopped && out.exit !== null}<span class="muted">{out.stopped}</span>{/if}
                    <button class="icon-btn sm end" onclick={copyOutput} aria-label={t("opsCopy")} title={t("opsCopy")}>
                      <Copy size={13} aria-hidden="true" />
                    </button>
                    {#if copied}<span class="muted" role="status">{t("opsCopied")}</span>{/if}
                  </div>
                  {#if out.keys?.length}
                    {#key ops.result.id}<OpsEnv result={ops.result} project={project.id} ask={askPassword} />{/key}
                  {:else}
                    <pre class="output">{#if out.parts.length}{#each out.parts as part, i (i)}{#if "text" in part}{part.text}{:else if revealed[part.hidden] !== undefined}<mark class="shown">{revealed[part.hidden]}</mark>{:else}<button
                              class="hidden"
                              title={t("opsReveal")}
                              aria-label={t("opsReveal")}
                              onclick={() => reveal(part.hidden)}>••••••</button
                            >{/if}{/each}{:else}{out.text || t("opsNoOutput")}{/if}</pre>
                  {/if}
                  {#if out.errors.trim()}
                    <span class="label">{t("opsErrors")}</span>
                    <pre class="errors">{out.errors}</pre>
                  {/if}
                </section>
              {/if}
            {/if}
          </section>
        </div>
        {/if}
        <ConfirmDialog
          bind:open={confirming}
          title={t("opsConfirmTitle", { recipe: recipe?.name ?? "", connection: ops.form.connection })}
          confirm={t("opsRun")}
          danger
          disabled={typed.trim() !== plan?.env}
          onconfirm={() => run(typed.trim())}
        >
          <p>{t("opsConfirmBody", { env: plan?.env ?? "" })}</p>
          {#if plan}<pre class="confirm-command">{plan.remote ?? plan.command}</pre>{/if}
          <label class="labelled">
            <span>{t("opsConfirmField")}</span>
            <input class="field mono" bind:value={typed} placeholder={plan?.env ?? ""} />
          </label>
        </ConfirmDialog>
      {:else if ops.tab === "sql" || ops.tab === "live"}
        {#if needs}<div class="asking">{@render passwordForm()}</div>{/if}
        {#if ops.tab === "sql"}
          <OpsSql project={project.id} ask={askPassword} />
        {:else}
          <OpsFollow project={project.id} ask={askPassword} />
        {/if}
      {:else if ops.tab === "forge"}
        {#key project.id}<OpsForge project={project.id} ask={askPassword} />{/key}
      {:else}
        <div class="pane">
          {#if !ops.history.length}
            <EmptyState icon={History} text={t("opsHistoryEmpty")} action={{ label: t("restOpsSeeRecipes"), icon: Play, run: () => (ops.tab = "recipes") }} />
          {:else}
            <ul class="history">
              {#each ops.history as h (h.id)}
                <li>
                  <details>
                    <summary>
                      <time class="muted" datetime={new Date(h.at * 1000).toISOString()}>{ago(h.at)}</time>
                      <span class="mono">{h.recipe}</span>
                      <span class="mono muted">{h.connection}</span>
                      {#if h.env}<span class="badge env" class:prod={h.env === "prod"}>{h.env}</span>{/if}
                      <span class="muted">{h.by === "agent" ? t("opsByAgent", { session: h.session ?? "" }) : t("opsByYou")}</span>
                      {#if h.approval !== "user"}
                        <span class="badge" class:danger={h.approval === "refused" || h.approval === "expired"}>
                          {approvalLabels[h.approval] ? t(approvalLabels[h.approval]) : h.approval}
                        </span>
                      {/if}
                      {#if h.event && eventLabels[h.event]}
                        <span class="badge event">{t(eventLabels[h.event], { key: h.key ?? "", run: h.run ?? "" })}</span>
                      {/if}
                      <span class="facts muted">
                        {#if h.exit !== null}{t("opsExit", { code: h.exit })}{/if}
                        {#if h.masks}· {t("opsMasked", { count: h.masks })}{/if}
                      </span>
                    </summary>
                    <pre>{h.command}</pre>
                    {#if h.error}<p class="notice error">{h.error}</p>{/if}
                  </details>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </Tabs>
  {/if}
</div>

<style>
  .ops-view {
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
  }

  .intro {
    margin: 0;
    padding: 10px 20px 2px;
    background: var(--chrome);
    font-size: var(--text-sm);
  }

  .requests {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 20px;
    background: var(--chrome);
  }

  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .row .failed + .badge {
    margin-left: 0;
  }

  .row .failed {
    display: inline-flex;
    margin-left: auto;
    margin-right: 6px;
    color: var(--danger);
  }

  .row .badge {
    margin-left: auto;
  }

  .next {
    padding: 4px 20px 10px;
    background: var(--chrome);
  }

  .cards-pane {
    padding: 20px 24px 32px;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .cards > li,
  .cards > li > :global(.card) {
    display: flex;
    flex-direction: column;
  }

  .cards > li > :global(.card) {
    flex: 1;
  }

  .cards :global(.card .body) {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 10px;
  }

  .what {
    margin: 0;
    color: var(--soft);
    font-size: var(--text-sm);
  }

  .recipe-facts {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--soft);
    font-size: var(--text-xs);
  }

  .recipe-facts .last {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .cards .btn {
    align-self: flex-start;
  }

  .run-pane {
    padding: 12px 28px 32px;
  }

  .run-pane > .detail {
    max-width: 820px;
    margin-top: 8px;
  }

  .back {
    margin-left: -8px;
  }

  .badge.prod {
    border-color: var(--danger);
    background: var(--danger);
    color: var(--danger-text);
  }

  header h2 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  header p {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 4px 0 0;
    font-size: var(--text-sm);
  }

  h3 {
    margin: 22px 0 8px;
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 16px;
    margin: 18px 0 0;
    font-size: var(--text-sm);
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .actions,
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 16px;
  }

  .gap {
    margin-top: 12px;
  }

  .recipes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .recipes li {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .notice.warn {
    margin-top: 12px;
  }

  .run {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 16px;
  }

  .command {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .label {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  pre {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .small {
    margin: 0;
    font-size: var(--text-xs);
  }

  .password {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    font-size: var(--text-sm);
  }

  .password input {
    width: 220px;
  }

  .asking {
    padding: 0 24px 12px;
  }

  .badge.event {
    border-color: var(--accent);
    color: var(--accent-ink);
  }

  .result {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 20px;
  }

  .result-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    font-size: var(--text-sm);
  }

  .end {
    margin-left: auto;
  }

  .output {
    max-height: 420px;
    overflow-y: auto;
  }

  .hidden {
    white-space: nowrap;
    padding: 0 2px;
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--accent-soft);
    color: var(--accent-ink);
    font: inherit;
    cursor: pointer;
  }

  .shown {
    border-radius: var(--radius-sm);
    background: var(--waiting-soft);
    color: var(--waiting-text);
  }

  .errors {
    max-height: 160px;
    overflow-y: auto;
    color: var(--danger);
  }

  .confirm-command {
    margin: 8px 0 12px;
  }

  .history {
    max-width: 980px;
    margin: 0;
    padding: 8px 28px 28px;
    list-style: none;
    font-size: var(--text-sm);
  }

  .history li {
    border-bottom: 1px solid var(--border);
  }

  summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    cursor: pointer;
    list-style: none;
  }

  summary::-webkit-details-marker {
    display: none;
  }

  summary time {
    min-width: 90px;
    font-variant-numeric: tabular-nums;
  }

  .facts {
    margin-left: auto;
  }

  details > pre,
  details > p {
    margin-bottom: 10px;
  }
</style>
