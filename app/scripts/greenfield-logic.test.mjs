import { test } from "node:test";
import assert from "node:assert/strict";
import { cleanName, nameFromIdea, parentOf } from "../src/lib/greenfield-logic.ts";

test("the name comes from the first words of the idea that say something", () => {
  assert.equal(
    nameFromIdea("Une app pour suivre l'arrosage de mes plantes d'intérieur et rappeler quand les arroser."),
    "suivre-arrosage-plantes",
  );
  assert.equal(nameFromIdea("A shared budget for flatmates: who paid what, who owes whom."), "shared-budget-flatmates");
  assert.equal(nameFromIdea("Un budget partagé entre colocataires"), "budget-partage-colocataires");
  assert.equal(nameFromIdea("A command-line watch on flight prices", 2), "watch-flight");
  assert.equal(nameFromIdea("  "), "");
  assert.equal(nameFromIdea("Une app pour moi"), "moi");
  assert.ok(nameFromIdea("x".repeat(80)).length <= 48);
});

test("a typed name becomes a folder and GitHub repository name", () => {
  assert.equal(cleanName("Mon Projet Été"), "Mon-Projet-Ete");
  assert.equal(cleanName("a/b\\c:d"), "abcd");
  assert.equal(cleanName("plant_care.v2"), "plant_care.v2");
});

test("the new project goes beside most of the known repositories, else in the home folder", () => {
  const H = "/Users/alex";
  const repos = [`${H}/code/acme/api`, `${H}/code/acme/web`, `${H}/perso/blog`].map((path) => ({ path }));
  assert.equal(parentOf(repos, H), `${H}/code/acme`);
  assert.equal(parentOf([], H), H);
  assert.equal(parentOf([{ path: "C:\\Users\\alex\\code\\api\\" }], "C:\\Users\\alex"), "C:\\Users\\alex\\code");
});
