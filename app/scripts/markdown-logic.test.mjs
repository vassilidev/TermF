import { test } from "node:test";
import assert from "node:assert/strict";
import { blocks, spans } from "../src/lib/markdown-logic.ts";

test("bold, code and http(s) links; everything else stays text", () => {
  assert.deepEqual(spans("Le **plan** de `api` : [la PR](https://github.com/acme/api/pull/486)."), [
    { kind: "text", text: "Le " },
    { kind: "strong", text: "plan" },
    { kind: "text", text: " de " },
    { kind: "code", text: "api" },
    { kind: "text", text: " : " },
    { kind: "link", text: "la PR", url: "https://github.com/acme/api/pull/486" },
    { kind: "text", text: "." },
  ]);
  assert.deepEqual(spans("Voir https://acme.dev/a?b=1, puis (https://acme.dev/c)."), [
    { kind: "text", text: "Voir " },
    { kind: "link", text: "https://acme.dev/a?b=1", url: "https://acme.dev/a?b=1" },
    { kind: "text", text: ", puis (" },
    { kind: "link", text: "https://acme.dev/c", url: "https://acme.dev/c" },
    { kind: "text", text: ")." },
  ]);
  for (const raw of [
    "<b>gras</b> & <script>alert(1)</script>",
    "[clic](javascript:alert(1)) [fichier](file:///etc/passwd)",
    "*italique* __init__ ** espacé ** # titre",
    "`https://dans-le-code.dev`",
  ]) {
    assert.ok(
      spans(raw).every((s) => s.kind !== "link"),
      `no link in ${raw}`,
    );
    assert.equal(spans(raw).map((s) => s.text).join(""), raw.replace(/`/g, ""), "the text is kept as written");
  }
  assert.deepEqual(spans("**a** et **b**").map((s) => s.kind), ["strong", "text", "strong"]);
  assert.deepEqual(spans(""), []);
});

test("paragraphs keep their line breaks, lists and fenced code are blocks", () => {
  const text = [
    "Deux runs sont ouverts :",
    "",
    "- Run 9 · **ACME-38**",
    "- Run 12 · ACME-42",
    "  suite de la ligne",
    "",
    "1. un",
    "2. deux",
    "Une ligne",
    "et la suivante.",
    "```ts",
    "const a = 1;",
    "",
    "```",
  ].join("\n");
  const out = blocks(text);
  assert.deepEqual(out.map((b) => b.kind), ["paragraph", "list", "list", "paragraph", "code"]);
  assert.equal(out[1].start, null);
  assert.deepEqual(out[1].items[0], [{ kind: "text", text: "Run 9 · " }, { kind: "strong", text: "ACME-38" }]);
  assert.deepEqual(out[1].items[1], [{ kind: "text", text: "Run 12 · ACME-42\nsuite de la ligne" }]);
  assert.equal(out[2].start, 1);
  assert.equal(out[2].items.length, 2);
  assert.deepEqual(out[3].spans, [{ kind: "text", text: "Une ligne\net la suivante." }]);
  assert.equal(out[4].text, "const a = 1;\n");
});

test("a list after another kind starts a new one; an unclosed fence runs to the end", () => {
  assert.deepEqual(blocks("- a\n1. b\n* c").map((b) => b.start), [null, 1, null]);
  assert.deepEqual(blocks("3) trois").map((b) => b.start), [3]);
  assert.deepEqual(blocks("avant\n```\nx\ny"), [
    { kind: "paragraph", spans: [{ kind: "text", text: "avant" }] },
    { kind: "code", text: "x\ny" },
  ]);
  assert.deepEqual(blocks("-pas une liste\n---"), [{ kind: "paragraph", spans: [{ kind: "text", text: "-pas une liste\n---" }] }]);
  assert.deepEqual(blocks("\r\n\n"), []);
});
