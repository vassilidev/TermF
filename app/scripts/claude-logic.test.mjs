import { test } from "node:test";
import assert from "node:assert/strict";
import { alerts, filterArtifacts, nextSort, selection, sortArtifacts, toggleAll, updatedAt, updatedDay } from "../src/lib/claude-logic.ts";

const art = (id, title, owner, updated, cwd = null, seen = 0) => ({ id, url: `https://claude.ai/code/artifact/${id}`, title, owner, updated, cwd, seen });
const list = [
  art("a", "Q3 roadmap", "me", "2026-11-20", "/home/alex/code/acme/api"),
  art("b", "design tokens", "me", "2026-11-18", "/home/alex/code/acme/web"),
  art("c", null, "shared", "3 days ago", null, 1_700_000_000),
  art("d", "Billing migration plan", "shared", "2026-10-28"),
  art("e", "Roadmap 2027", null, null, "/home/alex/perso/blog", 1_800_000_000),
];
const ids = (rows) => rows.map((r) => r.id).join("");
const projectsOf = (cwd) => (cwd.startsWith("/home/alex/code/acme/") ? ["acme"] : []);
const origin = (a) => (a.cwd ? a.cwd.split("/").pop() : "");

test("a date the account gives sorts as one; anything else falls back to when TermF saw it", () => {
  assert.equal(updatedAt(list[0]), Date.parse("2026-11-20"));
  assert.equal(updatedAt(list[2]), 1_700_000_000_000);
  assert.equal(updatedAt(list[4]), 1_800_000_000_000);
});

test("a day without a time stays that day west of Greenwich; a full date is shown where you are", () => {
  const tz = process.env.TZ;
  process.env.TZ = "America/Los_Angeles";
  try {
    assert.equal(updatedDay("2026-11-20", "en-US").text, "Nov 20, 2026");
    assert.equal(updatedDay("2026-11-20T03:00:00Z", "en-US").text, "Nov 19, 2026");
    assert.equal(updatedDay("3 days ago", "en-US"), null);
  } finally {
    if (tz === undefined) delete process.env.TZ;
    else process.env.TZ = tz;
  }
});

test("filters by words, owner and project, together", () => {
  const all = { text: "", owner: "", project: "" };
  assert.equal(ids(filterArtifacts(list, all, projectsOf)), "abcde");
  assert.equal(ids(filterArtifacts(list, { ...all, text: "ROADmap" }, projectsOf)), "ae");
  assert.equal(ids(filterArtifacts(list, { ...all, text: "road 2027" }, projectsOf)), "e");
  assert.equal(ids(filterArtifacts(list, { ...all, text: "blog" }, projectsOf)), "e", "its folder");
  assert.equal(ids(filterArtifacts(list, { ...all, text: "artifact/d" }, projectsOf)), "d", "its link");
  assert.equal(ids(filterArtifacts(list, { ...all, owner: "shared" }, projectsOf)), "cd");
  assert.equal(ids(filterArtifacts(list, { ...all, owner: "me", project: "acme" }, projectsOf)), "ab");
  assert.equal(ids(filterArtifacts(list, { ...all, project: "acme", text: "plan" }, projectsOf)), "");
});

test("sorts by each column, both ways, with empty values last", () => {
  const sort = (key, dir) => ids(sortArtifacts(list, { key, dir }, origin));
  assert.equal(sort("title", "asc"), "dbaec");
  assert.equal(sort("title", "desc"), "eabdc");
  assert.equal(sort("origin", "asc"), "aebdc", "no folder last, ties by most recent");
  assert.equal(sort("origin", "desc"), "beadc");
  assert.equal(sort("owner", "asc"), "abdce");
  assert.equal(sort("updated", "desc"), "eabdc");
  assert.equal(sort("updated", "asc"), "cdbae");
});

test("a column header turns its order around, another starts in reading order", () => {
  assert.deepEqual(nextSort({ key: "updated", dir: "desc" }, "updated"), { key: "updated", dir: "asc" });
  assert.deepEqual(nextSort({ key: "updated", dir: "desc" }, "title"), { key: "title", dir: "asc" });
  assert.deepEqual(nextSort({ key: "title", dir: "asc" }, "updated"), { key: "updated", dir: "desc" });
});

test("the header checkbox selects every shown row, and only clears those", () => {
  const shown = list.slice(0, 3);
  assert.equal(selection([], shown), "none");
  assert.equal(selection(["a", "e"], shown), "some");
  assert.deepEqual(toggleAll(["e"], shown), ["e", "a", "b", "c"]);
  assert.equal(selection(["e", "a", "b", "c"], shown), "all");
  assert.deepEqual(toggleAll(["e", "a", "b", "c"], shown), ["e"], "hidden by a filter, kept");
  assert.equal(selection(["a"], []), "none");
});

test("the overview lists what needs a look, worst first", () => {
  const o = (more = {}) => ({
    settings: { files: [{ path: "/u/settings.json", state: "ok", error: null }] },
    mcp: { servers: [{ name: "gh", health: "connected" }], error: null },
    plugins: { error: null },
    account: { auth: { loggedIn: true }, authError: null },
    ...more,
  });
  assert.deepEqual(alerts(o()), []);
  const found = alerts(
    o({
      settings: { files: [{ path: "/r/.claude/settings.local.json", state: "invalid", error: "line 4" }] },
      mcp: { servers: [{ name: "linear", health: "needsauth" }, { name: "pg", health: "failed" }, { name: "docs", health: "pending" }], error: null },
      account: { auth: { loggedIn: false }, authError: null },
    }),
  );
  assert.deepEqual(
    found.map((a) => [a.section, a.tone, a.server ?? a.file ?? (a.loggedOut ? "out" : a.text)]),
    [
      ["settings", "error", "/r/.claude/settings.local.json"],
      ["account", "warn", "out"],
      ["mcp", "error", "pg"],
      ["mcp", "warn", "linear"],
      ["mcp", "warn", "docs"],
    ],
  );
  const broken = alerts(o({ account: { auth: null, authError: "claude: not found" }, plugins: { error: "exit 2" } }));
  assert.deepEqual(broken.map((a) => [a.section, a.text]), [["mcp", "exit 2"], ["account", "claude: not found"]]);
});
