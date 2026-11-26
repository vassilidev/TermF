import { test } from "node:test";
import assert from "node:assert/strict";
import { hasWords, mostRecent, projectsOf, rank, tally, validProject } from "../src/lib/nav-logic.ts";

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

test("a conversation is found by its title, role, project or passage, never by scattered letters", () => {
  const list = [
    cmd("conversation-1", "CTO · Acme", "conversations", { searchOnly: true, terms: "cto · Acme" }),
    cmd("conversation-2", "Fix the login", "conversations", { searchOnly: true, terms: "devops · Docs …the déploiement failed…" }),
  ];
  assert.deepEqual(rank(list, "").map((c) => c.id), []);
  assert.deepEqual(rank(list, "devops").map((c) => c.id), ["conversation-2"]);
  assert.deepEqual(rank(list, "acme").map((c) => c.id), ["conversation-1"]);
  assert.deepEqual(rank(list, "deploiement login").map((c) => c.id), ["conversation-2"]);
  assert.deepEqual(rank(list, "cdx").map((c) => c.id), []);
});

test("a page's search finds the sections holding every word, case and accents aside", () => {
  const text = "Mémoire du projet L'expert du projet y cherche avant de lire le code";
  assert.equal(hasWords(text, ""), true);
  assert.equal(hasWords(text, "  memoire "), true);
  assert.equal(hasWords(text, "EXPERT mém"), true);
  assert.equal(hasWords(text, "mémoire tickets"), false);
});
