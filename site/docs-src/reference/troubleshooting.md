---
title: Troubleshooting and FAQ
order: 6
---

Short answers to the common questions, and what to do when something does not work.

## Questions

### Is TermF free? {#free}

Yes. It is open source under the MIT license, with no account and no paid tier. Your agents run on your own Claude Code, with your subscription or your API key.

### Does my code leave my machine? {#privacy}

TermF has no server and no telemetry. Your code goes where Claude Code sends it, to the model, as it would without TermF. The other connections are the ones you set up: GitHub or Jira for tickets, and the remote you choose for the sync. See [Security and privacy](security.md).

### What do I need? {#requirements}

Claude Code 2.1.288 or later, signed in, and `git`. The GitHub CLI, signed in, for GitHub tickets, pull requests and CI. TermF runs on macOS 11 or later (Apple Silicon and Intel), Windows (x64 and ARM64) and Linux (x86_64 and ARM64) with WebKitGTK 4.1. See [First launch](first-launch.md).

### How much does a run cost? {#cost}

It uses your Claude Code. The CTO sizes the team to the ticket, and each team run has a budget, 15 dollars by default, that stops it when it is spent; you can stop a run, or one agent, at any time. Even sized down, the team costs more than one session: about 3.4 times on a small ticket, 4.8 times on a complex one, where it does better. With a Claude subscription, these dollars are an API equivalent: nothing extra is billed, the run draws on your usage limits, which TermF shows. See [the team or one session?](team-vs-solo.md)

### Do I have to use the team? {#without-team}

No. Without the team, TermF is a terminal for Claude Code and your other agents, with tabs, splits and states, plus Git, CI/CD, a password vault and the rest. A ticket can also go to a single agent. For a small, clear change, one session is often enough.

### Does it work with Codex or Gemini? {#codex-gemini}

The CTO and the team run on Claude Code. Codex CLI and Gemini CLI open in terminals and conversations, and a team role can be set to run on one of them. These two adapters are new and have not been tried with the real tools yet. See [Codex and Gemini](other-agents.md).

### How far along is it? {#status}

Early development. The builds are not signed by Apple or Microsoft yet; the app updates itself. Every feature is driven in the real app by an end-to-end test on macOS, with a fake Claude Code; whole team runs have gone from a ticket to a real pull request with a real Claude Code. Little has been tried by hand on real projects yet, and Windows and Linux are built and tested in CI, not yet by hand. The full list of what is not verified is in [docs/STATUS.md](https://github.com/vassilidev/TermF/blob/main/docs/STATUS.md), in French.

## Troubleshooting

### macOS says the app cannot be opened {#macos-blocked}

Since 0.4.0, TermF is signed and notarized by Apple: this only happens with an older version. Install the latest one, or clear the quarantine once:

```sh
xattr -dr com.apple.quarantine /Applications/TermF.app
```

Or open it once, then in **System Settings › Privacy & Security**, click **Open Anyway**. Installing with the [one-line command](installation.md) avoids this step.

### Windows shows a SmartScreen warning {#smartscreen}

The builds are not signed by Microsoft yet. Click **More info**, then **Run anyway**. Installing with the [one-line command](installation.md) avoids this step.

### The install script refuses the release {#no-checksums}

The script refuses a release that publishes no `SHA256SUMS`, or a file whose sum differs: nothing is installed. Download the file for your system from the [download page](../download.html) instead, and [check its SHA-256](installation.md#check-a-download).

### A tool is missing on the Home {#missing-tools}

The Home lists the tools TermF needs that are missing, with the official command to install each one and its official page. For an older Claude Code, TermF tells you how to update it.

### termf is not found in my terminal {#termf-path}

Put it on your `PATH` from **Settings › What you need**, or by hand (see [Installation](installation.md#cli)).

### My shortcut goes to the shell, not to TermF {#shortcut-shell}

On Linux and Windows, Ctrl or Alt + a letter stays with the shell inside a terminal; add Shift to trigger TermF's action. See [Keyboard shortcuts](shortcuts.md).

### A team run stopped {#run-stopped}

A run stops when it waits for you (a gate, a question, a permission), when its budget is spent, or after a restart of TermF, which pauses every working team until you start it again. What waits for you shows on the run's page and on the project's Home. See [Runs and their steps](runs.md#budgets).

### Something else {#issue}

[Report an issue on GitHub](https://github.com/vassilidev/TermF/issues), with your system, TermF's version and what you did.
