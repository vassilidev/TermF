#!/usr/bin/env node
// Turns W3C Design Tokens (format 2025.10) into CSS custom properties, with no dependency.
// Usage: node design/tokens-to-css.mjs [design/tokens.json] [design/tokens.css]
// A sibling `<name>.dark.json` (same paths, dark values only) becomes the dark theme block.
import { existsSync, readFileSync, writeFileSync } from "node:fs";

const input = process.argv[2] ?? "design/tokens.json";
const output = process.argv[3] ?? input.replace(/\.(tokens\.)?json$/, ".css");
const dark = input.replace(/\.json$/, ".dark.json");
// The project's dark theme switch; the system preference when it has none.
const darkBlock = (body) => `@media (prefers-color-scheme: dark) {\n  :root {\n${body}\n  }\n}\n`;

const name = (path) =>
  "--" + path.join("-").replace(/([a-z0-9])([A-Z])/g, "$1-$2").toLowerCase().replace(/[^a-z0-9-]/g, "-");
const num = (n) => Number(n.toFixed(4)).toString();

function color(v) {
  const alpha = v.alpha ?? 1;
  if (v.hex && alpha === 1) return v.hex;
  const c = v.components.map((x) => (x === "none" ? "none" : num(x)));
  const a = alpha === 1 ? "" : ` / ${num(alpha)}`;
  switch (v.colorSpace) {
    case "oklch":
      return `oklch(${num(v.components[0] * 100)}% ${c[1]} ${c[2]}${a})`;
    case "oklab":
      return `oklab(${num(v.components[0] * 100)}% ${c[1]} ${c[2]}${a})`;
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

function css(type, v) {
  if (typeof v === "string" && /^\{[^}]+\}$/.test(v)) return `var(${name(v.slice(1, -1).split("."))})`;
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
    default:
      return typeof v === "object" ? null : String(v);
  }
}

// Walks the groups: `$type` is inherited from the closest group; a composite value (typography…) becomes one
// property per member.
function walk(node, path, inherited, out) {
  const type = node.$type ?? inherited;
  if ("$value" in node) {
    const value = css(type, node.$value);
    if (value !== null) out.push(`  ${name(path)}: ${value};`);
    else for (const [k, v] of Object.entries(node.$value)) {
      const sub = { fontSize: "dimension", lineHeight: "number", letterSpacing: "dimension", fontFamily: "fontFamily", width: "dimension", color: "color" }[k];
      out.push(`  ${name([...path, k])}: ${css(sub, v)};`);
    }
    return;
  }
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
console.log(`${output}: ${text.split("\n").filter((l) => l.startsWith("  --")).length} properties`);
