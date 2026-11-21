import { test } from "node:test";
import assert from "node:assert/strict";
import { neighbour, tileName } from "../src/lib/tile-logic.ts";

test("a tile title loses the tool it was opened with", () => {
  assert.equal(tileName("api · claude", "claude"), "api");
  assert.equal(tileName("web · dashboard · shell", "shell"), "web · dashboard");
  // A ticket run's title, or one given by the CLI, stays whole.
  assert.equal(tileName("acme/api#42 · Fix login", "claude"), "acme/api#42 · Fix login");
  assert.equal(tileName(" · claude", "claude"), " · claude");
  assert.equal(tileName("api · claude", "codex"), "api · claude");
});

test("closing a tile hands the keyboard to the next one, else the previous one", () => {
  assert.equal(neighbour([1, 2, 3], 2), 3);
  assert.equal(neighbour([1, 2, 3], 3), 2);
  assert.equal(neighbour([4], 4), null);
  assert.equal(neighbour([1, 2], 9), null);
});
