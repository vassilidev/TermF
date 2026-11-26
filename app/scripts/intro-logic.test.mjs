import { test } from "node:test";
import assert from "node:assert/strict";
import { introResume, introSteps, nextStep, pending, putOff, readSetup } from "../src/lib/intro-logic.ts";

const none = { env: false, vault: false, sync: false };
const tail = ["project", "cto", "help"];

test("a first machine sets up its environment, its vault, then the sync that carries them", () => {
  assert.deepEqual(introSteps("first", none), ["welcome", "claude", "env", "vault", "sync", ...tail]);
  assert.deepEqual(introSteps(null, none), introSteps("first", none), "not chosen yet: as a first machine");
});

test("a second machine joins the sync first, before the environment and the vault", () => {
  assert.deepEqual(introSteps("other", none), ["welcome", "claude", "sync", "env", "vault", ...tail]);
});

test("what is set up when the introduction opens is left out", () => {
  assert.deepEqual(introSteps("first", { env: false, vault: true, sync: true }), ["welcome", "claude", "env", ...tail]);
  assert.deepEqual(introSteps("other", { env: true, vault: true, sync: true }), ["welcome", "claude", ...tail]);
});

test("resumed, only the steps put off and still to do, in the machine's order", () => {
  assert.deepEqual(introSteps("first", none, ["sync", "env"]), ["env", "sync"]);
  assert.deepEqual(introSteps("other", none, ["vault", "sync"]), ["sync", "vault"]);
  assert.deepEqual(introSteps("first", { ...none, vault: true }, ["vault"]), []);
});

test("Continue passes over a vault or a sync set up meanwhile, never the environment", () => {
  const steps = introSteps("other", none);
  assert.equal(nextStep(steps, "sync", none), "env");
  assert.equal(nextStep(steps, "env", { ...none, vault: true }), "project", "the sync brought the vault");
  assert.equal(nextStep(steps, "claude", { env: true, vault: true, sync: true }), "env");
  assert.equal(nextStep(introSteps("first", none), "env", { ...none, sync: true }), "vault");
  assert.equal(nextStep(introSteps("first", none), "vault", { ...none, sync: true }), "project");
  assert.equal(nextStep(steps, "help", none), null);
  assert.equal(nextStep(["env", "sync"], "env", { ...none, sync: true }), null, "resumed: nothing left");
});

test("closing puts off the setup steps not done, and keeps those put off before", () => {
  const steps = introSteps("first", none);
  assert.deepEqual(putOff([], steps, { env: true, vault: false, sync: false }), ["vault", "sync"]);
  assert.deepEqual(putOff([], steps, { env: true, vault: true, sync: true }), []);
  assert.deepEqual(putOff(["sync"], ["vault"], none), ["vault", "sync"], "resumed for the vault only");
  assert.deepEqual(putOff(["sync", "env"], ["env"], { ...none, env: true }), ["sync"]);
  assert.deepEqual(putOff(["vault"], ["welcome", "claude", ...tail], { ...none, vault: true }), [], "done elsewhere since");
});

test("the Home offers the steps put off that are still to do", () => {
  assert.deepEqual(pending("other", ["env", "vault", "sync"], none), ["sync", "env", "vault"]);
  assert.deepEqual(pending("first", ["vault", "sync"], { ...none, vault: true }), ["sync"]);
  assert.deepEqual(pending(null, [], none), []);
});

test("a project created from the introduction goes on to the CTO", () => {
  assert.equal(introResume("project", 0, 1), "cto");
  assert.equal(introResume("project", 2, 2), "project", "the assistant left without creating one");
  assert.equal(introResume("help", 1, 2), "help");
});

test("what is kept reads back, anything else reads as nothing", () => {
  assert.deepEqual(readSetup('{"machine":"other","passed":["sync","env","nope"]}'), { machine: "other", passed: ["env", "sync"] });
  assert.deepEqual(readSetup(null), { machine: null, passed: [] });
  assert.deepEqual(readSetup("{not json"), { machine: null, passed: [] });
  assert.deepEqual(readSetup('{"machine":"laptop","passed":"env"}'), { machine: null, passed: [] });
});
