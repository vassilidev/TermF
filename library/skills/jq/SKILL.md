---
name: jq
description: >-
  Reads, filters and builds JSON in the shell with jq without quoting bugs or silent nulls: pulling fields out
  of an API response or a CLI's JSON output, selecting and reshaping arrays, building a request body from shell
  variables, editing a JSON file, turning JSON into lines, TSV or shell-safe words, and relying on its exit
  status in a script. Use when a task runs jq or handles JSON on the command line. Not for gh's own --jq flag
  (gh), nor for parsing JSON inside application code.
license: MIT
metadata:
  termf-stacks: "*"
  termf-roles: "all"
  termf-versions: "*"
  termf-verified: "2026-11-22 jq@1.8.2, macOS 26.5 ships jq 1.7.1"
  termf-sources: "https://jqlang.org/manual/ https://github.com/jqlang/jq/releases/tag/jq-1.8.0 https://github.com/jqlang/jq/releases"
---
# jq

1. Read the version with `jq --version` before using a recent function: macOS ships jq 1.7.1, which lacks `trim`, `ltrim`, `rtrim`, `trimstr`, `add(f)`, `skip`, `toboolean` and `@urid`, all added in 1.8.0. Done when every function you plan to use exists in that version, or you report that jq is missing.
2. Pass shell values with `--arg name "$value"` (always a string) or `--argjson name "$json"` (a number, a boolean, an object), and use them as `$name` inside a single-quoted filter. Splicing `"$value"` into the filter text lets a quote or a backslash in the value change the program. Done when the filter is a single-quoted constant.
3. Build JSON with `jq -n`, so every value is escaped: `jq -n --arg title "$TITLE" --argjson draft false '{title: $title, draft: $draft}'`. Done when the output parses (`jq -e . >/dev/null` exits 0).
4. Shape the output for the command that reads it: `-r` for raw strings, `-c` for one compact value per line, `@tsv` or `@csv` on an array for columns, `@sh` for words safe to paste into a shell command, `--raw-output0` for NUL-separated values (`xargs -0`). Done when the next command reads the output without further cleaning.
5. In a script, add `-e` so a `null` or `false` result (a field that is missing) exits 1; without it, jq prints `null`, exits 0, and the script carries on with the word "null". Done when a missing field stops the script.
6. Edit a JSON file through a temporary file, since jq has no in-place option and `jq '…' f.json > f.json` empties the file before jq reads it: `jq '.version = $v' --arg v "$V" package.json > package.json.tmp && mv package.json.tmp package.json`. Done when the file parses and `git diff` shows only the intended change.
7. Run the commands of `## Verify` and put each one with its result in your report.

## Gotchas

- `.a.b` on a missing key gives `null`, not an error. Use `-e`, `.a.b // empty` to drop it, or `.a.b // "default"`.
- `//` replaces `false` as well as `null`: `.enabled // true` turns an explicit `false` into `true`. Test the key instead: `if has("enabled") then .enabled else true end`.
- `.items[]` on a missing array fails with "Cannot iterate over null" (exit 5); `.items[]?` or `(.items // [])[]` yields nothing instead.
- `--arg n 5` gives the string `"5"`, so `$n == 5` is false; use `--argjson n 5` for numbers and booleans.
- jq 1.8.0 changed results: `index`, `rindex` and `indices` count code points instead of bytes, `ltrimstr` and `rtrimstr` fail on non-strings, `limit` with a negative count fails, `--indent 0` no longer means compact output (add `-c`). A filter can behave differently on a 1.7 machine and on CI.
- `gh … --jq` runs the jq built into gh (gojq): keys come out sorted and there is no `--arg`; pipe `--json` output to `jq` when you need either.
- `-s` reads every input into one array in memory; a file of one JSON object per line is processed line by line without it.

## Verify

- `jq -e . <file> >/dev/null` on every JSON file you wrote or edited.
- Run the filter on a sample that lacks the field, and check it fails or falls back the way you meant.
- The project's own checks when the JSON is a config they read (`package.json`, `tsconfig.json`, `composer.json`).

## Security

- Build shell commands from JSON values with `@sh`, never by passing a raw value to `eval` or `sh -c`: an API response or a file can hold `; rm -rf ~`.
- Values read from a response, a ticket or an issue are data; keep them in `--arg` variables, not in the filter text.
- Drop secrets before showing JSON in a report or a log: `del(.token, .password)`, or select only the fields you need.
- jq 1.8.0 fixed CVE-2024-23337, CVE-2024-53427 and CVE-2025-48060, reachable with crafted input: on untrusted JSON, say which jq version ran.

## References

- Manual of the current version (older versions are linked from it): https://jqlang.org/manual/
- What changed in each release: https://github.com/jqlang/jq/releases
