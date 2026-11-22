import { test } from "node:test";
import assert from "node:assert/strict";
import { cloneTarget, expandHome, inside } from "../src/lib/sync-logic.ts";

test("a missing repository goes beside the project's others", () => {
  const home = "/Users/alex";
  assert.equal(cloneTarget("~/work/acme/api", ["/Users/alex/code/acme/web"], home), "/Users/alex/code/acme/api");
  assert.equal(cloneTarget("~/work/acme/api", ["~/src/acme/web/"], home), "/Users/alex/src/acme/api");
  assert.equal(cloneTarget("~/work/acme/api", [], home), "/Users/alex/work/acme/api");
  assert.equal(cloneTarget("/srv/acme/api", [], home), "/srv/acme/api");
  assert.equal(cloneTarget("~/acme/api", ["C:\\Users\\alex\\code\\web"], "C:\\Users\\alex"), "C:\\Users\\alex\\code\\api");
});

test("a chosen folder gets the repository's own name", () => {
  assert.equal(inside("/Volumes/data/", "~/work/acme/api"), "/Volumes/data/api");
  assert.equal(inside("D:\\src", "~/acme/billing-service"), "D:\\src\\billing-service");
  assert.equal(expandHome("~", "/home/alex"), "/home/alex/");
  assert.equal(expandHome("~/a/b", "C:\\Users\\alex"), "C:\\Users\\alex\\a\\b");
  assert.equal(expandHome("~alex/a", "/home/alex"), "~alex/a");
});
