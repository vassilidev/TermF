// The tokens script shipped with the brand-identity skill, run the way an agent runs it in a project.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("../../library/skills/brand-identity/assets/tokens-to-css.mjs", import.meta.url));
const project = () => mkdtempSync(join(tmpdir(), "termf-tokens-"));
const run = (dir, ...args) => spawnSync(process.execPath, [script, ...args], { cwd: dir, encoding: "utf8" });

test("a .tokens file gets its own .css next to it and is never overwritten", () => {
  const dir = project();
  const tokens = JSON.stringify({ space: { $type: "dimension", 1: { $value: { value: 4, unit: "px" } } } });
  writeFileSync(join(dir, "brand.tokens"), tokens);
  assert.equal(run(dir, "brand.tokens").status, 0);
  assert.equal(readFileSync(join(dir, "brand.tokens"), "utf8"), tokens);
  assert.match(readFileSync(join(dir, "brand.css"), "utf8"), /--space-1: 4px;/);

  const same = run(dir, "brand.tokens", "brand.tokens");
  assert.notEqual(same.status, 0, "an output that is the input is refused");
  assert.equal(readFileSync(join(dir, "brand.tokens"), "utf8"), tokens);
});

test("composites become properties, a group's $root is the group, and nothing prints null or NaN", () => {
  const dir = project();
  const black = { colorSpace: "srgb", components: [0, 0, 0], hex: "#000000" };
  const white = { colorSpace: "srgb", components: [1, 1, 1], hex: "#ffffff" };
  writeFileSync(
    join(dir, "tokens.json"),
    JSON.stringify({
      space: { $type: "dimension", $root: { $value: { value: 4, unit: "px" } }, 2: { $value: { value: 8, unit: "px" } } },
      motion: {
        fade: {
          $type: "transition",
          $value: { duration: { value: 200, unit: "ms" }, delay: { value: 0, unit: "ms" }, timingFunction: [0.2, 0, 0, 1], wobble: 3 },
        },
      },
      hero: { $type: "gradient", $value: [{ color: black, position: 0 }, { color: white, position: 1 }] },
      grey: { $type: "color", $value: { colorSpace: "oklch", components: ["none", 0, "none"] } },
    }),
  );
  writeFileSync(join(dir, "tokens.dark.json"), JSON.stringify({ hero: { $type: "color", $value: white } }));
  const said = run(dir, "tokens.json");
  assert.equal(said.status, 0, said.stderr);
  const css = readFileSync(join(dir, "tokens.css"), "utf8");
  for (const line of [
    "--space: 4px;",
    "--space-2: 8px;",
    "--motion-fade-duration: 200ms;",
    "--motion-fade-delay: 0ms;",
    "--motion-fade-timing-function: cubic-bezier(0.2, 0, 0, 1);",
    "--hero: #000000 0%, #ffffff 100%;",
    "--grey: oklch(none 0 none);",
  ]) {
    assert.ok(css.includes(line), `${line} in:\n${css}`);
  }
  assert.doesNotMatch(css, /null|NaN|undefined/);
  assert.doesNotMatch(css, /wobble/, "an unknown member is left out");
  assert.match(css, /@media \(prefers-color-scheme: dark\) \{\n {2}:root \{\n {4}--hero: #ffffff;/);
  assert.match(said.stdout, /tokens\.css: 8 properties/, "the dark theme's lines are counted");
  assert.match(said.stderr, /--motion-fade-wobble/, "and said");
});
