import { test } from "node:test";
import assert from "node:assert/strict";
import { describe, literal, passes, request, sectionOf, size, stillThere, swatches, tally, tokensOf } from "../src/lib/design-logic.ts";

const t = (key, vars = {}) => `${key}${Object.keys(vars).length ? JSON.stringify(vars) : ""}`;
const number = (n) => String(n);

const finding = (over) => ({
  id: "x",
  kind: "hardcoded",
  level: "warn",
  family: "color",
  subject: "#4f46e6",
  other: ["--accent"],
  measure: null,
  theme: null,
  at: [],
  count: 0,
  ...over,
});

const token = (over) => ({
  name: "--accent",
  path: null,
  family: "color",
  format: "css",
  value: "#4f46e5",
  file: "src/app.css",
  line: 3,
  light: true,
  dark: null,
  color: "#4f46e5",
  darkColor: "#818cf8",
  px: null,
  alias: false,
  uses: 2,
  generated: false,
  ...over,
});

test("sections: typography gathers fonts, sizes, weights and line heights", () => {
  assert.deepEqual(["font", "text", "weight", "leading", "color", "spacing"].map(sectionOf), ["type", "type", "type", "type", "color", "spacing"]);
  const tokens = [token({}), token({ name: "--text-sm", family: "text" }), token({ name: "--font", family: "font" })];
  assert.deepEqual(tokensOf(tokens, "type").map((t) => t.name), ["--text-sm", "--font"]);
});

test("describe: one sentence per kind, the theme said when it matters", () => {
  assert.equal(describe(finding({}), t, number), 'designKindHardcoded{"subject":"#4f46e6","other":"--accent","first":"--accent","measure":""}');
  assert.match(describe(finding({ measure: 1.4 }), t, number), /^designKindHardcodedNear.*"measure":"1.4"/);
  assert.match(describe(finding({ other: [] }), t, number), /^designKindHardcodedLone/);
  assert.match(describe(finding({ kind: "offScale", subject: "13px", other: ["4px", "8px"] }), t, number), /^designKindOffScale\{/);
  assert.match(
    describe(finding({ kind: "offScale", subject: "--space-6", other: ["18px", "4px"] }), t, number),
    /^designKindOffScaleToken.*"value":"18px","base":"4px"/,
  );
  assert.match(describe(finding({ kind: "dark", other: ["dark-only"] }), t, number), /^designKindDarkOnly/);
  assert.match(describe(finding({ kind: "component", level: "info", other: ["Panel"] }), t, number), /^designKindComponentShape/);
  assert.match(describe(finding({ kind: "unused", other: ["--a", "--b"] }), t, number), /^designKindUnused.*"count":2/);
  assert.match(describe(finding({ kind: "contrast", measure: 2.56, theme: "dark" }), t, number), /^designKindContrast.*\(designThemeDark\)$/);
  assert.match(describe(finding({ kind: "undefined", other: [] }), t, number), /^designKindUndefined/);
});

test("request: the chosen findings with their first places, for the CTO", () => {
  const at = [1, 2, 3, 4, 5].map((line) => ({ file: "src/a.css", line, text: "" }));
  const text = request([finding({ at, count: 7 }), finding({ kind: "undefined", subject: "--x", other: [] })], "acme-web", t, number);
  const lines = text.split("\n");
  assert.equal(lines[0], 'designFixRequest{"repo":"acme-web"}');
  assert.match(lines[1], /^- designKindHardcoded.* \(src\/a\.css:1, src\/a\.css:2, src\/a\.css:3, designMore\{"n":4\}\)$/);
  assert.match(lines[2], /^- designKindUndefined[^(]*$/);
});

test("swatches: token colors in the theme of the finding, literals as they are", () => {
  const tokens = [token({})];
  assert.deepEqual(swatches(finding({}), tokens), ["#4f46e6", "#4f46e5"]);
  assert.deepEqual(swatches(finding({ kind: "contrast", subject: "--accent", other: ["#ffffff"], theme: "dark" }), tokens), ["#818cf8", "#ffffff"]);
  assert.deepEqual(swatches(finding({ kind: "duplicate", subject: "#4f46e5", other: ["--a", "--b"] }), tokens), ["#4f46e5"]);
  assert.deepEqual(swatches(finding({ family: "spacing" }), tokens), []);
});

test("tally, literal, passes, stillThere, size", () => {
  assert.deepEqual(tally([finding({}), finding({ level: "error" }), finding({})]), { error: 1, warn: 2, info: 0 });
  assert.ok(literal("13px") && literal("#fff") && !literal("--space-6") && !literal("$primary"));
  assert.ok(passes(4.5) && !passes(4.49));
  const kept = stillThere(new Set(["a", "gone"]), [finding({ id: "a" })]);
  assert.deepEqual([...kept], ["a"]);
  assert.equal(size(512, "en"), "512 byte");
  assert.equal(size(2048, "en"), "2 kB");
});
