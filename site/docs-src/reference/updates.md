---
title: Updates
order: 5
---

TermF checks for updates by itself, verifies their signature and installs them when you click; this page also covers what each system's build includes.

## Install and updates {#updates}

1. Install with the one-line command (see [Installation](installation.md)). It downloads the latest release for your system and processor, checks it against the release's checksums and installs it.
2. On first launch, the Home lists the tools TermF needs (git, Claude Code, the GitHub CLI, Chrome) with the official command to install each missing one. See [First launch](first-launch.md).
3. When an update is out, a banner offers it. Click it: the app restarts on the new version.

Updates are checked at start and once a day, and installed only when you click. The stable channel is the default; while there is no stable release, it takes the newest pre-release.

| Setting (`termf.toml`) | What it does |
|---|---|
| `[updates] channel` | `"stable"` (default) or `"beta"` |
| `[updates] check` | Turns the automatic check on or off |

The one-line command also updates an installed copy:

```sh
curl -fsSL https://vassilidev.github.io/TermF/install.sh | sh
```

> [!NOTE]
> The Windows build is not signed by Microsoft yet: downloaded with a browser, it still asks for one extra step on first launch. Installing an update has not been done for real yet: checking its signature, downloading it and restarting follow Tauri's documentation. A copy older than 0.2.0 has no updater: install 0.2.0 once by hand or with the script.
>
> **Tried so far.** The script ran in CI on the three systems against a local stand-in for GitHub's API, including a real AppImage install and the refusal of an altered file; an update offered with its notes is driven in the real app on macOS by an end-to-end test, against a local server.

## A native app on every system {#native-app}

TermF ships for macOS (Apple Silicon, Intel, or one universal image), Windows (x64 and ARM64) and Linux (x86_64 and ARM64), each in the form that system expects, with the TermF icon drawn for each size.

| System | Package | Notes |
|---|---|---|
| macOS 11 or later | `.dmg`: drag TermF onto Applications | A native menu in your system's language, Settings… on <kbd>⌘,</kbd>. Closing the window hides it while the agents keep working; <kbd>⌘Q</kbd> quits |
| Windows | `-setup.exe` or `.msi`, in English or French depending on your system | WebView2 is installed if it is missing. Uninstalling also takes TermF off your `Path` |
| Linux | `.deb`, `.rpm` or AppImage | Needs WebKitGTK 4.1. TermF appears in your applications, under Development |

The window opens where you left it, in your theme's color, without a white flash. On a Mac, a cold start takes about 0.4 second, about 100 MB of memory and close to no CPU when idle (release build, measured locally).

> [!NOTE]
> On Linux, the system's web view (WebKitGTK) takes about 450 MB at rest on a CI runner without a GPU: over the budget, and not TermF's own code. The window's place, the menu and the installers were checked by tests and by inspecting the files, not yet by hand on each system.
>
> **Tried so far.** Built for the seven targets by the release workflow; the app was launched in a throwaway home on macOS to check where its window comes back.
