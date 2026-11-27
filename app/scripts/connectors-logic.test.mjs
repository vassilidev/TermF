import { test } from "node:test";
import assert from "node:assert/strict";
import { rows, strictNote } from "../src/lib/connectors-logic.ts";

const listing = {
  connectors: [
    { name: "docs", account: false, target: "https://docs.acme.test/mcp", access: { roles: ["po"], write: false }, keys: true },
    { name: "legacy", account: false, target: "npx legacy-mcp", access: null, keys: false },
    { name: "claude.ai Gmail", account: true, target: "", access: { roles: ["cto"], write: false }, keys: false },
  ],
  roles: [{ name: "po", prod: false }, { name: "cto", prod: true }],
};
const account = {
  connectors: [
    { name: "claude.ai Gmail", health: "connected" },
    { name: "claude.ai Google Calendar", health: "needsauth" },
  ],
  error: null,
};

test("lists the project's connectors, then the account's it does not give yet", () => {
  const all = rows(listing, account);
  assert.deepEqual(
    all.map((r) => [r.label, r.account, r.roles, r.write, r.health, r.configured]),
    [
      ["docs", false, ["po"], false, null, true],
      ["legacy", false, null, null, null, true],
      ["Gmail", true, ["cto"], false, "connected", true],
      ["Google Calendar", true, null, null, "needsauth", false],
    ],
  );
  assert.equal(rows(listing, null).length, 3, "before `claude mcp list` answers, the project's own");
});

test("says why the CTO and the expert do not get a connector", () => {
  const [docs, legacy, gmail] = rows(listing, account);
  assert.equal(strictNote(gmail), "connStrictAccount");
  assert.equal(strictNote(docs), null, "not ticked for them");
  assert.equal(strictNote({ ...docs, roles: ["expert"] }), "connStrictKeys");
  assert.equal(strictNote(legacy), null);
});
