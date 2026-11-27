import { test } from "node:test";
import assert from "node:assert/strict";
import { ending, layout, matches, proposal, rows, tally, worst } from "../src/lib/stack-logic.ts";

const dep = (name, version, more = {}) => ({ name, ecosystem: "npm", manager: "npm", range: `^${version}`, version, locked: true, dev: false, manifest: "package.json", ...more });
const repo = (name, more = {}) => ({ name, path: `/code/${name}`, tags: [], deps: [], services: [], databases: [], ports: [], env: [], envFile: null, workflows: [], publishes: [], ...more });
const vuln = (severity) => ({ id: `GHSA-${severity}`, aliases: [], summary: "", severity, fixed: null, url: "" });
const support = (tag, more = {}) => ({ tag, product: tag, dep: null, version: "18.19.0", cycle: "18", eol: "2025-04-30", ended: true, lts: true, latestInCycle: null, latest: null, currentLts: "22", ...more });

test("a repository's lines put runtimes first, then the worst, and its filters count them", () => {
  const web = repo("web", { deps: [dep("lodash", "4.17.20"), dep("react", "17.0.2"), dep("vite", "5.4.2", { dev: true }), dep("zod", "3.23.8")] });
  const check = {
    deps: [
      { name: "lodash", ecosystem: "npm", version: "4.17.20", latest: "4.17.21", safe: "4.17.21", bump: "patch", safeBump: "patch", safeFixes: [], vulns: [vuln("high"), vuln("low")], command: "npm install lodash@4.17.21" },
      { name: "react", ecosystem: "npm", version: "17.0.2", latest: "18.3.1", safe: null, bump: "major", vulns: [], command: null },
      { name: "vite", ecosystem: "npm", version: "5.4.2", latest: "6.0.1", safe: "5.4.11", bump: "major", vulns: [vuln("moderate")], command: "npm install vite@5.4.11 --save-dev" },
      { name: "zod", ecosystem: "npm", version: "3.23.8", latest: "3.23.8", safe: null, bump: null, vulns: [], command: null },
    ],
    support: [support("node"), support("react", { dep: "react", version: "17.0.2", cycle: "17", eol: null, ended: false, lts: false })],
    errors: [],
    offline: false,
    checked: 0,
  };
  const today = "2026-11-27";
  const list = rows(web, check, today);
  assert.deepEqual(list.map((r) => r.name), ["node", "lodash", "vite", "react", "zod"]);
  assert.equal(list[3].support.cycle, "17");
  const count = (f) => list.filter((r) => matches(r, f, "", today)).length;
  assert.deepEqual([count("all"), count("updates"), count("vulnerable"), count("eol")], [5, 3, 2, 1]);
  assert.equal(list.filter((r) => matches(r, "all", "LOD", today)).length, 1);
  const t = tally(list, today);
  assert.deepEqual([t.updates, t.safe, t.major, t.vulnerable, t.eol, t.worst], [3, 2, 2, 2, 1, "high"]);
  assert.equal(t.bySeverity.high, 1);
  assert.equal(worst([vuln("low"), vuln("critical")]), "critical");
  // The safe update first, the major when there is none.
  assert.deepEqual(proposal(list[2]), { to: "5.4.11", major: false });
  assert.deepEqual(proposal(list[3]), { to: "18.3.1", major: true });
  assert.equal(proposal(list[4]), null);
  assert.equal(ending(support("php", { ended: false, eol: "2027-03-01" }), today), "soon");
  assert.equal(ending(support("php", { ended: false, eol: "2028-12-31" }), today), null);
});

test("the map of five services fits one row, arcs above, and a shared database is drawn once", () => {
  const names = ["users", "orders", "billing", "search", "gateway"];
  const db = (name) => ({ engine: "postgres", name, host: "localhost", port: 5432, source: ".env.example:DATABASE_URL" });
  const repos = names.map((n, i) => repo(n, { databases: [db(i < 2 ? "acme" : n)] }));
  const links = [{ from: "/code/gateway", to: "/code/users", kind: "calls", via: ["USERS_URL"] }];
  const map = layout({ repos, links }, 1200);
  assert.equal(new Set(map.repos.map((b) => b.y)).size, 1);
  assert.ok(map.repos.every((b) => b.x >= 0 && b.x + b.w <= 1200));
  assert.ok(map.top > 70, "room for an arc over four nodes");
  assert.equal(map.dbs.length, 4);
  assert.deepEqual(map.dbs[0].repos, ["/code/users", "/code/orders"]);
  assert.ok(map.dbs.every((d) => d.y > map.repos[0].y + map.repos[0].h));
  const narrow = layout({ repos, links: [] }, 500);
  assert.equal(new Set(narrow.repos.map((b) => b.y)).size, 3);
});
