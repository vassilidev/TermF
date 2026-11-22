// A throwaway world for one run of the app: home, config, data, a git repository "acme", and fake `claude` and
// `gh` first on the PATH. Nothing of the machine's own config, keyring or accounts is reachable from it.
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const windows = process.platform === "win32";
const here = dirname(fileURLToPath(import.meta.url));
const fakes = join(here, "fakes");

/**
 * Windows reads the profile folders from the system, whatever the environment says: there the app's config and
 * data are the account's own, so the runs only go on a throwaway CI machine, which they wipe before each run.
 */
function windowsDirs() {
  const { APPDATA, LOCALAPPDATA } = process.env;
  return {
    configDir: join(APPDATA, "termf"),
    dataDir: join(APPDATA, "dev.termf.app"),
    webviewDir: join(LOCALAPPDATA, "dev.termf.app"),
  };
}

export function assertSafe() {
  if (windows && !process.env.CI) {
    throw new Error("On Windows the app's folders cannot be redirected: run the end-to-end tests on a CI machine (CI=true).");
  }
}

/** A tiny program that runs `node <fakes>/<its name>.mjs`, since Windows only finds `.exe` files on the PATH. */
let shim;
function windowsShim() {
  if (shim) return shim;
  const cache = mkdtempSync(join(tmpdir(), "termf-e2e-shim-"));
  const src = join(cache, "shim.rs");
  writeFileSync(
    src,
    `use std::process::{Command, exit};
fn main() {
    let exe = std::env::current_exe().unwrap();
    let script = std::path::Path::new(${JSON.stringify(fakes)}).join(format!("{}.mjs", exe.file_stem().unwrap().to_string_lossy()));
    let status = Command::new(${JSON.stringify(process.execPath)}).arg(script).args(std::env::args_os().skip(1)).status();
    exit(status.map_or(1, |s| s.code().unwrap_or(1)));
}
`,
  );
  shim = join(cache, "shim.exe");
  execFileSync("rustc", ["-O", "-o", shim, src], { stdio: "inherit" });
  return shim;
}

function git(cwd, env, ...args) {
  execFileSync("git", args, { cwd, env, stdio: "ignore" });
}

/** `lang`: the system language the app sees ("en" or "fr"). `project`: an "acme" project with the repository. */
export function createSandbox({ lang = "en", project = true, dark = false } = {}) {
  // The long form: Windows gives the temporary folder as RUNNER~1, which the app would not show the same.
  const root = mkdtempSync(join(realpathSync.native(tmpdir()), "termf-e2e-"));
  const home = join(root, "home");
  const bin = join(root, "bin");
  const code = join(home, "code");
  const acme = join(code, "acme");
  const dirs = windows
    ? windowsDirs()
    : { configDir: join(home, ".config", "termf"), dataDir: join(home, ".local", "share", "dev.termf.app") };
  if (windows) for (const d of Object.values(dirs)) rmSync(d, { recursive: true, force: true, maxRetries: 10 });
  // Child processes (git, shells, the fakes) follow these; the app itself reads the system's, so the config goes to both.
  const appData = { APPDATA: join(root, "appdata", "roaming"), LOCALAPPDATA: join(root, "appdata", "local") };
  const configDirs = windows ? [dirs.configDir, join(appData.APPDATA, "termf")] : [dirs.configDir];
  for (const d of [bin, acme, ...configDirs.map((c) => join(c, "projects"))]) mkdirSync(d, { recursive: true });

  for (const name of ["claude", "gh"]) {
    if (windows) copyFileSync(windowsShim(), join(bin, `${name}.exe`));
    else {
      const file = join(bin, name);
      writeFileSync(file, `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} ${JSON.stringify(join(fakes, `${name}.mjs`))} "$@"\n`);
      chmodSync(file, 0o755);
    }
  }

  const gitconfig = join(root, "gitconfig");
  writeFileSync(gitconfig, "[user]\n\tname = Alex\n\temail = alex@example.com\n[init]\n\tdefaultBranch = main\n");
  const locale = lang === "fr" ? "fr_FR.UTF-8" : "en_US.UTF-8";
  const env = { ...process.env };
  // The machine's accounts and agents stay out of reach.
  for (const k of ["SSH_AUTH_SOCK", "GH_TOKEN", "GITHUB_TOKEN", "ANTHROPIC_API_KEY", "CLAUDECODE", "LANGUAGE", "LC_MESSAGES"]) delete env[k];
  Object.assign(env, {
    HOME: home,
    PATH: `${bin}${windows ? ";" : ":"}${process.env.PATH}`,
    LANG: locale,
    LC_ALL: locale,
    GIT_CONFIG_GLOBAL: gitconfig,
    GIT_CONFIG_NOSYSTEM: "1",
    TERMF_E2E_LOG: join(root, "calls.log"),
  });
  if (windows) Object.assign(env, appData);
  else {
    Object.assign(env, {
      XDG_CONFIG_HOME: join(home, ".config"),
      XDG_DATA_HOME: join(home, ".local", "share"),
      XDG_CACHE_HOME: join(home, ".cache"),
      XDG_STATE_HOME: join(home, ".local", "state"),
      SHELL: "/bin/bash",
      // No session bus: no keyring of the machine, no single instance shared with a TermF already open.
      DBUS_SESSION_BUS_ADDRESS: `unix:path=${join(root, "no-bus")}`,
      NO_AT_BRIDGE: "1",
    });
    if (dark) env.GTK_THEME = "Adwaita:dark";
    // A login shell resets the PATH (Debian's /etc/profile): the profile puts the fakes back first, as a user's would.
    const profile = `export PATH=${JSON.stringify(bin)}:"$PATH"\nexport PS1='$ '\nunset HISTFILE\n`;
    writeFileSync(join(home, ".bash_profile"), profile);
    writeFileSync(join(home, ".profile"), profile);
  }

  writeFileSync(join(acme, "README.md"), "# Acme API\n\nThe API of Acme.\n");
  mkdirSync(join(acme, "src"));
  writeFileSync(join(acme, "src", "app.js"), 'export const greet = (name) => "Hello " + name;\n');
  writeFileSync(join(acme, "package.json"), JSON.stringify({ name: "acme-api", scripts: { test: "node --test" } }, null, 2) + "\n");
  git(acme, env, "init", "-q", "-b", "main");
  git(acme, env, "add", "-A");
  git(acme, env, "commit", "-q", "-m", "First commit");
  git(acme, env, "remote", "add", "origin", "https://github.com/acme/api.git");

  const toml = (s) => JSON.stringify(s);
  for (const dir of configDirs) {
    writeFileSync(
      join(dir, "termf.toml"),
      `roots = [${toml(code)}]\nscan_depth = 3\nkeep_awake = "off"\n\n[agents]\nclaude = "claude"\n`,
    );
    if (project) writeFileSync(join(dir, "projects", "acme.toml"), `name = "acme"\n\n[[repos]]\npath = ${toml(acme)}\n`);
  }

  return {
    root,
    home,
    acme,
    bin,
    ...dirs,
    env,
    calls: env.TERMF_E2E_LOG,
    git: (...args) => execFileSync("git", args, { cwd: acme, env, encoding: "utf8" }),
    cleanup() {
      for (const d of [root, ...(windows ? Object.values(dirs) : [])]) {
        if (existsSync(d)) rmSync(d, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
      }
    },
  };
}
