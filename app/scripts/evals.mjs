// Runs the `claude plugin eval` suites of the skills and roles TermF ships, with the Claude Code on the PATH and the
// account it is signed in with (or ANTHROPIC_API_KEY). Each suite is a throwaway plugin built outside the
// repository: the skills every session gets, the skill under test, and for a role its prompt appended to the system
// prompt, as TermF gives it to a session.
//
//   pnpm evals                     the key suites (KEY), Haiku, one run per case, $3 at most in all
//   pnpm evals git cto             only these suites
//   pnpm evals --all               every skill and role that has evals
//   pnpm evals --budget 5 -- --runs 3 --ablation with-without
//                                  what follows `--` goes to `claude plugin eval` and wins over the defaults
//   pnpm evals --dry-run           build the plugins and print the commands, run nothing
//
// Exit code: 0 every case passed, 1 a case failed or a suite could not run, 2 the budget stopped the runs.
import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const LIBRARY = resolve(dirname(fileURLToPath(import.meta.url)), "../../library");

/** The most used skills and the key roles: what `pnpm evals` runs without names. */
export const KEY = [
  "git",
  "gh",
  "testing-js",
  "rust",
  "python",
  "sql-postgres",
  "security-web",
  "tests-from-criteria",
  "ux-writing",
  "legal-sources",
  "stock-media",
  "cto",
  "reviewer",
  "qa",
  "front",
  "back",
];

/** Tools a case may list that need a grant; Bash never gets one here (it would need the sandbox). */
const GRANTABLE = ["Write", "Edit", "WebFetch", "WebSearch"];

const read = (file) => (existsSync(file) ? readFileSync(file, "utf8") : "");
const frontmatter = (text) => /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(text);

/** Every suite that has evals: a shipped skill's `evals/`, or a role's `roles/evals/<role>/`. */
export function suites(library = LIBRARY) {
  const skills = readdirSync(join(library, "skills"))
    .filter((n) => !/^[_.]/.test(n) && existsSync(join(library, "skills", n, "evals")))
    .map((name) => ({ name, kind: "skill", evals: join(library, "skills", name, "evals") }));
  const dir = join(library, "roles", "evals");
  const roles = existsSync(dir)
    ? readdirSync(dir)
        .filter((n) => existsSync(join(library, "roles", `${n}.md`)))
        .map((name) => ({ name, kind: "role", evals: join(dir, name) }))
    : [];
  return [...skills, ...roles];
}

/** The prompt of a session playing `name`, as TermF's `materialize` writes it. */
export function rolePrompt(text, name) {
  const head = frontmatter(text);
  return `## Role: ${name}\n\n${text.slice(head ? head[0].length : 0).trim()}`;
}

/** `prompt.md` with `system` appended to the session's system prompt; a JSON string is a YAML scalar. */
export function withSystemPrompt(prompt, system) {
  return prompt.replace(/^---\r?\n/, `---\nappend_system_prompt: ${JSON.stringify(system)}\n`);
}

/** Writes the plugin of `suite` in `dir`, and returns the grants and the flags its cases need. */
export function build(suite, dir, library = LIBRARY) {
  const skills = readdirSync(join(library, "skills")).filter(
    (n) =>
      (suite.kind === "skill" && n === suite.name) ||
      /termf-stacks:\s*"\*"/.test(read(join(library, "skills", n, "SKILL.md"))),
  );
  mkdirSync(join(dir, ".claude-plugin"), { recursive: true });
  writeFileSync(join(dir, ".claude-plugin", "plugin.json"), JSON.stringify({ name: "termf" }));
  for (const skill of skills) {
    const from = join(library, "skills", skill);
    cpSync(from, join(dir, "skills", skill), { recursive: true, filter: (src) => src !== join(from, "evals") });
  }
  cpSync(suite.evals, join(dir, "evals"), { recursive: true });
  const role = suite.kind === "role" && rolePrompt(read(join(library, "roles", `${suite.name}.md`)), suite.name);
  const grants = new Set();
  let scaffold = false;
  for (const name of readdirSync(join(dir, "evals"))) {
    const file = join(dir, "evals", name, "prompt.md");
    const prompt = read(file);
    const tools = /allowed_tools:\s*\[([^\]]*)\]/.exec(frontmatter(prompt)?.[1] ?? "")?.[1] ?? "";
    for (const tool of tools.split(",").map((t) => t.trim())) if (GRANTABLE.includes(tool)) grants.add(tool);
    if (role && prompt) writeFileSync(file, withSystemPrompt(prompt, role));
    scaffold ||= /scaffold_script:/.test(read(join(dir, "evals", name, "case.yaml")));
  }
  return { grants: [...grants], scaffold };
}

/** The `claude plugin eval` arguments of one suite: the defaults, then `extra`, which win. */
export function command(dir, out, { grants, scaffold }, budget, extra) {
  const args = ["plugin", "eval", dir, "--trust-plugin", "--no-publish", "--model", "haiku", "--runs", "1"];
  args.push("--ablation", "none", "--max-cost-usd", budget.toFixed(2), "--output-dir", out);
  if (grants.length) args.push("--allow-tools", ...grants);
  if (scaffold) args.push("--scaffold");
  return [...args, ...extra];
}

function main(argv) {
  const dash = argv.indexOf("--");
  const own = dash < 0 ? argv : argv.slice(0, dash);
  const extra = dash < 0 ? [] : argv.slice(dash + 1);
  if (own.includes("--help") || own.includes("-h")) {
    console.log(readFileSync(fileURLToPath(import.meta.url), "utf8").split("\nimport ")[0].replace(/^\/\/ ?/gm, ""));
    return 0;
  }
  const flag = (name, fallback) => {
    const at = own.indexOf(name);
    return at < 0 ? fallback : own.splice(at, 2)[1];
  };
  const budget = Number(flag("--budget", "3"));
  const out = resolve(flag("--out", "") || mkdtempSync(join(tmpdir(), "termf-evals-")));
  const dry = own.includes("--dry-run");
  const all = own.includes("--all");
  const names = own.filter((a) => !a.startsWith("--"));
  const known = suites();
  const unknown = names.filter((n) => !known.some((s) => s.name === n));
  if (unknown.length || !(budget > 0)) {
    console.error(unknown.length ? `No evals for: ${unknown.join(", ")}` : "--budget takes a positive number of dollars");
    return 1;
  }
  const chosen = known.filter((s) => (names.length ? names.includes(s.name) : all || KEY.includes(s.name)));
  let spent = 0;
  const results = [];
  for (const suite of chosen) {
    const left = budget - spent;
    if (left < 0.05) {
      results.push({ suite: suite.name, skipped: "budget" });
      continue;
    }
    const plugin = join(out, suite.name, "plugin");
    const report = join(out, suite.name, "results");
    const args = command(plugin, report, build(suite, plugin), left, extra);
    console.log(`\n== ${suite.kind} ${suite.name}\nclaude ${args.join(" ")}`);
    if (dry) continue;
    const run = spawnSync("claude", args, { stdio: "inherit", cwd: plugin });
    const result = JSON.parse(read(join(report, "aggregate-result.json")) || "null");
    spent += result?.costUsd ?? 0;
    results.push({
      suite: suite.name,
      exit: run.status ?? run.error?.message,
      cost: result?.costUsd ?? null,
      passed: result?.aggregates?.casesPassed ?? 0,
      total: result?.aggregates?.casesTotal ?? 0,
      cases: (result?.cases ?? []).map((c) => ({
        name: c.name,
        score: c.aggregates?.score,
        failed: [...new Set((c.arms?.with ?? []).flatMap((r) => (r.graders ?? []).filter((g) => !g.passed).map((g) => g.name)))],
        error: c.arms?.with?.find((r) => r.error)?.error ?? null,
      })),
    });
  }
  if (dry) return 0;
  writeFileSync(join(out, "summary.json"), JSON.stringify({ spent, results }, null, 2));
  console.log("\nSUITE                 CASES   COST");
  for (const r of results) {
    const cases = r.skipped ? `skipped (${r.skipped})` : `${r.passed}/${r.total}`;
    const cost = r.cost == null ? "" : `$${r.cost.toFixed(2)}`;
    console.log(`${r.suite.padEnd(22)}${cases.padEnd(8)}${cost}`);
  }
  console.log(`\n$${spent.toFixed(2)} in all · reports in ${out}${sep}<suite>${sep}results`);
  if (results.some((r) => r.skipped || r.exit === 2)) return 2;
  return results.every((r) => r.exit === 0) ? 0 : 1;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) process.exit(main(process.argv.slice(2)));
