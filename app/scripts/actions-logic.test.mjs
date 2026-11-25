import { test } from "node:test";
import assert from "node:assert/strict";
import { Approved, canon, forAgent, refused, secretsOf } from "../src/lib/actions-logic.ts";

const add = {
  id: "ops.add_connection",
  description: "Adds a connection.",
  fields: {
    name: { type: "string", description: "Its name." },
    kind: { type: "string", description: "What it reaches.", enum: ["ssh", "mysql", "sqlite"] },
    port: { type: "integer", description: "Its port." },
    password: { type: "string", secret: true, description: "A database server's password." },
  },
  required: ["name", "kind", "password"],
  asks: (args) => (args.kind === "mysql" ? ["password"] : []),
};

test("an agent reads a secret field only by its name, never as an argument it gives", () => {
  const listed = forAgent(add);
  assert.deepEqual(Object.keys(listed.arguments.properties), ["name", "kind", "port"]);
  assert.deepEqual(listed.arguments.required, ["name", "kind"]);
  assert.equal(listed.arguments.additionalProperties, false);
  assert.deepEqual(listed.askedToUser, [{ field: "password", description: "A database server's password." }]);
  assert.ok(!JSON.stringify(listed.arguments).includes("password"));
  assert.equal(forAgent({ ...add, fields: { name: add.fields.name } }).askedToUser, undefined);
});

test("a call that gives a secret, an unknown or a wrong argument never reaches the user", () => {
  const ok = { name: "acme-prod", kind: "mysql", port: 3306 };
  assert.equal(refused(add, ok, { password: "DB_PASSWORD in the server's .env" }), null);
  assert.match(refused(add, { ...ok, password: "s3cret" }, null), /asked to the user by TermF/);
  assert.match(refused(add, { ...ok, force: true }, null), /no argument force/);
  assert.match(refused(add, { ...ok, kind: "redis" }, null), /one of ssh, mysql, sqlite/);
  assert.match(refused(add, { ...ok, port: "3306" }, null), /port: an integer/);
  assert.match(refused(add, { kind: "mysql" }, null), /name is required/);
  assert.match(refused(add, ok, { name: "x" }), /hints are for the fields asked to the user/);
  assert.match(refused(add, ok, { password: "x".repeat(201) }), /200 characters/);
  assert.match(refused(add, "acme", null), /args: an object/);
  const inputs = { ...add, fields: { inputs: { type: "object", description: "Inputs." } }, required: [] };
  assert.equal(refused(inputs, { inputs: { env: "prod" } }, null), null);
  assert.match(refused(inputs, { inputs: { env: 1 } }, null), /an object of texts/);
});

test("the card asks only the secrets these arguments need", () => {
  assert.deepEqual(secretsOf(add, { kind: "mysql" }), ["password"]);
  assert.deepEqual(secretsOf(add, { kind: "sqlite" }), []);
  assert.deepEqual(secretsOf({ ...add, asks: undefined }, {}), ["password"]);
});

test("what the permission card approved runs once, with what the user typed, whatever the keys' order", () => {
  assert.equal(canon({ b: 1, a: [{ d: 2, c: null }] }), canon({ a: [{ c: null, d: 2 }], b: 1 }));
  const approved = new Approved(1000);
  approved.keep(4, "ops.add_connection", { name: "acme-prod", kind: "mysql" }, { password: "s3cret" }, 0);
  assert.equal(approved.take(5, "ops.add_connection", { kind: "mysql", name: "acme-prod" }, 10), undefined, "another conversation");
  assert.equal(approved.take(4, "ops.add_connection", { kind: "mysql", name: "acme-other" }, 10), undefined, "other arguments");
  assert.deepEqual(approved.take(4, "ops.add_connection", { kind: "mysql", name: "acme-prod" }, 10), { password: "s3cret" });
  assert.equal(approved.take(4, "ops.add_connection", { kind: "mysql", name: "acme-prod" }, 20), undefined, "taken once");
  approved.keep(4, "ci.rerun", { run: 1 }, {}, 0);
  assert.equal(approved.take(4, "ci.rerun", { run: 1 }, 2000), undefined, "too late");
});
