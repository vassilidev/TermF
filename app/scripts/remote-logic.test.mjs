import { test } from "node:test";
import assert from "node:assert/strict";
import { phase, remoteTiles, trayShown } from "../src/lib/remote-logic.ts";

const state = (over) => ({ name: "acme/api", on: false, url: null, pending: false, ask: false, locked: false, ...over });

test("phase: what the switch shows", () => {
  assert.equal(phase(state()), "off");
  assert.equal(phase(state({ on: true })), "connecting");
  assert.equal(phase(state({ on: true, pending: true })), "waiting");
  assert.equal(phase(state({ on: true, url: "https://claude.ai/code/session_1" })), "live");
  assert.equal(phase(state({ on: true, url: "https://claude.ai/code/session_1", pending: true })), "stopping");
  assert.equal(phase(state({ pending: true })), "stopping");
  assert.equal(phase(state({ on: true, url: "https://claude.ai/code/session_1", locked: true })), "locked");
});

const tile = (id, over = {}) => ({ id, title: `tile ${id}`, running: true, remoteControl: false, remote: state(), ...over });

test("remoteTiles: running tiles on, on their way, or not off yet", () => {
  const tiles = [
    tile(1, { remoteControl: true }),
    tile(2),
    tile(3, { remoteControl: true, running: false }),
    tile(4, { remote: state({ pending: true }) }),
    tile(5, { remoteControl: true, remote: null }),
  ];
  assert.deepEqual(
    remoteTiles(tiles).map((t) => t.id),
    [1, 4, 5],
  );
});

const words = {
  tooltip: (n) => `TermF: ${n} waiting`,
  waits: (n) => `Waiting for you (${n})`,
  nothing: "Nothing waits for you",
  remote: (n) => `Remote (${n})`,
  cutAll: "Turn all off",
  quick: "Quick terminal",
  open: "Open TermF",
  awake: "Stay awake",
  quit: "Quit TermF",
};

test("trayShown: the badge counts what waits, none at zero", () => {
  const waits = [
    { key: "tile:3", label: "back waits for your answer" },
    { key: "ask:7:plan", label: "Plan to approve · #12" },
  ];
  const shown = trayShown(waits, [tile(1, { remoteControl: true }), tile(2)], words);
  assert.equal(shown.count, 2);
  assert.equal(shown.tooltip, "TermF: 2 waiting");
  assert.equal(shown.waitsTitle, "Waiting for you (2)");
  assert.deepEqual(shown.remote, [{ key: "1", label: "acme/api" }]);
  assert.equal(shown.remoteTitle, "Remote (1)");
  const none = trayShown([], [], words);
  assert.equal(none.count, 0);
  assert.equal(none.tooltip, "TermF");
  assert.deepEqual(none.remote, []);
});
