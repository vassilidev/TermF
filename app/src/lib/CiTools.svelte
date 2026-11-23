<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleCheck from "@lucide/svelte/icons/circle-check";
  import Copy from "@lucide/svelte/icons/copy";
  import Pencil from "@lucide/svelte/icons/pencil";
  import Plus from "@lucide/svelte/icons/plus";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import Button from "$lib/ui/Button.svelte";
  import Field from "$lib/ui/Field.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { t } from "$lib/i18n";
  import { deployLabel, deployMark, deploys, loadDeploys, type Target, type Tool, type ToolInfo } from "$lib/ci.svelte";

  /** The deploy tools' CLIs on this machine, and where the project deploys with them. */
  let { project, repos }: { project: string; repos: { name: string; path: string }[] } = $props();

  const order: Tool[] = ["forge", "vercel", "netlify", "fly", "railway"];
  /** The form: a new target, or the one named `previous`. */
  let form = $state<(Target & { previous: string | null }) | null>(null);
  let error = $state("");
  let saving = $state(false);

  $effect(() => {
    if (deploys.tools === null) invoke<ToolInfo[]>("deploy_tools").then((found) => (deploys.tools = found), () => (deploys.tools = []));
  });

  const info = (tool: Tool) => deploys.tools?.find((x) => x.tool === tool);
  const fields: Record<Tool, { target: Parameters<typeof t>[0]; scope?: Parameters<typeof t>[0]; service?: boolean; example: string }> = {
    forge: { target: "cvFieldSite", scope: "cvFieldServer", example: "acme.com" },
    vercel: { target: "cvFieldProject", scope: "cvFieldTeam", example: "web-dashboard" },
    netlify: { target: "cvFieldSiteId", example: "3f1c2a9b-8d7e-4f50-9c1a-2b3c4d5e6f70" },
    fly: { target: "cvFieldApp", example: "acme-api" },
    railway: { target: "cvFieldProject", scope: "cvFieldEnvironment", service: true, example: "acme" },
  };

  function start(tool: Tool = "vercel") {
    error = "";
    form = { previous: null, name: deploys.targets?.length ? "" : "production", tool, target: "", scope: "", service: "", repo: repos[0]?.path ?? "", token: "", env: "dev" };
  }

  function edit(target: Target) {
    error = "";
    form = { previous: target.name, ...target, scope: target.scope ?? "", service: target.service ?? "", repo: target.repo ?? "", token: target.token ?? "" };
  }

  const clean = (v: string | null | undefined) => (v?.trim() ? v.trim() : null);

  async function save() {
    const f = form;
    if (!f) return;
    const spec = fields[f.tool];
    const target: Target = {
      name: f.name.trim(),
      tool: f.tool,
      target: f.target.trim(),
      scope: spec.scope ? clean(f.scope) : null,
      service: spec.service ? clean(f.service) : null,
      repo: clean(f.repo),
      token: clean(f.token),
      env: f.env,
    };
    [saving, error] = [true, ""];
    try {
      deploys.targets = await invoke<Target[]>("deploy_target_save", { project, target, previous: f.previous });
      form = null;
      toast(t("cvTargetSaved", { target: target.name, tool: deployLabel[target.tool] }));
      void loadDeploys(project);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function remove(target: Target) {
    try {
      deploys.targets = await invoke<Target[]>("deploy_target_delete", { project, name: target.name });
      delete deploys.found[target.name];
      toast(t("cvTargetDeleted", { target: target.name }), {
        undo: () =>
          void invoke<Target[]>("deploy_target_save", { project, target, previous: null }).then(
            (list) => ((deploys.targets = list), void loadDeploys(project)),
            (e) => toast(String(e), { kind: "error" }),
          ),
      });
    } catch (e) {
      toast(String(e), { kind: "error" });
    }
  }

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      toast(t("cvCopied"));
    } catch (e) {
      toast(String(e), { kind: "error" });
    }
  }

  const toolOptions = order.map((id) => ({ id, label: deployLabel[id] }));
  const envOptions = ["dev", "staging", "prod"].map((id) => ({ id, label: id }));
  const said = (target: Target) => {
    const found = deploys.found[target.name];
    return !found ? null : Array.isArray(found) ? t("cvTargetOk", { count: found.length }) : found.error;
  };
</script>

<div class="tools">
  <section aria-labelledby="tools-title">
    <h2 id="tools-title">{t("cvToolsTitle")}</h2>
    <p class="muted lead">{t("cvToolsText")}</p>
    {#if deploys.tools === null}
      <Skeleton shape="list" rows={5} label={t("cvToolsLoading")} />
    {:else}
      <ul class="targets">
        {#each order as tool (tool)}
          {@const i = info(tool)}
          <li class:missing={!i?.path}>
            <span class="logo" aria-hidden="true">{deployMark[tool]}</span>
            <span class="what">
              <b>{deployLabel[tool]}</b>
              {#if i}<span class="muted small">{t("cvTokenVar", { name: i.tokenVar })}</span>{/if}
            </span>
            {#if i?.path}
              <span class="ok" title={i.path}><CircleCheck size={13} aria-hidden="true" />{t("cvInstalled")}</span>
            {:else if i}
              <span class="muted small">{t("cvNotInstalled")}</span>
              <code>{i.install}</code>
              <button class="icon-btn sm" aria-label={t("cvCopyCommand")} title={t("cvCopyCommand")} onclick={() => copy(i.install)}><Copy size={13} aria-hidden="true" /></button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section aria-labelledby="targets-title">
    <div class="head">
      <h2 id="targets-title">{t("cvTargetsTitle")}</h2>
      {#if !form}<Button size="sm" icon={Plus} onclick={() => start()}>{t("cvConnectTool")}</Button>{/if}
    </div>
    {#if deploys.targets?.length}
      <ul class="targets">
        {#each deploys.targets as target (target.name)}
          {@const reading = said(target)}
          <li>
            <span class="logo" aria-hidden="true">{deployMark[target.tool]}</span>
            <span class="what">
              <b>{target.name} · {deployLabel[target.tool]}</b>
              <span class="muted small">{[target.target, target.scope, target.service].filter(Boolean).join(" · ")} · {target.token ? t("cvTokenFrom", { name: target.token, env: target.env }) : t("cvTokenCli")}</span>
              {#if reading}<span class="small" class:bad={!Array.isArray(deploys.found[target.name])}>{reading}</span>{/if}
            </span>
            <Button size="sm" variant="quiet" icon={Pencil} onclick={() => edit(target)}>{t("cvEdit")}</Button>
            <Button size="sm" variant="quiet-danger" icon={Trash2} onclick={() => remove(target)}>{t("cvRemove")}</Button>
          </li>
        {/each}
      </ul>
    {:else if !form}
      <p class="muted">{t("cvNoTargetsYet")}</p>
    {/if}

    {#if form}
      {@const f = form}
      {@const spec = fields[f.tool]}
      <form class="form" onsubmit={(e) => (e.preventDefault(), save())} aria-labelledby="target-form-title">
        <h3 id="target-form-title">{f.previous ? t("cvTargetEdit", { target: f.previous }) : t("cvConnectTool")}</h3>
        <div class="field-row">
          <span class="label">{t("cvFieldTool")}</span>
          <Segments label={t("cvFieldTool")} options={toolOptions} selected={f.tool} onselect={(id) => (f.tool = id as Tool)} />
        </div>
        {#if !info(f.tool)?.path}<p class="notice warn small">{t("cvToolMissing", { tool: deployLabel[f.tool], command: info(f.tool)?.install ?? "" })}</p>{/if}
        <div class="grid2">
          <Field label={t("cvFieldName")} help={t("cvFieldNameHelp")} bind:value={f.name} required />
          <Field label={t(spec.target)} example={spec.example} bind:value={f.target} spellcheck="false" required />
          {#if spec.scope}<Field label={t(spec.scope)} bind:value={f.scope as string} spellcheck="false" />{/if}
          {#if spec.service}<Field label={t("cvFieldService")} bind:value={f.service as string} spellcheck="false" />{/if}
          <Field label={t("cvFieldRepo")} help={t("cvFieldRepoHelp")}>
            {#snippet control(attrs)}
              <select class="field" {...attrs} bind:value={f.repo}>
                <option value="">{t("cvFieldRepoNone")}</option>
                {#each repos as r (r.path)}<option value={r.path}>{r.name}</option>{/each}
              </select>
            {/snippet}
          </Field>
          <Field label={t("cvFieldToken")} help={t("cvFieldTokenHelp", { name: info(f.tool)?.tokenVar ?? "" })} bind:value={f.token as string} spellcheck="false" />
        </div>
        {#if f.token?.trim()}
          <div class="field-row">
            <span class="label">{t("cvFieldEnv")}</span>
            <Segments label={t("cvFieldEnv")} options={envOptions} selected={f.env} onselect={(id) => (f.env = id)} />
          </div>
        {/if}
        {#if error}<p class="notice error small">{error}</p>{/if}
        <div class="buttons">
          <Button variant="quiet" size="sm" type="button" onclick={() => (form = null)}>{t("cancel")}</Button>
          <Button variant="primary" size="sm" type="submit" loading={saving} disabled={!f.name.trim() || !f.target.trim()}>{t("cvTargetSave")}</Button>
        </div>
      </form>
    {/if}
  </section>
</div>

<style>
  .tools {
    display: flex;
    flex-direction: column;
    gap: 32px;
    max-width: 980px;
    padding: 20px 24px 80px;
    overflow-y: auto;
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
  }

  h3 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: 600;
  }

  .lead {
    margin: 4px 0 14px;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  .ok {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--done);
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .missing {
    background: var(--well);
  }

  code {
    padding: 3px 8px;
    border-radius: var(--radius);
    background: var(--panel);
    border: 1px solid var(--border);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .logo {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--well);
    color: var(--soft);
    font-size: 10px;
    font-weight: 700;
  }

  .small {
    font-size: var(--text-sm);
  }

  .bad {
    color: var(--danger);
  }

  .targets {
    margin: 0 0 16px;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .targets li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
  }

  .targets li + li {
    border-top: 1px solid var(--border);
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .grid2 {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px 16px;
  }

  .field-row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .label {
    font-size: var(--text-sm);
    font-weight: 600;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
