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

const nine = { tiles: 9, projects: 9 };

test("shortcuts on macOS", () => {
  const mac = (k, inTerminal = false, count = nine) => shortcut(k, true, inTerminal, count);
  assert.deepEqual(mac(key("1", "Digit1", { metaKey: true })), { action: "tile", n: 1 });
  assert.deepEqual(mac(key("3", "Digit3", { ctrlKey: true }), true), { action: "project", n: 3 });
  // AZERTY: the digit row types & é " without Shift; the code still says which digit.
  assert.deepEqual(mac(key("&", "Digit1", { ctrlKey: true })), { action: "project", n: 1 });
  assert.equal(mac(key("0", "Digit0", { ctrlKey: true })), null);
  assert.deepEqual(mac(key("o", "KeyO", { metaKey: true }), true), { action: "switcher" });
  assert.deepEqual(mac(key("t", "KeyT", { metaKey: true })), { action: "launcher" });
  assert.equal(mac(key("t", "KeyT", { metaKey: true, shiftKey: true })), null);
  assert.deepEqual(mac(key("k", "KeyK", { metaKey: true }), true), { action: "palette" });
  assert.equal(mac(key("k", "KeyK", { ctrlKey: true })), null);
  assert.equal(mac(key("1", "Digit1", { metaKey: true, altKey: true })), null);
});

test("a digit with no tile or project behind it is left to the terminal", () => {
  const count = { tiles: 4, projects: 3 };
  assert.deepEqual(shortcut(key("4", "Digit4", { metaKey: true }), true, true, count), { action: "tile", n: 4 });
  assert.equal(shortcut(key("5", "Digit5", { metaKey: true }), true, true, count), null);
  assert.deepEqual(shortcut(key("3", "Digit3", { ctrlKey: true }), true, true, count), { action: "project", n: 3 });
  assert.equal(shortcut(key("4", "Digit4", { ctrlKey: true }), true, true, count), null);
  assert.equal(shortcut(key("4", "Digit4", { altKey: true }), false, false, count), null);
  assert.equal(shortcut(key("1", "Digit1", { ctrlKey: true }), false, false, { tiles: 0, projects: 0 }), null);
});

test("shortcuts on Linux and Windows", () => {
  const other = (k, inTerminal = false, count = nine) => shortcut(k, false, inTerminal, count);
  assert.deepEqual(other(key("1", "Digit1", { ctrlKey: true })), { action: "tile", n: 1 });
  assert.deepEqual(other(key("2", "Digit2", { altKey: true })), { action: "project", n: 2 });
  // AltGr is Ctrl+Alt on Windows: it types characters, it is no shortcut.
  assert.equal(other(key("@", "Digit0", { ctrlKey: true, altKey: true })), null);
  assert.equal(other(key("2", "Digit2", { ctrlKey: true, altKey: true })), null);
  assert.deepEqual(other(key("T", "KeyT", { ctrlKey: true, shiftKey: true }), true), { action: "launcher" });
  assert.equal(other(key("t", "KeyT", { ctrlKey: true })), null);
  // Ctrl+K, J, O and Alt+digit (readline's digit argument) belong to the shell in a terminal, unless Shift is held.
  assert.equal(other(key("o", "KeyO", { ctrlKey: true }), true), null);
  assert.deepEqual(other(key("O", "KeyO", { ctrlKey: true, shiftKey: true }), true), { action: "switcher" });
  assert.deepEqual(other(key("o", "KeyO", { ctrlKey: true })), { action: "switcher" });
  assert.equal(other(key("j", "KeyJ", { ctrlKey: true }), true), null);
  assert.equal(other(key("2", "Digit2", { altKey: true }), true), null);
  assert.deepEqual(other(key("@", "Digit2", { altKey: true, shiftKey: true }), true), { action: "project", n: 2 });
  assert.deepEqual(other(key("l", "KeyL", { ctrlKey: true, shiftKey: true }), true), { action: "ticket" });
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
