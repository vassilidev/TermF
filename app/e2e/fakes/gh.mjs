// A stand-in for the GitHub CLI, signed in as "alex", with one open issue on acme/api; never on the network.
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";

const args = process.argv.slice(2);
if (process.env.TERMF_E2E_LOG) appendFileSync(process.env.TERMF_E2E_LOG, `gh ${JSON.stringify(args)}\n`);

export const ISSUE = {
  html_url: "https://github.com/acme/api/issues/42",
  number: 42,
  title: "Fix the login button",
  body: "Nothing happens on Safari.",
  state: "open",
  user: { login: "sam" },
  labels: [{ name: "bug" }],
  assignees: [{ login: "alex" }],
  updated_at: "2026-11-20T10:00:00Z",
};

/** A file the scenario wrote beside its runs (`gh-jobs.json`, `gh-log.txt`, `gh-workflow.yml`), or null. */
function given(name) {
  const runs = process.env.TERMF_E2E_GH_RUNS;
  const file = runs && join(dirname(runs), name);
  return file && existsSync(file) ? readFileSync(file, "utf8") : null;
}

/** What gh prints for `args`, its exit code, and what it prints on stderr. */
export function answer(args) {
  const line = args.join(" ");
  const http = (body) => `HTTP/2.0 200 OK\r\nEtag: "e2e"\r\n\r\n${JSON.stringify(body)}\n`;
  if (args[0] === "--version") return ["gh version 2.83.0 (e2e)\n", 0];
  if (args[0] === "auth") return ["github.com\n  ✓ Logged in to github.com account alex (keyring)\n", 0];
  if (args[0] === "api") {
    if (line.includes("search/issues")) return [http({ total_count: 1, items: [ISSUE] }), 0];
    // The scenario's runs, as GitHub lists them; none by default.
    const runs = JSON.parse(given("gh-runs.json") ?? "[]");
    if (/\/actions\/runs\/\d+\/jobs\b/.test(line)) {
      const jobs = JSON.parse(given("gh-jobs.json") ?? "[]");
      return [http({ total_count: jobs.length, jobs }), 0];
    }
    const one = args.map((a) => /\/actions\/runs\/(\d+)$/.exec(a)?.[1]).find(Boolean);
    if (one) return [http(runs.find((r) => String(r.id) === one) ?? {}), 0];
    if (line.includes("/actions/runs")) return [http({ total_count: runs.length, workflow_runs: runs }), 0];
    if (/\/actions\/jobs\/\d+\/logs/.test(line)) return [given("gh-log.txt") ?? "", 0];
    if (line.includes("/contents/.github/workflows/")) return [given("gh-workflow.yml") ?? "", 0];
    // Only a repository's admins see its webhooks.
    if (/repos\/acme\/api\/hooks\b/.test(line)) return ["HTTP/2.0 404 Not Found\r\n\r\n{}\n", 1, "gh: Not Found (HTTP 404)\n"];
    if (line.includes("/actions/workflows")) return [http({ total_count: 0, workflows: [] }), 0];
    if (/repos\/acme\/api\/issues\/42\/comments/.test(line)) return ["[[]]\n", 0];
    if (/repos\/acme\/api\/issues\/42\b/.test(line)) return [JSON.stringify(ISSUE) + "\n", 0];
    if (line.includes("user --jq")) return ["alex\n", 0];
    if (args.includes("graphql")) return ['{"data":{}}\n', 0];
    return [args.includes("--include") ? http({}) : "{}\n", 0];
  }
  if (args[1] === "list" || args.includes("--json")) return ["[]\n", 0];
  return ["", 0];
}

if (process.argv[1]?.endsWith("gh.mjs")) {
  const [out, code, err = ""] = answer(args);
  process.stdout.write(out);
  process.stderr.write(err);
  process.exitCode = code;
}
