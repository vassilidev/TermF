// What a developer discovering TermF does, one scenario at a time, each in its own sandbox. `views`: the views of
// `views.ts` (and the pages beside them) the scenario is about; every view of `views.ts` needs one.
import { createHmac } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { text } from "./harness.mjs";
import { Key } from "./webdriver.mjs";

const read = (file) => readFileSync(file, "utf8");
// macOS gives the webview the app's languages, not the environment's: set as for one app in its settings.
const french = { sandbox: { lang: "fr" }, launch: { args: { win32: ["--lang=fr-FR"], darwin: ["-AppleLanguages", "(fr-FR)"] }[process.platform] ?? [] } };

/** RFC 6238 with its defaults (SHA-1, 6 digits, 30 s), computed here to check the app's. */
export function totp(secret, at = Date.now()) {
  const bits = [...secret.replace(/=+$/, "").toUpperCase()].map((c) => "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".indexOf(c).toString(2).padStart(5, "0")).join("");
  const key = Buffer.from(bits.match(/.{8}/g).map((b) => parseInt(b, 2)));
  const counter = Buffer.alloc(8);
  counter.writeBigUInt64BE(BigInt(Math.floor(at / 30_000)));
  const mac = createHmac("sha1", key).update(counter).digest();
  const offset = mac[19] & 15;
  return String((mac.readUInt32BE(offset) & 0x7fffffff) % 1_000_000).padStart(6, "0");
}

/** The lines of terminal `id` once one of them is exactly `line`. */
const showsLine = (t, id, line) =>
  t.s.wait(async () => (await t.terminal(id)).split("\n").some((l) => l.trim() === line), { what: `${line} in terminal ${id}`, timeout: 30_000 });

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

/** Notes what the CTO's thread shows during a turn: the thinking dots, each length of the text as it streams. */
function watchStream(css) {
  const root = document.querySelector(css);
  const seen = (window.__e2e.stream = { thinking: false, lengths: [] });
  const note = () => {
    if (root.querySelector(".thinking")) seen.thinking = true;
    const live = root.querySelector(".text.live")?.textContent.length;
    if (live && seen.lengths.at(-1) !== live) seen.lengths.push(live);
  };
  new MutationObserver(note).observe(root, { subtree: true, childList: true, characterData: true });
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
    name: "new project: one guided setup, nothing written before its last step, then its home",
    views: ["newProject"],
    sandbox: { project: false },
    async run(t) {
      const { s, box } = t;
      const go = ".new-project .wizard footer .go:not([disabled])";
      const at = (step) => `.new-project .wizard .steps li:nth-child(${step}) [aria-current="step"]`;
      const file = join(box.configDir, "projects", "acme.toml");
      await t.ready();
      await t.command("project-new");
      await s.waitFor(t.shown("newProject"));
      // Focus mode: the navigation steps aside while the setup is open.
      check(!(await s.exec(() => !!document.querySelector(".app .nav")?.offsetParent)), "the navigation stays during the setup");
      await s.click(go);
      await s.waitFor(at(2));
      const repo = `#repo-list li${t.sel("data-path", box.acme)}`;
      await s.click(repo);
      await s.waitFor(`${repo}[aria-selected="true"]`);
      // Left for later, then back: the choice is still there.
      await s.exec(() => document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
      await s.waitGone(t.shown("newProject"));
      await t.command("project-new");
      await s.waitFor(at(2));
      await s.waitFor(`${repo}[aria-selected="true"]`);
      for (const step of [3, 4, 5]) {
        await s.click(go);
        await s.waitFor(at(step), { timeout: 30_000 });
        check(!existsSync(file), `written before the last step: ${file}`);
        await t.clean();
      }
      await s.click(go);
      await s.waitFor(`.nav ${t.sel("data-project", "acme")}[aria-current="true"]`, { timeout: 30_000 });
      await s.waitFor(t.shown("home"));
      const toml = read(file);
      check(/name = "acme"/.test(toml) && toml.includes("acme"), `unexpected project file:\n${toml}`);
      // The test command found in package.json is what the team's checks run before delivering.
      check(toml.includes('checks = "npm test"'), `no checks in the project file:\n${toml}`);
      await t.settled("home");
      await t.clean();
    },
  },
  {
    name: "project setup again: opens on the project from the palette, its repositories given",
    views: [],
    async run(t) {
      await t.ready();
      await t.project();
      await t.command("project-setup");
      await t.s.waitFor(t.shown("newProject"));
      // It opens on what TermF understood, the repositories being the project's.
      await t.s.waitFor('.new-project .wizard .steps li:nth-child(2) [aria-current="step"]');
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
      if (!t.windows) {
        // The bridge's token never reaches a terminal (e2e build only; the WebDriver's app has none anyway).
        await t.typeIn(id, "echo token-${TERMF_E2E_TOKEN:-none}" + Key.enter);
        await showsLine(t, id, "token-none");
      }
      await t.clean();
    },
  },
  {
    name: "terminal: off screen, xterm goes; back, it shows what was written meanwhile",
    views: [],
    only: ["linux", "macos"],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      await t.command(`repo-shell-${box.acme}`);
      const id = await t.tile();
      await t.prompt(id);
      // Asks where the cursor is while shown, then waits to be hidden, asks again (TermF answers for the missing
      // view), and once shown again tells whether the replay typed anything into it.
      const cpr = "printf '\\033[6n'; read -rs -d R -t 5";
      const script = [
        `${cpr} first; seq 1 50`,
        'until [ -e "$HOME/hidden" ]; do sleep 0.1; done',
        `${cpr} asked; echo "asked-\${#asked}"; touch "$HOME/written"`,
        'read -rs -t 5 -n 1 late; echo "late-${#late}"',
      ].join("; ");
      await t.typeIn(id, script + Key.enter);
      await s.wait(async () => (await t.terminal(id)).split("\n").some((l) => l.trim() === "50"), { what: "50 in the terminal" });
      await t.view("home");
      await s.wait(() => s.exec((sid) => !document.querySelector(`.tile[data-session="${sid}"] .xterm`), id), {
        what: "the hidden terminal to let xterm go",
      });
      writeFileSync(join(box.home, "hidden"), "");
      await s.wait(() => existsSync(join(box.home, "written")), { what: "the shell to write while hidden", timeout: 30_000 });
      await t.view("terminals");
      const lines = await s.wait(
        async () => {
          const lines = (await t.terminal(id)).split("\n").map((l) => l.trim());
          return lines.includes("50") && lines.some((l) => l.startsWith("late-")) && lines;
        },
        { what: "the scrollback and what was written while hidden", timeout: 30_000 },
      );
      // The answer can come before `read -s` silences the terminal, which then echoes it on that line.
      const asked = lines.find((l) => l.includes("asked-"));
      check(asked?.endsWith("asked-5"), `the cursor position asked while hidden: ${asked}`);
      check(lines.includes("late-0"), `the replay typed into the shell: ${lines.find((l) => l.startsWith("late-"))}`);
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
      await s.press(Key.mod, Key.end);
      await s.keys("Edited by e2e.");
      await s.press(Key.mod, "s");
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
      // The settings open on Claude: the keep-awake choice has its own section.
      const awake = async () => {
        await t.s.click(`${t.shown("settings")} [data-testid="section-awake"]`);
        await t.s.waitFor(auto);
      };
      await t.ready();
      await t.view("settings");
      await awake();
      await t.s.click(auto);
      await t.s.wait(() => /keep_awake = "auto"/.test(read(join(box.configDir, "termf.toml"))), { what: "termf.toml updated" });
      await t.restart();
      await t.ready();
      await t.view("settings");
      await awake();
      check(await t.s.exec((css) => document.querySelector(css)?.checked, auto), "the choice is lost after a restart");
    },
  },
  {
    name: "language: with a French system the interface is in French",
    views: [],
    ...french,
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
  {
    name: "terminals: a split beside the first shell, then one command broadcast to both",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      await t.command(`repo-shell-${box.acme}`);
      const first = await t.tile();
      await t.prompt(first);
      await t.command("split-right");
      await s.click("section.launcher .segmented.agents:not(.place) label:last-child span");
      await s.click(`section.launcher label.row[title=${JSON.stringify(box.acme)}] input`);
      await s.click("section.launcher button.open:not([disabled])");
      const ids = await s.wait(async () => {
        const list = await s.exec(() => [...document.querySelectorAll(".tile[data-session]")].filter((e) => e.getClientRects().length).map((e) => e.dataset.session));
        return list.length === 2 && list;
      }, { what: "two tiles side by side" });
      const second = ids.find((id) => id !== first);
      await t.prompt(second);
      const [a, b] = await s.exec(
        (x, y) => [x, y].map((id) => document.querySelector(`.tile[data-session="${id}"]`).getBoundingClientRect().left),
        first,
        second,
      );
      check(b > a, `the split is not on the right (${a}, ${b})`);
      await t.command("broadcast");
      await s.click("form.broadcast .ticks button:first-child");
      const line = t.windows ? '"b-" + (2+3)' : "echo b-$((2+3))";
      await s.type("form.broadcast input.field", line + Key.enter);
      for (const id of [first, second]) await showsLine(t, id, "b-5");
      await t.clean();
    },
  },
  {
    name: "quick terminal: opens from the palette, runs a command, closes",
    views: [],
    async run(t) {
      const { s } = t;
      await t.ready();
      await t.command("quick");
      await s.waitFor("section.quick.open .xterm-rows");
      await s.wait(async () => /(\$|>)\s*$/.test((await s.exec(() => document.querySelector("section.quick .xterm-rows").innerText)).trimEnd()), {
        what: "the quick terminal's prompt",
        timeout: 30_000,
      });
      await s.exec(() => document.querySelector("section.quick .xterm-helper-textarea").focus());
      await s.keys((t.windows ? '"q-" + 7*6' : "echo q-$((7*6))") + Key.enter);
      await s.wait(async () => (await s.exec(() => document.querySelector("section.quick .xterm-rows").innerText)).split("\n").some((l) => l.trim() === "q-42"), {
        what: "q-42 in the quick terminal",
      });
      await t.command("quick");
      await s.waitGone("section.quick.open");
      await t.clean();
    },
  },
  {
    name: "help and guided tour: both open from the palette, Escape closes the help",
    views: [],
    async run(t) {
      const { s } = t;
      await t.ready();
      await t.project();
      await t.command("help");
      await s.waitFor('dialog.help[open]');
      await t.clean();
      await s.press(Key.escape);
      // Gone from the page, not only closed: the app itself let go of the help.
      await s.wait(() => s.exec(() => !document.querySelector("dialog.help")), { what: "the help to go away" });
      await t.command("tour");
      await s.waitFor('[aria-labelledby="tour-title"]');
      for (let step = 0; step < 4; step++) await s.click('[aria-labelledby="tour-title"] footer .primary');
      await s.waitGone('[aria-labelledby="tour-title"]');
      await t.clean();
    },
  },
  {
    name: "vault: the TOTP code is the one RFC 6238 gives, a wrong password is refused",
    views: [],
    async run(t) {
      const { s } = t;
      const pane = t.shown("vault");
      const password = "correct horse battery staple";
      const secret = "JBSWY3DPEHPK3PXP";
      await t.ready();
      await t.project();
      await t.view("vault");
      const field = `${pane} form.unlock input[type=password]`;
      for (const i of [0, 1]) await s.type(field, password, i);
      await s.click(`${pane} form.unlock button[type=submit]:not([disabled])`);
      await s.click('[data-testid="vault-new"]');
      await s.type(`${pane} form.form input.field`, "Acme 2FA");
      await s.type(`${pane} #vault-password`, "s3cret-e2e");
      await s.type(`${pane} #vault-totp`, secret);
      await s.click(`${pane} form.form button[type=submit]:not([disabled])`);
      await s.click(`${pane} .list .pick`);
      const code = await s.wait(async () => (await s.text(`${pane} .totp .code`))?.replace(/\s/g, "").match(/^\d{6}$/)?.[0], { what: "the TOTP code" });
      const now = Date.now();
      check([now - 30_000, now, now + 30_000].some((at) => totp(secret, at) === code), `the app shows ${code}, RFC 6238 gives ${totp(secret, now)}`);
      await s.click('[data-testid="vault-lock"]');
      await s.type(`${pane} form.unlock input[type=password]`, "not the password" + Key.enter);
      await s.wait(() => s.exec((css) => !!document.querySelector(css)?.textContent.trim(), `${pane} form.unlock [role=alert], ${pane} form.unlock .error`), {
        what: "the refusal of a wrong password",
      });
      check(!(await s.exec((css) => !!document.querySelector(css), `${pane} .list`)), "the vault opened with a wrong password");
      await s.exec((css) => (document.querySelector(css).value = ""), `${pane} form.unlock input[type=password]`);
      await s.type(`${pane} form.unlock input[type=password]`, password + Key.enter);
      await s.waitFor(`${pane} .list`);
      await t.clean({ allow: [/password|mot de passe/i] });
    },
  },
  {
    name: "git: a change stashed, then applied back",
    views: [],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("git");
      writeFileSync(join(box.acme, "README.md"), "# Acme API\n\nStashed by e2e.\n");
      await t.ready();
      await t.project();
      await t.view("git");
      await s.click(`${pane} button.tree.dirty`);
      await s.click(`${pane} button.stash`);
      await s.type(`${pane} form.inline-form input.field`, "e2e stash");
      await s.click(`${pane} form.inline-form button.primary:not([disabled])`);
      await s.wait(() => box.git("status", "--porcelain") === "" && box.git("stash", "list").includes("e2e stash"), { what: "the stash" });
      const pick = `${pane} button.pick`;
      const index = await s.wait(
        () => s.exec((css) => [...document.querySelectorAll(css)].findIndex((b) => b.textContent.includes("e2e stash")) + 1, pick),
        { what: "the stash in the list" },
      );
      await s.click(pick, index - 1);
      await s.click(`${pane} dl.meta + .actions .btn.primary:not([disabled])`);
      await s.wait(() => read(join(box.acme, "README.md")).includes("Stashed by e2e.") && box.git("stash", "list") === "", {
        what: "the change back and the stash gone",
      });
      await t.settled("git");
      await t.clean();
    },
  },
  {
    name: "git: a merge conflict resolved with the other branch's side, then the merge continued",
    views: [],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("git");
      box.git("checkout", "-q", "-b", "feature");
      writeFileSync(join(box.acme, "README.md"), "# Acme API\n\nFrom the feature branch.\n");
      box.git("commit", "-qam", "Feature README");
      box.git("checkout", "-q", "main");
      writeFileSync(join(box.acme, "README.md"), "# Acme API\n\nFrom main.\n");
      box.git("commit", "-qam", "Main README");
      try {
        box.git("merge", "feature");
      } catch {
        // The conflict this scenario is about.
      }
      check(box.git("diff", "--name-only", "--diff-filter=U").trim() === "README.md", "no conflict to resolve");
      await t.ready();
      await t.project();
      await t.view("git");
      await s.click(`${pane} .op .op-actions button:first-child`);
      await s.click(`${pane} li.conflict .conflict-actions button:nth-child(2)`);
      await s.wait(() => box.git("diff", "--name-only", "--diff-filter=U").trim() === "", { what: "the conflict resolved" });
      check(read(join(box.acme, "README.md")).includes("From the feature branch."), "the other branch's side is not the one kept");
      await s.click(`${pane} .op .op-actions .btn.primary:not([disabled])`);
      await s.wait(() => box.git("log", "-1", "--format=%P").trim().split(" ").length === 2, { what: "the merge commit" });
      await t.settled("git");
      await t.clean();
    },
  },
  {
    name: "SSH keys: a new ed25519 key written in the throwaway home",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.command("ssh-generate");
      await s.click(`${t.shown("ssh")} section.panel button[type=submit]:not([disabled])`);
      await s.wait(() => existsSync(join(box.home, ".ssh", "id_ed25519")) && existsSync(join(box.home, ".ssh", "id_ed25519.pub")), {
        what: "~/.ssh/id_ed25519 and its .pub",
      });
      check(read(join(box.home, ".ssh", "id_ed25519.pub")).startsWith("ssh-ed25519 "), "the public key is not an ed25519 one");
      await t.settled("ssh");
      await t.clean();
    },
  },
  {
    name: "scheduler: a task created in the form shows in the list",
    views: [],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("scheduler");
      await t.ready();
      await t.project();
      await t.view("scheduler");
      await s.click(`${pane} .btn.primary`);
      await s.type(`${pane} form.form input.field`, "Nightly e2e");
      await t.choose(`${pane} form.form select.field`, box.acme);
      await s.type(`${pane} form.form textarea.prompt`, "Say hello");
      await s.click(`${pane} form.form button[type=submit]:not([disabled])`);
      await s.wait(async () => (await s.text(`${pane}`))?.includes("Nightly e2e") && !(await s.exec((c) => !!document.querySelector(c), `${pane} form.form`)), {
        what: "the task in the list",
      });
      await t.clean();
    },
  },
  {
    name: "CTO: a message to the project's CTO reaches Claude Code (fake)",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      await t.view("home");
      await s.type(`${t.shown("home")} form.composer textarea`, "Add a health check endpoint");
      await s.click(`${t.shown("home")} form.composer button[type=submit]:not([disabled])`);
      // The message goes to a headless session on its standard input; its answer comes back into the conversation.
      await s.wait(async () => (await s.text(t.shown("home")))?.includes("Fake Claude received: Add a health check endpoint"), {
        what: "the CTO's answer in the conversation",
      });
      check(/claude \["-p","--input-format","stream-json"/.test(read(box.calls)), "the CTO is not a headless session");
      await t.settled("home");
      await t.clean();
    },
  },
  {
    name: "CTO: the answer streams word by word and the tool at work is said in plain words",
    views: [],
    async run(t) {
      const { s } = t;
      await t.ready();
      await t.project();
      await t.view("home");
      const home = t.shown("home");
      await s.exec(watchStream, home);
      // "slowly": the fake streams its words, then runs `npm test` for a while (e2e/fakes/claude.mjs).
      await s.type(`${home} form.composer textarea`, "Check the tests slowly");
      await s.click(`${home} form.composer button[type=submit]:not([disabled])`);
      const running = await s.wait(() => s.exec((css) => document.querySelector(`${css} .tool .state`)?.closest("summary")?.textContent, home), {
        what: "the Bash call shown at work",
      });
      check(/Runs\s*npm test/.test(running), `the call is not said in plain words: ${running}`);
      await s.wait(async () => (await s.text(home))?.includes("The tests pass."), { what: "the end of the answer" });
      const seen = await s.exec(() => window.__e2e.stream);
      check(seen.thinking, "no thinking sign before the first word");
      check(seen.lengths.length >= 3, `the answer came whole, not word by word: ${JSON.stringify(seen.lengths)}`);
      check(!(await s.exec((css) => !!document.querySelector(`${css} .tool .state`), home)), "the call still shows as running");
      await t.settled("home");
      await t.clean();
    },
  },
  {
    name: "expert: a question in its conversation finds what was committed just before",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      writeFileSync(join(box.acme, "DECISIONS.md"), "# Decisions\n\nA failed webhook is replayed 4 times.\n");
      box.git("add", "-A");
      box.git("commit", "-q", "-m", "Record the webhook decision");
      await t.command("expert");
      const panel = t.shown("conversations");
      await s.type(`${panel} form.composer textarea`, "How many times is a webhook replayed?");
      await s.click(`${panel} form.composer button[type=submit]:not([disabled])`);
      await s.wait(async () => (await s.text(panel))?.includes("Fake Claude received: How many times"), { what: "the expert's answer" });
      // Long before the first background indexing (60 s after the start): only the question brought the memory up to date.
      const status = await s.exec(() => window.__TAURI_INTERNALS__.invoke("memory_status", { project: "acme" }));
      const docs = status.sources.find((x) => x.source === "docs").docs;
      check(docs === 2, `the expert searched a memory not brought up to date: ${docs} documents instead of README.md and DECISIONS.md`);
      await t.clean();
    },
  },
  {
    name: "files: find a file by name, then a text across the repository",
    views: [],
    async run(t) {
      const { s } = t;
      const pane = t.shown("files");
      await t.ready();
      await t.project();
      await t.view("files");
      await t.command("file");
      await s.type("dialog.finder input", "app.js");
      await s.wait(async () => (await s.text("#finder-0"))?.includes("app.js"), { what: "app.js found" });
      await s.press(Key.enter);
      await s.waitGone("dialog.finder[open]");
      await s.wait(async () => (await s.text(`${pane} .cm-content`))?.includes("greet"), { what: "src/app.js in the editor" });
      await s.click(`${pane} .segmented button:nth-child(2)`);
      // Until the search answers, a skeleton stands for its hits.
      await s.exec((css) => {
        const root = document.querySelector(css);
        window.__e2e.searchPending = false;
        new MutationObserver(() => (window.__e2e.searchPending ||= !!root.querySelector("[data-skeleton]"))).observe(root, { subtree: true, childList: true });
      }, `${pane} .search`);
      await s.type(`${pane} .search input.field`, "The API of Acme");
      await s.wait(async () => (await s.text(`${pane} .search .hit`))?.includes("The API of Acme"), { what: "the README line found" });
      check(await s.exec(() => window.__e2e.searchPending), "no loading state while the search ran");
      check(!(await s.exec((css) => !!document.querySelector(css), `${pane} .search [data-skeleton]`)), "the skeleton stays after the hits");
      await t.clean();
    },
  },
  {
    name: "CI/CD: a failed run on main listed from GitHub (fake gh)",
    views: [],
    async run(t) {
      const { s, box } = t;
      const run = {
        id: 4242,
        name: "CI",
        display_title: "Fix the login button on Safari",
        run_number: 12,
        run_attempt: 1,
        event: "push",
        status: "completed",
        conclusion: "failure",
        head_branch: "main",
        head_sha: box.git("rev-parse", "HEAD").trim(),
        actor: { login: "alex" },
        pull_requests: [],
        html_url: "https://github.com/acme/api/actions/runs/4242",
        created_at: "2026-11-22T09:00:00Z",
        updated_at: "2026-11-22T09:05:00Z",
        run_started_at: "2026-11-22T09:00:05Z",
      };
      writeFileSync(join(box.root, "gh-runs.json"), JSON.stringify([run]));
      await t.ready();
      await t.project();
      await t.view("ci");
      await s.wait(async () => (await s.text(t.shown("ci")))?.includes("Fix the login button on Safari"), { what: "the run in the list" });
      await t.clean();
    },
  },
  {
    name: "Ops: an SSH connection added to the project, kept in its config",
    views: [],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("ops");
      await t.ready();
      await t.project();
      await t.view("ops");
      await s.click(`${pane} .btn.primary`);
      // Past the database URL at the top: the name, then the first field of an SSH connection, its host.
      await s.type(`${pane} form.form .grid input.field`, "acme-staging", 0);
      await s.type(`${pane} form.form .grid input.field`, "staging.acme.test", 1);
      await s.click(`${pane} form.form button[type=submit]:not([disabled])`);
      await s.waitGone(`${pane} form.form`);
      await s.wait(async () => (await s.text(`${pane}`))?.includes("acme-staging"), { what: "the connection in the list" });
      const saved = read(join(box.configDir, "projects", "acme.toml"));
      check(saved.includes("acme-staging") && saved.includes("staging.acme.test"), `the connection is not in acme.toml:\n${saved}`);
      await t.clean();
    },
  },
  {
    name: "updates: a newer release on a fake server is offered, with its notes",
    views: [],
    async run(t) {
      const { s, box } = t;
      // The release list as GitHub gives it (fake curl), its latest.json on a local server, as tauri-action makes it.
      const server = createServer((req, res) => {
        const url = `http://127.0.0.1:${server.address().port}/termf.tar.gz`;
        const platforms = Object.fromEntries(
          ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "linux-aarch64", "windows-x86_64", "windows-aarch64"].map((p) => [p, { signature: "e2e", url }]),
        );
        res.writeHead(req.url === "/latest.json" ? 200 : 404, { "content-type": "application/json" });
        res.end(JSON.stringify({ version: "9.9.9", notes: "From latest.json", pub_date: "2026-11-21T10:00:00Z", platforms }));
      });
      await new Promise((r) => server.listen(0, "127.0.0.1", r));
      try {
        const manifest = `http://127.0.0.1:${server.address().port}/latest.json`;
        writeFileSync(
          join(box.root, "releases.json"),
          JSON.stringify([
            { tag_name: "v9.9.9", draft: false, prerelease: false, body: "Faster terminals in e2e.", assets: [{ name: "latest.json", browser_download_url: manifest }] },
            { tag_name: "v0.1.0", draft: false, prerelease: true, body: "", assets: [] },
          ]),
        );
        await t.ready();
        await t.command("release-check");
        await s.waitFor("#update-title");
        const shown = await s.text(t.shown("settings"));
        check(shown.includes("9.9.9") && shown.includes("Faster terminals in e2e."), `the offer does not show the release:\n${shown.slice(0, 600)}`);
        check(/curl \[.*api\.github\.com\/repos\/vassilidev\/TermF\/releases/.test(read(box.calls)), "the releases were not asked to curl");
        await t.clean();
      } finally {
        server.close();
      }
    },
  },
  {
    name: "updates: up to date when no release is newer",
    views: [],
    async run(t) {
      const { s, box } = t;
      writeFileSync(join(box.root, "releases.json"), JSON.stringify([{ tag_name: "v0.0.9", draft: false, prerelease: false, assets: [] }]));
      await t.ready();
      await t.command("release-check");
      await s.wait(async () => (await s.text(t.shown("settings")))?.includes(text("en", "releaseUpToDate")), { what: "up to date" });
      check(!(await s.exec(() => !!document.querySelector("#update-title"))), "an update is offered");
      await t.clean();
    },
  },
  {
    name: "Remote Control: Claude Code (fake) opened from the launcher with Remote Control on",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.project();
      await t.command("launcher");
      await s.click("section.launcher .segmented.agents:not(.place) label:first-child span");
      await s.click(`section.launcher label.row[title=${JSON.stringify(box.acme)}] input`);
      await s.click("section.launcher .rc input.switch:not([disabled])");
      await s.click("section.launcher button.open:not([disabled])");
      await s.wait(() => existsSync(box.calls) && /claude \[[^\n]*"--remote-control"/.test(read(box.calls)), { what: "claude started with --remote-control" });
      const id = await t.tile();
      await s.waitFor(`.tile[data-session="${id}"] [data-status="idle"]`, { timeout: 30_000 });
      await t.clean();
    },
  },
  {
    name: "library: a skill switched off for the project, written in its file",
    views: [],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("library");
      const file = join(box.configDir, "projects", "acme.toml");
      await t.ready();
      await t.project();
      await t.view("library");
      const row = `${pane} .list li.row`;
      const { name, on } = await s.wait(() =>
        s.exec((css) => {
          const li = document.querySelector(css);
          return li && { name: li.querySelector(".name").textContent, on: li.querySelector("input.switch").checked };
        }, row),
      );
      const before = read(file);
      await s.click(`${row} input.switch:not([disabled])`);
      await s.wait(() => s.exec((css, was) => document.querySelector(`${css} input.switch`)?.checked === !was, row, on), { what: "the switch flipped" });
      await s.wait(() => read(file) !== before && read(file).includes(name), { what: `${name} in acme.toml` });
      await t.settled("library");
      await t.clean();
    },
  },
  {
    name: "conventions: what Claude Code (fake) reads shows while it proposes",
    views: [],
    async run(t) {
      const { s } = t;
      const pane = t.shown("library");
      await t.ready();
      await t.project();
      await t.view("library");
      await s.click(`${pane} [data-tab="conventions"]`);
      await s.type(`${pane} .conventions form.new input`, "style");
      await s.click(`${pane} .conventions form.new button[type=submit]:not([disabled])`);
      await s.type(`${pane} .conventions textarea.text`, "Read the README slowly.");
      await s.click(`${pane} .conventions .step .row-end button.primary:not([disabled])`);
      const read = await s.wait(() => s.exec((css) => document.querySelector(`${css} .conventions .feed li`)?.textContent, pane), {
        what: "the file read shown while Claude Code proposes",
      });
      check(/README\.md/.test(read), `the read is not said: ${read}`);
      // The fake's empty answer proposes nothing, which the page says.
      await s.waitGone(`${pane} .conventions .notice.working`);
      await t.clean({ allow: [/proposed nothing/] });
    },
  },
  {
    name: "scout: each library item to pick reads on one line, its name whole",
    views: [],
    async run(t) {
      const { s } = t;
      const pane = t.shown("library");
      await t.ready();
      await t.project();
      await t.view("library");
      await s.exec((css, label) => [...document.querySelectorAll(`${css} button`)].find((b) => b.textContent.trim() === label).click(), pane, text("en", "scoutOpen"));
      await s.click(`${pane} [role=radio]:not([aria-checked=true])`);
      const rows = await s.wait(() =>
        s.exec((css) => {
          const rows = [...document.querySelectorAll(`${css} .picker label`)].slice(0, 20).map((l) => [l.querySelector(".item-name").textContent, l.getBoundingClientRect().height]);
          return rows.length && rows;
        }, pane),
      );
      const tall = rows.filter(([, h]) => h > 40);
      check(rows.length && !tall.length, `names broken over several lines: ${JSON.stringify(tall)}`);
      await t.clean();
    },
  },
  {
    name: "Claude: two artifacts deleted after one confirmation, then one without any, no tile (fake 2.1.330)",
    views: [],
    async run(t) {
      const { s, box } = t;
      const pane = t.shown("claude");
      const now = Math.floor(Date.now() / 1000);
      const artifacts = ["Acme launch page", "Acme pricing", "Acme roadmap"].map((title, i) => ({
        id: `acme${i}`,
        url: `https://claude.ai/artifact/acme${i}`,
        title,
        owner: "me",
        seen: now,
      }));
      // Listed just now: no automatic listing during the run.
      writeFileSync(join(box.dataDir, "artifacts.json"), JSON.stringify({ artifacts, listed: now }));
      const titles = () => s.exec((css) => [...document.querySelectorAll(`${css} td.title .name`)].map((e) => e.textContent), pane);
      const gone = (n) => s.wait(async () => (await titles()).length === n, { what: `${n} artifact(s) left`, timeout: 30_000 });
      const noTile = async () => check(!(await s.exec(() => document.querySelector(".tile[data-session]"))), "a tile of the hidden session showed");
      await t.ready();
      await t.command("claude-artifacts");
      await s.wait(async () => (await titles()).length === 3, { what: "the three artifacts" });
      for (const i of [0, 1]) await s.click(`${pane} td.check input`, i);
      await s.click(`${pane} .bulk .btn.danger-quiet`);
      await s.waitFor("dialog.confirm[open]");
      await s.click("dialog.confirm .btn.danger");
      await gone(1);
      await noTile();
      // Asked in the settings not to be asked: no dialog either.
      await t.view("settings");
      await s.click(t.sel("data-testid", "section-claude"));
      const skip = `${t.shown("settings")} input.switch`;
      await s.waitFor(`${skip}:not([disabled])`);
      const label = text("en", "claudeRedesignSkipConfirm");
      await s.click(skip, await s.exec((css, l) => [...document.querySelectorAll(css)].findIndex((e) => e.closest("label")?.textContent.trim() === l), skip, label));
      await s.wait(() => /delete_without_confirmation = true/.test(read(join(box.configDir, "termf.toml"))), { what: "the choice in termf.toml" });
      await t.command("claude-artifacts");
      await s.wait(async () => (await titles()).length === 1, { what: "the last artifact" });
      await s.click(`${pane} td.actions .icon-btn.delete`);
      await gone(0);
      check(!(await s.exec(() => document.querySelector("dialog.confirm[open]"))), "asked to confirm although told not to");
      await noTile();
      await t.clean();
    },
  },
  {
    name: "shortcuts: the palette moved to Mod+Shift+Y, saved in termf.toml, and it opens the palette",
    views: [],
    async run(t) {
      const { s, box } = t;
      await t.ready();
      await t.command("shortcuts");
      await s.click("#key-palette");
      await s.waitFor("#key-palette.rec");
      await s.press(Key.mod, Key.shift, "y");
      await s.wait(() => /palette = ".*\+Y"/i.test(read(join(box.configDir, "termf.toml"))), { what: "the shortcut in termf.toml" });
      await t.clean();
      await s.exec(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
      await s.press(Key.mod, Key.shift, "y");
      await s.waitFor("dialog.palette");
    },
  },
  {
    name: "French: every view of the project and of This machine, no English text",
    views: [],
    ...french,
    async run(t) {
      await t.ready();
      await t.project();
      for (const id of t.views) await t.view(id);
    },
  },
  viewOnly("ops", "Ops: the project's connections and recipes"),
  viewOnly("ci", "CI/CD: the project's runs (fake gh, none yet)"),
  {
    name: "design system: the repository's own font, its specimen, its components and every button of the view",
    views: ["design"],
    async run(t) {
      await t.ready();
      await t.project();
      const panel = t.shown("design");
      const composer = () => t.s.exec(() => document.querySelector('[data-panel="home"] .composer textarea')?.value ?? "");
      const asked = async (what) => {
        await t.s.waitFor(t.shown("home"));
        await t.s.wait(async () => (await composer()).includes(what), { what: `${what} in the CTO's message box`, timeout: 30_000 });
      };
      const section = async (id) => {
        await t.s.click(`${panel} [data-testid="section-${id}"]`);
        await t.clean();
      };

      // No tokens yet: the empty state says how to add some, or asks the brand role.
      await t.view("design");
      await t.s.waitFor(`${panel} .how .example`);
      await t.s.click(`${panel} .empty .btn.primary`);
      await asked("brand");

      // A repository with tokens, a font file of its own, a web font, components.
      const acme = t.box.acme;
      for (const dir of ["src/components", "static/fonts"]) mkdirSync(join(acme, dir), { recursive: true });
      copyFileSync(new URL("../node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2", import.meta.url), join(acme, "static/fonts/AcmeGrotesk.woff2"));
      writeFileSync(join(acme, "package.json"), '{ "name": "acme-web", "private": true }\n');
      writeFileSync(
        join(acme, "index.html"),
        '<link href="https://fonts.googleapis.com/css2?family=Fraunces:wght@600&amp;display=swap" rel="stylesheet">\n',
      );
      writeFileSync(
        join(acme, "src/app.css"),
        `@font-face { font-family: "Acme Grotesk"; src: url("../static/fonts/AcmeGrotesk.woff2") format("woff2"); font-weight: 100 900; }
:root {
  --bg: #ffffff; --text: #16181d; --muted: #5d626d; --border: #e3e5ea; --primary: #3b5bdb; --primary-foreground: #ffffff;
  --brand-100: #dbe4ff; --brand-500: #4c6ef5; --brand-900: #1c2f8f;
  --font-sans: "Acme Grotesk", system-ui, sans-serif; --font-display: "Fraunces", Georgia, serif;
  --text-sm: 14px; --text-base: 16px; --text-2xl: 28px;
  --space-2: 8px; --space-4: 16px; --radius: 8px; --radius-lg: 16px;
  --shadow-1: 0 1px 2px rgb(0 0 0 / 0.08); --ease-out: cubic-bezier(0.22, 1, 0.36, 1);
}
@media (prefers-color-scheme: dark) { :root { --bg: #101216; --text: #eceef2; --border: #2a2d35; } }
body { font-family: var(--font-sans); color: var(--text); background: var(--bg); }
h1, h2 { font-family: var(--font-display); }
.card { padding: var(--space-4); border-radius: var(--radius-lg); box-shadow: var(--shadow-1); transition: transform 200ms var(--ease-out); color: #16181e; }
`,
      );
      writeFileSync(
        join(acme, "src/components/Button.tsx"),
        'export function Button({ variant }: { variant?: "primary" | "outline" | "ghost" }) { return <button className={variant} />; }\n',
      );
      writeFileSync(join(acme, "src/components/Badge.tsx"), 'export const Badge = ({ tone }: { tone?: "success" | "warning" }) => <span className={tone} />;\n');
      writeFileSync(join(acme, "src/components/Card.tsx"), 'export function Card() { return <div className="card" />; }\n');
      writeFileSync(join(acme, "src/components/Chart.tsx"), "export function Chart() { return <svg />; }\n");
      writeFileSync(join(acme, "src/App.tsx"), "export default function App() { return <Card><Badge /><Button /><Chart /></Card>; }\n");

      // Back on the view, which reads again; then "Refresh".
      await t.view("design");
      await t.s.exec((css) => document.querySelector(`${css} svg.lucide-refresh-cw`).closest("button").click(), panel);
      await t.s.waitFor(`${panel} .ramp`);
      await t.settled("design");
      check((await t.s.exec((css) => document.querySelectorAll(`${css} .swatch`).length, panel)) >= 3, "the colors are not shown");
      await t.clean();

      // Typography: the repository's font file drawn under a name of its own, the web font left for the user to fetch.
      await section("type");
      await t.s.wait(
        () => t.s.exec(() => [...document.fonts].some((f) => f.family.includes("termf-design-") && f.status === "loaded")),
        { what: "the repository's font to load in the window" },
      );
      await t.s.waitFor(`${panel} .specimen .status svg.lucide-circle-check`);
      const drawn = await t.s.exec((css) => getComputedStyle(document.querySelector(`${css} .proof`)).fontFamily, panel);
      check(drawn.includes("termf-design-"), `the specimen is not drawn with the repository's font: ${drawn}`);
      check(!(await t.s.exec(() => getComputedStyle(document.body).fontFamily)).includes("termf-design-"), "the project's font changed TermF's");
      await t.s.waitFor(`${panel} .pairing h1`);
      await t.s.waitFor(`${panel} .scale li`);
      // No network in the sandbox: fetching the web font fails, said plainly.
      await t.s.click(`${panel} .status .btn`);
      await t.s.waitFor(`${t.shown("design")} .banners`);
      await t.s.click(`${panel} .banners button`);
      await t.s.waitGone(`${panel} .banners`);
      // The preview theme, both ways.
      await t.s.click(`${panel} .head .segmented button`, 1);
      await t.s.click(`${panel} .head .segmented button`, 0);
      await t.clean();

      await section("spacing");
      await t.s.waitFor(`${panel} .bar`);
      await section("depth");
      await t.s.waitFor(`${panel} .lifts .lift`);
      await t.s.click(`${panel} .rows .icon-btn`);
      await t.s.waitFor(`${panel} .dot.moved`);

      await section("components");
      check((await t.s.exec((css) => document.querySelectorAll(`${css} .gallery .stage`).length, panel)) >= 3, "the base components are not drawn");
      await t.s.waitFor(`${panel} .others`);
      await t.s.click(`${panel} .others .src`);
      await t.s.waitFor(t.shown("files"));
      await t.view("design");

      // Inconsistencies: the filters, a finding opened, its place in the editor, then the request to the team.
      await section("issues");
      const filters = await t.s.exec((css) => document.querySelectorAll(`${css} .head .segmented button`).length, panel);
      for (let i = filters - 1; i >= 0; i--) await t.s.click(`${panel} .head .segmented button`, i);
      await t.s.click(`${panel} .finding .line`);
      await t.s.click(`${panel} .place`);
      await t.s.waitFor(t.shown("files"));
      await t.view("design");
      await t.s.click(`${panel} .bulk input[type=checkbox]`);
      await t.s.click(`${panel} .bulk .btn.primary`);
      await asked("app.css");

      await t.view("design");
      await section("brand");
      await t.s.click(`${panel} .empty .btn.primary`);
      await asked("brand/guidelines.html");
    },
  },
  {
    name: "components: the design system's page, its list, toast, dialog and side panel at work",
    views: [],
    async run(t) {
      await t.ready();
      await t.command("ui-components");
      const page = t.shown("components");
      await t.s.waitFor(`${page} [data-testid="tabular"]`);
      await t.clean();

      // The list: the arrows move the selection, a right click opens the row's actions.
      await t.s.click(`${page} [role=listbox] [role=option]`);
      await t.s.press("\uE015");
      await t.s.wait(async () => (await t.s.exec((c) => document.querySelector(`${c} [role=option][aria-selected=true]`)?.dataset.key, page)) === "front", {
        what: "the second row selected",
      });
      await t.s.exec((c) => {
        const row = document.querySelector(`${c} [role=option][aria-selected=true]`);
        row.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 300, clientY: 300 }));
      }, page);
      await t.s.waitFor(".popover [role=menuitem]");
      await t.s.press(Key.escape);
      await t.s.waitGone(".popover [role=menuitem]");

      // A button inside a row keeps its click and its keys: the selection and the toasts do not move.
      const picked = () => t.s.exec((c) => document.querySelector(`${c} [role=option][aria-selected=true]`)?.dataset.key, page);
      await t.s.click(`${page} ${t.sel("data-testid", "ui-row-ask")}`, 2);
      await t.s.exec((c) => document.querySelectorAll(`${c} [data-testid="ui-row-ask"]`)[0].focus(), page);
      await t.s.press("");
      await t.s.press(Key.enter);
      if ((await picked()) !== "front") throw new Error(`a row's button moved the selection to ${await picked()}`);
      if (await t.s.exec(() => document.querySelectorAll(".toaster .toast").length)) throw new Error("Enter on a row's button opened the row");

      // A toast and its Undo.
      await t.s.click(`${page} ${t.sel("data-testid", "ui-toast")}`);
      await t.s.waitFor(".toaster .toast .btn");
      await t.s.click(".toaster .toast .btn");
      await t.s.waitFor(".toaster .toast.info");

      // The dialog says the loss on its button, and Keep closes it.
      await t.s.click(`${page} ${t.sel("data-testid", "ui-dialog")}`);
      await t.s.waitFor("dialog.confirm[open]");
      await t.s.click("dialog.confirm .answers .btn:not(.danger)");
      await t.s.waitGone("dialog.confirm[open]");

      // The side panel folds away, opens again with its width kept.
      const width = () => t.s.exec((c) => document.querySelector(`${c} aside.side [role=separator]`)?.getAttribute("aria-valuenow"), page);
      const before = await width();
      await t.s.click(`${page} aside.side header .icon-btn`);
      await t.s.waitGone(`${page} aside.side`);
      await t.s.click(`${page} ${t.sel("data-testid", "ui-side")}`);
      await t.s.waitFor(`${page} aside.side`);
      if ((await width()) !== before) throw new Error("the side panel's width was not kept");
      // Its edge widens it from the keyboard, and the width is stored for the next launch.
      await t.s.click(`${page} aside.side [role=separator]`);
      await t.s.press("\uE012");
      const wider = String(Number(before) + 16);
      await t.s.wait(async () => (await width()) === wider, { what: "the panel 16 px wider" });
      if ((await t.s.exec(() => localStorage.getItem("termf.side.ui-f1-demo"))) !== wider) throw new Error("the side panel's width was not stored");
      // A drag of its edge cut short (pointercancel) ends there: stored once, the pointer's later moves ignored.
      const dragged = await t.s.exec((c) => {
        const grip = document.querySelector(`${c} aside.side [role=separator]`);
        const x = grip.parentElement.getBoundingClientRect().left;
        const fire = (type, clientX) => grip.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 7, clientX }));
        fire("pointerdown", x);
        fire("pointermove", x - 30);
        const during = localStorage.getItem("termf.side.ui-f1-demo");
        fire("pointercancel", x - 30);
        const after = localStorage.getItem("termf.side.ui-f1-demo");
        fire("pointermove", x - 100);
        return { during, after };
      }, page);
      const dragWidth = String(Number(wider) + 30);
      await t.s.wait(async () => (await width()) === dragWidth, { what: "the panel 30 px wider after the drag" });
      if (dragged.during !== wider || dragged.after !== dragWidth) throw new Error(`the drag's width was not stored once at its end: ${JSON.stringify(dragged)}`);
      await t.clean();
    },
  },
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
