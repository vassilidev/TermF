import { test } from "node:test";
import assert from "node:assert/strict";
import { ALL_REPOS, hasWords, mostRecent, pickRepo, projectsOf, rank, repoOf, tally, validProject } from "../src/lib/nav-logic.ts";

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

test("the views' common repository: all of a project's by default, kept per scope, a vanished one falls back", () => {
  const [api, web] = projects[0].repos;
  // A project of several repositories opens on all of them; one of a single repository, or no project, on a repository.
  assert.equal(pickRepo(undefined, [api, web], true), ALL_REPOS);
  assert.equal(pickRepo(undefined, projects[1].repos, true), "/home/alex/code/docs");
  assert.equal(pickRepo(undefined, [api, web], false), web.path);
  assert.equal(pickRepo(undefined, [], true), "");
  // The stored choice holds while it is still there.
  assert.equal(pickRepo(api.path, [api, web], true), api.path);
  assert.equal(pickRepo(ALL_REPOS, [api, web], true), ALL_REPOS);
  // A repository gone from the project: back to all; once the project has a single one: that one.
  assert.equal(pickRepo("/home/alex/code/acme/old", [api, web], true), ALL_REPOS);
  assert.equal(pickRepo(ALL_REPOS, [api], true), api.path);
  assert.equal(pickRepo(web.path, [api], true), api.path);
  // "All" is a project's: never every project's repositories at once.
  assert.equal(pickRepo(ALL_REPOS, [api, web], false), web.path);
  assert.equal(pickRepo(42, [api, web], true), ALL_REPOS);
});

test("a folder says its repository only in a project of several", () => {
  const run = { project: "acme", worktree: "/data/worktrees/acme/9/web", repo: "/home/alex/code/acme/web" };
  assert.equal(repoOf("/home/alex/code/acme/api/src", projects, []).path, "/home/alex/code/acme/api");
  assert.equal(repoOf("/data/worktrees/acme/9/web", projects, [run]).path, "/home/alex/code/acme/web");
  assert.equal(repoOf("/home/alex/code/docs", projects, []), null);
  assert.equal(repoOf("/home/alex/perso/blog", projects, [run]), null);
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
