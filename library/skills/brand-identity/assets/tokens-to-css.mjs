#!/usr/bin/env node
// Turns W3C Design Tokens (format 2025.10) into CSS custom properties, with no dependency.
// Usage: node design/tokens-to-css.mjs [design/tokens.json] [design/tokens.css]
// A sibling `<name>.dark<ext>` (same paths, dark values only) becomes the dark theme block.
import { existsSync, readFileSync, writeFileSync } from "node:fs";

const input = process.argv[2] ?? "design/tokens.json";
const ext = input.match(/(\.tokens)?(\.json)?$/)[0] || "";
const base = input.slice(0, input.length - ext.length);
const output = process.argv[3] ?? `${base}.css`;
const dark = `${base}.dark${ext}`;
if (output === input) {
  console.error(`${input}: the CSS would overwrite the tokens; give another output file`);
  process.exit(1);
}
// The project's dark theme switch; the system preference when it has none.
const darkBlock = (body) => `@media (prefers-color-scheme: dark) {\n  :root {\n${body}\n  }\n}\n`;

const skipped = [];
const name = (path) =>
  "--" + path.join("-").replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase().replace(/[^a-z0-9-]/g, "-");
const num = (n) => Number(n.toFixed(4)).toString();
const alias = (v) => typeof v === "string" && /^\{[^}]+\}$/.test(v);

function color(v) {
  const alpha = v.alpha ?? 1;
  if (v.hex && alpha === 1) return v.hex;
  const c = v.components.map((x) => (x === "none" ? "none" : num(x)));
  // OKLCH and OKLab lightness is 0 to 1 in the tokens, a percentage in CSS.
  const l = v.components[0] === "none" ? "none" : `${num(v.components[0] * 100)}%`;
  const a = alpha === 1 ? "" : ` / ${num(alpha)}`;
  switch (v.colorSpace) {
    case "oklch":
    case "oklab":
      return `${v.colorSpace}(${l} ${c[1]} ${c[2]}${a})`;
    case "hsl":
    case "hwb":
      return `${v.colorSpace}(${c[0]} ${c[1]}% ${c[2]}%${a})`;
    case "lab":
    case "lch":
      return `${v.colorSpace}(${c[0]} ${c[1]} ${c[2]}${a})`;
    default:
      return `color(${v.colorSpace} ${c.join(" ")}${a})`;
  }
}

// One CSS value, or null when the type has none (a composite, or a value this script does not know).
function css(type, v) {
  if (alias(v)) return `var(${name(v.slice(1, -1).split("."))})`;
  switch (type) {
    case "color":
      return color(v);
    case "dimension":
    case "duration":
      return `${v.value}${v.unit}`;
    case "fontFamily":
      return [v].flat().map((f) => (/^[\w-]+$/.test(f) ? f : JSON.stringify(f))).join(", ");
    case "cubicBezier":
      return `cubic-bezier(${v.join(", ")})`;
    case "shadow":
      return [v].flat().map((s) => [s.inset ? "inset" : "", css("dimension", s.offsetX), css("dimension", s.offsetY), css("dimension", s.blur), css("dimension", s.spread), css("color", s.color)].filter(Boolean).join(" ")).join(", ");
    case "border":
      return `${css("dimension", v.width)} ${css("strokeStyle", v.style)} ${css("color", v.color)}`;
    case "strokeStyle":
      return typeof v === "string" ? v : null;
    case "gradient":
      // The stops only: use it as `linear-gradient(<direction>, var(--name))`.
      return v.map((s) => `${css("color", s.color)} ${alias(s.position) ? css("number", s.position) : `${num(s.position * 100)}%`}`).join(", ");
    default:
      return typeof v === "object" ? null : String(v);
  }
}

// The members a composite token becomes, each its own property.
const MEMBERS = {
  typography: { fontFamily: "fontFamily", fontSize: "dimension", fontWeight: "fontWeight", letterSpacing: "dimension", lineHeight: "number" },
  transition: { duration: "duration", delay: "duration", timingFunction: "cubicBezier" },
};

function token(path, type, value, out) {
  const whole = css(type, value);
  if (whole !== null) return out.push(`  ${name(path)}: ${whole};`);
  const members = MEMBERS[type];
  if (!members || typeof value !== "object") return skipped.push(name(path));
  for (const [k, v] of Object.entries(value)) {
    const member = k in members ? css(members[k], v) : null;
    if (member === null) skipped.push(name([...path, k]));
    else out.push(`  ${name([...path, k])}: ${member};`);
  }
}

// Walks the groups: `$type` is inherited from the closest group; `$root` is the group's own token.
function walk(node, path, inherited, out) {
  const type = node.$type ?? inherited;
  if ("$value" in node) return token(path, type, node.$value, out);
  if (node.$root && typeof node.$root === "object") token(path, node.$root.$type ?? type, node.$root.$value, out);
  for (const [key, child] of Object.entries(node)) {
    if (!key.startsWith("$") && child && typeof child === "object") walk(child, [...path, key], type, out);
  }
}

const block = (file) => {
  const out = [];
  walk(JSON.parse(readFileSync(file, "utf8")), [], undefined, out);
  return out.join("\n");
};

let text = `/* Generated from ${input} by tokens-to-css.mjs: edit the tokens, then run it again. */\n:root {\n${block(input)}\n}\n`;
if (existsSync(dark)) text += darkBlock(block(dark).replace(/^/gm, "  "));
writeFileSync(output, text);
console.log(`${output}: ${text.split("\n").filter((l) => /^\s+--/.test(l)).length} properties`);
if (skipped.length) console.warn(`skipped, no CSS form: ${skipped.join(", ")}`);
