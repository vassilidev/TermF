import { test } from "node:test";
import assert from "node:assert/strict";
import { clean, historyRow, missing, runsOn, secondsLeft, target } from "../src/lib/ops-logic.ts";

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
    via: "acme-prod",
    database: "acme",
    user: "readonly",
    secrets: { password: "ACME_DB" },
  });
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
