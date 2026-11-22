import { test } from "node:test";
import assert from "node:assert/strict";
import {
  bounded,
  clean,
  historyRow,
  level,
  missing,
  pastQueries,
  runsOn,
  secondsLeft,
  shows,
  sortRows,
  target,
  tsv,
} from "../src/lib/ops-logic.ts";

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
    ],
    ["sql-readonly"],
  );
  assert.deepEqual(
    queries.map((q) => q.query),
    ["SELECT 2 LIMIT 500", "SELECT 1 LIMIT 500"],
  );
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
