---
name: make
description: >-
  Runs and writes Makefile targets with GNU Make the way both a Mac (Make 3.81) and Linux (Make 4.x) read them:
  finding what a target runs before running it, dry runs that really stay dry, recipe lines that each get their
  own shell, phony targets, $$ for the shell, parallel builds, stopping at the first error. Use when a
  repository has a Makefile and a task runs make, adds or fixes a target, or asks why make says "Nothing to be
  done" or "missing separator". Not for Dockerfiles (docker), CI workflows (github-actions), nor a language's own
  build tool (cargo, npm, gradle).
license: MIT
metadata:
  termf-stacks: "make"
  termf-roles: "all"
  termf-versions: "*"
  termf-verified: "2026-11-22 GNU Make 4.4.1, macOS 26.5 ships GNU Make 3.81"
  termf-sources: "https://www.gnu.org/software/make/manual/make.html https://www.gnu.org/software/make/manual/html_node/Instead-of-Execution.html https://git.savannah.gnu.org/cgit/make.git/tree/NEWS https://ftp.gnu.org/gnu/make/"
---
# make

1. Read a target before running it: its recipe in the `Makefile` and in the files it `include`s, and the `help` target if there is one. `make -n <target>` prints the commands without running them, except lines that start with `+` and lines that call `$(MAKE)`, which run anyway. Done when you can say what the target will run.
2. Use the project's targets for its checks (`make lint`, `make test`, `make build`), because they carry the flags and versions the CI uses. Run them from the repository root or with `make -C <dir> <target>`. Done when the target's output and exit status are in your report: 0 is success, 2 is an error, 1 only answers `make -q`.
3. Write a target so it runs the same everywhere:
   - each recipe line starts with a real tab character;
   - each line runs in its own shell, so `cd` and shell variables do not carry over: join dependent commands with `&&` on one line, or continue the line with `\`;
   - write `$$` for a dollar the shell should see (`$$HOME`, `$$(git rev-parse --short HEAD)`), since make expands `$` first;
   - declare every target that is not a file in `.PHONY`, so a file or a folder of the same name never makes it look done;
   - keep `-` (ignore errors) off lines that must succeed.
   Done when `make -n <target>` prints what you expect and `make <target>` succeeds from a clean checkout.
4. Keep to what Make 3.81 reads when the project has no other requirement, since that is the `make` of every Mac. Done when the Makefile uses none of the features listed in `## Gotchas`, or the README already asks for a newer GNU Make.
5. Run the commands of `## Verify` and put each one with its result in your report.

## Gotchas

- macOS ships GNU Make 3.81 (2006). It lacks `.ONESHELL`, `.SHELLFLAGS` and `.RECIPEPREFIX` (added in 3.82), `--output-sync`, `!=`, `::=` and `$(file …)` (4.0), `.EXTRA_PREREQS` (4.3), `.WAIT` and `--shuffle` (4.4). Make 3.81 does not reject the first ones, it gets them wrong without a word: under `.ONESHELL` each line still gets its own shell, and `V != cmd` leaves `V` empty. A Makefile that needs them needs a newer GNU Make on the Mac: say so instead of rewriting it.
- "missing separator. Stop." means a recipe line starts with spaces instead of a tab. A file written by a tool that turns tabs into spaces breaks the same way; check with `make -n` after every edit.
- "Nothing to be done for 'test'" or "'test' is up to date" means a file or a folder named like the target exists and the target is not in `.PHONY`.
- A pipe in a recipe (`pytest | tee log`) fails only if the last command fails. Avoid the pipe, or start that line with `set -o pipefail;` when the Makefile sets `SHELL := /bin/bash`.
- `make -j` runs targets in parallel: a target that works alone but fails under `-j` is missing a prerequisite; declare it rather than dropping `-j`.
- Variables given on the command line (`make test V=1`) override the Makefile's; `?=` only sets a variable the environment has not set.

## Verify

- `make -n <target>` for every target you wrote or changed.
- The target itself, and the project targets that depend on it.

## Security

- Make runs whatever the recipe says: read the target, and the files it includes, before running it in a repository you have not read.
- `$(shell …)` in a `:=` assignment and `!=` run while the Makefile is read, so even `make -n` or `make help` runs them.
- Make prints each recipe line, variables expanded, before running it: a token on a recipe's command line ends up in the logs. Let the tool read it from the environment.
- Targets that deploy, publish, release, or `clean` with `rm -rf` run only when the user asked for them; read what `clean` deletes first.

## References

- GNU Make manual: https://www.gnu.org/software/make/manual/make.html
- What each version added (NEWS): https://git.savannah.gnu.org/cgit/make.git/tree/NEWS
