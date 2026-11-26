<script lang="ts">
  import LoaderCircle from "@lucide/svelte/icons/loader-circle";
  import Search from "@lucide/svelte/icons/search";
  import Trash2 from "@lucide/svelte/icons/trash-2";
  import { onMount } from "svelte";
  import OpsWrite from "$lib/OpsWrite.svelte";
  import Switch from "$lib/ui/Switch.svelte";
  import { t } from "$lib/i18n";
  import { hitRate, pairs, redisInfo, redisScan, type Connection } from "$lib/ops-logic";
  import { runRecipe } from "$lib/ops.svelte";
  import { toast } from "$lib/ui/toast.svelte";

  /** A Redis server, read through `redis-read`'s allow list: its INFO, its keys by pattern, a key's type, TTL and
   * value; its changes, once allowed, go through `redis-write` and its card. `browse`: its first keys listed at once. */
  let {
    project,
    conn,
    ask,
    browse = false,
  }: { project: string; conn: Connection; ask: (e: unknown, retry: () => Promise<void>) => boolean; browse?: boolean } = $props();

  let busy = $state("");
  let error = $state("");
  let info = $state<Record<string, string> | null>(null);
  let pattern = $state("*");
  let cursor = $state("0");
  let keys = $state<string[]>([]);
  let searched = $state(false);
  let key = $state<string | null>(null);
  let shown = $state<{ type: string; ttl: number; lines: string[]; text: string } | null>(null);
  let seconds = $state(3600);
  let writing = $state<{ title: string; loses: string; params: Record<string, string> } | null>(null);
  let confirming = $state(false);
  let changes = $state(false);

  onMount(() => {
    if (browse) void scan();
  });

  /** One command of the allow list; its output as text. */
  async function read(command: string, params: Record<string, string> = {}): Promise<string> {
    const ran = await runRecipe(project, "redis-read", conn.name, { command, ...params });
    if (ran.output.exit !== 0) throw new Error(ran.output.errors.trim() || ran.output.text.trim() || ran.output.stopped || "redis-cli");
    return ran.output.text;
  }

  async function step(name: string, work: () => Promise<void>) {
    busy = name;
    error = "";
    try {
      await work();
    } catch (e) {
      if (!ask(e, () => step(name, work))) error = String(e instanceof Error ? e.message : e);
    } finally {
      busy = "";
    }
  }

  const readInfo = () => step("info", async () => void (info = redisInfo(await read("INFO"))));

  function scan(more = false) {
    return step("scan", async () => {
      const page = redisScan(await read("SCAN {cursor} MATCH {pattern} COUNT 200", { cursor: more ? cursor : "0", pattern: pattern.trim() || "*" }));
      keys = more ? [...keys, ...page.keys] : page.keys;
      cursor = page.cursor;
      searched = true;
      if (!info) await readInfo();
    });
  }

  const commands: Record<string, string> = {
    string: "GET {key}",
    hash: "HSCAN {key} {cursor} COUNT 200",
    list: "LRANGE {key} 0 199",
    set: "SSCAN {key} {cursor} COUNT 200",
    zset: "ZRANGE {key} 0 199 WITHSCORES",
    stream: "XRANGE {key} - + COUNT 100",
  };

  function open(k: string) {
    key = k;
    shown = null;
    return step("key", async () => {
      const type = (await read("TYPE {key}", { key: k })).trim();
      const ttl = Number((await read("PTTL {key}", { key: k })).trim());
      const command = commands[type];
      const text = command ? await read(command, { key: k, cursor: "0" }) : "";
      const lines = text.split("\n").slice(0, -1);
      // HSCAN and SSCAN start with their next cursor.
      shown = { type, ttl, text, lines: type === "hash" || type === "set" ? lines.slice(1) : lines };
    });
  }

  function propose(title: string, loses: string, params: Record<string, string>) {
    writing = { title, loses, params };
    confirming = true;
  }

  function write(typed: string) {
    const w = writing;
    if (!w) return;
    void step("write", async () => {
      const ran = await runRecipe(project, "redis-write", conn.name, w.params, typed);
      if (ran.output.exit !== 0) throw new Error(ran.output.errors || ran.output.text);
      toast(t("opsmoreDone", { what: w.title }));
      if (w.params.command === "FLUSHDB" || w.params.command.startsWith("DEL")) {
        key = null;
        shown = null;
        await scan();
      } else if (key) await open(key);
    });
  }

  const ttl = (ms: number) =>
    ms < 0 ? t("opsmoreNoTtl") : ms < 60_000 ? `${Math.round(ms / 1000)} s` : ms < 3_600_000 ? `${Math.round(ms / 60_000)} min` : `${Math.round(ms / 3_600_000)} h`;
  const keyCount = $derived(Object.entries(info ?? {}).find(([k]) => k === `db${conn.database ?? "0"}`)?.[1].match(/keys=(\d+)/)?.[1] ?? "0");
  const rate = $derived(info ? hitRate(info) : null);
</script>

<section class="redis" aria-label={t("opsmoreRedis")}>
  {#if error}<p class="notice error" role="alert">{error}</p>{/if}
  {#if info}
    <dl class="info">
      <div><dt>{t("opsmoreVersion")}</dt><dd class="mono">{info.redis_version ?? "—"}</dd></div>
      <div><dt>{t("opsmoreUsedMemory")}</dt><dd>{info.used_memory_human ?? "—"}</dd></div>
      <div><dt>{t("opsmoreClients")}</dt><dd>{info.connected_clients ?? "—"}</dd></div>
      <div><dt>{t("opsmoreKeysTotal")}</dt><dd>{keyCount}</dd></div>
      <div><dt>{t("opsmoreHitRate")}</dt><dd>{rate === null ? "—" : `${rate} %`}</dd></div>
    </dl>
  {/if}

  <form
    class="search"
    onsubmit={(e) => {
      e.preventDefault();
      void scan();
    }}
  >
    <label class="labelled">
      <span>{t("opsmorePattern")}</span>
      <input class="field mono" bind:value={pattern} placeholder="user:*" />
    </label>
    <button class="btn sm primary" disabled={!!busy}>
      {#if busy === "scan"}<LoaderCircle size={13} aria-hidden="true" class="spin" />{:else}<Search size={13} aria-hidden="true" />{/if}{t("opsmoreScan")}
    </button>
    {#if changes}
      <button
        type="button"
        class="btn sm danger quiet"
        disabled={!!busy}
        onclick={() => propose(t("opsmoreFlush"), t("opsmoreFlushLoses", { db: conn.database ?? "0", name: conn.name }), { command: "FLUSHDB" })}
      >
        <Trash2 size={13} aria-hidden="true" />{t("opsmoreFlush")}
      </button>
    {/if}
  </form>
  <div class="changes"><Switch bind:checked={changes} label={t("opstidyRedisWrite")} hint={t("opstidyRedisWriteHint")} /></div>

  {#if searched}
    <div class="browse">
      <div class="keys">
        <p class="muted small">{t("opsmoreKeys", { count: keys.length })}</p>
        <ul>
          {#each keys as k (k)}
            <li><button class="mono" class:on={k === key} onclick={() => open(k)}>{k}</button></li>
          {:else}<li class="muted small">{t("opsmoreNoKey")}</li>{/each}
        </ul>
        {#if cursor !== "0"}<button class="btn sm quiet" disabled={!!busy} onclick={() => scan(true)}>{t("opsmoreMoreKeys")}</button>{/if}
      </div>
      <div class="value">
        {#if key}
          <header>
            <b class="mono">{key}</b>
            {#if busy === "key"}<LoaderCircle size={14} aria-hidden="true" class="spin" />{/if}
          </header>
          {#if shown}
            <p class="facts">
              <span><small class="muted">{t("opsmoreType")}</small> {shown.type}</span>
              <span><small class="muted">{t("opsmoreTtl")}</small> {ttl(shown.ttl)}</span>
            </p>
            {#if shown.type === "hash" || shown.type === "zset"}
              <table>
                <tbody>{#each pairs(shown.lines) as [a, b], i (i)}<tr><td class="mono">{a}</td><td class="mono">{b}</td></tr>{/each}</tbody>
              </table>
            {:else if shown.type === "list" || shown.type === "set"}
              <ol class="mono">{#each shown.lines as l, i (i)}<li>{l}</li>{/each}</ol>
            {:else}
              <pre>{shown.type === "string" ? shown.text.replace(/\n$/, "") : shown.text}</pre>
            {/if}
            {#if shown.type !== "string" && shown.type !== "none"}<p class="muted small">{t("opsmoreFirst")}</p>{/if}
            {#if changes}
              <div class="actions">
                <button class="btn sm danger" onclick={() => propose(t("opsmoreDelete"), t("opsmoreDeleteLoses", { key: key ?? "" }), { command: "DEL {key}", key: key ?? "" })}>
                  <Trash2 size={13} aria-hidden="true" />{t("opsmoreDelete")}
                </button>
                <input class="field seconds" type="number" min="1" bind:value={seconds} aria-label={t("opsmoreParamSeconds")} />
                <button
                  class="btn sm"
                  onclick={() =>
                    propose(t("opsmoreExpire"), t("opsmoreExpireLoses", { key: key ?? "", seconds }), {
                      command: "EXPIRE {key} {seconds}",
                      key: key ?? "",
                      seconds: String(seconds),
                    })}>{t("opsmoreExpire")}</button
                >
                {#if shown.ttl >= 0}
                  <button class="btn sm" onclick={() => propose(t("opsmorePersist"), t("opsmorePersistLoses", { key: key ?? "" }), { command: "PERSIST {key}", key: key ?? "" })}>
                    {t("opsmorePersist")}
                  </button>
                {/if}
              </div>
            {/if}
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</section>

<OpsWrite bind:open={confirming} title={writing?.title ?? ""} loses={writing?.loses ?? ""} env={conn.env} onconfirm={write} />

<style>
  .redis {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-top: 16px;
  }

  .info {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
  }

  .info div {
    padding: 8px 12px;
    border-radius: var(--radius-md);
    background: var(--hover);
  }

  .info dt {
    color: var(--muted);
    font-size: var(--text-xs);
  }

  .info dd {
    margin: 0;
    font-weight: 600;
  }

  .search {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: 8px;
  }

  .search .labelled {
    flex: 1;
    min-width: 180px;
  }

  .browse {
    display: grid;
    grid-template-columns: minmax(180px, 2fr) minmax(0, 3fr);
    gap: 12px;
    min-height: 0;
  }

  .keys ul {
    max-height: 360px;
    margin: 4px 0;
    padding: 0;
    overflow: auto;
    list-style: none;
  }

  .keys button {
    width: 100%;
    padding: 4px 8px;
    border: 0;
    border-radius: var(--radius);
    background: none;
    color: var(--text);
    font-size: var(--text-sm);
    text-align: left;
    overflow-wrap: anywhere;
    cursor: pointer;
  }

  .keys button:hover,
  .keys button.on {
    background: var(--hover);
  }

  .keys button.on {
    color: var(--accent-ink, var(--accent));
  }

  .value {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .value header {
    display: flex;
    align-items: center;
    gap: 8px;
    overflow-wrap: anywhere;
  }

  .facts {
    display: flex;
    gap: 14px;
    margin: 0;
    font-size: var(--text-sm);
  }

  pre,
  table,
  ol {
    max-height: 320px;
    margin: 0;
    overflow: auto;
    font-size: var(--text-xs);
  }

  pre {
    padding: 8px 10px;
    border-radius: var(--radius);
    background: var(--hover);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  table {
    display: block;
    border-collapse: collapse;
  }

  td {
    padding: 3px 10px 3px 0;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
    overflow-wrap: anywhere;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .changes {
    max-width: 520px;
    font-size: var(--text-sm);
  }

  .seconds {
    width: 90px;
  }

  .small {
    margin: 0;
    font-size: var(--text-sm);
  }
</style>
