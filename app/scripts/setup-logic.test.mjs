import { test } from "node:test";
import assert from "node:assert/strict";
import { byActivity, derive, forge, githubSlugs, gitlabRepos, groups, projectId, sharedPrefix, shown, tagText, unique } from "../src/lib/setup-logic.ts";

const H = "/Users/alex";
const gh = (slug) => ({ host: "github.com", slug });
const card = (name, path, remote = null, modified = 0, stack = []) => ({ name, path, remote, modified, stack });
const api = card("api", `${H}/code/acme/api`, gh("acme/api"), 100, ["node", "postgres"]);
const web = card("web-dashboard", `${H}/code/acme/web-dashboard`, gh("acme/web-dashboard"), 300, ["react"]);
const infra = card("infra", `${H}/code/acme/infra`, { host: "gitlab.com", slug: "acme/infra" }, 200);
const blog = card("blog", `${H}/perso/blog`, gh("alex/blog"), 50);
const scratch = card("scratch", `${H}/scratch`, null, 10);

test("the name comes from the repository, then the GitHub owner, the folder, the first one", () => {
  assert.equal(derive([], H), null);
  assert.deepEqual(derive([web], H), { name: "web-dashboard", from: "repo" });
  assert.deepEqual(derive([api, web], H), { name: "acme", from: "owner" });
  assert.deepEqual(derive([api, infra], H), { name: "acme", from: "folder" });
  assert.deepEqual(derive([api, blog], H), { name: "api", from: "first" });
  // The home folder names nothing.
  assert.deepEqual(derive([scratch, card("notes", `${H}/notes`)], H), { name: "scratch", from: "first" });
});

test("several repositories named alike give their shared start, not the folder that holds them", () => {
  const front = card("acme-front", `${H}/code/acme-front`);
  const back = card("acme-back", `${H}/code/acme-back`);
  assert.deepEqual(derive([front, back], H), { name: "acme", from: "prefix" });
  assert.equal(sharedPrefix(["shop_api", "shop_web", "Shop.admin"]), "shop");
  assert.equal(sharedPrefix(["acme-cloud-api", "acme-cloud-web"]), "acme-cloud");
  assert.equal(sharedPrefix(["api", "web-dashboard"]), "");
  assert.equal(sharedPrefix(["a-api", "a-web"]), "", "one letter names nothing");
});

test("stack tags read as people write them", () => {
  assert.equal(tagText({ name: "nextjs", version: "16.0.3" }), "Next.js 16");
  assert.equal(tagText({ name: "typescript", version: "^5.9" }), "TypeScript 5");
  assert.equal(tagText({ name: "github-actions", version: null }), "GitHub Actions");
  assert.equal(tagText({ name: "css-tailwind" }), "css-tailwind");
});

test("a name already taken gets the next free number", () => {
  assert.equal(unique("acme", ["Docs"]), "acme");
  assert.equal(unique("acme", ["ACME", "acme 2"]), "acme 3");
});

test("the id follows save_project's", () => {
  assert.equal(projectId("Acme platform", []), "acme-platform");
  assert.equal(projectId("acme", ["acme", "acme-2"]), "acme-3");
  assert.equal(projectId("  !! ", []), "project");
  assert.equal(projectId("Équipe Été", []), "équipe-été");
});

test("repositories are listed by activity, filtered and searched", () => {
  assert.deepEqual(byActivity([api, web, infra]).map((c) => c.name), ["web-dashboard", "infra", "api"]);
  const now = 14 * 86_400 + 150;
  assert.equal(shown(api, "recent", "", now), false);
  assert.equal(shown(web, "recent", "", now), true);
  assert.equal(shown(infra, "github", "", now), false);
  assert.equal(shown(api, "all", "acme postgres", now), true);
  assert.equal(shown(api, "all", "react", now), false);
});

test("groups are suggested by GitHub owner and by folder, not once all taken", () => {
  const all = [api, web, infra, blog, scratch];
  assert.deepEqual(
    groups(all, [], H).map((g) => [g.kind, g.label, g.paths.length]),
    [
      ["folder", `${H}/code/acme`, 3],
      ["owner", "acme", 2],
    ],
  );
  assert.deepEqual(groups(all, [api.path, web.path], H).map((g) => g.label), [`${H}/code/acme`]);
  // The same repositories by owner and by folder are suggested once.
  assert.deepEqual(groups([api, web], [], H).map((g) => g.kind), ["owner"]);
});

test("GitHub repositories and forges", () => {
  assert.deepEqual(githubSlugs([api, infra, web, { ...api }]), ["acme/api", "acme/web-dashboard"]);
  assert.equal(forge("github.com"), "github");
  assert.equal(forge("gitlab.example.org"), "gitlab");
  assert.equal(forge("git.example.org"), "other");
  assert.equal(forge(undefined), "none");
});

test("GitLab repositories, on the first one's host", () => {
  const other = card("tools", `${H}/code/acme/tools`, { host: "gitlab.acme.dev", slug: "acme/tools" });
  assert.equal(gitlabRepos([api, web]), null);
  assert.deepEqual(gitlabRepos([api, infra, other, { ...infra }]), { host: "gitlab.com", slugs: ["acme/infra"] });
});
