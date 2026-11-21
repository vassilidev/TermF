import { test } from "node:test";
import assert from "node:assert/strict";
import { cleanupNotice, liveSession, readyToClean, refusal, resumable } from "../src/lib/tickets-logic.ts";

const run = (o = {}) => ({
  status: "delivered",
  worktree: "/home/alex/code/acme/api/.claude/worktrees/termf-5-479-rate-limit",
  claudeSession: "0b9c3c1e-5f0a-4d6b-9a57-3f1d2c4e8a10",
  prState: null,
  sessionId: null,
  ticket: { key: "acme/api#479" },
  title: "Rate limit per API key",
  ...o,
});

test("a run's session is found by its id, else a Claude Code session running in its worktree", () => {
  const wt = run().worktree;
  const sessions = [
    { id: 3, running: true, cwd: "/elsewhere", status: "idle" },
    { id: 7, running: false, cwd: wt, status: "idle" },
  ];
  assert.equal(liveSession(run({ sessionId: 3 }), sessions), sessions[0]);
  assert.equal(liveSession(run({ sessionId: 7 }), sessions), undefined);
  assert.equal(liveSession(run({ sessionId: 9 }), sessions), undefined);
  assert.equal(liveSession(run(), sessions), undefined);
  const byHand = { id: 8, running: true, cwd: wt, status: "starting" };
  const shell = { id: 9, running: true, cwd: wt, status: null };
  assert.equal(liveSession(run(), [...sessions, shell]), undefined);
  assert.equal(liveSession(run(), [...sessions, shell, byHand]), byHand);
  assert.equal(liveSession(run({ sessionId: 3 }), [...sessions, byHand]), sessions[0]);
});

test("only a delivered run whose pull request was merged or closed is ready to clean up", () => {
  assert.equal(readyToClean(run({ prState: "merged" })), true);
  assert.equal(readyToClean(run({ prState: "closed" })), true);
  assert.equal(readyToClean(run()), false);
  assert.equal(readyToClean(run({ status: "closed", prState: "merged" })), false);
});

test("an open run with a known conversation and a worktree can be resumed", () => {
  assert.equal(resumable(run()), true);
  assert.equal(resumable(run({ status: "running" })), true);
  assert.equal(resumable(run({ claudeSession: null })), false);
  assert.equal(resumable(run({ worktree: null })), false);
  assert.equal(resumable(run({ status: "closed" })), false);
  assert.equal(resumable(run({ status: "failed" })), false);
});

test("a refused close or clean-up keeps its kind; anything else is an error", () => {
  const unpushed = { kind: "unpushed", message: "2 commits not pushed", uncommitted: false, commits: 2 };
  assert.deepEqual(refusal(unpushed), { kind: "unpushed", message: "2 commits not pushed" });
  assert.deepEqual(refusal({ ...unpushed, kind: "uncommitted" }).kind, "uncommitted");
  assert.deepEqual(refusal({ kind: "error", message: "the pull request is open" }), {
    kind: "error",
    message: "the pull request is open",
  });
  assert.deepEqual(refusal({ kind: "other", message: "x" }).kind, "error");
  assert.deepEqual(refusal("IPC failed"), { kind: "error", message: "IPC failed" });
  assert.deepEqual(refusal(null), { kind: "error", message: "null" });
});

test("the notification says whether the pull request was merged and what TermF did", () => {
  const merged = run({ prState: "merged" });
  assert.deepEqual(cleanupNotice({ run: merged, cleaned: false, error: null }), {
    title: "ticketsUiNotifyMerged",
    body: "ticketsUiNotifyReady",
    vars: { key: "acme/api#479", title: "Rate limit per API key", error: "" },
  });
  assert.equal(cleanupNotice({ run: merged, cleaned: true, error: null }).body, "ticketsUiNotifyCleaned");
  const failed = cleanupNotice({ run: merged, cleaned: false, error: "uncommitted changes" });
  assert.equal(failed.body, "ticketsUiNotifyFailed");
  assert.equal(failed.vars.error, "uncommitted changes");
  assert.equal(cleanupNotice({ run: run({ prState: "closed" }), cleaned: false, error: null }).title, "ticketsUiNotifyClosed");
});
