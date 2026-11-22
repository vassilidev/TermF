---
name: sed-awk
description: >-
  Edits and extracts text with sed and awk so the same command works on macOS (BSD) and on Linux (GNU, mawk):
  in-place substitutions across many files, inserting or deleting lines, printing a range, splitting columns,
  summing a field, passing shell values without breaking the script. Use when a task runs sed or awk, or when
  a one-liner that works on Linux fails on a Mac or the reverse. Not for JSON (jq), nor for changing one or two
  files, which the editor tool does more safely.
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "all"
  termf-versions: "*"
  termf-verified: "2026-11-22 GNU sed 4.10, FreeBSD sed(1), macOS 26.5 sed and awk 20200816, POSIX.1-2024 awk"
  termf-sources: "https://www.gnu.org/software/sed/manual/sed.html https://man.freebsd.org/cgi/man.cgi?query=sed&sektion=1 https://pubs.opengroup.org/onlinepubs/9799919799/utilities/awk.html https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sed.html"
---
# sed and awk

1. Change one or two files with the editor tool instead: it shows the exact lines, fails when the text is not there, and behaves the same on every system. Keep sed and awk for the same mechanical change across many files, for extracting data, and for scripts the project keeps. Done when you can say why a shell tool fits better.
2. Write the portable form, because the project's scripts run on the user's Mac and on Linux CI: `sed --version` works only with GNU sed, and BSD sed answers "illegal option". Use a GNU-only form only in a script that already requires GNU tools. Done when each command below would run on both.
3. Edit in place with the backup suffix attached to `-i`, which GNU and BSD sed both read, then remove the backup: `sed -i.bak -e 's/old/new/g' "$f" && rm "$f.bak"`, one file at a time in a loop over the files `git ls-files` or `grep -l` lists. Done when `git diff` shows only the intended lines and `git status --porcelain` shows no `.bak` file.
4. Write regular expressions with `-E` (extended syntax on both) and POSIX classes: `[[:space:]]` for `\s`, `[[:alnum:]_]` for `\w`, `+`, `?` and `|` under `-E` instead of `\+`, `\?` and `\|`. Done when the expression has no backslash class or operator.
5. Escape a replacement that comes from a variable, since sed reads `&`, `\` and the delimiter in it as syntax: `esc=$(printf '%s' "$value" | sed -e 's/[\/&]/\\&/g')`, then `sed -e "s/old/$esc/"` (a bracket expression takes `\` literally, so this escapes backslashes too). Values with newlines go through the editor tool instead. Done when a value with `/`, `&` and `\` comes out unchanged.
6. In awk, pass a plain value with `-v name="$value"`, and a raw one (a path, a regex, anything with a backslash) through the environment: `NAME="$value" awk '{ print ENVIRON["NAME"] }'`, because `-v` turns `\t` and `\\` into a tab and a backslash. Done when the value prints exactly as given.
7. Keep awk programs to POSIX awk unless the project requires gawk: `gensub`, `asort`, `strftime`, `-i inplace` and `--csv` are not in every awk. Done when the program runs under the awk of both systems.
8. Run the commands of `## Verify` and put each one with its result in your report.

## Gotchas

- `sed -i 's/a/b/' file` on macOS takes `s/a/b/` as the backup suffix and `file` as the script, and fails; `sed -i '' …`, the macOS form, fails on Linux because GNU sed takes `''` as the script. `-i.bak` works on both.
- `a text`, `i text` and `c text` on one line are GNU extensions. BSD sed needs the command, a backslash, a real newline, then the text, which GNU sed reads too:
  ```sh
  sed -i.bak -e '/^\[deps\]/a\
  serde = "1"' Cargo.toml
  ```
- On BSD sed, `\s`, `\w` and `\+` match the plain characters `s`, `w` and `+`: the command exits 0 and edits the wrong text, or none. Check `git diff` after every bulk edit.
- In awk's `sub` and `gsub`, an `&` in the replacement stands for the matched text, even when the replacement comes from `ENVIRON`; write `\\&` for a literal ampersand.
- `awk` is gawk on some Linux systems, mawk on others, and the one-true-awk on macOS (version 20200816 on macOS 26.5), which prints a warning for `--csv`, ignores it and exits 0.
- The GNU coreutils forms differ too: `date -d` (BSD: `date -j -f`), `stat -c` (BSD: `stat -f`), `grep -P` (absent from BSD grep). Use them only where GNU tools are known to be installed.

## Verify

- `git diff` on the edited files shows only the lines you meant, and `git status --porcelain` shows no leftover `.bak`.
- Run the command on a two-line sample first (`printf 'a\nb\n' | sed …`) and compare with the expected output.
- The project's own checks on the files you changed (formatter, linter, tests).

## Security

- Never build a sed or awk program from text you did not write (a ticket, a file's content): GNU sed's `e` command and the `w` flag, and awk's `system()` and `|` pipes, run commands or write files.
- Quote the file list (`"$f"`, `-print0 | xargs -0`) so a name with spaces or a leading `-` is not read as an option.
- A bulk replacement can reach secrets, lockfiles or vendored code: restrict it to the paths the task names, from `git ls-files -- <paths>`.

## References

- GNU sed manual, with what is a GNU extension: https://www.gnu.org/software/sed/manual/sed.html
- BSD sed as on macOS: https://man.freebsd.org/cgi/man.cgi?query=sed&sektion=1
- POSIX awk and sed: https://pubs.opengroup.org/onlinepubs/9799919799/utilities/awk.html and https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sed.html
