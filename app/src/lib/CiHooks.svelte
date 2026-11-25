<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Plus from "@lucide/svelte/icons/plus";
  import RefreshCw from "@lucide/svelte/icons/refresh-cw";
  import RotateCw from "@lucide/svelte/icons/rotate-cw";
  import ShieldAlert from "@lucide/svelte/icons/shield-exclamation-point";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import WebhookIcon from "@lucide/svelte/icons/webhook";
  import Button from "$lib/ui/Button.svelte";
  import ConfirmDialog from "$lib/ui/ConfirmDialog.svelte";
  import EmptyState from "$lib/ui/EmptyState.svelte";
  import Field from "$lib/ui/Field.svelte";
  import List from "$lib/ui/List.svelte";
  import Row from "$lib/ui/Row.svelte";
  import Segments from "$lib/ui/Segments.svelte";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { ago, t } from "$lib/i18n";
  import { askAI } from "$lib/ask.svelte";
  import { openUrl } from "$lib/tickets.svelte";
  import { deploys, loadHooks, NO_ADMIN, type Delivery, type Hook } from "$lib/ci.svelte";

  /** The repository's webhooks (`slug`: its `owner/name` for GitHub's pages, "" until known), their deliveries, and redelivery. */
  let { repo, slug }: { repo: string; slug: string } = $props();

  let selected = $state<number | null>(null);
  let deliveries = $state<Delivery[] | null>(null);
  let deliveriesError = $state("");
  let busy = $state<number | "delete" | "create" | null>(null);
  let removing = $state(false);
  let adding = $state<{ url: string; events: string; confirm: boolean } | null>(null);

  const hooks = $derived(deploys.hooksOf === repo ? deploys.hooks : null);
  const hook = $derived(hooks?.find((h) => h.id === selected) ?? hooks?.[0]);
  const noAdmin = $derived(deploys.hooksError.startsWith(NO_ADMIN));
  const events = { push: ["push"], prs: ["push", "pull_request"], all: ["*"] } as Record<string, string[]>;
  const command = "gh auth refresh -s admin:repo_hook";

  let request = 0;
  $effect(() => {
    const id = hook?.id;
    void (id !== undefined && loadDeliveries(id));
  });

  async function loadDeliveries(id: number) {
    const r = ++request;
    [deliveries, deliveriesError] = [null, ""];
    try {
      const found = await invoke<Delivery[]>("ci_deliveries", { repo, hook: id });
      if (r === request) deliveries = found;
    } catch (e) {
      if (r === request) deliveriesError = String(e);
    }
  }

  /** `forge.laravel.com/servers/…/deploy`: the host, and a path short enough to read. */
  function where(url: string) {
    try {
      const u = new URL(url);
      const path = u.pathname.length > 24 ? `${u.pathname.slice(0, 12)}…${u.pathname.slice(-10)}` : u.pathname;
      return `${u.host}${path === "/" ? "" : path}`;
    } catch {
      return url;
    }
  }

  const ok = (code: number | null) => code !== null && code >= 200 && code < 300;
  const tone = (h: Hook) => (h.lastCode === null ? "unused" : ok(h.lastCode) ? "done" : "failed");

  async function redeliver(d: Delivery) {
    if (!hook) return;
    busy = d.id;
    try {
      await invoke("ci_redeliver", { repo, hook: hook.id, delivery: d.id });
      toast(t("cvRedelivered", { event: d.event, host: where(hook.url) }));
      // GitHub records the new attempt a moment later.
      setTimeout(() => hook && loadDeliveries(hook.id), 2500);
    } catch (e) {
      toast(String(e), { kind: "error" });
    } finally {
      busy = null;
    }
  }

  async function remove() {
    if (!hook) return;
    const gone = hook;
    busy = "delete";
    try {
      await invoke("ci_hook_delete", { repo, hook: gone.id });
      toast(t("cvHookDeleted", { host: where(gone.url) }));
      selected = null;
      await loadHooks(repo);
    } catch (e) {
      toast(String(e), { kind: "error" });
    } finally {
      busy = null;
    }
  }

  const validUrl = (url: string) => /^https?:\/\/\S+$/.test(url.trim());

  async function create() {
    const a = adding;
    if (!a) return;
    busy = "create";
    try {
      const made = await invoke<Hook>("ci_hook_create", { repo, url: a.url.trim(), events: events[a.events] });
      adding = null;
      toast(t("cvHookCreated", { host: where(made.url) }));
      await loadHooks(repo);
      selected = made.id;
    } catch (e) {
      a.confirm = false;
      toast(String(e), { kind: "error" });
    } finally {
      busy = null;
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      toast(t("cvCopied"));
    } catch (e) {
      toast(String(e), { kind: "error" });
    }
  }

  const askDevops = () => askAI({ role: "devops", draft: t("cvHooksAskDraft") });
  const eventOptions = $derived([
    { id: "push", label: t("cvEventsPush") },
    { id: "prs", label: t("cvEventsPrs") },
    { id: "all", label: t("cvEventsAll") },
  ]);
</script>

{#snippet form()}
  {#if adding}
    {@const a = adding}
    <form class="add" onsubmit={(e) => (e.preventDefault(), validUrl(a.url) && (a.confirm = true))} aria-labelledby="hook-add-title">
      <h3 id="hook-add-title">{t("cvHookAddTitle")}</h3>
      <Field label={t("cvHookUrl")} help={t("cvHookUrlHelp")} example="https://forge.laravel.com/servers/…/deploy/http?token=…" bind:value={a.url} spellcheck="false" />
      <div class="events">
        <span class="label">{t("cvHookEvents")}</span>
        <Segments label={t("cvHookEvents")} options={eventOptions} selected={a.events} onselect={(id) => { a.events = id; }} />
      </div>
      <div class="buttons">
        <Button variant="quiet" size="sm" type="button" onclick={() => (adding = null)}>{t("cancel")}</Button>
        <Button variant="primary" size="sm" type="submit" disabled={!validUrl(a.url)}>{t("cvHookAddGo")}</Button>
      </div>
    </form>
    <ConfirmDialog bind:open={a.confirm} title={t("cvHookAddConfirm")} confirm={t("cvHookAddGo")} onconfirm={create}>
      <p>{t("cvHookAddConfirmText", { url: a.url.trim(), events: eventOptions.find((o) => o.id === a.events)?.label ?? "" })}</p>
    </ConfirmDialog>
  {/if}
{/snippet}

{#if hooks === null}
  <div class="pad"><Skeleton shape="list" rows={3} label={t("cvHooksLoading")} /></div>
{:else if noAdmin}
  <EmptyState icon={ShieldAlert} title={t("cvNoAdmin")} text={t("cvNoAdminText", { repo: slug || (repo.split("/").pop() ?? repo) })}>
    <code class="cmd">{command}</code>
    <div class="row-btns">
      <Button size="sm" icon={Copy} onclick={copy}>{t("cvCopyCommand")}</Button>
      <Button size="sm" variant="quiet" icon={RefreshCw} onclick={() => loadHooks(repo)}>{t("ciRetry")}</Button>
    </div>
  </EmptyState>
{:else if deploys.hooksError}
  <EmptyState>
    <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{deploys.hooksError}</p>
    <Button icon={RefreshCw} onclick={() => loadHooks(repo)}>{t("ciRetry")}</Button>
  </EmptyState>
{:else if !hooks.length && !adding}
  <EmptyState
    icon={WebhookIcon}
    title={t("cvNoHooks")}
    text={t("cvNoHooksText")}
    action={{ label: t("cvHookAdd"), icon: Plus, run: () => (adding = { url: "", events: "push", confirm: false }) }}
    onask={askDevops}
    askLabel={t("cvAskDevops")}
  />
{:else if !hooks.length}
  <div class="pad">{@render form()}</div>
{:else}
  <div class="split hooks">
    <div class="list">
      <div class="list-head">
        <span>{t("cvHooksCount", { count: hooks.length })}</span>
        <Button size="sm" variant="quiet" icon={Plus} onclick={() => (adding = { url: "", events: "push", confirm: false })}>{t("cvHookAdd")}</Button>
      </div>
      <List label={t("cvHooks")} items={hooks} key={(h) => String(h.id)} selected={hook ? String(hook.id) : null} onselect={(h) => (selected = h.id)}>
        {#snippet row(h)}
          <Row title={where(h.url)} sub={`${h.events.join(", ")}${h.active ? "" : ` · ${t("cvHookOff")}`}`}>
            {#snippet lead()}<span class="mark"><WebhookIcon size={13} aria-hidden="true" /></span>{/snippet}
            {#snippet trail()}<span class="dot {tone(h)}" title={h.lastCode === null ? t("cvNeverDelivered") : String(h.lastCode)}></span>{/snippet}
          </Row>
        {/snippet}
      </List>
    </div>
    <section class="detail" aria-live="polite">
      {@render form()}
      {#if hook && !adding}
        <header class="hook-head">
          <div class="grow">
            <h2 class="mono">{where(hook.url)}</h2>
            <p class="muted">
              {hook.events.join(", ")} · {hook.active ? t("cvHookOn") : t("cvHookOff")}
              {#if hook.lastCode !== null} · {t("cvLastDelivery", { code: hook.lastCode, status: hook.lastStatus ?? "" })}{/if}
            </p>
          </div>
          {#if slug}<Button size="sm" variant="quiet" icon={ExternalLink} onclick={() => openUrl(`https://github.com/${slug}/settings/hooks/${hook.id}`)}>{t("cvOnGithub")}</Button>{/if}
          <Button size="sm" variant="quiet-danger" icon={Trash2} loading={busy === "delete"} onclick={() => (removing = true)}>{t("cvHookDelete")}</Button>
        </header>
        <h3>{t("cvDeliveries")}</h3>
        {#if deliveriesError}
          <p class="notice error"><CircleAlert size={14} aria-hidden="true" />{deliveriesError}</p>
        {:else if !deliveries}
          <Skeleton shape="table" rows={5} label={t("cvDeliveriesLoading")} />
        {:else if !deliveries.length}
          <p class="muted">{t("cvNoDeliveries")}</p>
        {:else}
          <ul class="deliveries" aria-label={t("cvDeliveries")}>
            {#each deliveries as d (d.id)}
              <li>
                <span class="dot {ok(d.code) ? 'done' : 'failed'}" aria-hidden="true"></span>
                <span class="code num" class:bad={!ok(d.code)}>{d.code || "—"}</span>
                <span class="what">
                  <span class="ev">{d.event}{d.action ? ` · ${d.action}` : ""}</span>
                  <span class="muted small">{d.status}{d.redelivery ? ` · ${t("cvRedelivery")}` : ""}</span>
                </span>
                <span class="muted small num">{Math.round(d.duration * 1000)} ms</span>
                <span class="muted small">{ago(Date.parse(d.delivered) / 1000)}</span>
                <Button size="sm" variant={ok(d.code) ? "quiet" : "secondary"} icon={RotateCw} loading={busy === d.id} disabled={busy !== null} onclick={() => redeliver(d)}>{t("cvRedeliver")}</Button>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </section>
  </div>
  {#if hook}
    <ConfirmDialog bind:open={removing} title={t("cvHookDeleteConfirm")} confirm={t("cvHookDelete")} danger onconfirm={remove}>
      <p>{t("cvHookDeleteText", { url: hook.url })}</p>
    </ConfirmDialog>
  {/if}
{/if}

<style>
  .pad {
    padding: 16px 24px;
  }

  .hooks {
    flex: 1;
  }

  .list-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 6px 8px 10px;
    color: var(--muted);
    font-size: var(--text-sm);
  }

  .mark {
    display: grid;
    flex-shrink: 0;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--well);
    color: var(--soft);
  }

  .dot {
    display: inline-block;
    flex-shrink: 0;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }

  .dot.done {
    background: var(--done);
  }

  .dot.failed {
    background: var(--danger);
  }

  .detail {
    padding: 20px 24px 60px;
    overflow-y: auto;
  }

  .hook-head {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-bottom: 20px;
  }

  .grow {
    flex: 1;
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  h3 {
    margin: 0 0 8px;
    font-size: var(--text-md);
    font-weight: 600;
  }

  p {
    margin: 4px 0 0;
  }

  .deliveries {
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    list-style: none;
  }

  .deliveries li {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: var(--row);
    padding: 4px 8px 4px 12px;
  }

  .deliveries li + li {
    border-top: 1px solid var(--border);
  }

  .code {
    width: 32px;
    font-weight: 600;
  }

  .code.bad {
    color: var(--danger);
  }

  .what {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }

  .ev {
    font-weight: 500;
  }

  .small {
    font-size: var(--text-sm);
  }

  .cmd {
    padding: 6px 10px;
    border-radius: var(--radius);
    background: var(--well);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .row-btns,
  .buttons {
    display: flex;
    justify-content: center;
    gap: 8px;
  }

  .add {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 620px;
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .add h3 {
    margin: 0;
  }

  .add .buttons {
    justify-content: flex-end;
  }

  .events {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .label {
    font-size: var(--text-sm);
    font-weight: 600;
  }
</style>
