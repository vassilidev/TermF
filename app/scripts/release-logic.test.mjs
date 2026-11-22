import { test } from "node:test";
import assert from "node:assert/strict";
import { found, installs, missingRequired, osOf, percent, working } from "../src/lib/release-logic.ts";

const tool = (name, path = null) => ({ name, version: path ? "1.0" : null, path });
const claude = (install, tooOld = false) => ({ install, others: [], minimum: "2.1.288", tooOld });
const at = { path: "/home/alex/.local/bin/claude", version: "2.1.330", origin: "native" };

test("git and a recent enough Claude Code are required; gh and Chrome are not", () => {
  const all = { git: tool("git", "/usr/bin/git"), gh: tool("gh"), claude: claude(at), chrome: null };
  assert.deepEqual(missingRequired(all), []);
  assert.equal(found(all, "gh"), false);
  assert.equal(found(all, "chrome"), false);
  assert.deepEqual(missingRequired({ ...all, git: tool("git") }), ["git"]);
  assert.deepEqual(missingRequired({ ...all, claude: claude(null) }), ["claude"]);
  assert.deepEqual(missingRequired({ ...all, claude: claude({ ...at, version: "2.0.1" }, true) }), ["claude"], "too old");
});

test("the system decides the install command", () => {
  assert.equal(osOf("MacIntel"), "mac");
  assert.equal(osOf("Win32"), "windows");
  assert.equal(osOf("Linux x86_64"), "linux");
  assert.match(installs.claude.command.windows, /install\.ps1/);
  assert.equal(installs.gh.command.linux, undefined, "the official page for Linux, no single command");
});

test("only agents at work count against an update's restart", () => {
  const tiles = [
    { running: true, status: "working" },
    { running: true, status: "waiting" },
    { running: false, status: "working" },
  ];
  assert.equal(working(tiles, [{ state: "working" }, { state: "idle" }]), 2);
  assert.equal(working([], []), 0);
});

test("download progress", () => {
  assert.equal(percent(50, 200), "25 %");
  assert.equal(percent(300, 200), "100 %");
  assert.equal(percent(50, null), "");
});
