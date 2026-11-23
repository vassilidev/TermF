import { test } from "node:test";
import assert from "node:assert/strict";
import { blocks, spans } from "../src/lib/markdown-logic.ts";

test("bold, code and http(s) links; everything else stays text", () => {
  assert.deepEqual(spans("Le **plan** de `api` : [la PR](https://github.com/acme/api/pull/486)."), [
    { kind: "text", text: "Le " },
    { kind: "strong", spans: [{ kind: "text", text: "plan" }] },
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
  assert.deepEqual(spans("**voir `api` et https://acme.dev**"), [
    {
      kind: "strong",
      spans: [
        { kind: "text", text: "voir " },
        { kind: "code", text: "api" },
        { kind: "text", text: " et " },
        { kind: "link", text: "https://acme.dev", url: "https://acme.dev" },
      ],
    },
  ]);
  const rust = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
  assert.deepEqual(spans(`[Rust](${rust}) ou ${rust}.`), [
    { kind: "link", text: "Rust", url: rust },
    { kind: "text", text: " ou " },
    { kind: "link", text: rust, url: rust },
    { kind: "text", text: "." },
  ]);
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
  assert.deepEqual(out[1].items[0], [{ kind: "text", text: "Run 9 · " }, { kind: "strong", spans: [{ kind: "text", text: "ACME-38" }] }]);
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

test("rich: headings and tables, as the CTO and the expert write them", () => {
  const text =
    "## Étapes\n\n| Étape | Rôle | Coût |\n|:---|:---:|---:|\n| Plan | `po` | 0,10 $ |\n| Code | **dev** |\n| a \\| b | c | d | e |\nAprès.\n#123 n'est pas un titre";
  const out = blocks(text, true);
  assert.deepEqual(out.map((b) => b.kind), ["heading", "table", "paragraph"]);
  assert.deepEqual(out[0], { kind: "heading", level: 2, spans: [{ kind: "text", text: "Étapes" }] });
  const table = out[1];
  assert.deepEqual(table.align, ["left", "center", "right"]);
  assert.deepEqual(table.head, [[{ kind: "text", text: "Étape" }], [{ kind: "text", text: "Rôle" }], [{ kind: "text", text: "Coût" }]]);
  assert.equal(table.rows.length, 3);
  assert.deepEqual(table.rows[0][1], [{ kind: "code", text: "po" }]);
  assert.deepEqual(table.rows[1][2], [], "a missing cell is empty");
  assert.deepEqual(table.rows[2][0], [{ kind: "text", text: "a | b" }], "an escaped bar stays in its cell");
  assert.equal(table.rows[2].length, 3, "extra cells are dropped");
  assert.deepEqual(out[2].spans, [{ kind: "text", text: "Après.\n#123 n'est pas un titre" }]);
  assert.deepEqual(blocks("| a | b |\n|---|\n", true).map((b) => b.kind), ["paragraph"], "the delimiter must match the header");
  assert.deepEqual(blocks("## Titre\n| a |\n|---|").map((b) => b.kind), ["paragraph"], "plain without rich");
});
