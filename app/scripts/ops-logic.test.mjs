import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import {
  asked,
  autoChart,
  chartPoints,
  columnKind,
  layout,
  pieReadable,
  relations,
  ticks,
  timeOf,
  bounded,
  clean,
  completions,
  copyName,
  engineRefused,
  exportedQuery,
  secretClash,
  firstTable,
  passwordName,
  savedGroups,
  sqlBlock,
  historyRow,
  level,
  missing,
  pastQueries,
  refusal,
  runsOn,
  secondsLeft,
  shippedTexts,
  shows,
  streamed,
  sortRows,
  target,
  tsv,
} from "../src/lib/ops-logic.ts";

test("every text of a shipped recipe has its translation, in French and in English", () => {
  const dir = new URL("../../library/recipes/", import.meta.url);
  const i18n = readFileSync(new URL("../src/lib/i18n.ts", import.meta.url), "utf8");
  for (const file of readdirSync(dir).filter((f) => f.endsWith(".toml"))) {
    for (const [, quoted] of readFileSync(new URL(file, dir), "utf8").matchAll(/^description = "(.*)"$/gm)) {
      const text = quoted.replace(/\\"/g, '"');
      const key = Object.hasOwn(shippedTexts, text) && shippedTexts[text];
      assert.ok(key, `${file}: "${text}" has no key in shippedTexts`);
      assert.equal(i18n.split(`\n  ${key}:`).length, 3, `${key} in both blocks`);
    }
  }
});

const recipe = (name, kinds, connection) => ({ name, connection, run: kinds.map((kind) => ({ kind })) });

test("a recipe runs on the connections of its kinds, or on the one it names", () => {
  const ssh = { name: "acme-prod", kind: "ssh", env: "prod" };
  const k8s = { name: "acme-k8s", kind: "kubernetes", env: "prod" };
  assert.ok(runsOn(recipe("logs-tail", ["ssh"]), ssh));
  assert.ok(!runsOn(recipe("logs-tail", ["ssh"]), k8s));
  assert.ok(!runsOn(recipe("mine", ["ssh"], "other"), ssh));
});

test("saving keeps only what the kind takes", () => {
  const out = clean({
    name: " acme-db ",
    kind: "postgres",
    env: "prod",
    host: "",
    port: 5432,
    via: "acme-prod",
    database: "acme",
    user: "readonly",
    context: "left over from kubernetes",
    secrets: { password: "ACME_DB", token: "not for postgres", api_key: "" },
  });
  assert.deepEqual(out, {
    name: "acme-db",
    kind: "postgres",
    env: "prod",
    ai_access: "none",
    port: 5432,
    via: "acme-prod",
    database: "acme",
    user: "readonly",
    secrets: { password: "ACME_DB" },
  });
  // Behind a tunnel, the port is the database's own on the far side: pgbouncer's 6432 stays.
  assert.equal(clean({ name: "db", kind: "postgres", env: "prod", via: "acme-prod", port: 6432, database: "a", user: "u" }).port, 6432);
  assert.deepEqual(missing({ name: "x", kind: "postgres", env: "dev" }), ["database", "user", "host"]);
  assert.deepEqual(missing({ name: "x", kind: "ssh", env: "dev", host: "acme" }), []);
});

test("the history reads an audit entry of a recipe", () => {
  const row = historyRow({
    id: 9,
    at: 1800000000,
    actor: { kind: "agent", session: "7" },
    target: "acme-prod",
    scope: "prod",
    size: 1200,
    masks: 3,
    detail: JSON.stringify({ recipe: "logs-tail", approval: "approved", command: "ssh …", params: { since: "1h" }, exit: 0 }),
  });
  assert.equal(row.recipe, "logs-tail");
  assert.equal(row.by, "agent");
  assert.equal(row.exit, 0);
  assert.deepEqual(row.params, { since: "1h" });
  assert.equal(historyRow({ id: 1, at: 0, actor: { kind: "human" }, detail: "not json" }).recipe, "");
  assert.equal(secondsLeft(100, 160), 0);
  assert.equal(secondsLeft(200.4, 100), 100);
  assert.equal(secondsLeft(null, 100), null);
});

test("a target reads as the approval card says it", () => {
  const ssh = { name: "acme-prod", kind: "ssh", env: "prod", host: "acme-prod", user: "deploy", jump: "bastion" };
  const db = { name: "acme-db", kind: "postgres", env: "prod", database: "acme", user: "readonly", via: "acme-prod" };
  assert.equal(target(ssh, [ssh]), "deploy@acme-prod (via bastion)");
  assert.equal(target(db, [ssh, db]), "readonly@127.0.0.1/acme (tunnel through deploy@acme-prod (via bastion))");
  assert.equal(target({ name: "dd", kind: "datadog", env: "prod" }, []), "api.datadoghq.com");
});

test("queries and streams have their own tabs", () => {
  const p = (type) => ({ name: "x", type, description: "", required: false });
  assert.ok(bounded({ name: "logs-tail", params: [p("text")] }));
  assert.ok(!bounded({ name: "sql-readonly", params: [p("sql")] }));
  assert.ok(!bounded({ name: "logs-follow", params: [], follow: true }));
});

test("a query's rows sort as numbers or text, NULL last", () => {
  const rows = [["10", "b"], ["NULL", "a"], ["9", "c"], ["100", "NULL"]];
  assert.deepEqual(sortRows(rows, 0, false).map((r) => r[0]), ["9", "10", "100", "NULL"]);
  assert.deepEqual(sortRows(rows, 0, true).map((r) => r[0]), ["100", "10", "9", "NULL"]);
  assert.deepEqual(sortRows(rows, 1, false).map((r) => r[1]), ["a", "b", "c", "NULL"]);
  assert.deepEqual(rows[0], ["10", "b"], "the rows given stay as they were");
  assert.equal(tsv({ columns: ["id", "note"], rows: [["1", "a\tb\nc"]] }), "id\tnote\n1\ta b c\n");
});

test("the console's history lists each past query once, newest first", () => {
  const run = (id, query, extra = {}) => ({
    id,
    at: 100 - id,
    recipe: "sql-readonly",
    connection: "acme-db",
    params: query ? { query } : {},
    event: null,
    ...extra,
  });
  const queries = pastQueries(
    [
      run(1, "SELECT 2 LIMIT 500"),
      run(2, "SELECT 1 LIMIT 500"),
      run(3, "SELECT 2 LIMIT 500"),
      run(4, null),
      run(5, "SELECT 3", { recipe: "logs-tail" }),
      run(6, "SELECT 4", { event: "reveal" }),
      // A project's own recipe names its query parameter otherwise.
      run(7, null, { recipe: "pg-stats", params: { sql: "SELECT 5 LIMIT 500", query: "not this" } }),
    ],
    [
      { name: "sql-readonly", params: [{ name: "query", type: "sql" }] },
      { name: "pg-stats", params: [{ name: "db", type: "name" }, { name: "sql", type: "sql" }] },
      { name: "logs-tail", params: [{ name: "query", type: "text" }] },
    ],
  );
  assert.deepEqual(
    queries.map((q) => q.query),
    ["SELECT 2 LIMIT 500", "SELECT 1 LIMIT 500", "SELECT 5 LIMIT 500"],
  );
});

test("a refused query reads as its code, any other error stays as it came", () => {
  const sent = JSON.stringify({ refused: { code: "notRead", subject: "DELETE" }, message: "DELETE is not a read: …" });
  assert.deepEqual(refusal(sent), { code: "notRead", subject: "DELETE", message: "DELETE is not a read: …" });
  assert.deepEqual(refusal(new String(sent)), refusal(sent));
  assert.equal(refusal("the vault is locked"), null);
  assert.equal(refusal('{"other": 1}'), null);
});

test("a stream's first lines count even before its start answers, and its end stops it", () => {
  const live = { id: 7, paused: false, lines: [], held: [], ended: null };
  const lines = (id, ...l) => ({ id, kind: "lines", lines: l, masks: 0 });
  // The front end set the id before starting: the event that comes first is kept.
  let change = streamed(live, lines(7, "a", "b"));
  assert.deepEqual(change, { lines: ["a", "b"] });
  Object.assign(live, change);
  assert.equal(streamed(live, lines(8, "other")), null, "another stream's events");
  Object.assign(live, { paused: true });
  Object.assign(live, streamed(live, lines(7, "c")));
  assert.deepEqual([live.lines, live.held], [["a", "b"], ["c"]], "held while paused");
  Object.assign(live, { paused: false });
  assert.deepEqual(streamed(live, lines(7, "d", "e", "f"), 3), { lines: ["d", "e", "f"] }, "the last ones kept");
  Object.assign(live, streamed(live, { id: 7, kind: "end", exit: 0, errors: "" }));
  assert.deepEqual([live.id, live.ended], [null, { exit: 0, errors: "" }]);
  assert.equal(streamed(live, lines(7, "late")), null, "nothing after its end");
});

test("a log line's level and the live filter", () => {
  assert.equal(level("2026-11-22 ERROR payment failed"), "error");
  assert.equal(level('{"level":"warn","msg":"slow"}'), "warn");
  assert.equal(level("[debug] cache miss"), "debug");
  assert.equal(level("GET /health 200"), null);
  assert.ok(shows("GET /health 200", "", "info"), "a line without a level counts as info");
  assert.ok(!shows("GET /health 200", "", "warn"));
  assert.ok(shows("WARN disk 91%", "disk", "warn"));
  assert.ok(!shows("WARN disk 91%", "payment", "all"));
  assert.ok(!shows("debug: x", "", "info"));
});

test("a history row says what was done with a .env's value", () => {
  const row = historyRow({
    id: 3,
    at: 1,
    actor: { kind: "human" },
    target: "acme-prod",
    detail: JSON.stringify({ recipe: "env-fetch", approval: "user", event: "write", key: "STRIPE_KEY", run: 42 }),
  });
  assert.deepEqual([row.event, row.key, row.run], ["write", "STRIPE_KEY", 42]);
  assert.equal(historyRow({ id: 1, at: 0, actor: { kind: "human" }, detail: "{}" }).event, null);
});

test("the DBA's answer gives its query, none while it streams", () => {
  const answer = "Voici :\n\n```sql\nSELECT id, total\nFROM orders\nORDER BY total DESC\n```\n\nLes 10 plus grosses **commandes**.";
  assert.equal(sqlBlock(answer), "SELECT id, total\nFROM orders\nORDER BY total DESC");
  assert.equal(sqlBlock("```sql\nSELECT 1"), null, "a block still streaming has no end yet");
  assert.equal(sqlBlock("```sql\nSELECT 1\n```\nthen\n```sql\nSELECT 2\n```"), "SELECT 2", "the last one");
  assert.equal(sqlBlock("no query"), null);
});

test("what the user asked is read back from the message the DBA got", () => {
  assert.deepEqual(asked("You are the DBA…\nSchema…\nRequest: les 10 dernières commandes"), {
    kind: "write",
    text: "les 10 dernières commandes",
  });
  assert.deepEqual(asked("Explain this query in plain words, step by step:\n```sql\nSELECT 1\n```"), {
    kind: "explain",
    text: "SELECT 1",
  });
  assert.deepEqual(asked("This query is slow. The database's plan for it:\n```\nSeq Scan\n```\n…\n```sql\nSELECT 2\n```"), {
    kind: "optimize",
    text: "SELECT 2",
  });
});

test("saved queries are searched everywhere and grouped by folder, the unfiled last", () => {
  const q = [
    { name: "Top clients", folder: "Ventes", sql: "SELECT * FROM customers", tags: ["daily"] },
    { name: "Errors", sql: "SELECT * FROM logs" },
    { name: "Abandoned carts", folder: "Ventes", sql: "SELECT * FROM carts", description: "Paniers" },
  ];
  assert.deepEqual(
    savedGroups(q, "").map((g) => [g.folder, g.queries.map((x) => x.name)]),
    [
      ["Ventes", ["Abandoned carts", "Top clients"]],
      ["", ["Errors"]],
    ],
  );
  assert.deepEqual(savedGroups(q, "daily")[0].queries.map((x) => x.name), ["Top clients"]);
  assert.deepEqual(savedGroups(q, "paniers ventes")[0].queries.map((x) => x.name), ["Abandoned carts"]);
  assert.equal(savedGroups(q, "logs")[0].queries[0].name, "Errors");
  assert.deepEqual(savedGroups(q, "nothing"), []);
  assert.equal(copyName("Top", ["Top", "Top (copie)"], "copie"), "Top (copie 2)");
});

test("the editor completes the schema's tables and columns, an export names its first table", () => {
  const schema = [
    { schema: "", name: "orders", columns: [{ name: "id", type: "bigint", nullable: false, key: "pk" }], indexes: [] },
    { schema: "billing", name: "invoices", columns: [{ name: "id", type: "bigint", nullable: false, key: "pk" }], indexes: [] },
  ];
  assert.deepEqual(
    completions(schema).map((c) => [c.label, c.type]),
    [
      ["orders", "class"],
      ["billing.invoices", "class"],
      ["id", "property"],
    ],
  );
  assert.equal(firstTable("select * from public.orders o join users u"), "orders");
  assert.equal(firstTable("SELECT 1"), "results");
  assert.equal(passwordName("acme-prod.db"), "ACME_PROD_DB_PASSWORD");
});

test("a SQLite database is a file: no host to ask for", () => {
  assert.deepEqual(missing({ name: "lite", kind: "sqlite", env: "dev" }), ["database"]);
  assert.deepEqual(missing({ name: "lite", kind: "sqlite", env: "dev", database: "/srv/a.db" }), []);
  assert.deepEqual(missing({ name: "ch", kind: "clickhouse", env: "dev", database: "events", user: "default" }), ["host"]);
  assert.equal(target({ name: "lite", kind: "sqlite", env: "dev", database: "/srv/a.db" }, []), "/srv/a.db");
  const kept = clean({ name: "x", kind: "mariadb", env: "dev", database: "d", user: "u", host: "h", sslmode: "require" });
  assert.equal(kept.sslmode, "require");
});

test("an engine TermF refuses is said in the interface's words", () => {
  assert.equal(engineRefused("redis"), "Redis");
  assert.equal(engineRefused("rediss"), "Redis");
  assert.equal(engineRefused("mongodb+srv"), "Mongodb");
  assert.equal(engineRefused("MSSQL"), "Sqlserver");
  assert.equal(engineRefused("cassandra"), "Other");
  for (const taken of ["postgres", "mysql", "mariadb", "sqlite", "clickhouse"]) assert.equal(engineRefused(taken), null);
});

test("a secret the save would replace is asked for, with its owner", () => {
  assert.deepEqual(
    secretClash("the secret SHOP_DB_PASSWORD already holds the password of shop-db: name another secret, or replace it"),
    { name: "SHOP_DB_PASSWORD", owner: "shop-db" },
  );
  assert.deepEqual(secretClash("the vault already has the secret OTHER_PASSWORD (prod): name another secret, or replace it"), {
    name: "OTHER_PASSWORD",
    owner: null,
  });
  assert.equal(secretClash("the vault is locked"), null);
});

test("a full export runs the query as it was launched, not the console's limited one", () => {
  const plan = { params: { query: "SELECT * FROM orders LIMIT 500" } };
  assert.equal(exportedQuery({ id: 1, plan, output: {}, query: "SELECT * FROM orders" }, "query"), "SELECT * FROM orders");
  assert.equal(exportedQuery({ id: 1, plan, output: {} }, "query"), "SELECT * FROM orders LIMIT 500");
});

test("a result's chart: a curve along a date, bars by a text, the ids left out", () => {
  const byDay = { columns: ["day", "orders", "total_eur"], rows: [["2026-11-02", "40", "120.5"], ["2026-11-01", "30", "NULL"]] };
  assert.deepEqual(autoChart(byDay), { type: "line", x: 0, series: [1, 2] });
  const points = chartPoints(byDay, { type: "line", x: 0, series: [1, 2] }, "date");
  assert.deepEqual(points.map((p) => [p.label, p.values]), [["2026-11-01", [30, null]], ["2026-11-02", [40, 120.5]]], "in date order");
  const top = { columns: ["id", "name", "total"], rows: [["7", "Northwind", "18420.00"], ["9", "Lumen", "-3"]] };
  assert.deepEqual(autoChart(top), { type: "bar", x: 1, series: [2] });
  assert.equal(columnKind({ columns: ["h"], rows: [["13:00"], ["NULL"], ["14:30:00"]] }, 0), "date", "a time of day");
  assert.equal(columnKind({ columns: ["at"], rows: [["2026-11-25 13:00:00+00"], ["2026-11-25T14:00:00.5+0100"]] }, 0), "date");
  assert.equal(columnKind({ columns: ["v"], rows: [["2026-13-45"]] }, 0), "text", "not a real date");
  assert.equal(timeOf("2026-11-25 13:00:00+01"), Date.parse("2026-11-25T12:00:00Z"));
  assert.equal(timeOf("2026-11-25"), Date.parse("2026-11-25T00:00:00Z"));
  assert.equal(autoChart({ columns: ["name", "email"], rows: [["a", "b"]] }), null, "no number");
  assert.equal(autoChart({ columns: ["count"], rows: [["4"]] }), null, "nothing for an axis");
  assert.equal(autoChart({ columns: ["name", "n"], rows: [] }), null);
  assert.deepEqual(autoChart({ columns: ["year", "sales"], rows: [["2025", "3"], ["2026", "5"]] }), { type: "line", x: 0, series: [1] });
  const scales = { columns: ["name", "orders", "total_eur"], rows: [["Northwind", "14", "18420.00"], ["Lumen", "9", "-12290"]] };
  assert.deepEqual(autoChart(scales).series, [2], "a series far smaller than the largest is left for the user to add");
});

test("a pie only when it reads, round ticks around the values", () => {
  const p = (...values) => values.map((v, i) => ({ label: String(i), at: i, values: [v] }));
  assert.ok(pieReadable(p(3, 1, 0), 1));
  assert.ok(!pieReadable(p(3, -1), 1), "below zero");
  assert.ok(!pieReadable(p(3, 1), 2), "two series");
  assert.ok(!pieReadable(p(...Array(9).fill(1)), 1), "too many slices");
  assert.ok(!pieReadable(p(0, 0), 1));
  assert.deepEqual(ticks(0, 18420), [0, 5000, 10000, 15000, 20000]);
  assert.deepEqual(ticks(-3, 7, 5), [-4, -2, 0, 2, 4, 6, 8]);
  assert.deepEqual(ticks(0, 0), [0, 0.2, 0.4, 0.6, 0.8, 1]);
  assert.deepEqual(ticks(0, 1.2, 4), [0, 0.5, 1, 1.5]);
});

test("the schema's diagram: keys between the tables read, referenced tables on the left, the rest last", () => {
  const col = (name, key = "", references) => ({ name, type: "bigint", nullable: false, key, ...(references ? { references } : {}) });
  const ref = (table, column = "id", schema = "") => ({ schema, table, column });
  const tables = [
    { schema: "", name: "customers", columns: [col("id", "pk")], indexes: [] },
    { schema: "", name: "orders", columns: [col("id", "pk"), col("customer_id", "fk", ref("customers"))], indexes: [] },
    { schema: "", name: "items", columns: [col("order_id", "fk", ref("orders", "")), col("parent", "fk", ref("items"))], indexes: [] },
    { schema: "billing", name: "notes", columns: [col("order_id", "fk", ref("orders")), col("x", "fk", ref("gone"))], indexes: [] },
    { schema: "", name: "settings", columns: [col("key")], indexes: [] },
  ];
  const links = relations(tables);
  assert.deepEqual(links, [
    { from: "orders", fromRow: 1, to: "customers", toRow: 0 },
    { from: "items", fromRow: 0, to: "orders", toRow: 0 },
    { from: "items", fromRow: 1, to: "items", toRow: 0 },
    { from: "billing.notes", fromRow: 0, to: "orders", toRow: 0 },
  ]);
  const at = layout(tables, links);
  const x = (n) => at.get(n).x;
  assert.ok(x("customers") < x("orders") && x("orders") < x("items"), "each table right of what it points to");
  assert.equal(x("items"), x("billing.notes"));
  assert.ok(x("settings") > x("items"), "a table without keys after the others");
  assert.equal(at.size, tables.length);
  // A cycle still places every table.
  const a = { schema: "", name: "a", columns: [col("b_id", "fk", ref("b"))], indexes: [] };
  const b = { schema: "", name: "b", columns: [col("a_id", "fk", ref("a"))], indexes: [] };
  assert.equal(layout([a, b], relations([a, b])).size, 2);
});
