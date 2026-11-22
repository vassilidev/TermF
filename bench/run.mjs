// The TermF agent team against a plain Claude Code session, on the same tickets of the same small project
// (`bench/fixture/`), with the same model and the same budget per trial. See bench/README.md.
//
//   node bench/run.mjs                              every ticket, both arms, 2 trials each, Sonnet, $25 at most
//   node bench/run.mjs --tickets s1,c1 --trials 1   a first figure quickly
//   node bench/run.mjs --arms team --tickets s1     one arm only
//   node bench/run.mjs --out bench/results/<date>   go on with a results folder: trials already there are kept
//   node bench/run.mjs --summary bench/results/<date>   only print the summary of a results folder
//   node bench/run.mjs --remeasure bench/results/<date> measure each trial again from its diff.patch, no judge
//
// Options: --model (sonnet), --trials (2), --parallel (4), --budget (25, US dollars in all, judge included),
// --cap (3, US dollars per trial), --minutes (40 per trial), --judge-model (sonnet), --keep (keep the trial folders).
// Needs Node 22.18+, git, cargo, and Claude Code signed in. Spends real money on your Claude Code account.
import { spawn, spawnSync } from "node:child_process";
import { appendFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const BENCH = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(BENCH, "..");

/** The tickets: `kind`, and whether the README must change. */
export const TICKETS = {
  "s1-done-filter": { kind: "simple", docs: false },
  "s2-due-date": { kind: "simple", docs: true },
  "s3-title-errors": { kind: "simple", docs: false },
  "c1-tags": { kind: "complex", docs: true },
  "c2-status": { kind: "complex", docs: true },
  // Accounts change every route on purpose: the first release's tests cannot pass any more.
  "c3-accounts": { kind: "complex", docs: true, original: false },
};

/** What a ticket of this project may touch; anything else is out of scope. */
export const IN_SCOPE = /^(src|test|public)\/|^README\.md$/;

/** Claude Code with none of the tester's settings, CLAUDE.md, skills, plugins or MCP servers. */
const BARE = ["--setting-sources", "project,local", "--strict-mcp-config"];

// SIGKILL: a test runner busy formatting a huge failure ignores SIGTERM, and spawnSync would wait on it forever.
const sh = (cmd, args, opts = {}) => spawnSync(cmd, args, { encoding: "utf8", maxBuffer: 64 << 20, killSignal: "SIGKILL", ...opts });
const git = (dir, ...args) => {
  const r = sh("git", ["-C", dir, ...args], { env: gitEnv });
  if (r.status !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr}`);
  return r.stdout.trim();
};
let gitEnv = process.env;

function options(argv) {
  const o = { model: "sonnet", trials: 2, parallel: 4, budget: 25, cap: 3, minutes: 40, "judge-model": "sonnet" };
  for (let i = 0; i < argv.length; i++) {
    const key = argv[i].replace(/^--/, "");
    if (key === "keep") o.keep = true;
    else o[key] = argv[++i];
  }
  for (const k of ["trials", "parallel", "budget", "cap", "minutes"]) o[k] = Number(o[k]);
  o.tickets = o.tickets ? o.tickets.split(",") : Object.keys(TICKETS);
  o.arms = o.arms ? o.arms.split(",") : ["solo", "team"];
  for (const t of o.tickets) if (!TICKETS[t]) throw new Error(`no ticket ${t}`);
  return o;
}

/** A fresh copy of the project, committed, with a local bare `origin`: what both arms start from. */
function prepare(dir) {
  const repo = join(dir, "repo");
  cpSync(join(BENCH, "fixture"), repo, { recursive: true });
  git(repo, "init", "-q", "-b", "main");
  git(repo, "add", "-A");
  git(repo, "commit", "-q", "-m", "Tasks: first release");
  git(dir, "clone", "-q", "--bare", repo, join(dir, "remote.git"));
  git(repo, "remote", "add", "origin", join(dir, "remote.git"));
  git(repo, "fetch", "-q", "origin");
  git(repo, "branch", "-q", "--set-upstream-to", "origin/main");
  return { repo, base: git(repo, "rev-parse", "HEAD") };
}

/** Runs `cmd` until it exits or `minutes` pass (its process group is then killed); its output goes to `log`. */
function run(cmd, args, { cwd, env, log, minutes }) {
  return new Promise((done) => {
    const started = Date.now();
    const child = spawn(cmd, args, { cwd, env, detached: true, stdio: ["ignore", "pipe", "pipe"] });
    let out = "";
    const keep = (d) => {
      out += d;
      appendFileSync(log, d);
    };
    writeFileSync(log, "");
    child.stdout.on("data", keep);
    child.stderr.on("data", keep);
    const timer = setTimeout(() => {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {}
    }, minutes * 60_000);
    child.on("close", (code) => {
      clearTimeout(timer);
      done({ code, out, wall_s: (Date.now() - started) / 1000 });
    });
  });
}

async function solo(ticket, repo, o, dir) {
  const prompt = `Implement this ticket in this repository.\n\n${readFileSync(join(BENCH, "tickets", `${ticket}.md`), "utf8")}`;
  const args = ["-p", prompt, "--model", o.model, "--effort", "high", "--output-format", "json", ...BARE];
  args.push("--disable-slash-commands", "--disallowedTools", "Agent", "--permission-mode", "bypassPermissions");
  args.push("--max-budget-usd", String(o.cap));
  const r = await run("claude", args, { cwd: repo, env: gitEnv, log: join(dir, "solo.log"), minutes: o.minutes });
  let j = {};
  try {
    j = JSON.parse(r.out.slice(r.out.indexOf("{")));
  } catch {}
  const u = j.usage ?? {};
  return {
    state: j.is_error ? `error: ${j.subtype}` : r.code === 0 ? "done" : `exit ${r.code}`,
    // Claude Code's own clock: the runner's loop may be busy measuring another trial when this one ends.
    wall_s: j.duration_ms ? j.duration_ms / 1000 : r.wall_s,
    cost_usd: j.total_cost_usd ?? 0,
    sessions: 1,
    turns: j.num_turns ?? 0,
    tokens: {
      input: u.input_tokens ?? 0,
      output: u.output_tokens ?? 0,
      cache_read: u.cache_read_input_tokens ?? 0,
      cache_creation: u.cache_creation_input_tokens ?? 0,
    },
    final: repo,
    said: (j.result ?? "").slice(0, 2000),
  };
}

let benchBinary;
function teamBinary() {
  if (benchBinary) return benchBinary;
  const r = sh("cargo", ["test", "-p", "termf-cli", "--test", "bench", "--no-run", "--message-format=json"], { cwd: ROOT });
  if (r.status !== 0) throw new Error(`cargo: ${r.stderr}`);
  for (const line of r.stdout.split("\n")) {
    if (!line.startsWith("{")) continue;
    const m = JSON.parse(line);
    if (m.reason === "compiler-artifact" && m.target?.name === "bench" && m.executable) benchBinary = m.executable;
  }
  if (!benchBinary) throw new Error("cargo built no bench test binary");
  return benchBinary;
}

async function team(ticket, repo, o, dir, work) {
  const out = join(dir, "team.json");
  const env = {
    ...gitEnv,
    TERMF_BENCH_REPO: repo,
    TERMF_BENCH_TICKET: join(BENCH, "tickets", `${ticket}.md`),
    TERMF_BENCH_ROOT: join(work, "termf"),
    TERMF_BENCH_OUT: out,
    TERMF_BENCH_MODEL: o.model,
    TERMF_BENCH_BUDGET: String(o.cap),
    TERMF_BENCH_MINUTES: String(o.minutes),
  };
  const args = ["--ignored", "--exact", "team_arm", "--nocapture", "--test-threads", "1"];
  const r = await run(teamBinary(), args, { cwd: ROOT, env, log: join(dir, "team.log"), minutes: o.minutes + 5 });
  if (!existsSync(out)) return { state: `harness failed (exit ${r.code})`, wall_s: r.wall_s, cost_usd: 0, final: repo };
  const j = JSON.parse(readFileSync(out, "utf8"));
  const steps = (j.run.steps ?? []).map((s) => ({
    key: s.key,
    role: s.role,
    state: s.state,
    cost: s.cost,
    sessions: s.sessions?.length ?? 0,
    attempt: s.attempt,
  }));
  return {
    state: j.timed_out ? "timed out" : j.gave_up ? "gave up" : j.state,
    wall_s: j.wall_s,
    cost_usd: j.cost_usd,
    sessions: j.sessions,
    turns: j.turns,
    tokens: j.tokens,
    final: j.worktree ?? repo,
    routing: j.run.routing,
    plan_tasks: j.run.plan?.tasks?.length ?? 0,
    steps,
    approved: j.approved,
    answered: j.answered,
    note: j.run.note,
  };
}

const EXCLUDE = [":(exclude).claude", ":(exclude).bench-hidden", ":(exclude).bench-original"];

/** Tests, typecheck, scope and the diff of the final tree against `base`. */
function measure(ticket, final, base, dir, work) {
  git(final, "add", "-A");
  // Not trimmed: a last context line may be a lone space.
  const patch = sh("git", ["-C", final, "diff", "--cached", "-U5", base, "--", ".", ...EXCLUDE], { env: gitEnv }).stdout;
  const numstat = git(final, "diff", "--cached", "--numstat", base, "--", ".", ...EXCLUDE);
  writeFileSync(join(dir, "diff.patch"), patch);
  const files = numstat
    ? numstat.split("\n").map((l) => {
        const [added, removed, path] = l.split("\t");
        return { path, added: Number(added) || 0, removed: Number(removed) || 0 };
      })
    : [];
  const eval_ = join(work, "eval");
  cpSync(final, eval_, {
    recursive: true,
    filter: (src) => !/[/\\](\.git|\.claude|node_modules)([/\\]|$)/.test(src.slice(final.length)),
  });
  cpSync(join(BENCH, "hidden"), join(eval_, ".bench-hidden"), { recursive: true });
  mkdirSync(join(eval_, ".bench-original"));
  cpSync(join(BENCH, "fixture", "test", "tasks.test.ts"), join(eval_, ".bench-original", "tasks.test.ts"));
  const hidden = nodeTest(eval_, [`.bench-hidden/${ticket}.test.ts`]);
  hidden.total = (readFileSync(join(BENCH, "hidden", `${ticket}.test.ts`), "utf8").match(/^test\(/gm) ?? []).length;
  const own = sh("npm", ["test"], { cwd: eval_, timeout: 180_000 });
  const ownCounts = tap(own.stdout + own.stderr);
  const original = TICKETS[ticket].original === false ? null : nodeTest(eval_, [".bench-original/tasks.test.ts"]);
  const tsc = typecheck(eval_);
  const outOfScope = files.map((f) => f.path).filter((p) => !IN_SCOPE.test(p));
  return {
    hidden,
    own_tests: { ok: own.status === 0, ...ownCounts },
    original_tests: original,
    typecheck: tsc,
    files,
    lines_added: files.reduce((n, f) => n + f.added, 0),
    lines_removed: files.reduce((n, f) => n + f.removed, 0),
    tests_changed: files.filter((f) => f.path.startsWith("test/")).length,
    out_of_scope: outOfScope,
    docs_updated: files.some((f) => f.path === "README.md"),
    docs_needed: TICKETS[ticket].docs,
    patch,
  };
}

function tap(text) {
  const n = (k) => Number(new RegExp(`^# ${k} (\\d+)`, "m").exec(text)?.[1] ?? 0);
  const failed = [...text.matchAll(/^not ok \d+ - (.*)$/gm)].map((m) => m[1]);
  return { pass: n("pass"), fail: n("fail"), tests: n("tests"), failed };
}

function nodeTest(cwd, files) {
  const args = ["--disable-warning=ExperimentalWarning", "--test", "--test-force-exit", "--test-timeout=20000"];
  const r = sh("node", [...args, "--test-reporter=tap", ...files], { cwd, timeout: 180_000 });
  return tap(r.stdout + r.stderr);
}

/** The project's typecheck with the TypeScript the app already installs; `null` without it. */
function typecheck(cwd) {
  const tsc = join(ROOT, "app", "node_modules", "typescript", "bin", "tsc");
  if (!existsSync(tsc) || !existsSync(join(cwd, "tsconfig.json"))) return null;
  const r = sh("node", [tsc, "-p", "tsconfig.json"], { cwd, timeout: 180_000 });
  const lines = r.stdout.split("\n").filter((l) => /error TS\d+/.test(l));
  return { ok: r.status === 0, errors: lines.length, first: lines.slice(0, 3) };
}

const VERDICT = {
  type: "object",
  properties: {
    criteria: {
      type: "array",
      items: {
        type: "object",
        properties: { id: { type: "integer" }, verdict: { enum: ["met", "partial", "unmet"] }, why: { type: "string" } },
        required: ["id", "verdict", "why"],
      },
    },
    scores: {
      type: "object",
      properties: Object.fromEntries(
        ["correctness", "security", "readability", "tests", "error_handling", "edge_cases", "scope"].map((k) => [
          k,
          { type: "integer", minimum: 0, maximum: 10 },
        ]),
      ),
      required: ["correctness", "security", "readability", "tests", "error_handling", "edge_cases", "scope"],
    },
    findings: {
      type: "array",
      items: {
        type: "object",
        properties: {
          severity: { enum: ["high", "medium", "low"] },
          category: { enum: ["bug", "security", "readability", "tests", "error-handling", "edge-case", "scope", "over-engineering"] },
          file: { type: "string" },
          what: { type: "string" },
        },
        required: ["severity", "category", "what"],
      },
    },
    over_engineering: { enum: ["none", "some", "heavy"] },
    overall: { type: "integer", minimum: 0, maximum: 10 },
    summary: { type: "string" },
  },
  required: ["criteria", "scores", "findings", "over_engineering", "overall", "summary"],
};

/** A blind review: the judge sees the ticket and the diff, never which arm wrote it. */
async function judge(ticket, patch, o, dir) {
  const text = readFileSync(join(BENCH, "tickets", `${ticket}.md`), "utf8");
  const clipped = patch.length > 150_000 ? patch.slice(0, 150_000) + "\n[diff cut at 150 kB]" : patch;
  const prompt = `You are a strict senior reviewer. A developer implemented the ticket below in a small Node.js / TypeScript project (a task tracker: JSON API over node:sqlite, a one-page interface in public/, node:test tests). Judge only the diff.

1. For each numbered acceptance criterion, say met, partial or unmet, with a one-line reason drawn from the diff.
2. Score from 0 to 10: correctness, security, readability, tests (are the new behaviours covered by meaningful tests), error_handling, edge_cases, scope (10 = only what the ticket needs, no unrelated change).
3. List your review findings as a code reviewer would: real bugs, security issues, missing tests, poor error handling, unhandled edge cases, changes outside the ticket, over-engineering. Only findings you can point to in the diff; none is fine.
4. over_engineering: none, some or heavy. overall: 0 to 10. summary: two sentences.

<ticket>
${text}
</ticket>

<diff>
${clipped || "(empty: nothing changed)"}
</diff>`;
  const cwd = mkdtempSync(join(tmpdir(), "bench-judge-"));
  const args = ["-p", prompt, "--model", o["judge-model"], "--output-format", "json", ...BARE];
  args.push("--disable-slash-commands", "--tools", "", "--no-session-persistence", "--json-schema", JSON.stringify(VERDICT));
  const r = await run("claude", args, { cwd, env: process.env, log: join(dir, "judge.log"), minutes: 10 });
  rmSync(cwd, { recursive: true, force: true });
  try {
    const j = JSON.parse(r.out.slice(r.out.indexOf("{")));
    return { ...j.structured_output, cost_usd: j.total_cost_usd ?? 0 };
  } catch {
    return { error: r.out.slice(0, 500), cost_usd: 0 };
  }
}

async function trial(t, o, results) {
  const dir = join(results, `${t.ticket}-${t.arm}-${t.n}`);
  mkdirSync(dir, { recursive: true });
  const work = mkdtempSync(join(tmpdir(), `termf-bench-${t.ticket}-${t.arm}-`));
  const { repo, base } = prepare(work);
  console.log(`start  ${t.ticket} ${t.arm} #${t.n}  (${work})`);
  let arm;
  try {
    arm = await (t.arm === "solo" ? solo : team)(t.ticket, repo, o, dir, work);
  } catch (e) {
    arm = { state: `failed: ${e.message}`, wall_s: 0, cost_usd: 0, final: repo };
  }
  const m = measure(t.ticket, existsSync(arm.final) ? arm.final : repo, base, dir, work);
  const verdict = await judge(t.ticket, m.patch, o, dir);
  const { patch, ...measures } = m;
  const result = { ...t, model: o.model, kind: TICKETS[t.ticket].kind, at: new Date().toISOString(), ...arm, ...measures, judge: verdict };
  delete result.final;
  writeFileSync(join(dir, "result.json"), JSON.stringify(result, null, 2));
  if (!o.keep) rmSync(work, { recursive: true, force: true });
  console.log(
    `done   ${t.ticket} ${t.arm} #${t.n}: ${result.state}, ${Math.round(result.wall_s)} s, $${result.cost_usd.toFixed(2)}, ` +
      `hidden ${m.hidden.pass}/${m.hidden.total}, judge ${verdict.overall ?? "?"}/10`,
  );
  return result;
}

export function load(results) {
  if (!existsSync(results)) return [];
  return readdirSync(results)
    .map((d) => join(results, d, "result.json"))
    .filter(existsSync)
    .map((f) => JSON.parse(readFileSync(f, "utf8")));
}

const mean = (xs) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null);

/** Means per ticket and arm, and per kind and arm. */
export function summarize(rows) {
  const group = (key) => {
    const out = {};
    for (const r of rows) (out[`${key(r)}|${r.arm}`] ??= []).push(r);
    return Object.entries(out).map(([k, rs]) => {
      const [name, arm] = k.split("|");
      const met = rs.flatMap((r) => r.judge?.criteria ?? []);
      return {
        name,
        arm,
        trials: rs.length,
        wall_s: mean(rs.map((r) => r.wall_s)),
        cost_usd: mean(rs.map((r) => r.cost_usd)),
        sessions: mean(rs.map((r) => r.sessions ?? 1)),
        hidden_pct: mean(rs.map((r) => (r.hidden.total ? (100 * r.hidden.pass) / r.hidden.total : 0))),
        own_tests_green: mean(rs.map((r) => (r.own_tests.ok ? 100 : 0))),
        own_tests: mean(rs.map((r) => r.own_tests.tests)),
        original_green: mean(rs.filter((r) => r.original_tests).map((r) => (r.original_tests.fail === 0 && r.original_tests.pass > 0 ? 100 : 0))),
        criteria_met_pct: met.length ? (100 * met.filter((c) => c.verdict === "met").length) / met.length : null,
        judge_overall: mean(rs.map((r) => r.judge?.overall).filter((x) => x !== undefined)),
        findings_high: mean(rs.map((r) => (r.judge?.findings ?? []).filter((f) => f.severity === "high").length)),
        findings: mean(rs.map((r) => (r.judge?.findings ?? []).length)),
        out_of_scope: mean(rs.map((r) => r.out_of_scope.length)),
        lines: mean(rs.map((r) => r.lines_added + r.lines_removed)),
        typecheck_ok: mean(rs.filter((r) => r.typecheck).map((r) => (r.typecheck.ok ? 100 : 0))),
        docs_ok: mean(rs.filter((r) => r.docs_needed).map((r) => (r.docs_updated ? 100 : 0))),
      };
    });
  };
  return { tickets: group((r) => r.ticket), kinds: group((r) => r.kind), all: group(() => "all") };
}

/** Markdown tables, ready for docs/BENCHMARK.md. */
function print(summary) {
  const f = (x, d = 0) => (x === null || x === undefined ? "–" : x.toFixed(d));
  const cols = ["", "arm", "n", "time (s)", "cost ($)", "sessions", "hidden tests %", "own tests green %", "AC met %", "judge /10", "findings", "high", "out of scope", "lines", "typecheck ok %", "docs %", "project tests"];
  for (const part of ["tickets", "kinds", "all"]) {
    console.log(`\n| ${cols.join(" | ")} |\n|${cols.map(() => "---").join("|")}|`);
    for (const s of summary[part].sort((a, b) => a.name.localeCompare(b.name) || a.arm.localeCompare(b.arm))) {
      const cells = [s.name, s.arm, s.trials, f(s.wall_s), f(s.cost_usd, 2), f(s.sessions, 1), f(s.hidden_pct), f(s.own_tests_green), f(s.criteria_met_pct)];
      cells.push(f(s.judge_overall, 1), f(s.findings, 1), f(s.findings_high, 1), f(s.out_of_scope, 1), f(s.lines), f(s.typecheck_ok), f(s.docs_ok), f(s.own_tests));
      console.log(`| ${cells.join(" | ")} |`);
    }
  }
}

/** Measures every trial of a results folder again (the hidden tests may have changed) from its `diff.patch`. */
function remeasure(results) {
  for (const r of load(results)) {
    const dir = join(results, `${r.ticket}-${r.arm}-${r.n}`);
    const work = mkdtempSync(join(tmpdir(), "termf-bench-remeasure-"));
    const { repo, base } = prepare(work);
    const patch = readFileSync(join(dir, "diff.patch"), "utf8");
    if (patch.trim()) git(repo, "apply", "--recount", "--whitespace=nowarn", join(dir, "diff.patch"));
    const log = join(dir, "solo.log");
    if (r.arm === "solo" && existsSync(log)) {
      const text = readFileSync(log, "utf8");
      const ms = JSON.parse(text.slice(text.indexOf("{"))).duration_ms;
      if (ms) r.wall_s = ms / 1000;
    }
    const { patch: _, ...m } = measure(r.ticket, repo, base, dir, work);
    writeFileSync(join(dir, "result.json"), JSON.stringify({ ...r, ...m }, null, 2));
    rmSync(work, { recursive: true, force: true });
    console.log(`${r.ticket} ${r.arm} #${r.n}: hidden ${m.hidden.pass}/${m.hidden.total} ${m.hidden.failed.join("; ")}`);
  }
}

async function main() {
  const o = options(process.argv.slice(2));
  if (o.remeasure) {
    const results = resolve(o.remeasure);
    gitEnv = { ...process.env, GIT_CONFIG_GLOBAL: join(results, ".gitconfig"), GIT_CONFIG_NOSYSTEM: "1" };
    writeFileSync(gitEnv.GIT_CONFIG_GLOBAL, "[user]\n\tname = Bench\n\temail = bench@termf.invalid\n");
    return remeasure(results);
  }
  if (o.summary) {
    const summary = summarize(load(o.summary));
    writeFileSync(join(o.summary, "summary.json"), JSON.stringify(summary, null, 2));
    return print(summary);
  }
  const results = resolve(o.out ?? join(BENCH, "results", new Date().toISOString().slice(0, 10)));
  mkdirSync(results, { recursive: true });
  const gitconfig = join(results, ".gitconfig");
  writeFileSync(gitconfig, "[user]\n\tname = Bench\n\temail = bench@termf.invalid\n");
  gitEnv = { ...process.env, GIT_CONFIG_GLOBAL: gitconfig, GIT_CONFIG_NOSYSTEM: "1" };
  if (o.arms.includes("team")) teamBinary();

  const done = load(results);
  let spent = done.reduce((n, r) => n + r.cost_usd + (r.judge?.cost_usd ?? 0), 0);
  const queue = [];
  for (let n = 1; n <= o.trials; n++) {
    for (const ticket of o.tickets) {
      // Alternate which arm starts first.
      const arms = n % 2 ? o.arms : [...o.arms].reverse();
      for (const arm of arms) {
        if (!done.some((r) => r.ticket === ticket && r.arm === arm && r.n === n)) queue.push({ ticket, arm, n });
      }
    }
  }
  console.log(`${queue.length} trials to run, $${spent.toFixed(2)} already spent of $${o.budget}`);
  let running = 0;
  await new Promise((finish) => {
    const next = () => {
      while (running < o.parallel && queue.length) {
        // Each running trial may still spend up to its cap: wait for them; stop once even one trial no longer fits.
        if (spent + (running + 1) * o.cap > o.budget) {
          if (running) break;
          console.log(`budget: $${spent.toFixed(2)} spent, ${queue.length} trials left undone`);
          queue.length = 0;
          break;
        }
        const t = queue.shift();
        running++;
        trial(t, o, results)
          .then((r) => (spent += r.cost_usd + (r.judge?.cost_usd ?? 0)))
          .catch((e) => console.error(`${t.ticket} ${t.arm} #${t.n}: ${e.stack}`))
          .finally(() => {
            running--;
            next();
          });
      }
      if (!running && !queue.length) finish();
    };
    next();
  });
  const summary = summarize(load(results));
  writeFileSync(join(results, "summary.json"), JSON.stringify(summary, null, 2));
  print(summary);
  console.log(`\n$${spent.toFixed(2)} spent; results in ${results}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
