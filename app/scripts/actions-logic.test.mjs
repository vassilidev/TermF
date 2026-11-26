import { test } from "node:test";
import assert from "node:assert/strict";
import { Approved, canon, forAgent, newMaster, noVault, refused, secretsOf, vaultBlock, vaultNeeds } from "../src/lib/actions-logic.ts";

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
  const list = { ...add, fields: { branches: { type: "array", description: "Branches." } }, required: [] };
  assert.equal(refused(list, { branches: ["feat/old"] }, null), null);
  assert.match(refused(list, { branches: "feat/old" }, null), /a list of texts/);
  assert.deepEqual(forAgent(list).arguments.properties.branches.items, { type: "string" });
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

test("a new vault's master password is typed twice in the card, never given by the agent", () => {
  const create = {
    id: "vault.create",
    description: "Creates the vault.",
    fields: {
      password: { type: "string", secret: true, description: "The master password." },
      confirm: { type: "string", secret: true, description: "The same, again." },
    },
  };
  const listed = forAgent(create);
  assert.deepEqual(listed.arguments.properties, {});
  assert.deepEqual(
    listed.askedToUser.map((f) => f.field),
    ["password", "confirm"],
  );
  assert.deepEqual(secretsOf(create, {}), ["password", "confirm"]);
  assert.match(refused(create, { password: "hunter22" }, null), /asked to the user by TermF/);
  assert.equal(refused(create, {}, { password: "the one you keep for acme" }), null);
  assert.deepEqual(newMaster({ password: "correct horse", confirm: "correct horse" }), { password: "correct horse" });
  assert.deepEqual(newMaster({ password: "short", confirm: "short" }), { why: "short" });
  assert.deepEqual(newMaster({ password: "correct horse", confirm: "correct house" }), { why: "differ" });
  assert.deepEqual(newMaster({}), { why: "short" });
});

test("an action that needs the vault names the action that settles it", () => {
  assert.match(vaultNeeds(null), /no vault yet: propose vault\.create first/);
  assert.match(vaultNeeds({ initialized: false, unlocked: false }), /propose vault\.create first/);
  assert.match(vaultNeeds({ initialized: true, unlocked: false }), /locked: propose vault\.unlock first/);
  assert.equal(vaultNeeds({ initialized: true, unlocked: true }), null);
  assert.match(noVault(null), /propose vault\.create first/);
  assert.equal(noVault({ initialized: true, unlocked: false }), null, "a locked vault opens in TermF's card when the action runs");
});

test("a run stopped at the vault asks for it in a card, any other error goes back to the agent", () => {
  assert.equal(vaultBlock("the vault is locked"), "unlock");
  assert.equal(vaultBlock(new Error("no secret ACME_REDIS_PASSWORD in the vault (prod): the vault is locked")), "unlock");
  assert.equal(vaultBlock("the prod scope needs the master password again"), "reauth");
  assert.equal(vaultBlock("no secret ACME_REDIS_PASSWORD in the vault (prod)"), null);
  assert.equal(vaultBlock("wrong master password"), null);
  assert.equal(vaultBlock(undefined), null);
});
