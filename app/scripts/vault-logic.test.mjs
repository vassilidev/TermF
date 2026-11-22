import { test } from "node:test";
import assert from "node:assert/strict";
import { entries, host, openable, secondsLeft, sections, spaced, strength, urlsOf, visible } from "../src/lib/vault-logic.ts";

const item = (id, title, project, extra = {}) => ({
  id,
  kind: "login",
  project,
  title,
  urls: [],
  username: "",
  hasPassword: true,
  totp: null,
  hasNotes: false,
  agents: false,
  created: 0,
  updated: 0,
  ...extra,
});
const secret = (name, scope, project) => ({ name, scope, project, description: "", created: 0, updated: 0, reference: null });

const list = entries(
  [
    item("a", "Stripe", "acme", { username: "alex@acme.test", urls: ["https://dashboard.stripe.com"] }),
    item("b", "GitHub", null, { username: "alex" }),
    item("c", "AWS", "other"),
    item("d", "Wi-Fi du bureau", "acme", { kind: "note", hasPassword: false }),
  ],
  [secret("DATABASE_URL", "prod", "acme")],
);
const names = (l) => l.map((e) => (e.type === "item" ? e.item.title : e.secret.name));

test("visible: the project's entries and the global ones, by title, secrets last; every one without a project", () => {
  assert.deepEqual(names(visible(list, "acme", "all", "")), ["GitHub", "Stripe", "Wi-Fi du bureau", "DATABASE_URL"]);
  assert.deepEqual(names(visible(list, null, "all", "")), ["AWS", "GitHub", "Stripe", "Wi-Fi du bureau", "DATABASE_URL"]);
});

test("visible: filters by kind, and searches title, user name, address and project name, accents aside", () => {
  assert.deepEqual(names(visible(list, null, "note", "")), ["Wi-Fi du bureau"]);
  assert.deepEqual(names(visible(list, null, "secret", "")), ["DATABASE_URL"]);
  assert.deepEqual(names(visible(list, null, "login", "")), ["AWS", "GitHub", "Stripe"]);
  assert.deepEqual(names(visible(list, null, "all", "DASHBOARD")), ["Stripe"]);
  assert.deepEqual(names(visible(list, null, "all", "alex git")), ["GitHub"]);
  assert.deepEqual(names(visible(list, null, "all", "wi-fî")), ["Wi-Fi du bureau"]);
  const name = (id) => ({ acme: "Acme Corp", other: "Other" })[id];
  assert.deepEqual(names(visible(list, null, "login", "corp", name)), ["Stripe"]);
});

test("sections: the current project first, the others by name, the global entries last", () => {
  const cut = sections(visible(list, null, "all", ""), "other");
  assert.deepEqual(
    cut.map((s) => [s.project, names(s.entries)]),
    [
      ["other", ["AWS"]],
      ["acme", ["Stripe", "Wi-Fi du bureau", "DATABASE_URL"]],
      [null, ["GitHub"]],
    ],
  );
});

test("spaced and secondsLeft: a code in two halves, the seconds until the next one", () => {
  assert.equal(spaced("123456"), "123 456");
  assert.equal(spaced("12345678"), "1234 5678");
  assert.equal(spaced("1234567"), "1234 567");
  assert.equal(secondsLeft(30, 59_000), 1);
  assert.equal(secondsLeft(30, 60_000), 30);
  assert.equal(secondsLeft(60, 61_500), 59);
});

test("host, openable and urlsOf: addresses as typed", () => {
  assert.equal(host("https://dashboard.stripe.com/login"), "dashboard.stripe.com");
  assert.equal(host("github.com/acme"), "github.com");
  assert.equal(host("not a url"), "not a url");
  assert.equal(openable("github.com"), "https://github.com/");
  assert.equal(openable("http://localhost:3000/x"), "http://localhost:3000/x");
  assert.equal(openable("javascript:alert(1)"), null);
  assert.equal(openable("file:///etc/passwd"), null);
  assert.deepEqual(urlsOf(" https://a.test\n\nb.test, c.test "), ["https://a.test", "b.test", "c.test"]);
});

test("strength: bits of the recipe, not of the password drawn", () => {
  const all = { kind: "password", length: 20, lower: true, upper: true, digits: true, symbols: true, unambiguous: false };
  assert.equal(strength(all), 126);
  assert.equal(strength({ ...all, unambiguous: true }), 123);
  assert.equal(strength({ ...all, lower: false, upper: false, digits: false, symbols: false }), 0);
  assert.equal(strength({ kind: "passphrase", words: 5, separator: "-" }), 51);
});
