import { test } from "node:test";
import assert from "node:assert/strict";
import { changeOf, chipsOf, compose, diffOf, MAX_CONTEXT, question, roleFor, target, within } from "../src/lib/ask-logic.ts";
import { ciFailed, runNext, setup } from "../src/lib/guide-logic.ts";

test("roleFor: the screen decides, what the screen gives wins, the CTO otherwise", () => {
  assert.equal(roleFor("ci"), "devops");
  assert.equal(roleFor("vault"), "secops");
  assert.equal(roleFor("run"), "lead");
  assert.equal(roleFor("ops", "dba"), "dba");
  assert.equal(roleFor("settings"), "cto");
});

test("target: @role at the start wins when the role exists, else the chosen role keeps the whole text", () => {
  assert.deepEqual(target("@dba why is it slow?", "devops", ["dba", "devops"]), { role: "dba", text: "why is it slow?" });
  assert.deepEqual(target("@cto plan it", "ux", []), { role: "cto", text: "plan it" });
  assert.deepEqual(target("@nobody hi there", "devops", ["dba"]), { role: "devops", text: "@nobody hi there" });
  assert.deepEqual(target("  mail me @dba  ", "cto", ["dba"]), { role: "cto", text: "mail me @dba" });
});

test("compose: the context before the question, a secret by its name only, a long selection cut", () => {
  assert.equal(compose("hi", [], "Home"), "hi");
  const sent = compose(
    "why?",
    [
      { id: "view", kind: "view", label: "CI/CD · acme" },
      { id: "item", kind: "object", label: "Login acme.com (login)", text: "hunter2", secret: true },
      { id: "selection", kind: "selection", label: "Selection", text: "x".repeat(MAX_CONTEXT + 10) },
    ],
    "CI/CD",
  );
  assert.ok(sent.startsWith('<termf-context screen="CI/CD">'));
  assert.ok(sent.endsWith("\n\nwhy?"));
  assert.ok(sent.includes("- Login acme.com (login)\n- Selection"));
  assert.ok(!sent.includes("hunter2"), "a secret's value went out");
  assert.ok(sent.includes("[…]"));
  assert.equal(question(sent), "why?");
  assert.equal(question("plain"), "plain");
});

test("chipsOf: one per id, the last given wins, those taken off stay off", () => {
  const view = { id: "view", kind: "view", label: "Home" };
  const run = { id: "run", kind: "object", label: "#57" };
  const again = { id: "run", kind: "object", label: "#57 Export" };
  assert.deepEqual(
    chipsOf([view, run], [again], []).map((c) => c.label),
    ["Home", "#57 Export"],
  );
  assert.deepEqual(chipsOf([view, run], [], ["run"]), [view]);
});

test("proposals: a file change, its repository and its diff", () => {
  assert.deepEqual(changeOf("Edit", { file_path: "/w/api/src/a.ts" }), { path: "/w/api/src/a.ts", kind: "edit" });
  assert.deepEqual(changeOf("Write", { file_path: "/w/api/new.ts" }), { path: "/w/api/new.ts", kind: "write" });
  assert.equal(changeOf("Bash", { command: "rm -rf /" }), null);
  assert.deepEqual(within("/w/api/src/a.ts", ["/w/web", "/w/api", "/w/api/src"]), { repo: "/w/api/src", rel: "a.ts" });
  assert.equal(within("/w/apix/a.ts", ["/w/api"]), null);
  assert.deepEqual(diffOf("Edit", { old_string: "a\nb", new_string: "c" }), [{ del: ["a", "b"], add: ["c"] }]);
  assert.deepEqual(diffOf("MultiEdit", { edits: [{ old_string: "x", new_string: "y" }] }), [{ del: ["x"], add: ["y"] }]);
  assert.deepEqual(diffOf("Write", { content: "new" }), [{ del: [], add: ["new"] }]);
  assert.deepEqual(diffOf("Bash", { command: "ls" }), []);
});

test("setup: done items, a run says the earlier ones were done", () => {
  const none = setup({ claude: false, projects: 0, tickets: false, runs: 0, delivered: false });
  assert.deepEqual(
    none.map((i) => i.done),
    [false, false, false, false, false],
  );
  const ran = setup({ claude: false, projects: 1, tickets: false, runs: 2, delivered: false });
  assert.deepEqual(
    ran.map((i) => [i.id, i.done]),
    [
      ["claude", true],
      ["project", true],
      ["tickets", true],
      ["run", true],
      ["deliver", false],
    ],
  );
});

test("runNext: a blocked step first, then the answer, the delivery, a team at work; the cleanup has its card", () => {
  assert.equal(runNext({ expected: "answer", blocked: true, working: true }), "failed");
  assert.equal(runNext({ expected: "answer", blocked: false, working: true }), "answer");
  assert.equal(runNext({ expected: "deliver", blocked: false, working: false }), "deliver");
  assert.equal(runNext({ expected: "clean", blocked: false, working: false }), null);
  assert.equal(runNext({ expected: null, blocked: false, working: true }), "working");
  assert.equal(runNext({ expected: null, blocked: false, working: false }), null);
});

test("ciFailed: only when the latest finished run of the branch failed", () => {
  const run = (conclusion, branch = "main", status = "completed") => ({ status, conclusion, workflow: "CI", branch });
  assert.equal(ciFailed([run("failure")], "main")?.conclusion, "failure");
  assert.equal(ciFailed([run(null, "main", "in_progress"), run("failure")], "main")?.conclusion, "failure");
  assert.equal(ciFailed([run("success"), run("failure")], "main"), null);
  assert.equal(ciFailed([run("failure", "dev")], "main"), null);
  assert.equal(ciFailed([run("failure", "dev")], null)?.branch, "dev");
  assert.equal(ciFailed([], "main"), null);
});
