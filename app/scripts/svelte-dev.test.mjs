import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath } from "node:url";
import { compile, compileModule } from "svelte/compiler";

const src = fileURLToPath(new URL("../src/", import.meta.url));
const files = readdirSync(src, { recursive: true }).filter((f) => /\.svelte(\.ts)?$/.test(f));

/** Each file compiled the way `vite dev` does, so what the dev console would say about it is visible here. */
const dev = (f) => {
  const code = readFileSync(src + f, "utf8");
  if (f.endsWith(".svelte")) return compile(code, { dev: true, filename: f }).js.code;
  // `strip` blanks the types out in place: the lines and columns stay those of the source.
  return compileModule(stripTypeScriptTypes(code, { mode: "strip" }), { dev: true, filename: f }).js.code;
};

test("no assignment's value is read back from reactive state (assignment_value_stale)", () => {
  assert.ok(files.length > 100, "the Svelte files were found");
  const found = files.flatMap((f) =>
    [...dev(f).matchAll(/\$\.assign(?:_async)?\(.*?:(\d+:\d+)'\)/g)].map((m) => `${f}:${m[1]}`),
  );
  assert.deepEqual(found, [], "use a statement (`{ a.b = x; }`) instead of the assignment's value, which is not the proxy kept in state");
});
