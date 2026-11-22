# Bench: the agent team against a plain Claude Code session

The same tickets, on the same small project, done twice: once by **TermF's agent team** (the CTO triages, the team
frames, plans, implements, checks, reviews; every gate approved as a hurried user would, delivery stopped before the
pull request), once by **one plain Claude Code session** (`claude -p "Implement this ticket…"`, no skill, no plugin,
no subagent, none of your settings). Same model, same starting commit, same ticket text, same budget cap per trial.
The report is [`docs/BENCHMARK.md`](../docs/BENCHMARK.md).

## Run it

```sh
node bench/run.mjs --tickets s1-done-filter,c1-tags --trials 1   # a first figure: ~$3, ~25 min
node bench/run.mjs                                                # 6 tickets x 2 arms x 2 trials: ~$20, ~1 h 30
node bench/run.mjs --summary bench/results/<date>                 # the table again, from the results
```

Needs Node.js 22.18 or later, git, cargo, and Claude Code signed in: **both arms spend real money on your Claude
Code account**. The bench never runs in CI. Options: `--model` (`sonnet`), `--trials` (2), `--parallel` (4),
`--budget` (25 US dollars in all, the judge included: no trial starts once what is spent plus the cap of each
running trial would pass it), `--cap` (3 US dollars per trial), `--minutes` (40 per trial), `--judge-model`
(`sonnet`), `--arms solo,team`, `--out <folder>` (go on with a results folder: trials already in it are kept),
`--keep` (keep the trial folders in the temporary folder).

## What a trial does

1. Copies `fixture/` to a temporary folder, commits it, gives it a local bare `origin` (nothing reaches GitHub).
2. **solo**: `claude -p` in that folder with `--setting-sources project,local --strict-mcp-config` (none of your
   settings, CLAUDE.md, skills, plugins or MCP servers), `--disable-slash-commands`, `Agent` disallowed,
   `--permission-mode bypassPermissions`, `--effort high`, `--max-budget-usd <cap>`; it works until it says it is
   done. **team**: the ignored test `crates/termf-cli/tests/bench.rs` (`cargo test -p termf-cli --test bench --
   --ignored team_arm`) starts a real team run on the ticket as a request, with throwaway TermF config and data,
   the shipped library, every role on the same model, the same Claude Code flags, and the run's budget set to the
   cap.
3. Measures the final tree (the team's: its run worktree): the hidden tests of the ticket (`hidden/`, copied in only
   now), the project's own `npm test`, the first release's tests (`fixture/test/`), the typecheck (`tsc` from
   `app/node_modules`, after `pnpm install` in `app/`), the files changed outside `src/`, `test/`, `public/` and
   `README.md`, whether the README changed when the ticket needs it.
4. A blind judge (`claude -p`, Sonnet, no tool) reads the ticket and the diff only, never which arm wrote it, and
   answers each acceptance criterion (met / partial / unmet), seven scores out of 10, its review findings
   (severity, category) and an overall mark.

Each trial writes `results/<date>/<ticket>-<arm>-<n>/result.json` and `diff.patch` (logs stay local);
`summary.json` holds the means per ticket and per kind.

## The tickets

| Id | Kind | What |
|---|---|---|
| `s1-done-filter` | simple | Bug: `?done=false` lists the done tasks |
| `s2-due-date` | simple | An optional `due_date` field, with its migration |
| `s3-title-errors` | simple | Error messages: a missing title is a 500 with a SQLite message |
| `c1-tags` | complex | Tags: API, own tables, filter, page, tests |
| `c2-status` | complex | Refactor `done` into a three-state `status`, migrating existing data, with compatibility |
| `c3-accounts` | complex | Accounts, sign-in, tokens, per-user tasks (security) |

The hidden tests talk to the API through `createServer(dbPath)` of `src/app.ts`, as the project's own tests do, and
read the database file for the data criteria.
