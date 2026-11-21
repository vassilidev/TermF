import { test } from "node:test";
import assert from "node:assert/strict";
import { neighbour, tileTitle } from "../src/lib/tile-logic.ts";

test("a tile title splits into the folder and the tool it was opened with", () => {
  assert.deepEqual(tileTitle("api · claude", "claude"), { name: "api", tool: "claude" });
  assert.deepEqual(tileTitle("web · dashboard · shell", "shell"), { name: "web · dashboard", tool: "shell" });
  // A ticket run's title, or one given by the CLI, stays whole.
  assert.deepEqual(tileTitle("acme/api#42 · Fix login", "claude"), { name: "acme/api#42 · Fix login", tool: "claude" });
  assert.deepEqual(tileTitle(" · claude", "claude"), { name: " · claude", tool: "claude" });
  assert.deepEqual(tileTitle("api · claude", "codex"), { name: "api · claude", tool: "codex" });
});

test("closing a tile hands the keyboard to the next one, else the previous one", () => {
  assert.equal(neighbour([1, 2, 3], 2), 3);
  assert.equal(neighbour([1, 2, 3], 3), 2);
  assert.equal(neighbour([4], 4), null);
  assert.equal(neighbour([1, 2], 9), null);
});
