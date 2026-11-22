import { test } from "node:test";
import assert from "node:assert/strict";
import {
  interval,
  intervalCron,
  liveLoops,
  loopEvery,
  neighbour,
  nextRun,
  period,
  pill,
  taskOf,
  tileName,
  toCron,
} from "../src/lib/tile-logic.ts";

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

const T0 = 1_800_000_000_000;
const loop = (over = {}) => ({
  id: "ab12cd34",
  kind: "cron",
  cron: "*/5 * * * *",
  every: null,
  recurring: true,
  prompt: "check the deploy",
  fromLoop: true,
  created: T0,
  nextRun: T0 + 285_000,
  ends: T0 + 7 * 86_400_000,
  ...over,
});

test("a loop's pill says how often it runs", () => {
  assert.equal(period("*/5 * * * *"), 300);
  assert.equal(period("* * * * *"), 60);
  assert.equal(period("7 * * * *"), 3600);
  assert.equal(period("0 */2 * * *"), 7200);
  assert.equal(period("30 9 * * *"), 86400);
  assert.equal(period("0 9 * * 1-5"), null);
  assert.equal(period("0,30 * * * *"), null);
  // Uneven: :00, :07 … :56, then :00 again; 0h, 5h … 20h, then 0h.
  assert.equal(period("*/7 * * * *"), null);
  assert.equal(period("*/15 * * * *"), 900);
  assert.equal(period("0 */5 * * *"), null);
  assert.equal(period("0 */6 * * *"), 21600);
  assert.equal(loopEvery(loop({ cron: "*/7 * * * *", every: 420 })), 420);
  assert.deepEqual(interval(300), { n: 5, unit: "min" });
  assert.deepEqual(interval(5400), { n: 90, unit: "min" });
  assert.deepEqual(interval(7200), { n: 2, unit: "h" });
  assert.deepEqual(interval(172_800), { n: 2, unit: "d" });
  assert.equal(loopEvery(loop()), 300);
  assert.equal(loopEvery(loop({ kind: "wakeup", cron: null, every: 1200 })), 1200);
  assert.equal(loopEvery(loop({ cron: "0 9 * * 1-5" })), null);

  const watch = loop({ id: "bg1", kind: "monitor", cron: null });
  assert.equal(pill([]), null);
  assert.deepEqual(pill([watch, loop()]), { lead: loop(), more: 1 });
  assert.deepEqual(pill([watch]), { lead: watch, more: 0 });
});

test("a loop ends after 7 days, and its next run rolls forward", () => {
  assert.equal(liveLoops([loop()], T0 + 7 * 86_400_000).length, 0);
  assert.equal(liveLoops([loop({ ends: null })], T0 + 9e9).length, 1);
  assert.equal(liveLoops(undefined, T0).length, 0);
  assert.equal(nextRun(loop(), T0), T0 + 285_000);
  // Every 5 minutes, at the offset Claude Code gave it.
  assert.equal(nextRun(loop(), T0 + 285_000), T0 + 585_000);
  assert.equal(nextRun(loop(), T0 + 1_000_000), T0 + 1_185_000);
  assert.equal(nextRun(loop({ cron: "0 9 * * 1-5" }), T0 + 300_000), null);
  assert.equal(nextRun(loop({ nextRun: null }), T0), null);
});

test("a loop becomes a Scheduler task with a cron TermF can run", () => {
  assert.equal(intervalCron(30), "* * * * *");
  assert.equal(intervalCron(300), "*/5 * * * *");
  assert.equal(intervalCron(420), "*/6 * * * *");
  assert.equal(intervalCron(1200), "*/20 * * * *");
  assert.equal(intervalCron(3600), "0 * * * *");
  assert.equal(intervalCron(5400), "0 */2 * * *");
  assert.equal(intervalCron(5 * 3600), "0 */4 * * *");
  assert.equal(intervalCron(86_400), "0 0 * * *");
  assert.equal(intervalCron(3 * 86_400), "0 0 */3 * *");
  assert.equal(toCron(loop()), "*/5 * * * *");
  assert.equal(toCron(loop({ kind: "wakeup", cron: null, every: 600 })), "*/10 * * * *");
  assert.equal(toCron(loop({ kind: "wakeup", cron: null, every: null })), null);
  assert.equal(toCron(loop({ recurring: false, cron: "45 15 22 11 *" })), null, "a reminder fires once");
  assert.equal(toCron(loop({ kind: "monitor", cron: null })), null);

  const long = "check the deploy ".repeat(100);
  assert.deepEqual(taskOf(loop({ prompt: long })), { prompt: long, cron: "*/5 * * * *" });
  assert.equal(taskOf(loop({ prompt: "" })), null, "/loop's built-in prompt");
  assert.equal(taskOf(loop({ prompt: "  " })), null);
  assert.equal(taskOf(loop({ prompt: `${long.slice(0, 1000)}… [+700 chars]` })), null, "clipped by Claude Code");
  assert.equal(taskOf(loop({ recurring: false })), null);
});
