import { test } from "node:test";
import assert from "node:assert/strict";
import {
  aliased,
  describe,
  families,
  fontState,
  kit,
  latest,
  literal,
  passes,
  primitive,
  ramps,
  request,
  roles,
  sectionOf,
  size,
  stackOf,
  stillThere,
  swatches,
  tally,
  tokensOf,
  tone,
  weights,
} from "../src/lib/design-logic.ts";

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

test("sections: typography gathers fonts, sizes, weights and line heights; spacing radii; depth shadows and motion", () => {
  assert.deepEqual(
    ["font", "text", "weight", "leading", "color", "spacing", "radius", "shadow", "motion", "other"].map(sectionOf),
    ["type", "type", "type", "type", "color", "spacing", "spacing", "depth", "depth", "other"],
  );
  const tokens = [token({}), token({ name: "--text-sm", family: "text" }), token({ name: "--font", family: "font" })];
  assert.deepEqual(tokensOf(tokens, "type").map((t) => t.name), ["--text-sm", "--font"]);
});

test("describe: one sentence per kind, the theme said when it matters", () => {
  assert.equal(describe(finding({}), t, number), 'designKindHardcoded{"subject":"#4f46e6","other":"--accent","first":"--accent","measure":""}');
  assert.match(describe(finding({ measure: 1.4 }), t, number), /^designKindHardcodedNear.*"measure":"1.4"/);
  assert.match(describe(finding({ other: [] }), t, number), /^designKindHardcodedLone/);
  assert.match(
    describe(finding({ kind: "offScale", family: "text", subject: "13px", other: ["14px", "16px"] }), t, number),
    /^designKindOffScale\{"subject":"13px \(designOfText\)"/,
  );
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

test("latest: a read of the repository left behind finishes without a say", () => {
  const reads = latest();
  const left = reads.start();
  const picked = reads.start();
  assert.ok(!reads.isLast(left));
  assert.ok(reads.isLast(picked));
});

const font = (over) => ({ family: "Satoshi", origin: "repo", faces: [], file: "src/app.css", line: 3, url: null, ...over });

test("fonts: a stack's families, resolved through the tokens and renamed to what the view loaded", () => {
  assert.deepEqual(families(`"Inter Variable", 'Satoshi', system-ui ,sans-serif`), ["Inter Variable", "Satoshi", "system-ui", "sans-serif"]);
  const tokens = [token({ name: "--font-sans", family: "font", value: '"Satoshi", sans-serif' }), token({ name: "--font-body", family: "font", value: "var(--font-sans)" })];
  assert.equal(stackOf("var(--font-body)", tokens), '"Satoshi", sans-serif');
  assert.equal(stackOf("var(--nope, Georgia, serif)", tokens), "Georgia, serif");
  assert.equal(aliased('"Satoshi", Helvetica, sans-serif', new Map([["satoshi", "termf-font-1"]])), '"termf-font-1", "Helvetica", sans-serif');
});

test("fonts: what the reader sees, never a font pretending to be the project's", () => {
  const none = () => false;
  const fonts = [font({ faces: [{ weight: "400", style: "normal", src: "data:", file: "a.woff2" }] }), font({ family: "Fraunces", origin: "service", url: "https://fonts.googleapis.com/css2?family=Fraunces" })];
  const loaded = (f) => f === "Satoshi";
  assert.deepEqual(fontState('"Satoshi", sans-serif', fonts, loaded, none), { family: "Satoshi", state: "loaded", shown: "Satoshi", font: fonts[0] });
  assert.equal(fontState("Fraunces, Georgia, serif", fonts, loaded, (f) => f === "Georgia").shown, "Georgia");
  assert.equal(fontState("Fraunces, serif", fonts, loaded, none).state, "service");
  assert.deepEqual(
    [fontState("Helvetica Neue, sans-serif", [], none, (f) => f === "Helvetica Neue").state, fontState("system-ui", [], none, none).state],
    ["installed", "system"],
  );
  assert.deepEqual(fontState('"Söhne", "Nope", monospace', [], none, none), { family: "Söhne", state: "missing", shown: "monospace", font: null });
});

test("roles: the stylesheets first, then the token names, the headings falling back on the body", () => {
  const tokens = [
    token({ name: "--font-sans", family: "font", value: "Inter, sans-serif" }),
    token({ name: "--font-display", family: "font", value: "Fraunces, serif" }),
    token({ name: "--font-mono", family: "font", value: "ui-monospace" }),
  ];
  assert.deepEqual(roles([], tokens), {
    body: { stack: "Inter, sans-serif", from: "--font-sans" },
    heading: { stack: "Fraunces, serif", from: "--font-display" },
    code: { stack: "ui-monospace", from: "--font-mono" },
  });
  const uses = [{ role: "heading", value: "var(--font-sans)", file: "a.css", line: 4 }];
  assert.deepEqual(roles(uses, tokens).heading, { stack: "Inter, sans-serif", from: "a.css:4" });
  assert.deepEqual(roles([], [tokens[0]]).heading, roles([], [tokens[0]]).body);
});

test("weights: the weight tokens, else what the files cover", () => {
  assert.deepEqual(weights(null, [token({ family: "weight", value: "700" }), token({ family: "weight", value: "400" })]), [400, 700]);
  const variable = font({ faces: [{ weight: "300 650", style: "normal", src: "", file: "" }] });
  assert.deepEqual(weights(variable, []), [300, 400, 500, 600]);
  assert.deepEqual(weights(font({ faces: [{ weight: "bold", style: "normal", src: "", file: "" }] }), []), [700]);
  assert.deepEqual(weights(null, []), [400, 700]);
});

test("ramps: a numbered scale together, the rest one by one", () => {
  const names = ["--bg", "--brand-100", "--brand-500", "--brand-900", "--space-1", "--accent"];
  assert.deepEqual(
    ramps(names.map((name) => token({ name }))).map((g) => [g.name, g.tokens.length]),
    [["--bg", 1], ["--brand", 3], ["--space-1", 1], ["--accent", 1]],
  );
});

test("components: the primitives drawn from the tokens, and the look of their variants", () => {
  assert.deepEqual(["IconButton", "TextField", "Card", "StatusBadge", "Toast", "DataGrid"].map(primitive), ["button", "field", "card", "badge", "alert", null]);
  assert.deepEqual(["primary", "outline", "destructive", "ghost", "fancy", "fancy"].map((v, i) => tone(v, i === 4)), ["accent", "outline", "danger", "ghost", "accent", "soft"]);
});

test("kit: the project's colors, radius and shadow by their names, in each theme", () => {
  const tokens = [
    token({ name: "--background", value: "#ffffff", color: "#ffffff", darkColor: "#0a0a0a" }),
    token({ name: "--foreground", value: "#0a0a0a", color: "#0a0a0a", darkColor: "#fafafa" }),
    token({ name: "--primary", value: "#2563eb", color: "#2563eb" }),
    token({ name: "--primary-foreground", value: "#fff", color: "#ffffff" }),
    token({ name: "--color-brand-100", value: "#eee", color: "#eeeeff" }),
    token({ name: "--border", value: "#e5e7eb", color: "#e5e7eb" }),
    token({ name: "--radius-sm", family: "radius", value: "4px", px: 4, color: null }),
    token({ name: "--radius-md", family: "radius", value: "8px", px: 8, color: null }),
    token({ name: "--radius-xl", family: "radius", value: "16px", px: 16, color: null }),
  ];
  const light = kit(tokens, false);
  assert.deepEqual([light.bg, light.text, light.accent, light.onAccent, light.border, light.radius, light.radiusLarge], ["#ffffff", "#0a0a0a", "#2563eb", "#ffffff", "#e5e7eb", "8px", "16px"]);
  assert.deepEqual([kit(tokens, true).bg, kit(tokens, true).text], ["#0a0a0a", "#fafafa"]);
  assert.ok(!light.own.includes("danger") && light.own.includes("accent"));
  const bare = kit([token({ name: "--color-brand-500", value: "#16a34a", color: "#16a34a" })], false);
  assert.deepEqual([bare.accent, bare.bg, bare.radius], ["#16a34a", "#ffffff", "6px"]);
});
