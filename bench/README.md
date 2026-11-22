# Bench: the agent team against a plain Claude Code session

The same tickets, on the same small project, done twice: once by **TermF's agent team** (the CTO triages, the team
frames, plans, implements, checks, reviews; every gate approved as a hurried user would, delivery stopped before the
pull request), once by **one plain Claude Code session** (`claude -p "Implement this ticket…"`, no skill, no plugin,
no subagent, none of your settings). Same model, same starting commit, same ticket text, same usage cap per trial.
The report is [`docs/BENCHMARK.md`](../docs/BENCHMARK.md).

## Run it

```sh
node bench/run.mjs --tickets s1-done-filter,c1-tags --trials 1   # a first figure: ~$3, ~25 min
node bench/run.mjs                                                # 6 tickets x 2 arms x 2 trials: ~$20, ~1 h 30
node bench/run.mjs --summary bench/results/<date>                 # the table again, from the results
```

Needs Node.js 22.18 or later, git, cargo, and Claude Code signed in: **both arms use your Claude Code account**. Every
amount in US dollars is the API-equivalent cost Claude Code computes (`total_cost_usd`): with a claude.ai subscription
nothing more is billed, the runs use your usage quota; with an API key, it is what you pay. The bench never runs in CI. Options: `--model` (`sonnet`), `--trials` (2), `--parallel` (4),
`--budget` (25 US dollars API-equivalent in all, the judge included: no trial starts once what is spent plus the cap of each
running trial would pass it), `--cap` (a usage cap of 3 US dollars API-equivalent per trial), `--minutes` (40 per trial), `--judge-model`
(`sonnet`), `--arms solo,team`, `--out <folder>` (go on with a results folder: trials already in it are kept),
`--keep` (keep the trial folders in the temporary folder), `--no-tsc` (no `tsc` on the PATH), `--remeasure <folder>`
(measure each trial again from its `diff.patch`, no judge). The bench machine has TypeScript installed: `tsc` from
`app/node_modules` is put on the PATH of both arms (after `pnpm install` in `app/`).

## What a trial does

1. Copies the ticket's project (`fixture/`, or `fixture-lists/` for the realistic tickets) to a temporary folder, commits it, gives it a local bare `origin` (nothing reaches GitHub).
2. **solo**: `claude -p` in that folder with `--setting-sources project,local --strict-mcp-config` (none of your
   settings, CLAUDE.md, skills, plugins or MCP servers), `--disable-slash-commands`, `Agent` disallowed,
   `--permission-mode bypassPermissions`, `--effort high`, `--max-budget-usd <cap>`; it works until it says it is
   done. **team**: the ignored test `crates/termf-cli/tests/bench.rs` (`cargo test -p termf-cli --test bench --
   --ignored team_arm`) starts a real team run on the ticket as a request, with throwaway TermF config and data,
   the shipped library, every role on the same model, the same Claude Code flags, and the run's budget set to the
   cap.
3. Measures the final tree (the team's: its run worktree): the hidden tests of the ticket (`hidden/`, copied in only
   now), the project's own `npm test`, the first release's tests (the project's `test/`), the typecheck (`tsc` from
   `app/node_modules`, after `pnpm install` in `app/`), the files changed outside `src/`, `test/`, `public/` and
   `README.md`, whether the README changed when the ticket needs it.
4. A blind judge (`claude -p`, Sonnet, no tool) reads the ticket and the diff only, never which arm wrote it, and
   answers each acceptance criterion (met / partial / unmet), seven scores out of 10, its review findings
   (severity, category) and an overall mark. For the realistic tickets it is told to weigh security, robustness and
   edge cases beyond the written criteria.

Each trial writes `results/<date>/<ticket>-<arm>-<n>/result.json` and `diff.patch` (logs stay local);
`summary.json` holds the means per ticket and per kind. Then the trial's folders and their Claude Code transcripts
(`claude purge`) are deleted, unless `--keep`.

## The tickets

| Id | Kind | What |
|---|---|---|
| `s1-done-filter` | simple | Bug: `?done=false` lists the done tasks |
| `s2-due-date` | simple | An optional `due_date` field, with its migration |
| `s3-title-errors` | simple | Error messages: a missing title is a 500 with a SQLite message |
| `c1-tags` | complex | Tags: API, own tables, filter, page, tests |
| `c2-status` | complex | Refactor `done` into a three-state `status`, migrating existing data, with compatibility |
| `c3-accounts` | complex | Accounts, sign-in, tokens, per-user tasks (security) |
| `r1-sharing` | realistic | Share a list with other accounts: roles, expiring invitation links (hidden security criteria) |
| `r2-slow-lists` | realistic | "The app is slow on big lists": cursor paging, N+1, indexes (hidden load and exactness tests) |
| `r3-lost-update` | realistic | "Edits sometimes disappear": a lost update under concurrency, versions and 409 |

The realistic tickets are on `fixture-lists/`, a shared task-list app of a few thousand lines (accounts, lists, tasks,
comments, activity, jobs, a front), with existing tests and a little debt.

The hidden tests talk to the API through `createServer(dbPath)` of `src/app.ts`, as the project's own tests do, and
read the database file for the data criteria.

## Your own project (a private suite)

`node bench/run.mjs --suite <folder> --out <folder>` runs the bench on a project of yours, with tickets, hidden tests
and results that never enter this repository: keep the suite and its results in a folder git ignores. `<folder>/suite.json`:
`source` (the project, only ever cloned: `git clone --no-hardlinks`, its remote removed, a local bare `origin`),
`ref` (the commit to check out), `copy` (what it does not commit, copied from it: its installed dependencies…), `env`
(the clone's test `.env`: no real service, no real database), `setup` and `measure_setup` (commands run in the clone,
and again before measuring), `checks` (the project's tests), `team_checks` (the team's checks step, `checks` by
default), `hidden_dir` (where the hidden tests are copied for their run), `hidden_run` (the command that runs them,
`<files>` replaced by their paths), `boot` (a command that says the app starts), `describe` (one generic sentence for
the judge), `in_scope` (a regex of the paths a ticket may touch) and `tickets` (`{ "<id>": { "kind": "real",
"hidden": ["<file>"], "implicit": true } }`), with `tickets/<id>.md` and `hidden/<file>` next to it. `checks` and
`hidden_run` must accept `--log-junit <file>` and write a JUnit report there; the tests run in the final tree.
