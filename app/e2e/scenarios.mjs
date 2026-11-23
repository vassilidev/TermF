// What a developer discovering TermF does, one scenario at a time, each in its own sandbox. `views`: the views of
// `views.ts` (and the pages beside them) the scenario is about; every view of `views.ts` needs one.
import { createHmac } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
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
      await s.type(`${pane} .search input.field`, "The API of Acme");
      await s.wait(async () => (await s.text(`${pane} .search .hit`))?.includes("The API of Acme"), { what: "the README line found" });
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
      await s.type(`${pane} form.form input.field`, "acme-staging", 0);
      await s.type(`${pane} form.form input.field`, "staging.acme.test", 1);
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
