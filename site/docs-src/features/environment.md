---
title: My environment
order: 9
---

My environment keeps the list of what your development machine is made of: your languages and their versions, your command-line tools, your apps and your editor's extensions. From that list, TermF writes the script that installs it all on another machine, skipping what is already there.

## A new machine in three commands {#new-machine}

```sh
# 1. TermF
curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh

# 2. your config, env.toml included (it asks your sync password)
termf sync setup --git <url>

# 3. everything env.toml lists
termf env install
```

On macOS, `termf` sits inside the app until you put it on your `PATH` (see [Installation](installation.md#cli)): run it as `/Applications/TermF.app/Contents/MacOS/termf`, or do steps 2 and 3 in the app, with **Settings › Encrypted sync**, then **My environment › Install what's missing**.

## Detect what you use

1. Open **My environment** (`⌘K`, then "My environment") and click **Detect**.
2. Each source shows as it ends. The detection only reads, with a time limit per command, and only asks the managers this machine has:
   - Homebrew, with `brew bundle dump`: taps, formulae, casks, App Store apps (with `mas`) and VS Code extensions;
   - the global packages of npm, pnpm and bun; `cargo install` and the rustup toolchains; pipx and `uv tool`; the GitHub CLI's extensions; `composer global`;
   - the global language versions of mise, asdf, fnm, nvm and Volta;
   - the programs installed by hand in `~/.local/bin`, `~/bin`, `~/.cargo/bin`, `~/.bun/bin`, `~/.deno/bin` and the other folders of your `PATH` under your home, and the apps of `/Applications` no cask installed.
3. In **Found here**, tick what you really use, then **Keep the selection**. What a manager installed comes ticked; what was installed by hand waits for your choice.

**Ask the agent** hands the result to the devops agent: it sorts out what a developer uses from the dependencies other packages pulled in, adds what the detection missed, and proposes the official install command of each tool installed by hand. Each change it writes shows in a card for you to approve.

## The list: env.toml {#manifest}

What you keep goes to `env.toml`, in TermF's config folder, by group: languages, command-line tools, apps and editors. It is written to be read by hand:

```toml
# Languages and their versions

[[languages]]
manager = "mise"
name = "node"
version = "22"
description = "JavaScript runtime"

# Command-line tools

[[cli]]
manager = "brew"
name = "ripgrep"
description = "Search tool like grep and The Silver Searcher"

[[cli]]
manager = "shell"
name = "claude"
description = "Claude Code"
install = "curl -fsSL https://claude.ai/install.sh | bash"
```

- Each entry keeps its manager, its name, its version when it counts (a language's) and a few words on what it is.
- **Add** takes any entry by hand, including a command of your own (manager `shell`): an official installer, a `curl … | sh`. It runs only when its program, or its `path`, is missing.
- Unticking an entry keeps it in the list but out of the script.
- `env.toml` and the scripts beside it sync with the **Config** scope of [sync](sync.md), and go into a config export.

## The install script {#script}

The **Script** tab shows the script TermF writes from the list. It is POSIX `sh`, for macOS and Linux:

- it installs Homebrew first when it is missing on macOS, then runs `brew bundle` with your Brewfile;
- then your own commands, the languages, and their packages (npm, pnpm, bun, cargo, pipx, uv, gh, composer), in that order;
- each step skips what is already installed, so the script runs again at will;
- it stops on the first error and says which step failed.

**Install what's missing** runs it in a TermF terminal you see, after a card that says what will happen. Homebrew and some casks ask for your password there.

**Prepare a script with the agent** opens a conversation with the devops agent. It asks a few short questions first (which machine, which groups to keep, whether to pin versions, whether to start with TermF and `termf sync setup`), then writes the script from your list, shows it, and saves it beside `env.toml` once you approve. The Script tab lists it and runs it the same way.

```sh
# what this machine has (read only); --save adds the new entries
termf env scan [--save]

# the list, an entry added or removed
termf env list
termf env add <manager> <name> [--version <v>] [--group <group>] [--description <text>] [--install <command>]
termf env remove <manager> <name>

# the script, printed or run
termf env script [<name>]
termf env install [<name>] [--yes]
```

> [!NOTE]
> Windows: not tried yet, and the install script is for macOS and Linux only (no `winget` export yet). A Brewfile line's options (`link: false`, `args`) are not kept: put such a formula in a script of your own.
>
> **Tried so far.** Tested with automated tests against stand-in package managers in a throwaway home: the detection, the list, the script's order, its skipping what is there and its stopping on a failure. Not yet run against real package managers on a fresh machine.
