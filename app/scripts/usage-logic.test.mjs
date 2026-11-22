import { test } from "node:test";
import assert from "node:assert/strict";
import { NEARLY_USED, approx, clock, fullest, pressure, subscribed } from "../src/lib/usage-logic.ts";

const max = { mode: "subscription", plan: "max" };
const api = { mode: "api", plan: null };
const win = (usedPercentage, resetsAt = 1_763_820_000) => ({ usedPercentage, resetsAt, usedUsd: null, limitUsd: null });

test("a subscriber's dollars are an API equivalent; an API key's are billed as they are", () => {
  assert.equal(subscribed(max), true);
  assert.equal(subscribed(api), false);
  assert.equal(subscribed(null), false);
  assert.equal(approx("$0.12", max), "≈ $0.12");
  assert.equal(approx("$0.12", api), "$0.12");
  assert.equal(approx("$0.12", { mode: "unknown", plan: null }), "$0.12");
});

test("the gauge shows the fuller window; a spend limit is not the subscription's quota", () => {
  assert.equal(fullest(null), null);
  assert.equal(fullest({ fiveHour: null, sevenDay: null, spendLimit: win(99) }), null);
  assert.equal(fullest({ fiveHour: win(40), sevenDay: win(62), spendLimit: null }).name, "sevenDay");
  assert.equal(fullest({ fiveHour: win(40), sevenDay: null, spendLimit: null }).name, "fiveHour");
});

test("a window warns from 80 %", () => {
  assert.equal(NEARLY_USED, 80);
  assert.equal(pressure({ fiveHour: win(79.9), sevenDay: win(10), spendLimit: null }), null);
  const near = pressure({ fiveHour: win(80), sevenDay: win(10), spendLimit: null });
  assert.equal(near.name, "fiveHour");
  assert.equal(pressure({ fiveHour: win(85), sevenDay: win(100), spendLimit: null }).name, "sevenDay");
});

test("a reset today shows its time, a later one its day too", () => {
  const now = new Date(2026, 10, 22, 10, 0).getTime();
  const today = new Date(2026, 10, 22, 15, 0).getTime() / 1000;
  const monday = new Date(2026, 10, 23, 9, 0).getTime() / 1000;
  assert.equal(clock(today, "en-GB", now), "15:00");
  assert.match(clock(monday, "en-GB", now), /^Mon 09:00$/);
});
