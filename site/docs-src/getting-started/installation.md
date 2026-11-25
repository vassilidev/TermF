---
title: Installation
order: 1
---

TermF installs with one command on macOS, Linux and Windows, or from the file for your system; it is free, open source, and needs Claude Code to run its agents.

## Install with one command

On macOS and Linux, in a terminal:

```sh
curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh
```

On Windows, in PowerShell:

```sh
irm https://vassilidev.github.io/TermF/install.ps1 | iex
```

The script takes the latest release for your system and processor, checks it against the release's SHA-256 sums, and installs it:

- **macOS**: `TermF.app` goes to `/Applications` (`~/Applications` when that is not writable).
- **Linux**: the package through `apt`, `dnf` or `zypper` (`sudo` asks for your password), or the AppImage in `~/.local/bin`.
- **Windows**: the per-user installer, silently, with no administrator rights.

On Windows, a file downloaded this way carries no mark of the web, so the installer, not signed yet, opens without SmartScreen's warning.

### Options

| Option | What it does |
|---|---|
| `--beta` | Takes the pre-releases too |
| `--format appimage`, `deb` or `rpm` | On Linux, the format to install (default: the system's package format, else the AppImage) |
| `--dry-run` | Says what would be installed, changes nothing |

Pass them after `sh -s --`:

```sh
curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh -s -- --beta
```

On Windows, set `TERMF_BETA=1` or `TERMF_DRY_RUN=1` before the command:

```sh
$env:TERMF_BETA=1; irm https://vassilidev.github.io/TermF/install.ps1 | iex
```

> [!NOTE]
> The script refuses a release that publishes no `SHA256SUMS`, or a file whose sum differs: nothing is installed. Such a release installs from its files, as below.

## Install from a file

Download the file for your system from the [download page](../download.html) or the [releases on GitHub](https://github.com/vassilidev/TermF/releases):

| System | File |
|---|---|
| macOS 11 or later, Apple Silicon | `TermF_<version>_aarch64.dmg` |
| macOS 11 or later, Intel | `TermF_<version>_x64.dmg` (or `_universal.dmg` for both) |
| Windows x64 | `TermF_<version>_x64-setup.exe`, or the `.msi` (English or French) |
| Windows ARM64 | `TermF_<version>_arm64-setup.exe` |
| Linux x86_64 or ARM64 | `.AppImage`, `.deb` or `.rpm` (needs WebKitGTK 4.1) |

### Open it the first time

**macOS.** Open the `.dmg` and drag TermF to Applications. TermF is signed and notarized by Apple: it opens like any other app.

**Windows.** Run the `-setup.exe` or the `.msi`. The build is not signed by Microsoft yet: on the SmartScreen warning, click **More info**, then **Run anyway**.

**Linux.** Run the AppImage, or install the package:

```sh
# any distribution
chmod +x TermF_<version>_amd64.AppImage && ./TermF_<version>_amd64.AppImage

# Debian, Ubuntu, Mint, Pop!_OS
sudo apt install ./TermF_<version>_amd64.deb

# Fedora, openSUSE, Rocky, Alma
sudo dnf install ./TermF-<version>-1.x86_64.rpm
```

TermF then appears in your applications, under Development.

### Check a download

Compare the file's SHA-256 with the one listed on the download page, which GitHub computes for each file of the release:

```sh
# macOS
shasum -a 256 TermF_*_aarch64.dmg

# Linux
sha256sum TermF_*_amd64.AppImage

# Windows (PowerShell)
Get-FileHash .\TermF_*_x64-setup.exe -Algorithm SHA256
```

## The termf command line {#cli}

`termf` ships inside the app: there is nothing else to install. Put it on your `PATH` in one click from the app's **Settings › What you need**, or by hand:

| System | Where it is |
|---|---|
| macOS | `/Applications/TermF.app/Contents/MacOS/termf`: `sudo ln -s /Applications/TermF.app/Contents/MacOS/termf /usr/local/bin/termf` |
| Linux | The `.deb` and `.rpm` install it as `/usr/bin/termf` |
| Windows | `termf.exe` sits next to the app: add its folder to your `PATH` |

Then `termf help` lists every command. See [the termf CLI](cli.md).

## Build it from source

You need Rust (stable), Node 22 or later, pnpm and the Tauri prerequisites for your system.

```sh
git clone https://github.com/vassilidev/TermF.git
cd TermF/app && pnpm install

# run it
pnpm tauri dev

# build the installers for your system, with the command line inside
pnpm bundle
```

## Updates

Once installed, TermF keeps itself up to date: it checks for updates, verifies their signature and installs them when you click. See [Updates](updates.md).

Next: [first launch and what you need](first-launch.md).
