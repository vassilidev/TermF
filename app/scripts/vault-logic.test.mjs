import { test } from "node:test";
import assert from "node:assert/strict";
import {
  counts,
  entries,
  folderProject,
  host,
  leftOut,
  openable,
  secondsLeft,
  secretNameOk,
  sections,
  spaced,
  strength,
  urlsOf,
  visible,
} from "../src/lib/vault-logic.ts";

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
  // Enter opens the first entry shown, the current project's, not the first by title of the whole list.
  assert.equal(names([cut[0].entries[0]])[0], "AWS");
  assert.equal(names(visible(list, null, "all", ""))[0], "AWS");
  const acmeFirst = sections(visible(list, null, "login", ""), "acme");
  assert.equal(names([acmeFirst[0].entries[0]])[0], "Stripe");
  assert.equal(names(visible(list, null, "login", ""))[0], "AWS");
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
  assert.equal(openable("localhost:3000"), "https://localhost:3000/");
  assert.deepEqual(urlsOf(" https://a.test\n\nb.test c.test "), ["https://a.test", "b.test", "c.test"]);
  assert.deepEqual(urlsOf("https://maps.test/@48.85,2.35,12z"), ["https://maps.test/@48.85,2.35,12z"]);
});

test("strength: bits of the recipe, not of the password drawn", () => {
  const all = { kind: "password", length: 20, lower: true, upper: true, digits: true, symbols: true, unambiguous: false };
  assert.equal(strength(all), 126);
  assert.equal(strength({ ...all, unambiguous: true }), 123);
  assert.equal(strength({ ...all, lower: false, upper: false, digits: false, symbols: false }), 0);
  assert.equal(strength({ kind: "passphrase", words: 5, separator: "-" }), 51);
});

test("counts: what a merge changed, unchanged left out", () => {
  const row = (change) => ({ key: change, kind: "login", scope: null, title: change, project: null, change });
  assert.deepEqual(counts(["added", "added", "unchanged", "conflict"].map(row)), [["added", 2], ["conflict", 1]]);
  assert.deepEqual(counts([row("unchanged")]), []);
});

test("leftOut: duplicates and the other manager's trash, by default", () => {
  const row = (index, duplicate, problem) => ({
    index,
    kind: "login",
    title: "",
    username: "",
    urls: [],
    folder: "",
    hasPassword: true,
    hasTotp: false,
    hasNotes: false,
    duplicate,
    problem,
  });
  const rows = [row(0, null, null), row(1, "GitHub", null), row(2, "", null), row(3, null, "trash"), row(4, null, "totp")];
  assert.deepEqual(leftOut(rows), [1, 2, 3]);
});

test("folderProject: a folder named as a project goes there", () => {
  const projects = [{ id: "acme-platform", name: "Acme Platform" }, { id: "docs", name: "Docs" }];
  assert.equal(folderProject("Acme Platform", projects), "acme-platform");
  assert.equal(folderProject("Work/docs", projects), "docs");
  assert.equal(folderProject("Personnel", projects), null);
  assert.equal(folderProject("", projects), null);
  assert.equal(folderProject("Acme", projects), "acme-platform", "the only one it starts");
  assert.equal(folderProject("Ac", projects), null, "too short to guess");
  assert.equal(folderProject("Acme", [...projects, { id: "acme-mobile", name: "Acme Mobile" }]), null, "two: no guess");
});

test("secretNameOk: the names termf-vault takes, and only those", () => {
  for (const ok of ["STRIPE_SECRET_KEY", "_private", "db.url-2", "a", "A".repeat(128)]) assert.ok(secretNameOk(ok), ok);
  for (const no of ["", "2FA", "-x", ".env", "MY KEY", "clé", "A".repeat(129), "a/b"]) assert.ok(!secretNameOk(no), no);
});
