<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import Bookmark from "@lucide/svelte/icons/bookmark";
  import Check from "@lucide/svelte/icons/check";
  import MessageSquarePlus from "@lucide/svelte/icons/message-square-plus";
  import Play from "@lucide/svelte/icons/play";
  import ShieldX from "@lucide/svelte/icons/shield-x";
  import SquarePen from "@lucide/svelte/icons/square-pen";
  import SquareArrowOutUpRight from "@lucide/svelte/icons/square-arrow-out-up-right";
  import Wrench from "@lucide/svelte/icons/wrench";
  import { agents, showConversation, upsert, type AgentSession } from "$lib/agents.svelte";
  import type { StoredEvent } from "$lib/conversation-logic";
  import { lang, t } from "$lib/i18n";
  import { blocks, type Span } from "$lib/markdown-logic";
  import { asked, prose, sqlBlock, type Connection } from "$lib/ops-logic";
  import { ops } from "$lib/ops.svelte";

  /** The SQL console's DBA: a conversation with the `dba` role about one database, its answers streamed, each query
   * checked by the console before it can run. */
  let {
    project,
    conn,
    check,
    ask: askVault,
    onuse,
    onrun,
    onsave,
  }: {
    project: string;
    conn: Connection;
    /** The console's verdict on a query: `null` when it reads, else why it is refused. */
    check: (sql: string) => Promise<string | null>;
    ask: (e: unknown, retry: () => Promise<void>) => boolean;
    onuse: (sql: string) => void;
    onrun: (sql: string) => void;
    onsave: (sql: string) => void;
  } = $props();

  const key = $derived(`${project}/${conn.name}`);
  const session = $derived(ops.studio.dba[key] ?? null);
  const phase = $derived(agents.sessions.find((s) => s.id === session)?.state);
  let events = $state<StoredEvent[]>([]);
  let streaming = $state("");
  let sending = $state(false);
  let error = $state("");
  let text = $state("");

  $effect(() => {
    const id = session;
    events = [];
    streaming = "";
    if (id === null) return;
    const off = listen<StoredEvent>("agent-event", ({ payload }) => {
      if (payload.session !== id) return;
      if (payload.kind === "delta") {
        streaming += payload.text;
        return;
      }
      if (payload.kind === "text" || payload.kind === "turn_end") streaming = "";
      if (!events.some((e) => e.id === payload.id)) events = [...events, payload];
    });
    // Listening first, then reading what is stored: nothing falls in between.
    off
      .then(() => invoke<StoredEvent[]>("agent_events", { id }))
      .then((stored) => {
        const known = new Set(stored.map((e) => e.id));
        events = [...stored, ...events.filter((e) => !known.has(e.id))].sort((a, b) => a.id - b.id);
      })
      .catch(() => {});
    return () => void off.then((stop) => stop());
  });

  type Line =
    | { who: "user"; kind: "write" | "explain" | "optimize"; text: string }
    | { who: "dba"; text: string; sql: string | null }
    | { who: "tool"; text: string }
    | { who: "error"; text: string };
  const lines = $derived(
    events.flatMap((e): Line[] => {
      if (e.kind === "user") return [{ who: "user", ...asked(e.text) }];
      if (e.kind === "text" && !e.parent) return [{ who: "dba", text: e.text, sql: sqlBlock(e.text) }];
      if (e.kind === "tool_use" && !e.parent) return [{ who: "tool", text: e.name.replace(/^mcp__termf__/, "") }];
      if (e.kind === "turn_end" && e.outcome !== "done") return [{ who: "error", text: e.error ?? e.outcome }];
      if (e.kind === "error") return [{ who: "error", text: e.message }];
      return [];
    }),
  );
  const working = $derived(sending || phase === "working" || phase === "waiting");
  const lastSql = $derived.by(() => {
    for (const l of [...lines].reverse()) if (l.who === "dba" && l.sql) return l.sql;
    return null;
  });

  // The console's verdict on the last query the DBA gave.
  let verdict = $state<{ sql: string; refused: string | null } | null>(null);
  $effect(() => {
    const sql = lastSql;
    if (!sql || verdict?.sql === sql) return;
    check(sql).then((refused) => (verdict = { sql, refused }));
  });

  /** Asks the DBA: a query from words, or what the editor holds explained or made faster. */
  export async function ask(kind: "write" | "explain" | "optimize", what: string) {
    if (!what.trim() || working) return;
    ops.studio.aside = "dba";
    sending = true;
    error = "";
    try {
      const s = await invoke<AgentSession>("ops_ask", {
        project,
        connection: conn.name,
        session,
        kind,
        text: what.trim(),
        lang,
      });
      upsert(s);
      ops.studio.dba[key] = s.id;
      if (kind === "write") text = "";
    } catch (e) {
      if (!askVault(e, () => ask(kind, what))) error = String(e);
    } finally {
      sending = false;
    }
  }

  function fresh() {
    delete ops.studio.dba[key];
    verdict = null;
    error = "";
  }

  const access = $derived(
    conn.ai_access === "full" ? t("opsAccessFull") : conn.ai_access === "redacted" ? t("opsAccessRedacted") : "",
  );
</script>

{#snippet inline(spans: Span[])}
  {#each spans as s, i (i)}
    {#if s.kind === "code"}<code>{s.text}</code>{:else if s.kind === "strong"}<strong>{@render inline(s.spans)}</strong
      >{:else}{s.text}{/if}
  {/each}
{/snippet}

<section class="dba" aria-label={t("opsdbAsk")}>
  <header>
    <p class="muted">
      {t("opsdbAskSees", { name: conn.name })}
      {conn.ai_access === "none" || !conn.ai_access ? t("opsdbAskNoData") : t("opsdbAskData", { access })}
    </p>
    {#if session !== null}
      <div class="head-actions">
        <button class="icon-btn" title={t("opsdbAskOpen")} aria-label={t("opsdbAskOpen")} onclick={() => showConversation(session)}>
          <SquareArrowOutUpRight size={14} aria-hidden="true" />
        </button>
        <button class="icon-btn" title={t("opsdbAskNew")} aria-label={t("opsdbAskNew")} disabled={working} onclick={fresh}>
          <MessageSquarePlus size={14} aria-hidden="true" />
        </button>
      </div>
    {/if}
  </header>

  <div class="thread" aria-live="polite">
    {#if !lines.length && !working}
      <p class="intro">{t("opsdbAskIntro")}</p>
    {/if}
    {#each lines as line, i (i)}
      {#if line.who === "user"}
        <div class="you">
          {#if line.kind === "write"}<p>{line.text}</p>{:else}<p class="muted">
              {t(line.kind === "explain" ? "opsdbExplainAsk" : "opsdbOptimizeAsk")}
            </p>{/if}
        </div>
      {:else if line.who === "tool"}
        <p class="tool muted"><Wrench size={12} aria-hidden="true" />{t("opsdbAskTool", { tool: line.text })}</p>
      {:else if line.who === "error"}
        <p class="notice error">{t("opsdbAskError", { error: line.text })}</p>
      {:else}
        <div class="answer">
          {#if line.sql}
            <div class="sql">
              <pre>{line.sql}</pre>
              {#if verdict?.sql === line.sql}
                {#if verdict.refused}
                  <p class="verdict refused"><ShieldX size={13} aria-hidden="true" />{t("opsdbAskRefused", { reason: verdict.refused })}</p>
                {:else}
                  <p class="verdict ok"><Check size={13} aria-hidden="true" />{t("opsdbAskChecked")}</p>
                {/if}
              {/if}
              <div class="actions">
                <button class="btn sm" onclick={() => onuse(line.sql ?? "")}><SquarePen size={13} aria-hidden="true" />{t("opsdbAskUse")}</button>
                <button
                  class="btn sm primary"
                  disabled={verdict?.sql === line.sql && !!verdict.refused}
                  onclick={() => onrun(line.sql ?? "")}><Play size={13} aria-hidden="true" />{t("opsdbRun")}</button
                >
                <button class="btn sm" onclick={() => onsave(line.sql ?? "")}><Bookmark size={13} aria-hidden="true" />{t("opsdbSave")}</button>
              </div>
            </div>
          {/if}
          {#each blocks(prose(line.text)) as b, j (j)}
            {#if b.kind === "paragraph"}<p>{@render inline(b.spans)}</p>{:else if b.kind === "list"}
              <ul>
                {#each b.items as item, k (k)}<li>{@render inline(item)}</li>{/each}
              </ul>
            {/if}
          {/each}
        </div>
      {/if}
    {/each}
    {#if working}
      <div class="answer live">
        {#if streaming}
          {@const partial = sqlBlock(streaming)}
          {#if partial}<pre class="partial">{partial}</pre>{/if}
          <p class="stream">{prose(streaming)}<span class="caret" aria-hidden="true"></span></p>
        {:else}
          <div class="thinking" role="status">
            <span class="dot"></span><span class="dot"></span><span class="dot"></span>
            <span class="muted">{t(lines.length ? "opsdbAskWriting" : "opsdbAskThinking")}</span>
          </div>
        {/if}
      </div>
    {/if}
    {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  </div>

  <form
    class="composer"
    onsubmit={(e) => {
      e.preventDefault();
      ask("write", text);
    }}
  >
    <textarea
      class="field"
      rows="3"
      bind:value={text}
      placeholder={lines.length ? t("opsdbAskFollow") : t("opsdbAskPlaceholder")}
      aria-label={t("opsdbAsk")}
      onkeydown={(e) => {
        if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
          e.preventDefault();
          ask("write", text);
        }
      }}
    ></textarea>
    <button class="btn sm primary send" disabled={working || !text.trim()}>
      <ArrowUp size={14} aria-hidden="true" />{t("opsdbAskSend")}
    </button>
  </form>
</section>

<style>
  .dba {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    height: 100%;
  }

  header {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  header p {
    flex: 1;
    margin: 0;
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .head-actions {
    display: flex;
    gap: 2px;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }

  .thread {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow-y: auto;
    font-size: var(--text-sm);
    line-height: 1.55;
  }

  .intro {
    margin: 0;
    padding: 12px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    color: var(--muted);
  }

  .you {
    align-self: flex-end;
    max-width: 88%;
    padding: 8px 11px;
    border-radius: var(--radius-lg) var(--radius-lg) 4px var(--radius-lg);
    background: var(--accent-soft);
  }

  .you p,
  .answer p {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .answer {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .answer ul {
    margin: 0;
    padding-left: 18px;
  }

  .answer code {
    padding: 0 3px;
    border-radius: 3px;
    background: var(--hover);
    font-family: var(--font-mono);
    font-size: 0.92em;
  }

  .sql {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  pre {
    margin: 0;
    padding: 2px 4px;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .partial {
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--panel);
  }

  .verdict {
    display: flex;
    align-items: flex-start;
    gap: 5px;
    margin: 0;
    font-size: var(--text-xs);
  }

  .verdict :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .verdict.ok {
    color: var(--done);
  }

  .verdict.refused {
    color: var(--danger);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tool {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 0;
    font-size: var(--text-xs);
  }

  .stream {
    color: var(--text);
  }

  .caret {
    display: inline-block;
    width: 7px;
    height: 1em;
    margin-left: 2px;
    vertical-align: text-bottom;
    background: var(--accent);
    animation: blink 1s steps(2, start) infinite;
  }

  .thinking {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: var(--text-xs);
  }

  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.2s ease-in-out infinite;
  }

  .dot:nth-child(2) {
    animation-delay: 0.15s;
  }

  .dot:nth-child(3) {
    margin-right: 4px;
    animation-delay: 0.3s;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    50% {
      opacity: 1;
    }
  }

  @keyframes blink {
    to {
      visibility: hidden;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dot,
    .caret {
      animation: none;
    }
  }

  .composer {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  textarea {
    resize: none;
    font: inherit;
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .send {
    align-self: flex-end;
  }
</style>
