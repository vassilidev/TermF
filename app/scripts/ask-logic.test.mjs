import { test } from "node:test";
import assert from "node:assert/strict";
import {
  afterChange,
  changeOf,
  chipsOf,
  ciRunChip,
  compose,
  diffOf,
  MAX_CONTEXT,
  Owners,
  question,
  removedOf,
  reroute,
  roleFor,
  target,
  threadKey,
  within,
} from "../src/lib/ask-logic.ts";
import { ciFailed, runNext, setup } from "../src/lib/guide-logic.ts";

test("roleFor: the screen decides, what the screen gives wins, the CTO otherwise", () => {
  assert.equal(roleFor("ci"), "devops");
  assert.equal(roleFor("vault"), "secops");
  assert.equal(roleFor("vaultAll"), "secops");
  assert.equal(roleFor("run"), "lead");
  assert.equal(roleFor("ops", "dba"), "dba");
  assert.equal(roleFor("settings"), "termf-expert", "the TermF expert knows the settings");
  assert.equal(roleFor("scheduler"), "cto");
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

test("removedOf: every chip taken off comes back, the view's and those added (a selection, a Why?) included", () => {
  const view = { id: "view", kind: "view", label: "CI/CD" };
  const shown = { id: "ci-run:7", kind: "object", label: "CI #7" };
  const why = { id: "ci-run:5", kind: "object", label: "CI #5" };
  const selection = { id: "selection", kind: "selection", label: "Selection", text: "x" };
  assert.deepEqual(
    removedOf([view, shown], [why, selection], ["view", "ci-run:5", "selection"]).map((c) => c.id),
    ["view", "ci-run:5", "selection"],
  );
  assert.deepEqual(removedOf([view, shown], [why], []), []);
});

test("ciRunChip: one chip per run, so the run asked about stays beside the one shown", () => {
  const run = (id) => ({ id, workflow: "CI", number: id, branch: "main", conclusion: "failure", status: "completed", url: `u/${id}` });
  const shown = ciRunChip(run(7));
  const asked = ciRunChip(run(5));
  assert.notEqual(shown.id, asked.id);
  assert.deepEqual(
    chipsOf([shown], [asked], []).map((c) => c.label),
    ["CI #7", "CI #5"],
  );
  assert.ok(asked.text.includes("failure") && asked.text.includes("u/5"));
});

test("threadKey: a role's conversation per run on the Run page, else per repository", () => {
  assert.equal(threadKey("acme", "lead", { run: 57, repo: "/w/api" }), "acme:lead:run:57");
  assert.notEqual(threadKey("acme", "lead", { run: 57 }), threadKey("acme", "lead", { run: 58 }));
  assert.notEqual(threadKey("acme", "devops", { repo: "/w/api" }), threadKey("acme", "devops", { repo: "/w/web" }));
  // A run's step answered on its page is never the conversation of the same role on another screen.
  assert.notEqual(threadKey("acme", "back", { run: 57, repo: "/w/api" }), threadKey("acme", "back", { repo: "/w/api" }));
  assert.equal(threadKey("acme", "ux", {}), "acme:ux:");
});

test("reroute: a follow-up to a run's step or to another role is routed again, never sent straight", () => {
  assert.equal(reroute("devops", "devops", false), false);
  assert.equal(reroute("lead", "lead", true), true, "a step's session got a follow-up without routing");
  assert.equal(reroute("dba", "devops", false), true);
});

test("Owners: a cleanup takes back only its own giving, never a newer one", () => {
  const owners = new Owners();
  const old = owners.take("run");
  const fresh = owners.take("run");
  assert.equal(owners.release("run", old), false, "the old Run page took back the new one's context");
  assert.equal(owners.release("run", fresh), true);
  assert.equal(owners.release("run", fresh), false);
  const ci = owners.take("ci");
  assert.equal(owners.release("ci", ci), true);
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

test("afterChange: the file as the change leaves it, null when it would not apply", () => {
  assert.equal(afterChange("Edit", { old_string: "a", new_string: "$&b" }, "xax a"), "x$&bx a");
  assert.equal(afterChange("Edit", { old_string: "a", new_string: "b", replace_all: true }, "xax a"), "xbx b");
  assert.equal(afterChange("Edit", { old_string: "zz", new_string: "b" }, "xax"), null);
  assert.equal(afterChange("MultiEdit", { edits: [{ old_string: "a", new_string: "b" }, { old_string: "b", new_string: "c" }] }, "a"), "c");
  assert.equal(afterChange("Write", { content: "new" }, "old"), "new");
  assert.equal(afterChange("Bash", { command: "ls" }, "x"), null);
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
