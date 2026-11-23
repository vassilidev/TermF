import { test } from "node:test";
import assert from "node:assert/strict";
import { Timers } from "../src/lib/ui/toast-logic.ts";

function setup(t) {
  t.mock.timers.enable({ apis: ["setTimeout", "Date"] });
  const gone = [];
  return { timers: new Timers((id) => gone.push(id)), gone, tick: (ms) => t.mock.timers.tick(ms) };
}

test("a toast goes once its time is up", (t) => {
  const { timers, gone, tick } = setup(t);
  timers.start(1, 5_000);
  tick(4_999);
  assert.deepEqual(gone, []);
  tick(1);
  assert.deepEqual(gone, [1]);
});

test("the pointer on a toast stops every timer, and leaving it starts them where they were", (t) => {
  const { timers, gone, tick } = setup(t);
  timers.start(1, 5_000);
  timers.start(2, 10_000);
  tick(3_000);
  timers.hold(1);
  tick(60_000);
  assert.deepEqual(gone, []);
  timers.release(1);
  tick(2_000);
  assert.deepEqual(gone, [1]);
  tick(5_000);
  assert.deepEqual(gone, [1, 2]);
});

test("a toast that arrives during the pause waits for it", (t) => {
  const { timers, gone, tick } = setup(t);
  timers.start(1, 5_000);
  timers.hold(1);
  timers.start(2, 5_000);
  tick(30_000);
  assert.deepEqual(gone, []);
  timers.release(1);
  tick(5_000);
  assert.deepEqual(gone.sort(), [1, 2]);
});

test("the hovered toast going away ends the pause", (t) => {
  const { timers, gone, tick } = setup(t);
  timers.start(1, 5_000);
  timers.start(2, 5_000);
  timers.hold(1);
  timers.remove(1);
  tick(5_000);
  assert.deepEqual(gone, [2]);
});

test("moving from one toast to the next keeps the pause, whatever the order of the events", (t) => {
  const { timers, gone, tick } = setup(t);
  timers.start(1, 5_000);
  timers.start(2, 5_000);
  timers.hold(1);
  timers.hold(2);
  timers.release(1);
  tick(30_000);
  assert.deepEqual(gone, []);
  timers.release(2);
  tick(5_000);
  assert.deepEqual(gone.sort(), [1, 2]);
});

test("a toast left with no time gets a second to be read", (t) => {
  const { timers, gone, tick } = setup(t);
  timers.start(1, 1_000);
  tick(999);
  timers.hold(1);
  timers.release(1);
  tick(999);
  assert.deepEqual(gone, []);
  tick(1);
  assert.deepEqual(gone, [1]);
});
