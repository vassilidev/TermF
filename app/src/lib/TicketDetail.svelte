<script lang="ts">
  import { onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import CircleAlert from "@lucide/svelte/icons/circle-alert";
  import Download from "@lucide/svelte/icons/download";
  import ExternalLink from "@lucide/svelte/icons/external-link";
  import FileText from "@lucide/svelte/icons/file-text";
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Play from "@lucide/svelte/icons/play";
  import Sparkles from "@lucide/svelte/icons/sparkles";
  import X from "@lucide/svelte/icons/x";
  import Skeleton from "$lib/ui/Skeleton.svelte";
  import { toast } from "$lib/ui/toast.svelte";
  import { askAI } from "$lib/ask.svelte";
  import { ago, lang, t } from "$lib/i18n";
  import { blocks, type Span } from "$lib/markdown-logic";
  import { openUrl, type TicketDetail, type TicketSummary } from "$lib/tickets.svelte";

  /**
   * One ticket in full, inside TermF whatever its tracker: its text rendered, its files, what the tracker says of it,
   * its comments, and what to do with it. What the tracker gives was written by anyone: it shows as text.
   */
  let {
    project,
    ticket,
    starting,
    onlaunch,
    onopen,
    onmoved,
  }: {
    project: string;
    ticket: TicketSummary;
    /** The URL of the ticket being launched. */
    starting: string;
    onlaunch: (ticket: TicketSummary) => void;
    /** A subtask or a linked ticket, opened in the same panel. */
    onopen: (ticket: TicketSummary) => void;
    onmoved?: (ticket: TicketSummary, to: string) => void;
  } = $props();

  type Attachment = { id: string; name: string; mime: string; size: number; image: boolean };
  type Full = TicketDetail & {
    details: {
      kind: string | null;
      priority: string | null;
      sprint: string | null;
      space: string | null;
      created: string | null;
      updated: string | null;
      due: string | null;
      subtasks: (TicketSummary & { relation: string })[];
      related: (TicketSummary & { relation: string })[];
      attachments: Attachment[];
    };
  };

  let detail = $state<Full | null>(null);
  let error = $state("");
  let states = $state<string[]>([]);
  let current = $state("");
  let moving = $state(false);
  let comment = $state("");
  let commenting = $state(false);
  let thumbs = $state<Record<string, string>>({});
  let big = $state<{ name: string; url: string | null } | null>(null);
  let lightbox = $state<HTMLDialogElement>();
  const urls: string[] = [];

  const ref = $derived({ provider: ticket.provider, key: ticket.key, url: ticket.url });
  const writable = $derived(!ticket.provider.startsWith("mcp:"));

  let asked = 0;
  $effect(() => {
    const [n, r] = [++asked, ref];
    [detail, error, states, current, comment] = [null, "", [], ticket.state, ""];
    invoke<Full>("tickets_ticket", { project, ticket: r }).then(
      (got) => {
        if (n !== asked) return;
        detail = got;
        current = got.state;
        for (const a of got.details.attachments.filter((x) => x.image)) void thumb(n, a);
      },
      (e) => n === asked && (error = String(e)),
    );
    invoke<string[]>("tickets_states", { project, ticket: r }).then(
      (list) => n === asked && (states = list),
      () => {},
    );
  });

  onDestroy(() => urls.forEach((u) => URL.revokeObjectURL(u)));

  async function bytes(id: string, thumbnail: boolean, mime: string): Promise<string> {
    const buffer = await invoke<ArrayBuffer>("tickets_attachment", { project, ticket: ref, id, thumbnail });
    const url = URL.createObjectURL(new Blob([buffer], { type: thumbnail ? "image/png" : mime }));
    urls.push(url);
    return url;
  }

  async function thumb(n: number, a: Attachment) {
    try {
      const url = await bytes(a.id, true, a.mime);
      if (n === asked) thumbs[a.id] = url;
    } catch {
      // A file Jira makes no thumbnail of still shows by its name.
    }
  }

  async function enlarge(a: Attachment) {
    big = { name: a.name, url: null };
    lightbox?.showModal();
    try {
      const url = await bytes(a.id, false, a.mime);
      if (big?.name === a.name) big = { name: a.name, url };
    } catch (e) {
      lightbox?.close();
      toast(String(e), { kind: "error" });
    }
  }

  async function keep(a: Attachment) {
    const path = await save({ defaultPath: a.name });
    if (!path) return;
    try {
      await invoke("tickets_attachment_save", { project, ticket: ref, id: a.id, path });
      toast(t("tjdSaved", { name: a.name }));
    } catch (e) {
      toast(String(e), { kind: "error" });
    }
  }

  /** The state shows moved at once; it comes back, with why, when the tracker refuses. */
  async function move(to: string) {
    const was = current;
    if (to === was) return;
    [current, moving] = [to, true];
    try {
      await invoke("tickets_move", { project, ticket: ref, to });
      toast(t("tjdMoved", { key: ticket.key, to }));
      onmoved?.(ticket, to);
      states = await invoke<string[]>("tickets_states", { project, ticket: ref }).catch(() => states);
    } catch (e) {
      current = was;
      toast(t("tjdMoveRefused", { key: ticket.key, error: String(e) }), { kind: "error" });
    } finally {
      moving = false;
    }
  }

  async function send() {
    commenting = true;
    try {
      await invoke("tickets_comment", { project, ticket: ref, body: comment });
      comment = "";
      toast(t("tjdCommented", { key: ticket.key }));
      const got = await invoke<Full>("tickets_ticket", { project, ticket: ref });
      if (detail?.key === got.key) detail = got;
    } catch (e) {
      toast(String(e), { kind: "error" });
    } finally {
      commenting = false;
    }
  }

  function askCto() {
    askAI({
      role: "cto",
      chips: [{ id: `ticket:${ticket.url}`, kind: "selection", label: ticket.key, text: `${ticket.key} ${detail?.title ?? ticket.title}\n${ticket.url}` }],
      draft: t("tjdAskDraft", { key: ticket.key }),
    });
  }

  const related = (r: TicketSummary & { relation: string }): TicketSummary => ({
    ...r,
    labels: [],
    assignees: [],
    updated: ticket.updated,
    project: ticket.project,
    source: ticket.source,
  });

  const size = (n: number) =>
    new Intl.NumberFormat(lang, { style: "unit", unit: n > 1 << 20 ? "megabyte" : "kilobyte", maximumFractionDigits: 1 }).format(n > 1 << 20 ? n / (1 << 20) : Math.max(1, n / 1024));
  const day = (iso: string | null) => (iso ? ago(Date.parse(iso) / 1000) : "");

  function open(e: MouseEvent, url: string) {
    e.preventDefault();
    openUrl(url);
  }

  /** Markdown as the trackers write it, without the quote marks and image marks the page does not draw. */
  const clean = (text: string) => text.replace(/^> ?/gm, "").replace(/!\[/g, "[");
</script>

{#snippet line(list: Span[])}{#each list as span, i (i)}{#if span.kind === "strong"}<strong>{@render line(span.spans)}</strong
      >{:else if span.kind === "code"}<code>{span.text}</code>{:else if span.kind === "link"}<a
        href={span.url}
        onclick={(e) => open(e, span.url)}>{span.text}</a
      >{:else}{span.text}{/if}{/each}{/snippet}

{#snippet text(body: string)}
  {#each blocks(clean(body)) as block, i (i)}
    {#if block.kind === "code"}
      <pre class="mono">{block.text}</pre>
    {:else if block.kind === "list" && block.start === null}
      <ul>{#each block.items as item, j (j)}<li>{@render line(item)}</li>{/each}</ul>
    {:else if block.kind === "list"}
      <ol start={block.start}>{#each block.items as item, j (j)}<li>{@render line(item)}</li>{/each}</ol>
    {:else if block.kind === "table"}
      <div class="table">
        <table>
          <thead><tr>{#each block.head as cell, j (j)}<th>{@render line(cell)}</th>{/each}</tr></thead>
          <tbody>{#each block.rows as row, j (j)}<tr>{#each row as cell, k (k)}<td>{@render line(cell)}</td>{/each}</tr>{/each}</tbody>
        </table>
      </div>
    {:else if block.kind === "paragraph" || block.kind === "heading"}
      <p class:heading={block.kind === "heading"}>{@render line(block.spans)}</p>
    {/if}
  {/each}
{/snippet}

{#snippet tickets(list: (TicketSummary & { relation: string })[], title: string)}
  {#if list.length}
    <h4>{title}</h4>
    <ul class="related">
      {#each list as r (r.url)}
        <li>
          <button class="link-row" onclick={() => onopen(related(r))}>
            <span class="relation">{r.relation}</span>
            <span class="mono">{r.key}</span>
            <span class="rtitle">{r.title}</span>
            {#if r.state}<span class="tag">{r.state}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
{/snippet}

<article class="detail" aria-busy={!detail && !error}>
  <header>
    <span class="mono key">{ticket.key}</span>
    {#if detail?.details.kind}<span class="tag">{detail.details.kind}</span>{/if}
    {#if states.length && writable}
      <label class="state">
        <span class="sr-only">{t("tjdState")}</span>
        <select class="field sm" value={current} onchange={(e) => move(e.currentTarget.value)} disabled={moving}>
          <option value={current}>{current}</option>
          {#each states.filter((s) => s !== current) as s (s)}<option value={s}>{s}</option>{/each}
        </select>
        {#if moving}<LoaderCircle size={13} aria-hidden="true" class="spin" />{/if}
      </label>
    {:else if current}
      <span class="tag">{current}</span>
    {/if}
  </header>
  <h3>{detail?.title ?? ticket.title}</h3>

  <div class="buttons">
    <button class="btn primary" onclick={() => onlaunch(ticket)} disabled={!!starting}>
      {#if starting === ticket.url}<LoaderCircle size={14} aria-hidden="true" class="spin" />{:else}<Play size={14} aria-hidden="true" />{/if}{t("tjdLaunch")}
    </button>
    <button class="btn" onclick={askCto}><Sparkles size={14} aria-hidden="true" />{t("tjdAskCto")}</button>
    <button class="btn quiet" onclick={() => openUrl(ticket.url)} title={t("tjdOpenBrowser")}>
      <ExternalLink size={14} aria-hidden="true" />{t("tjdOpenBrowser")}
    </button>
  </div>

  {#if error}
    <p class="notice error" role="alert"><CircleAlert size={14} aria-hidden="true" />{t("tjdDetailFailed", { error })}</p>
  {:else if !detail}
    <Skeleton shape="lines" rows={8} delay={0} label={t("tjdLoading")} />
  {:else}
    {@const d = detail.details}
    <dl class="meta">
      <dt>{t("tjdAssignee")}</dt>
      <dd>{detail.assignees.join(", ") || t("tjdNobody")}</dd>
      {#if detail.author}<dt>{t("tjdReporter")}</dt><dd>{detail.author}</dd>{/if}
      {#if d.priority}<dt>{t("tjdPriority")}</dt><dd>{d.priority}</dd>{/if}
      {#if d.sprint}<dt>{t("tjdSprintLabel")}</dt><dd>{d.sprint}</dd>{/if}
      {#if d.space}<dt>{t("tjdSpace")}</dt><dd class="mono">{d.space}</dd>{/if}
      {#if d.created}<dt>{t("tjdCreated")}</dt><dd>{day(d.created)}</dd>{/if}
      {#if d.updated}<dt>{t("tjdUpdated")}</dt><dd>{day(d.updated)}</dd>{/if}
      {#if d.due}<dt>{t("tjdDue")}</dt><dd>{d.due}</dd>{/if}
      {#if detail.labels.length}
        <dt>{t("tjdLabels")}</dt>
        <dd class="labels">{#each detail.labels as label (label)}<span class="tag">{label}</span>{/each}</dd>
      {/if}
    </dl>

    <div class="body">
      {#if detail.body.trim()}{@render text(detail.body)}{:else}<p class="muted">{t("tjdNoBody")}</p>{/if}
    </div>

    {#if d.attachments.length}
      <h4>{t("tjdFiles", { count: d.attachments.length })}</h4>
      <ul class="files">
        {#each d.attachments as a (a.id)}
          <li>
            {#if a.image}
              <button class="thumb" onclick={() => enlarge(a)} aria-label={t("tjdEnlarge", { name: a.name })} title={a.name}>
                {#if thumbs[a.id]}<img src={thumbs[a.id]} alt={a.name} />{:else}<span class="ph"><FileText size={18} aria-hidden="true" /></span>{/if}
              </button>
            {:else}
              <span class="file"><FileText size={16} aria-hidden="true" /><span class="fname">{a.name}</span><span class="muted">{size(a.size)}</span></span>
            {/if}
            <button class="icon-btn sm" onclick={() => keep(a)} aria-label={t("tjdSave", { name: a.name })} title={t("tjdSave", { name: a.name })}>
              <Download size={14} aria-hidden="true" />
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    {@render tickets(d.subtasks, t("tjdSubtasks"))}
    {@render tickets(d.related, t("tjdRelated"))}

    <h4>{t("tjdComments", { count: detail.comments.length })}</h4>
    {#if detail.comments.length}
      <ol class="comments">
        {#each detail.comments as c, i (i)}
          <li>
            <p class="muted small">{c.author ?? "?"} · {ago(Date.parse(c.created) / 1000)}</p>
            <div class="body">{@render text(c.body)}</div>
          </li>
        {/each}
      </ol>
    {/if}
    {#if writable}
      <form
        class="reply"
        onsubmit={(e) => {
          e.preventDefault();
          void send();
        }}
      >
        <textarea class="field" rows="3" bind:value={comment} placeholder={t("tjdCommentPlaceholder")} aria-label={t("tjdCommentPlaceholder")}></textarea>
        <button class="btn sm" type="submit" disabled={!comment.trim() || commenting}>
          {#if commenting}<LoaderCircle size={13} aria-hidden="true" class="spin" />{/if}{t("tjdComment")}
        </button>
      </form>
    {/if}
  {/if}
</article>

<dialog bind:this={lightbox} class="lightbox" aria-label={big?.name ?? ""} onclose={() => (big = null)}>
  {#if big}
    <header>
      <span>{big.name}</span>
      <button class="icon-btn sm" onclick={() => lightbox?.close()} aria-label={t("close")}><X size={14} aria-hidden="true" /></button>
    </header>
    {#if big.url}<img src={big.url} alt={big.name} />{:else}<Skeleton shape="block" height="320px" delay={0} />{/if}
  {/if}
</dialog>

<style>
  .detail {
    padding: 16px 20px 24px;
  }

  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .key {
    color: var(--muted);
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  h3 {
    margin: 6px 0 2px;
    font-size: var(--text-lg);
  }

  h4 {
    margin: 18px 0 6px;
    font-size: var(--text-sm);
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 12px 0;
  }

  .meta {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 14px;
    margin: 0 0 12px;
    font-size: var(--text-sm);
  }

  .meta dt {
    color: var(--muted);
  }

  .meta dd {
    margin: 0;
  }

  .labels {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .tag {
    flex: none;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--muted);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .body {
    overflow-wrap: anywhere;
    line-height: 1.55;
  }

  .body :global(pre) {
    overflow-x: auto;
    padding: 8px;
    border-radius: var(--radius-sm);
    background: var(--hover);
  }

  .heading {
    font-weight: 600;
  }

  .table {
    overflow-x: auto;
  }

  table {
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th,
  td {
    padding: 4px 8px;
    border: 1px solid var(--border);
    text-align: left;
  }

  .files {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .files li {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .thumb {
    width: 96px;
    height: 72px;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--hover);
    cursor: zoom-in;
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .ph {
    display: grid;
    height: 100%;
    place-items: center;
    color: var(--muted);
  }

  .file {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
  }

  .related {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .link-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 5px 8px;
    border: 0;
    border-radius: var(--radius-sm);
    background: none;
    color: inherit;
    font: inherit;
    font-size: var(--text-sm);
    text-align: left;
    cursor: pointer;
  }

  .link-row:hover {
    background: var(--hover);
  }

  .relation {
    min-width: 90px;
    color: var(--muted);
  }

  .rtitle {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .comments {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
  }

  .comments li {
    padding-left: 10px;
    border-left: 2px solid var(--border);
  }

  .comments p {
    margin: 0;
  }

  .reply {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
  }

  .reply textarea {
    width: 100%;
    resize: vertical;
  }

  .small {
    font-size: var(--text-sm);
  }

  .mono {
    font-family: var(--font-mono);
  }

  .lightbox {
    max-width: 92vw;
    max-height: 92vh;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg, 12px);
    background: var(--panel);
    color: inherit;
  }

  .lightbox::backdrop {
    background: rgb(0 0 0 / 0.6);
  }

  .lightbox header {
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .lightbox img {
    display: block;
    max-width: 88vw;
    max-height: 80vh;
  }
</style>
