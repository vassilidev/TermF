import { test } from "node:test";
import assert from "node:assert/strict";
import { mostRecent, projectsOf, rank, shortcut, tally, validProject } from "../src/lib/nav-logic.ts";

const repo = (path, modified = null) => ({ name: path.split(/[/\\]/).pop(), path, modified });
const projects = [
  { id: "acme", repos: [repo("/home/alex/code/acme/api", 30), repo("/home/alex/code/acme/web", 50)] },
  { id: "docs", repos: [repo("/home/alex/code/docs", 10)] },
  { id: "win", repos: [repo("C:\\code\\tool")] },
  { id: "shared", repos: [repo("/home/alex/code/acme/api")] },
];
const runs = [{ project: "docs", worktree: "/data/worktrees/docs/7/docs" }];

test("a session belongs to the projects of its repository, its subfolders, or its run's worktree", () => {
  assert.deepEqual(projectsOf("/home/alex/code/acme/api", projects, runs), ["acme", "shared"]);
  assert.deepEqual(projectsOf("/home/alex/code/acme/api/src", projects, runs), ["acme", "shared"]);
  assert.deepEqual(projectsOf("/home/alex/code/acme/api-v2", projects, runs), []);
  assert.deepEqual(projectsOf("C:\\code\\tool\\src", projects, runs), ["win"]);
  assert.deepEqual(projectsOf("/data/worktrees/docs/7/docs", projects, runs), ["docs"]);
  assert.deepEqual(projectsOf("/home/alex/perso/blog", projects, runs), []);
});

const session = (cwd, running, status, exitCode = null) => ({ cwd, running, status, exitCode });

test("agent states are counted per project, outside every project, and in all", () => {
  const counts = tally(
    [
      session("/home/alex/code/acme/web", true, "working"),
      session("/home/alex/code/acme/web", true, "waiting"),
      session("/home/alex/code/acme/web", false, "waiting", 0),
      session("/home/alex/code/docs", false, null, 2),
      session("/data/worktrees/docs/7/docs", true, "idle"),
      session("/home/alex/perso/blog", true, null),
    ],
    projects,
    runs,
  );
  assert.deepEqual(counts.acme, { total: 3, working: 1, waiting: 1, failed: 0 });
  assert.deepEqual(counts.docs, { total: 2, working: 0, waiting: 0, failed: 1 });
  assert.deepEqual(counts[""], { total: 1, working: 0, waiting: 0, failed: 0 });
  assert.deepEqual(counts["*"], { total: 6, working: 1, waiting: 1, failed: 1 });
  assert.equal(counts.mobile, undefined);
});

test("the stored current project is dropped once it no longer exists", () => {
  assert.equal(validProject("docs", projects), "docs");
  assert.equal(validProject("gone", projects), null);
  assert.equal(validProject(null, projects), null);
  assert.equal(validProject(42, projects), null);
});

test("the most recent repository", () => {
  assert.equal(mostRecent(projects[0].repos).path, "/home/alex/code/acme/web");
  assert.equal(mostRecent([]), null);
});

const key = (key, code, mods = {}) => ({ key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

test("shortcuts on macOS", () => {
  assert.deepEqual(shortcut(key("1", "Digit1", { metaKey: true }), true, false), { action: "tile", n: 1 });
  assert.deepEqual(shortcut(key("3", "Digit3", { ctrlKey: true }), true, true), { action: "project", n: 3 });
  // AZERTY: the digit row types & é " without Shift; the code still says which digit.
  assert.deepEqual(shortcut(key("&", "Digit1", { ctrlKey: true }), true, false), { action: "project", n: 1 });
  assert.equal(shortcut(key("0", "Digit0", { ctrlKey: true }), true, false), null);
  assert.deepEqual(shortcut(key("o", "KeyO", { metaKey: true }), true, true), { action: "switcher" });
  assert.deepEqual(shortcut(key("t", "KeyT", { metaKey: true }), true, false), { action: "launcher" });
  assert.equal(shortcut(key("t", "KeyT", { metaKey: true, shiftKey: true }), true, false), null);
  assert.deepEqual(shortcut(key("k", "KeyK", { metaKey: true }), true, true), { action: "palette" });
  assert.equal(shortcut(key("k", "KeyK", { ctrlKey: true }), true, false), null);
  assert.equal(shortcut(key("1", "Digit1", { metaKey: true, altKey: true }), true, false), null);
});

test("shortcuts on Linux and Windows", () => {
  assert.deepEqual(shortcut(key("1", "Digit1", { ctrlKey: true }), false, false), { action: "tile", n: 1 });
  assert.deepEqual(shortcut(key("2", "Digit2", { altKey: true }), false, true), { action: "project", n: 2 });
  // AltGr is Ctrl+Alt on Windows: it types characters, it is no shortcut.
  assert.equal(shortcut(key("@", "Digit0", { ctrlKey: true, altKey: true }), false, false), null);
  assert.equal(shortcut(key("2", "Digit2", { ctrlKey: true, altKey: true }), false, false), null);
  assert.deepEqual(shortcut(key("T", "KeyT", { ctrlKey: true, shiftKey: true }), false, true), { action: "launcher" });
  assert.equal(shortcut(key("t", "KeyT", { ctrlKey: true }), false, false), null);
  // Ctrl+K, J and O belong to the shell in a terminal, unless Shift is held.
  assert.equal(shortcut(key("o", "KeyO", { ctrlKey: true }), false, true), null);
  assert.deepEqual(shortcut(key("O", "KeyO", { ctrlKey: true, shiftKey: true }), false, true), { action: "switcher" });
  assert.deepEqual(shortcut(key("o", "KeyO", { ctrlKey: true }), false, false), { action: "switcher" });
  assert.equal(shortcut(key("j", "KeyJ", { ctrlKey: true }), false, true), null);
  assert.deepEqual(shortcut(key("l", "KeyL", { ctrlKey: true, shiftKey: true }), false, true), { action: "ticket" });
});

const cmd = (id, label, group, extra = {}) => ({ id, label, group, icon: null, run() {}, ...extra });

test("palette results are grouped, the group of the best match first", () => {
  const list = [
    cmd("switch-acme", "Passer au projet Acme platform", "projects"),
    cmd("switch-docs", "Passer au projet Docs", "projects"),
    cmd("s1", "api · claude", "sessions", { hint: "travaille" }),
    cmd("repo-api", "Ouvrir claude dans api", "repos", { hint: "~/code/acme/api" }),
    cmd("repo-web", "Ouvrir claude dans web", "repos", { hint: "~/code/acme/web" }),
    cmd("other-site", "Ouvrir claude dans docs-site", "repos", { hint: "~/code/acme/docs-site", searchOnly: true }),
    cmd("view-files", "Afficher les fichiers", "views"),
  ];
  assert.deepEqual(rank(list, "").map((c) => c.id), ["switch-acme", "switch-docs", "s1", "repo-api", "repo-web", "view-files"]);
  // The path in the hint brings the current project's repositories, not the other projects' ones.
  assert.deepEqual(rank(list, "acme").map((c) => c.id), ["switch-acme", "repo-api", "repo-web"]);
  assert.deepEqual(rank(list, "docs").map((c) => c.id), ["switch-docs", "other-site"]);
  assert.deepEqual(rank(list, "fichiers").map((c) => c.id), ["view-files"]);
  // Scattered letters do not bring a search-only command.
  assert.deepEqual(rank(list, "dcs").map((c) => c.id), ["switch-docs"]);
  // Groups follow their best match: here the sessions.
  assert.equal(rank(list, "api claude")[0].id, "s1");
});
