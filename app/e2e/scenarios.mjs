// What a developer discovering TermF does, one scenario at a time, each in its own sandbox. `views`: the views of
// `views.ts` (and the pages beside them) the scenario is about; every view of `views.ts` needs one.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { text } from "./harness.mjs";
import { Key } from "./webdriver.mjs";

const read = (file) => readFileSync(file, "utf8");

function check(ok, message) {
  if (!ok) throw new Error(message);
}

const viewOnly = (id, title, extra) => ({
  name: title,
  views: [id],
  async run(t) {
    await t.ready();
    await t.project();
    await t.view(id);
    await extra?.(t);
  },
});

function watchStatus(id) {
  const tile = document.querySelector(`.tile[data-session="${id}"]`);
  const seen = (window.__e2e.statuses = []);
  const note = () => {
    const s = tile.querySelector("[data-status]")?.dataset.status;
    if (s && seen.at(-1) !== s) seen.push(s);
  };
  note();
  new MutationObserver(note).observe(tile, { subtree: true, childList: true, attributes: true });
}

export const scenarios = [
  {
    name: "first launch: the guided tour, then the home",
    views: ["home"],
    sandbox: { project: false },
    async run(t) {
      const { s } = t;
      await t.ready({ keepTour: true });
      await t.clean();
      for (let step = 0; step < 4; step++) await s.click('[aria-labelledby="tour-title"] footer .primary');
      await s.waitGone('[aria-labelledby="tour-title"]');
      check((await s.exec(() => localStorage.getItem("termf.tour"))) === "1", "the tour is not remembered as seen");
      await t.view("home");
    },
  },
  {
    name: "new project: pick the repository, go through the setup, land on its home",
    views: ["newProject"],
    sandbox: { project: false },
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.command("project-new");
      await s.waitFor(t.shown("newProject"));
      const repo = `#repo-list li${t.sel("data-path", box.acme)}`;
      await s.click(repo);
      await s.waitFor(`${repo}[aria-selected="true"]`);
      for (const step of [2, 3, 4, 5]) {
        await s.click(".new-project .actions .go:not([disabled])");
        await s.waitFor(`.new-project .stepper li:nth-child(${step}) [aria-current="step"]`);
        await t.clean();
      }
      await s.click(".new-project .actions .go:not([disabled])");
      await s.waitFor(`.nav ${t.sel("data-project", "acme")}[aria-current="true"]`, { timeout: 30_000 });
      await s.waitFor(t.shown("home"));
      const toml = read(join(box.configDir, "projects", "acme.toml"));
      check(/name = "acme"/.test(toml) && toml.includes("acme"), `unexpected project file:\n${toml}`);
      await t.settled("home");
      await t.clean();
    },
  },
  {
    name: "project assistant: opens on the project from the palette",
    views: ["onboarding"],
    async run(t) {
      await t.ready();
      await t.project();
      await t.command("project-setup");
      await t.s.waitFor(t.shown("onboarding"));
      await t.clean();
    },
  },
  {
    name: "terminal: open a shell in a tile, type a command, read its output",
    views: ["terminals"],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      await t.command(`repo-shell-${box.acme}`);
      await s.waitFor(t.shown("terminals"));
      const id = await t.tile();
      await t.prompt(id);
      // The answer is not in the command line, so it can only come from the shell.
      const [command, answer] = t.windows ? ['"e2e-" + 6*7', "e2e-42"] : ["echo e2e-$((6*7))", "e2e-42"];
      await t.typeIn(id, command + Key.enter);
      await s.wait(async () => (await t.terminal(id)).split("\n").some((l) => l.trim() === answer), {
        what: `${answer} in the terminal`,
      });
      await t.clean();
    },
  },
  {
    name: "agent: open Claude Code (fake), see its status, talk to it",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      await t.command(`repo-claude-${box.acme}`);
      const id = await t.tile();
      await s.waitFor(`.tile[data-session="${id}"] [data-status="idle"]`, { timeout: 30_000 });
      await s.exec(watchStatus, id);
      await t.typeIn(id, "hello from e2e" + Key.enter);
      await s.wait(async () => (await t.terminal(id)).includes("Fake Claude received: hello from e2e"), { what: "the agent's answer" });
      const seen = await s.wait(async () => {
        const list = await s.exec(() => window.__e2e.statuses);
        return list.includes("working") && list.at(-1) === "idle" && list;
      }, { what: "working, then idle" });
      check(seen.join(" ") === "idle working idle", `statuses seen: ${seen.join(" → ")}`);
      check(/--plugin-dir/.test(read(box.calls)), "the agent got no hooks plugin");
      await t.clean();
    },
  },
  {
    name: "palette: Ctrl+K, then every view of the project and of This machine",
    views: [],
    async run(t) {
      const { s } = t;
      await t.ready();
      await t.project();
      await t.palette();
      await s.press(Key.escape);
      await s.waitGone("dialog.palette");
      for (const id of t.views) await t.view(id);
    },
  },
  {
    name: "files: open README.md, edit it, save it",
    views: ["files"],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("files");
      await t.ready();
      await t.project();
      await t.view("files");
      await s.click(`${pane} button.row[title="README.md"]`);
      await s.click(`${pane} .cm-content`);
      await s.press(Key.ctrl, Key.end);
      await s.keys("Edited by e2e.");
      await s.press(Key.ctrl, "s");
      await s.wait(() => read(join(box.acme, "README.md")).includes("Edited by e2e."), { what: "README.md saved" });
      await t.clean();
    },
  },
  {
    name: "git: a modified file shows in the changes, then is staged and committed",
    views: ["git"],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("git");
      writeFileSync(join(box.acme, "README.md"), "# Acme API\n\nChanged by e2e.\n");
      await t.ready();
      await t.project();
      await t.view("git");
      await s.click(`${pane} button.tree.dirty`);
      await s.click(`${pane} [data-testid="git-stage"]`);
      await s.click(`${pane} .commit input.field`);
      await s.keys("Update the README");
      await s.click(`${pane} .commit .btn.primary:not([disabled])`);
      await s.wait(() => box.git("log", "-1", "--format=%s").trim() === "Update the README", { what: "the commit" });
      check(box.git("status", "--porcelain") === "", "the work tree is not clean after the commit");
      await t.settled("git");
      await t.clean();
    },
  },
  {
    name: "tickets: the inbox lists the issue assigned on GitHub (fake gh)",
    views: ["tickets"],
    async run(t) {
      await t.ready();
      await t.project();
      await t.view("tickets");
      await t.s.wait(async () => (await t.s.text(t.shown("tickets")))?.includes("Fix the login button"), { what: "the issue in the inbox" });
      await t.clean();
    },
  },
  {
    name: "vault: create it, add a login, lock it, unlock it",
    views: ["vault"],
    async run(t) {
      const { s } = t;
      const pane = t.shown("vault");
      const password = "correct horse battery staple";
      await t.ready();
      await t.project();
      await t.view("vault");
      const field = `${pane} form.unlock input[type=password]`;
      await s.waitFor(field);
      const count = await s.exec((css) => document.querySelectorAll(css).length, field);
      check(count === 2, `the creation form has ${count} password fields`);
      for (const i of [0, 1]) await s.type(field, password, i);
      await s.click(`${pane} form.unlock button[type=submit]:not([disabled])`);
      await s.click('[data-testid="vault-new"]');
      await s.type(`${pane} form.form input.field`, "Acme admin");
      await s.type(`${pane} #vault-password`, "s3cret-e2e");
      await s.click(`${pane} form.form button[type=submit]:not([disabled])`);
      const listed = async () => (await s.text(`${pane} .list`))?.includes("Acme admin");
      await s.wait(listed, { what: "the login in the list" });
      await s.click('[data-testid="vault-lock"]');
      await s.type(`${pane} form.unlock input[type=password]`, password + Key.enter);
      await s.wait(listed, { what: "the login after unlocking" });
      await t.clean();
    },
  },
  {
    name: "settings: a choice is saved in termf.toml and kept after a restart",
    views: ["settings"],
    async run(t) {
      const { box } = t;
      const auto = `${t.shown("settings")} .keep-awake input[value="auto"]`;
      await t.ready();
      await t.view("settings");
      await t.s.click(auto);
      await t.s.wait(() => /keep_awake = "auto"/.test(read(join(box.configDir, "termf.toml"))), { what: "termf.toml updated" });
      await t.restart();
      await t.ready();
      await t.view("settings");
      check(await t.s.exec((css) => document.querySelector(css)?.checked, auto), "the choice is lost after a restart");
    },
  },
  {
    name: "language: with a French system the interface is in French",
    views: [],
    sandbox: { lang: "fr" },
    launch: { args: process.platform === "win32" ? ["--lang=fr-FR"] : [] },
    async run(t) {
      const { s } = t;
      await t.ready();
      await t.project();
      check((await s.exec(() => document.documentElement.lang)) === "fr", "the page is not in French");
      const home = await s.text('.nav [data-view="home"] .label');
      check(home === text("fr", "ux3t1Home"), `the navigation says ${home}`);
      for (const id of ["home", "terminals", "settings"]) await t.view(id);
    },
  },
  {
    name: "theme: a dark system gives the dark theme",
    views: [],
    only: "linux",
    sandbox: { dark: true },
    async run(t) {
      await t.ready();
      const dark = await t.s.exec(() => {
        const [r, g, b] = getComputedStyle(document.body).backgroundColor.match(/\d+/g).map(Number);
        return matchMedia("(prefers-color-scheme: dark)").matches && r + g + b < 200;
      });
      check(dark, "the window is not dark");
    },
  },
  {
    name: "sync: the encrypted sync section of the settings, from the palette",
    views: [],
    async run(t) {
      await t.ready();
      await t.command("sync");
      await t.s.waitFor(`${t.shown("settings")} [aria-current="page"]`);
      await t.settled("settings");
      await t.clean();
    },
  },
  viewOnly("ops", "Ops: the project's connections and recipes"),
  viewOnly("ci", "CI/CD: the project's runs (fake gh, none yet)"),
  viewOnly("design", "design system: the tokens of the repository"),
  viewOnly("project", "configuration: the project's page"),
  viewOnly("conversations", "conversations: the list of headless sessions"),
  viewOnly("library", "library: skills, commands and rules of the project"),
  viewOnly("claude", "Claude: the machine's Claude Code (fake 2.1.330)", async (t) => {
    await t.s.wait(async () => (await t.s.text(t.shown("claude")))?.includes("2.1.330"), { what: "the version of Claude Code" });
  }),
  viewOnly("ssh", "SSH keys: the keys of the (empty) home"),
  viewOnly("scheduler", "scheduler: the scheduled tasks"),
  viewOnly("vaultAll", "vault of the machine: offers to create it"),
];
