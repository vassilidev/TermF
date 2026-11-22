<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Check from "@lucide/svelte/icons/check";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Copy from "@lucide/svelte/icons/copy";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import Info from "@lucide/svelte/icons/info";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Send from "@lucide/svelte/icons/send";
  import TriangleAlert from "@lucide/svelte/icons/triangle-alert";
  import DoneCriteria from "$lib/DoneCriteria.svelte";
  import { t } from "$lib/i18n";
  import { openUrl, upsert, type Delivery, type DeliveryPreview, type Run } from "$lib/tickets.svelte";

  /** The delivery in two guarded steps: what it would send, then the confirmation of exactly that. */
  let { run, onclose }: { run: Run; onclose: () => void } = $props();

  let preview = $state<DeliveryPreview | null>(null);
  let delivered = $state<{ warning: string | null; again: boolean } | null>(null);
  let busy = $state<"" | "preview" | "deliver">("");
  let error = $state("");
  let copied = $state(false);

  async function load() {
    busy = "preview";
    error = "";
    try {
      preview = await invoke<DeliveryPreview>("tickets_deliver_preview", { id: run.id });
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }
  load();

  async function deliver(p: DeliveryPreview) {
    busy = "deliver";
    error = "";
    try {
      const again = !!run.prUrl;
      const delivery = await invoke<Delivery>("tickets_deliver", { id: run.id, confirm: p.fingerprint });
      upsert(delivery.run);
      delivered = { warning: delivery.warning, again };
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  async function copy(url: string) {
    try {
      await navigator.clipboard.writeText(url);
      copied = true;
    } catch (e) {
      error = String(e);
    }
  }

  const short = (commit: string) => commit.slice(0, 7);
  const changeLabel = {
    added: "statusAdded",
    modified: "statusModified",
    deleted: "statusDeleted",
    renamed: "statusRenamed",
  } as const;
</script>

{#if delivered}
  <section class="card done" role="status">
    <p class="ok"><Check size={14} aria-hidden="true" />{t(delivered.again ? "deliveredAgain" : "delivered")}</p>
    {#if run.prUrl}
      <span class="link-row">
        <span class="mono">{run.prUrl}</span>
        <button class="icon-btn sm" onclick={() => run.prUrl && openUrl(run.prUrl)} aria-label={t("ticketsUiOpenLink")} title={t("ticketsUiOpenLink")}>
          <ExternalLink size={13} aria-hidden="true" />
        </button>
        <button class="icon-btn sm" onclick={() => run.prUrl && copy(run.prUrl)} aria-label={t("runCopyLink")} title={t("runCopyLink")}>
          {#if copied}<Check size={13} aria-hidden="true" />{:else}<Copy size={13} aria-hidden="true" />{/if}
        </button>
      </span>
    {/if}
    {#if delivered.warning}
      <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{delivered.warning}</p>
    {/if}
    <div class="buttons">
      <button class="btn sm" onclick={onclose}>{t("dismiss")}</button>
    </div>
  </section>
{:else}
  <section class="card" aria-labelledby="deliver-title" aria-busy={busy === "preview"}>
    <h3 id="deliver-title">
      <Send size={14} aria-hidden="true" />{t("deliverTitle")}
      {#if busy === "preview"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
    </h3>
    {#if preview}
      {@const p = preview}
      {#if p.done}
        <DoneCriteria done={p.done} />
        <p class="notice warn"><TriangleAlert size={14} aria-hidden="true" />{t("disciplineDoneStopped")}</p>
      {/if}
      {#if p.files.length}
        <p class="sub">{t("deliverFiles", { count: p.files.length })}</p>
        <ul class="changes">
          {#each p.files as file (file.path)}
            <li>
              <span class="change {file.change}">{t(changeLabel[file.change])}</span>
              <span class="mono">{file.from ? `${file.from} → ${file.path}` : file.path}</span>
            </li>
          {/each}
        </ul>
      {/if}
      {#if p.commits.length}
        <p class="sub">{t("deliverCommits", { count: p.commits.length })}</p>
        <ul class="changes">
          {#each p.commits as commit (commit.commit)}
            <li><span class="mono muted">{short(commit.commit)}</span><span>{commit.subject}</span></li>
          {/each}
        </ul>
      {/if}
      {#if !p.files.length && !p.commits.length}
        <p class="notice"><Info size={14} aria-hidden="true" />{t("deliverNothing")}</p>
      {/if}
      <p class="how">
        {run.prUrl
          ? t("deliverHowAgain", { branch: run.branch ?? "" })
          : t("deliverHow", { branch: run.branch ?? "", target: (run.base ?? "").replace(/^origin\//, "") })}
      </p>
      <p class="how muted">{t("deliverFingerprint", { fingerprint: p.fingerprint.split(":").map(short).join(":") })}</p>
    {/if}
    {#if error}<p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{error}</p>{/if}
    <div class="buttons">
      <button class="btn sm" onclick={onclose}>{t("cancel")}</button>
      {#if error}
        <button class="btn sm" onclick={load} disabled={!!busy}>{t("deliverAgain")}</button>
      {/if}
      {#if preview}
        {@const p = preview}
        <button class="btn primary" onclick={() => deliver(p)} disabled={!!busy}>
          {#if busy === "deliver"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
          {t("deliverConfirm")}
        </button>
      {/if}
    </div>
  </section>
{/if}

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
    box-shadow: var(--shadow-1);
    font-size: var(--text-sm);
  }

  .card.done {
    border-color: transparent;
    background: var(--done-soft);
    box-shadow: none;
  }

  h3 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: var(--text-base);
    font-weight: 650;
  }

  p {
    margin: 0;
  }

  .sub {
    color: var(--muted);
    font-weight: 600;
  }

  .changes {
    max-height: 220px;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }

  .changes li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 2px 0;
  }

  .change {
    flex-shrink: 0;
    width: 64px;
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .added {
    color: var(--done);
  }

  .modified,
  .renamed {
    color: var(--warning);
  }

  .deleted {
    color: var(--danger);
  }

  .how {
    line-height: 1.5;
  }

  .ok {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--done);
    font-weight: 600;
  }

  .link-row {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
  }

  .mono {
    overflow-wrap: anywhere;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
